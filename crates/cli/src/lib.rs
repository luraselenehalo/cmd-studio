//! The `cmdstudio` command line (spec §5.7): `apply`, `doctor`, `undo`, `--cleanup` and
//! `set-aside`. A thin layer over the core: it reads the command line, picks Thai or English,
//! says what the core found and did, and turns it into an exit code. The Phase 2 app links this
//! library and calls `run` when it is started with arguments, so the app and the CLI are one exe.
//!
//! Only the commands that change the machine (apply, undo, `--cleanup`, set-aside) start the
//! way the core's commands do (`recover_at_start`): under CMD Studio's lock, a transaction a
//! crash cut off is rolled back first. `doctor` and `--help` only read: they take no lock and
//! run no recovery, so another run never blocks them.
pub mod args;

use args::{parse, Args, Command, UsageError, USAGE, USAGE_TH};
use cmdstudio_core::actions::{
    pending_incomplete, prepare_apply, recover_at_start, run_apply, undo, ActionError, Ctx,
    Handled, StartReport, UndoReport, UndoResult, UnreadablePoint, BUSY_TH,
};
use cmdstudio_core::adapters::{Host, System};
use cmdstudio_core::cleanup::{cleanup, Report};
use cmdstudio_core::doctor::{doctor, Status};
use cmdstudio_core::exec::{op_label, CLEANUP_FIX_EN, CLEANUP_FIX_TH, COMMAND_PROCESSOR_KEY};
use cmdstudio_core::generate::lua::Lang;
use cmdstudio_core::model::{parse_theme, Target};
use cmdstudio_core::plan::{Op, Paths, PlanError, Request, Warning, NERD_FONT_FAMILY};
use cmdstudio_core::resources::Resources;
use cmdstudio_core::snapshot::{Incomplete, SetAside, SnapshotStore, Unrestored};
use cmdstudio_core::txn::{Failure, Outcome};
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Done.
pub const EXIT_OK: i32 = 0;
/// Failed, and everything was rolled back, or nothing was changed: a refusal (another run holds
/// the lock, a restore point that must be read first, an AutoRun that needs a hand edit).
pub const EXIT_FAILED: i32 = 1;
/// The command line or the theme file is not valid, or another Clink is in AutoRun and
/// `--replace-other-clink` was not given.
pub const EXIT_INVALID: i32 = 2;
/// A rollback could not restore everything: the user must fix what it lists by hand.
pub const EXIT_INCOMPLETE: i32 = 3;

pub struct Io<'a> {
    pub out: &'a mut dyn Write,
    pub err: &'a mut dyn Write,
}

/// Where things are when no `--data-dir` / `--resources` is given, and the clock.
#[derive(Clone, Debug)]
pub struct Defaults {
    pub data_dir: PathBuf,
    pub resources: PathBuf,
    pub now_ms: u64,
}

/// `GetUserDefaultUILanguage` of Thai Windows.
const LANG_THAI: u16 = 0x041E;

/// What comes before a core's English text when it is shown in Thai.
const DETAILS_TH: &str = "รายละเอียด";

/// The language of everything said: `apply --lang`, else Windows' display language.
fn chosen_lang(parsed: &Result<Args, UsageError>, host: &dyn Host) -> Lang {
    match parsed {
        Ok(Args {
            command: Command::Apply {
                lang: Some(lang), ..
            },
            ..
        }) => *lang,
        _ if host.system_info().ui_language == LANG_THAI => Lang::Th,
        _ => Lang::En,
    }
}

/// Runs one command on the machine `sys` and returns the exit code.
pub fn run_with(args: &[OsString], io: &mut Io<'_>, sys: System<'_>, defaults: &Defaults) -> i32 {
    let parsed = parse(args);
    let mut say = Say::new(io, chosen_lang(&parsed, sys.host));
    let Args {
        command,
        data_dir,
        resources,
    } = match parsed {
        Ok(args) => args,
        Err(e) => {
            let (message, usage) = say.pick((&e.en, USAGE), (&e.th, USAGE_TH));
            let _ = write!(say.io.err, "cmdstudio: {message}\n\n{usage}");
            return EXIT_INVALID;
        }
    };
    let data_dir_given = data_dir.is_some();
    let ctx = Ctx {
        paths: Paths {
            data: data_dir.unwrap_or_else(|| defaults.data_dir.clone()),
        },
        resources: Resources {
            root: resources.unwrap_or_else(|| defaults.resources.clone()),
        },
        now_ms: defaults.now_ms,
    };
    match command {
        Command::Help => {
            let usage = say.pick(USAGE, USAGE_TH);
            let _ = write!(say.io.out, "{usage}");
            EXIT_OK
        }
        Command::Doctor { json } => say.doctor(sys, &ctx, json),
        Command::Apply {
            theme,
            target,
            replace_other_clink,
            lang: _,
        } => {
            // The welcome box speaks the language the messages do (`chosen_lang`).
            let req = Request {
                target,
                // Like the wizard's checkbox, which is ticked by default (spec §4.2 page 4).
                set_wt_default_profile: true,
                lang: say.lang,
                replace_foreign_clink: replace_other_clink,
            };
            say.apply(sys, &ctx, &theme, &req)
        }
        Command::Undo => say.undo(sys, &ctx),
        Command::Cleanup { remove_themes } => say.cleanup(sys, &ctx, remove_themes, data_dir_given),
        Command::SetAside { id } => say.set_aside(sys, &ctx, &id),
    }
}

/// What a code means in Thai (spec §5.9), for a text the core gives in English only.
fn meaning_th(code: &str) -> Option<&'static str> {
    Some(match code {
        "E_WT_NOT_FOUND" => "ไม่พบ Windows Terminal หรือพบแต่ยังไม่เคยเปิด จึงยังไม่มี settings.json",
        "E_WT_PARSE" => {
            "อ่าน settings.json ของ Windows Terminal ไม่ได้ หรือไฟล์เขียนคีย์ที่ CMD Studio ดูแลซ้ำ \
             หรือซ้อนลึกเกิน 64 ชั้น ต้องแก้ไฟล์ด้วยมือ"
        }
        "E_WT_WRITE" => "เขียน settings.json ของ Windows Terminal ไม่ได้",
        "E_AUTORUN_FOREIGN_CLINK" => "เจอ Clink ตัวอื่นใน AutoRun",
        "E_AUTORUN_WRITE" => "เขียน AutoRun ไม่ได้ หรือแก้อย่างปลอดภัยไม่ได้จึงต้องแก้ด้วยมือ",
        "E_CLINK_PATH" => "ไม่มี path ที่ cmd ใช้เรียก Clink ได้อย่างปลอดภัย",
        "E_FONT_INSTALL" => "ติดตั้งหรือถอนฟอนต์ไม่สำเร็จ",
        "E_RUNTIME_MISSING" => "ไฟล์ส่วนเสริมหาย ถูกแก้ หรือรันไม่ได้",
        "E_VERIFY_OMP" => "ขั้นตรวจผล: Oh My Posh สร้าง prompt จาก config ไม่ได้",
        "E_VERIFY_CLINK" => "ขั้นตรวจผล: Clink โหลดสคริปต์ไม่ได้",
        "E_VERIFY_READBACK" => "ขั้นตรวจผล: ค่าที่อ่านกลับมาไม่ตรงกับที่เขียน",
        "E_SNAPSHOT_WRITE" => "บันทึกจุดย้อนกลับไม่สำเร็จ",
        "E_SNAPSHOT_READ" => "อ่านจุดย้อนกลับไม่ได้ (ไฟล์หาย เสีย หรือเปิดไม่ได้ในตอนนี้)",
        "E_THEME_INVALID" => "ไฟล์ธีมผิดรูปแบบ",
        "E_THEME_NEWER_SCHEMA" => "ไฟล์ธีมมาจาก CMD Studio รุ่นที่ใหม่กว่า",
        "E_ROLLBACK_INCOMPLETE" => "ย้อนกลับได้ไม่ครบ บางส่วนต้องแก้ด้วยมือ",
        "E_TXN_INTERRUPTED" => "การเปลี่ยนแปลงครั้งก่อนถูกตัดกลางทาง (โปรแกรมหยุดทำงานก่อนเสร็จ)",
        "E_UNSUPPORTED_PLATFORM" => "เครื่องนี้ไม่ใช่ Windows แบบ x64 ส่วนเสริมที่แนบมามีแต่รุ่น x64",
        "E_DATA_WRITE" => "เขียนลงโฟลเดอร์ข้อมูลของ CMD Studio ไม่ได้",
        "E_STATE_INVALID" => {
            "ใช้ state.json ไม่ได้ เพราะไฟล์เสีย ถูกแก้ เปิดไม่ได้ หรือเขียนโดย CMD Studio รุ่นที่ใหม่กว่า"
        }
        "E_GENERATED_EDITED" => "มีคนแก้ไฟล์ใน generated\\ ด้วยมือ",
        "E_APP_PACKAGED" => "แอปรันอยู่ในแพ็กเกจ MSIX สิ่งที่แอปเขียนจึงไม่มีผลกับโปรแกรมอื่น",
        "E_THEME_NOT_LOADED" => "ใช้ธีมแล้ว แต่ยังไม่มีหน้าต่าง cmd ไหนโหลดธีม",
        "E_MACHINE_READ" => "อ่านการตั้งค่าของเครื่องนี้ไม่ได้ (registry ไฟล์ หรือรายชื่อฟอนต์)",
        "E_BUSY" => BUSY_TH,
        _ => return None,
    })
}

/// What `--cleanup` says when steps failed. The uninstaller shows the exit code; a font file
/// still in use or the record of a rollback that did not finish is often done by a second run.
const CLEANUP_AGAIN_EN: &str = "Not everything could be done: see each FAILED step above. Some \
     finish when you run `cmdstudio --cleanup` again, for example once every cmd window and \
     Windows Terminal are closed; the others say how to fix them by hand.";
const CLEANUP_AGAIN_TH: &str = "ยังทำไม่ครบทุกอย่าง ดูแต่ละขั้นที่ไม่สำเร็จด้านบน บางขั้นจะเสร็จเมื่อรัน \
     `cmdstudio --cleanup` อีกครั้ง เช่น หลังปิดหน้าต่าง cmd และ Windows Terminal ทุกหน้าต่าง \
     ส่วนขั้นอื่นบอกวิธีแก้ด้วยมือไว้";

/// What is in a data folder that only CMD Studio puts there.
const DATA_FOLDER_SIGNS: [&str; 5] = [
    "state.json",
    "snapshots",
    "runtime",
    "generated",
    "cmdstudio.lock",
];

/// What the user reads, in one language, and what this run said already: the starts of the CLI
/// and of the core's command both report what they found, and each thing is said once.
struct Say<'s, 'a> {
    io: &'s mut Io<'a>,
    lang: Lang,
    said: Vec<String>,
    /// The restore points whose record of a rollback that did not finish was listed.
    records: Vec<String>,
}

impl<'s, 'a> Say<'s, 'a> {
    fn new(io: &'s mut Io<'a>, lang: Lang) -> Self {
        Say {
            io,
            lang,
            said: Vec::new(),
            records: Vec::new(),
        }
    }

    fn pick<T>(&self, en: T, th: T) -> T {
        match self.lang {
            Lang::Th => th,
            Lang::En => en,
        }
    }

    fn out(&mut self, text: impl std::fmt::Display) {
        let _ = writeln!(self.io.out, "{text}");
    }

    fn err(&mut self, text: impl std::fmt::Display) {
        let _ = writeln!(self.io.err, "{text}");
    }

    /// Says `text` on stderr, unless this run said it already.
    fn once(&mut self, text: String) {
        if !self.said.contains(&text) {
            self.err(&text);
            self.said.push(text);
        }
    }

    /// `code: text`, where `en` is the core's English text. In Thai the code's meaning (spec
    /// §5.9) comes first, and the English text follows as details.
    fn coded(&self, code: &str, en: &str) -> String {
        match (self.lang, meaning_th(code)) {
            (Lang::Th, Some(th)) => format!("{code}: {th}\n  {DETAILS_TH}: {en}"),
            _ => format!("{code}: {en}"),
        }
    }

    fn note(&self, text: &str) -> String {
        format!("{}{text}", self.pick("Note: ", "หมายเหตุ: "))
    }

    /// The core's words for `e`: its Thai text where it has one.
    fn error_text(&self, e: &ActionError) -> String {
        match (self.lang, e.message_th()) {
            (Lang::Th, Some(th)) => format!("{}: {th}", e.code()),
            _ => self.coded(e.code(), &e.to_string()),
        }
    }

    /// How to fix one step by hand, in the chosen language. A record written before the Thai
    /// text existed has only the English one.
    fn step_fix<'r>(&self, step: &'r Unrestored) -> &'r str {
        if self.lang == Lang::Th && !step.fix_th.is_empty() {
            &step.fix_th
        } else {
            &step.fix
        }
    }

    /// What every command that changes the machine does first (`recover_at_start`), told.
    /// `Err` with the exit code when the command must not go on: another run holds the lock,
    /// or (`strict`) the rollback of a transaction a crash cut off could not put everything
    /// back just now, so the user fixes that first. `--cleanup` and set-aside go on: they are
    /// how it is fixed.
    fn begin(&mut self, sys: System<'_>, ctx: &Ctx, strict: bool) -> Result<(), i32> {
        match recover_at_start(sys, ctx) {
            Ok(start) => {
                if self.start(&start) && strict {
                    Err(EXIT_INCOMPLETE)
                } else {
                    Ok(())
                }
            }
            Err(e) => Err(self.refused(sys, ctx, &e)),
        }
    }

    /// Tells what a start found and did, each thing once: a crash rolled back, restore points
    /// that cannot be read, what the start could not do, and the record of a rollback that did
    /// not finish, which is said before every command until a newer transaction runs. Whether
    /// the rollback of a transaction a crash cut off could not put everything back just now.
    fn start(&mut self, start: &StartReport) -> bool {
        let mut not_back = false;
        match &start.recovered {
            Some(Outcome::RolledBack { code, reason }) => {
                let text = self.pick(
                    format!("{code}: {reason}; the machine is as it was before it."),
                    format!(
                        "{code}: การเปลี่ยนแปลงครั้งก่อนถูกตัดกลางทาง จึงย้อนกลับให้แล้ว \
                         เครื่องกลับเป็นเหมือนก่อนการเปลี่ยนแปลงนั้น\n  {DETAILS_TH}: {reason}"
                    ),
                );
                self.once(text);
            }
            // Its journal cannot be read now, so nothing was rolled back: the command that
            // changes the machine refuses, and says what to do (`refused`).
            Some(Outcome::RollbackIncomplete { .. }) if start.recovery_pending => {}
            Some(Outcome::RollbackIncomplete {
                code,
                reason,
                failed,
            }) => {
                let text = self.coded(code, &format!("{reason}."));
                self.once(text);
                self.unrestored(start.incomplete.as_ref(), failed, &[]);
                not_back = true;
            }
            Some(Outcome::Committed) | None => {}
        }
        for point in &start.unreadable {
            let text = self.unreadable(point);
            self.once(text);
        }
        for problem in &start.problems {
            let text = self.note(&self.coded(problem.code, &problem.message));
            self.once(text);
        }
        if let Some(record) = &start.incomplete {
            if !self.records.contains(&record.id) {
                let text = self.pick(
                    format!(
                        "Note: the rollback of restore point {} did not finish.",
                        record.id
                    ),
                    format!("หมายเหตุ: การย้อนกลับของจุดย้อนกลับ {} ทำได้ไม่ครบ", record.id),
                );
                self.err(text);
                self.unrestored(Some(record), &[], &[]);
            }
        }
        not_back
    }

    fn unreadable(&self, u: &UnreadablePoint) -> String {
        let (id, code, why) = (&u.point.id, u.point.failure.code, &u.point.failure.message);
        match &u.handled {
            Handled::SetAside => self.pick(
                format!(
                    "Note: {code}: restore point {id} could not be used ({why}). It was set \
                     aside, its files kept for a look, and an undo no longer goes back to it."
                ),
                format!(
                    "หมายเหตุ: {code}: ใช้จุดย้อนกลับ {id} ไม่ได้ ({why}) จึงกันไว้ต่างหากแล้ว \
                     ไฟล์ของมันยังเก็บไว้ให้ดู และการย้อนกลับจะไม่ย้อนไปถึงจุดนี้อีก"
                ),
            ),
            Handled::LeftForLater => self.pick(
                format!(
                    "Note: {code}: restore point {id} can't be read right now ({why}). It was \
                     left as it is, and an undo does not go past it until it can be read."
                ),
                format!(
                    "หมายเหตุ: {code}: ตอนนี้อ่านจุดย้อนกลับ {id} ไม่ได้ ({why}) จึงปล่อยไว้ตามเดิม \
                     และการย้อนกลับจะไม่ย้อนข้ามจุดนี้จนกว่าจะอ่านได้"
                ),
            ),
            Handled::SetAsideFailed(f) => self.pick(
                format!(
                    "Note: {code}: restore point {id} could not be used ({why}), and setting it \
                     aside failed ({}: {}). An undo does not go past it.",
                    f.code, f.message
                ),
                format!(
                    "หมายเหตุ: {code}: ใช้จุดย้อนกลับ {id} ไม่ได้ ({why}) และกันไว้ต่างหากไม่สำเร็จ \
                     ({}: {}) การย้อนกลับจะไม่ย้อนข้ามจุดนี้",
                    f.code, f.message
                ),
            ),
        }
    }

    /// What a rollback could not put back, and how to fix each by hand (spec §5.5), from the
    /// core's record; without one, the targets come from `failed` and `ops`, and the fix is a
    /// clean-up.
    fn unrestored(&mut self, record: Option<&Incomplete>, failed: &[(usize, Failure)], ops: &[Op]) {
        let head = self.pick(
            "E_ROLLBACK_INCOMPLETE: not everything could be put back. Fix these by hand:",
            "E_ROLLBACK_INCOMPLETE: ย้อนกลับได้ไม่ครบ ให้แก้รายการเหล่านี้ด้วยมือ:",
        );
        self.err(head);
        let (cannot, fix) = self.pick(("could not put back", "fix"), ("ย้อนกลับไม่ได้", "วิธีแก้"));
        if let Some(r) = record {
            self.records.push(r.id.clone());
        }
        match record {
            Some(r) if !r.steps.is_empty() => {
                for s in &r.steps {
                    self.err(format!(
                        "  {cannot}: {} ({}: {})",
                        s.target, s.code, s.message
                    ));
                    let how = self.step_fix(s);
                    self.err(format!("    {fix}: {how}"));
                }
            }
            _ => {
                if failed.is_empty() {
                    let what = self.pick(
                        "what that change did (its journal could not be read)",
                        "สิ่งที่การเปลี่ยนแปลงนั้นทำไว้ (อ่านบันทึกของมันไม่ได้)",
                    );
                    self.err(format!("  {cannot}: {what}"));
                }
                for (i, f) in failed {
                    let what = ops.get(*i).map_or_else(|| format!("step {i}"), op_label);
                    self.err(format!("  {cannot}: {what} ({}: {})", f.code, f.message));
                }
                let how = self.pick(CLEANUP_FIX_EN, CLEANUP_FIX_TH);
                self.err(format!("    {fix}: {how}"));
            }
        }
    }

    /// Says why the core refused or stopped a command, and returns the exit code: 2 for another
    /// Clink without `--replace-other-clink`, which the user must agree to first; else 1, since
    /// nothing was changed.
    fn refused(&mut self, sys: System<'_>, ctx: &Ctx, e: &ActionError) -> i32 {
        let code = e.code();
        match e {
            ActionError::Plan(PlanError::ForeignClink { existing }) => {
                let head = self.pick(
                    "another Clink starts with cmd, from AutoRun. Turning it off takes out \
                     these commands:",
                    "มี Clink ตัวอื่นเริ่มพร้อม cmd จาก AutoRun ถ้าปิดมัน จะต้องเอาคำสั่งเหล่านี้ออก:",
                );
                self.err(format!("{code}: {head}"));
                for command in existing {
                    self.err(format!("  {command}"));
                }
                self.err(self.pick(
                    "Nothing was changed. To turn it off and use CMD Studio's Clink instead, run \
                     the same command again with --replace-other-clink (undo turns it back on).",
                    "ยังไม่ได้เปลี่ยนอะไร ถ้าจะปิด Clink ตัวนั้นแล้วใช้ของ CMD Studio แทน \
                     ให้รันคำสั่งเดิมอีกครั้งโดยเพิ่ม --replace-other-clink (undo จะเปิดกลับให้)",
                ));
                EXIT_INVALID
            }
            ActionError::Plan(PlanError::AutoRunNeedsHandEdit { part }) => {
                let head = self.pick(
                    "CMD Studio can't safely change AutoRun because of this command:",
                    "CMD Studio แก้ AutoRun อย่างปลอดภัยไม่ได้ เพราะคำสั่งนี้:",
                );
                self.err(format!("{code}: {head}"));
                self.err(format!("  {part}"));
                let key = COMMAND_PROCESSOR_KEY;
                self.err(self.pick(
                    format!(
                        "Nothing was changed. Fix it by hand: in Registry Editor, open {key} and \
                         correct or take out that command in the AutoRun value. Then run this \
                         again."
                    ),
                    format!(
                        "ยังไม่ได้เปลี่ยนอะไร ให้แก้ด้วยมือ: เปิด Registry Editor (regedit) ไปที่ {key} \
                         แล้วแก้หรือเอาคำสั่งนั้นออกจากค่า AutoRun จากนั้นรันคำสั่งนี้อีกครั้ง"
                    ),
                ));
                EXIT_FAILED
            }
            // What the user can do is reinstall; what exactly is missing is for whoever looks
            // closer (the core's text names a developer script).
            ActionError::Resources(why) => {
                self.err(self.pick(
                    format!(
                        "{code}: The files that come with CMD Studio are missing or damaged. \
                         Reinstall CMD Studio."
                    ),
                    format!("{code}: ไฟล์ที่แนบมากับ CMD Studio หายหรือเสีย ให้ติดตั้ง CMD Studio ใหม่"),
                ));
                let details = self.pick("details", DETAILS_TH);
                self.err(format!("  {details}: {why}"));
                EXIT_FAILED
            }
            ActionError::RecoveryPending { .. } => {
                self.err(self.error_text(e));
                self.set_aside_way_out(sys, ctx);
                EXIT_FAILED
            }
            ActionError::PendingIncomplete(record) => {
                self.err(self.error_text(e));
                if !self.records.contains(&record.id) {
                    self.unrestored(Some(record), &[], &[]);
                }
                EXIT_FAILED
            }
            _ => {
                self.err(self.error_text(e));
                EXIT_FAILED
            }
        }
    }

    /// While the journal of a transaction a crash may have cut off can't be read, every change
    /// is refused; if it never becomes readable, `cmdstudio set-aside` is the way out, at the
    /// price of checking by hand (doctor's R1 says the same).
    fn set_aside_way_out(&mut self, sys: System<'_>, ctx: &Ctx) {
        let root = ctx.paths.snapshots();
        // Recovery looks at the newest restore point, so that is the one.
        let newest = SnapshotStore::new(sys.fs, &root)
            .ids()
            .ok()
            .and_then(|mut ids| ids.pop());
        let command = match &newest {
            Some(id) => format!("`cmdstudio set-aside {id}`"),
            None => self.pick(
                "`cmdstudio set-aside <id>` (`cmdstudio doctor` names the id)".to_string(),
                "`cmdstudio set-aside <id>` (`cmdstudio doctor` บอกรหัสให้)".to_string(),
            ),
        };
        self.err(self.pick(
            format!(
                "If it never becomes readable, run {command} to set that restore point aside. \
                 What that change did is then not rolled back by itself, and must be checked by \
                 hand: {CLEANUP_FIX_EN}"
            ),
            format!(
                "ถ้าจุดย้อนกลับนั้นอ่านไม่ได้ตลอด ให้รันคำสั่ง {command} เพื่อกันไว้ต่างหาก \
                 แต่สิ่งที่การเปลี่ยนแปลงนั้นทำไว้จะไม่ถูกย้อนเอง ต้องตรวจด้วยมือ: {CLEANUP_FIX_TH}"
            ),
        ));
    }

    /// Tells how a transaction ended and returns its exit code. `ops` are its ops, when known,
    /// for a rollback whose record could not be read.
    fn finish(
        &mut self,
        sys: System<'_>,
        ctx: &Ctx,
        outcome: &Outcome,
        ops: &[Op],
        done: String,
    ) -> i32 {
        match outcome {
            Outcome::Committed => {
                self.out(done);
                EXIT_OK
            }
            Outcome::RolledBack { code, reason } => {
                self.err(self.coded(code, reason));
                self.err(self.pick(
                    "Everything was rolled back; nothing changed.",
                    "ย้อนกลับทุกอย่างแล้ว ไม่มีอะไรเปลี่ยน",
                ));
                EXIT_FAILED
            }
            Outcome::RollbackIncomplete {
                code,
                reason,
                failed,
            } => {
                self.err(self.coded(code, reason));
                // The record the core just wrote for this transaction: the newest one.
                let record = match pending_incomplete(sys, ctx) {
                    Ok(record) => record,
                    Err(e) => {
                        let text = self.note(&self.coded(e.code(), &e.to_string()));
                        self.err(text);
                        None
                    }
                };
                self.unrestored(record.as_ref(), failed, ops);
                EXIT_INCOMPLETE
            }
        }
    }

    fn warning(&self, w: &Warning) -> String {
        let text = match w {
            Warning::NoWindowsTerminal => self.pick(
                "Windows Terminal is not installed: only the prompt and the welcome box change."
                    .to_string(),
                "ไม่พบ Windows Terminal จึงเปลี่ยนแค่ prompt และกล่องต้อนรับ".to_string(),
            ),
            Warning::MachineWideClink => self.pick(
                "Clink is also set up for the whole machine; the theme's prompt may not work."
                    .to_string(),
                "มี Clink ตั้งไว้สำหรับทั้งเครื่องด้วย prompt ของธีมอาจไม่ทำงาน".to_string(),
            ),
            Warning::FontFallback { wanted } => self.pick(
                format!(
                    "The font {wanted:?} is not installed; Windows Terminal gets \
                     {NERD_FONT_FAMILY} instead."
                ),
                format!("ไม่ได้ติดตั้งฟอนต์ {wanted:?} Windows Terminal จึงใช้ {NERD_FONT_FAMILY} แทน"),
            ),
        };
        format!("{}{text}", self.pick("Warning: ", "คำเตือน: "))
    }

    fn apply(&mut self, sys: System<'_>, ctx: &Ctx, path: &Path, req: &Request) -> i32 {
        if let Err(code) = self.begin(sys, ctx, true) {
            return code;
        }
        let shown = path.display();
        let bytes = match sys.fs.read(path) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => {
                self.err(self.pick(
                    format!("E_THEME_INVALID: {shown} does not exist"),
                    format!("E_THEME_INVALID: ไม่มีไฟล์ {shown}"),
                ));
                return EXIT_INVALID;
            }
            Err(e) => {
                self.err(self.pick(
                    format!("E_THEME_INVALID: {shown} could not be read: {e}"),
                    format!("E_THEME_INVALID: อ่านไฟล์ {shown} ไม่ได้: {e}"),
                ));
                return EXIT_INVALID;
            }
        };
        let theme = match parse_theme(&bytes) {
            Ok(theme) => theme,
            Err(e) => {
                self.err(self.coded(e.code(), &e.to_string()));
                return EXIT_INVALID;
            }
        };
        let prepared = match prepare_apply(sys, ctx, &theme, req) {
            Ok(prepared) => prepared,
            Err(e) => return self.refused(sys, ctx, &e),
        };
        // Its own start found what the CLI's did, so only something new is said; one that left
        // a rollback unfinished just now stops it (and lets the lock go).
        if self.start(&prepared.start) {
            return EXIT_INCOMPLETE;
        }
        let name = &theme.name;
        self.out(self.pick(format!("Plan for {name:?}:"), format!("แผนของธีม {name:?}:")));
        let summary = self.pick(&prepared.plan.summary_en, &prepared.plan.summary_th);
        for line in summary {
            self.out(format!("  - {line}"));
        }
        for w in &prepared.plan.warnings {
            self.out(self.warning(w));
        }
        let ops = prepared.ops.clone();
        let outcome = run_apply(sys, ctx, prepared);
        let done = match req.target {
            Target::All => self.pick(
                format!(
                    "Applied {name:?}. Open a new cmd window to see the prompt and the welcome box."
                ),
                format!("ใช้ธีม {name:?} แล้ว เปิดหน้าต่าง cmd ใหม่เพื่อดู prompt และกล่องต้อนรับ"),
            ),
            Target::Wt => self.pick(
                format!(
                    "Applied {name:?}. Open a new cmd tab in Windows Terminal to see the prompt \
                     and the welcome box."
                ),
                format!(
                    "ใช้ธีม {name:?} แล้ว เปิดแท็บ cmd ใหม่ใน Windows Terminal เพื่อดู prompt \
                     และกล่องต้อนรับ"
                ),
            ),
        };
        self.finish(sys, ctx, &outcome, &ops, done)
    }

    fn undo(&mut self, sys: System<'_>, ctx: &Ctx) -> i32 {
        if let Err(code) = self.begin(sys, ctx, true) {
            return code;
        }
        let UndoReport {
            start,
            result,
            problems,
        } = match undo(sys, ctx) {
            Ok(report) => report,
            Err(e) => return self.refused(sys, ctx, &e),
        };
        if self.start(&start) {
            return EXIT_INCOMPLETE;
        }
        let code = match result {
            UndoResult::NothingToUndo => {
                self.out(self.pick("Nothing to undo.", "ไม่มีอะไรให้ย้อนกลับ"));
                EXIT_OK
            }
            UndoResult::Done { undid, outcome } => {
                let done = self.pick(
                    format!(
                        "Undid the theme applied in restore point {undid}: what it changed is \
                         back as it was. Open a new cmd window to see it."
                    ),
                    format!(
                        "ย้อนการใช้ธีมของจุดย้อนกลับ {undid} แล้ว สิ่งที่มันเปลี่ยนกลับเป็นเหมือนเดิม \
                         เปิดหน้าต่าง cmd ใหม่เพื่อดู"
                    ),
                );
                self.finish(sys, ctx, &outcome, &[], done)
            }
        };
        // What the undo could not do besides its transaction; the result stands.
        for problem in &problems {
            self.err(self.coded(problem.code, &problem.message));
        }
        code
    }

    /// `--cleanup` with `--data-dir`: deletes every entry of that folder but `themes\`, so a
    /// folder that does not look like CMD Studio's (a mistyped path) is refused before anything
    /// is touched. CMD Studio's is absent, empty, or holds something only CMD Studio puts there.
    fn check_data_dir(&mut self, sys: System<'_>, ctx: &Ctx) -> Result<(), i32> {
        let data = &ctx.paths.data;
        let entries = match sys.fs.list_dir(data) {
            Ok(entries) => entries,
            Err(e) => {
                self.err(self.coded(
                    "E_MACHINE_READ",
                    &format!(
                        "what {} holds could not be listed ({e}), so nothing was deleted",
                        data.display()
                    ),
                ));
                return Err(EXIT_FAILED);
            }
        };
        let ours = entries.iter().any(|entry| {
            entry
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    DATA_FOLDER_SIGNS
                        .iter()
                        .any(|sign| sign.eq_ignore_ascii_case(name))
                })
        });
        if entries.is_empty() || ours {
            return Ok(());
        }
        let signs = DATA_FOLDER_SIGNS.join(", ");
        self.err(self.pick(
            format!(
                "cmdstudio: {} does not look like CMD Studio's data folder: it holds none of \
                 {signs}. Nothing was deleted; check the path after --data-dir.",
                data.display()
            ),
            format!(
                "cmdstudio: {} ไม่ใช่โฟลเดอร์ข้อมูลของ CMD Studio เพราะไม่มี {signs} อยู่เลย \
                 จึงยังไม่ได้ลบอะไร ให้ตรวจ path ที่ใส่หลัง --data-dir",
                data.display()
            ),
        ));
        Err(EXIT_INVALID)
    }

    fn cleanup(
        &mut self,
        sys: System<'_>,
        ctx: &Ctx,
        remove_themes: bool,
        data_dir_given: bool,
    ) -> i32 {
        if data_dir_given {
            if let Err(code) = self.check_data_dir(sys, ctx) {
                return code;
            }
        }
        // A rollback a crash left unfinished does not stop it: the clean-up checks what is left.
        if let Err(code) = self.begin(sys, ctx, false) {
            return code;
        }
        let report = match cleanup(sys, ctx, remove_themes) {
            Ok(report) => report,
            Err(e) => return self.refused(sys, ctx, &e),
        };
        self.start(&report.start);
        // The AutoRun and settings.json transaction has no step of its own; it committed only
        // after reading back what it wrote.
        if report.outcome == Some(Outcome::Committed) {
            self.out(self.pick(
                "done   AutoRun and Windows Terminal's settings: CMD Studio's part is taken out, \
                 and read back to check",
                "เสร็จ AutoRun และการตั้งค่าของ Windows Terminal: เอาส่วนของ CMD Studio ออกแล้ว \
                 และอ่านค่ากลับมาตรวจแล้ว",
            ));
        }
        self.steps(&report);
        match &report.outcome {
            Some(Outcome::RolledBack { code, reason }) => {
                self.err(self.coded(code, reason));
                self.err(self.pick(
                    "The revert was rolled back, so CMD Studio's data was not deleted.",
                    "การย้อนทุกอย่างถูกย้อนกลับ จึงยังไม่ได้ลบข้อมูลของ CMD Studio",
                ));
                EXIT_FAILED
            }
            Some(outcome @ Outcome::RollbackIncomplete { .. }) => {
                self.finish(sys, ctx, outcome, &[], String::new())
            }
            _ if !report.ok() => {
                self.err(self.pick(CLEANUP_AGAIN_EN, CLEANUP_AGAIN_TH));
                EXIT_INCOMPLETE
            }
            _ => {
                let themes = ctx.paths.themes();
                let kept = !remove_themes && sys.fs.is_dir(&themes);
                let shown = themes.display();
                self.out(match (kept, self.lang) {
                    (false, Lang::En) => {
                        "CMD Studio's changes are taken out, and its data is deleted.".to_string()
                    }
                    (false, Lang::Th) => {
                        "เอาสิ่งที่ CMD Studio เปลี่ยนออกแล้ว และลบข้อมูลของมันแล้ว".to_string()
                    }
                    (true, Lang::En) => format!(
                        "CMD Studio's changes are taken out, and its data is deleted; your \
                         themes are kept in {shown}."
                    ),
                    (true, Lang::Th) => format!(
                        "เอาสิ่งที่ CMD Studio เปลี่ยนออกแล้ว และลบข้อมูลของมันแล้ว ส่วนธีมของคุณยังเก็บไว้ที่ {shown}"
                    ),
                });
                EXIT_OK
            }
        }
    }

    /// One line per step of a clean-up: what went well on stdout, what failed on stderr with
    /// its code (the step's detail starts with it) and how to finish it by hand. Step texts are
    /// the core's English; the code's Thai meaning is in spec §5.9.
    fn steps(&mut self, report: &Report) {
        let (done, failed, fix) =
            self.pick(("done  ", "FAILED", "fix"), ("เสร็จ", "ไม่สำเร็จ", "วิธีแก้"));
        for step in &report.steps {
            if step.ok {
                self.out(format!("{done} {}: {}", step.what, step.detail));
                continue;
            }
            self.err(format!("{failed} {}: {}", step.what, step.detail));
            if let Some(how) = &step.fix {
                let how = if self.lang == Lang::Th && !how.th.is_empty() {
                    &how.th
                } else {
                    &how.en
                };
                self.err(format!("    {fix}: {how}"));
            }
        }
    }

    /// `cmdstudio set-aside <id>`: the way out when a restore point's journal can never be read
    /// again, so every change is refused (doctor's R1). Under the lock it sets aside only a
    /// restore point the listing has and that cannot be read; one that reads is refused, since
    /// CMD Studio can still roll it back or undo it.
    fn set_aside(&mut self, sys: System<'_>, ctx: &Ctx, id: &str) -> i32 {
        if let Err(code) = self.begin(sys, ctx, false) {
            return code;
        }
        let _lock = match sys.fs.try_lock(&ctx.paths.lock_file()) {
            Ok(Some(lock)) => lock,
            Ok(None) => return self.refused(sys, ctx, &ActionError::Busy),
            Err(e) => return self.refused(sys, ctx, &ActionError::Lock(e)),
        };
        let root = ctx.paths.snapshots();
        let store = SnapshotStore::new(sys.fs, &root);
        let ids = match store.ids() {
            Ok(ids) => ids,
            Err(f) => {
                self.err(self.coded(f.code, &f.message));
                return EXIT_FAILED;
            }
        };
        // Only a name the listing gives: the id typed is never made into a path of its own.
        if !ids.iter().any(|known| known == id) {
            self.err(self.pick(
                format!(
                    "cmdstudio: there is no restore point {id} in {}. `cmdstudio doctor` names \
                     the one to set aside.",
                    root.display()
                ),
                format!(
                    "cmdstudio: ไม่มีจุดย้อนกลับ {id} ใน {} ให้ดูรหัสที่ต้องกันไว้จาก `cmdstudio doctor`",
                    root.display()
                ),
            ));
            return EXIT_INVALID;
        }
        if store.is_set_aside(id) {
            self.out(self.pick(
                format!("Restore point {id} is already set aside; nothing was changed."),
                format!("จุดย้อนกลับ {id} ถูกกันไว้ต่างหากอยู่แล้ว จึงไม่ได้เปลี่ยนอะไร"),
            ));
            return EXIT_OK;
        }
        let why = match store.manifest(id).and_then(|_| store.journal(id)) {
            Ok(_) => {
                self.err(self.pick(
                    format!(
                        "Restore point {id} can be read, so it is not set aside: CMD Studio can \
                         still roll back or undo what it changed. Nothing was changed."
                    ),
                    format!(
                        "จุดย้อนกลับ {id} ยังอ่านได้ จึงไม่กันไว้ต่างหาก เพราะ CMD Studio \
                         ยังย้อนสิ่งที่มันเปลี่ยนได้ ยังไม่ได้เปลี่ยนอะไร"
                    ),
                ));
                return EXIT_FAILED;
            }
            Err(why) => why,
        };
        if let Err(f) = store.set_aside(id) {
            self.err(self.coded(f.code, &f.message));
            return EXIT_FAILED;
        }
        // The newest one is the transaction recovery would have rolled back. Like the start of
        // a command does for one it sets aside itself, its record keeps saying what to check by
        // hand, on every start and in doctor, until a newer transaction runs.
        let newest = ids.last().is_some_and(|last| last == id);
        if newest && store.set_aside_as(id) == Some(SetAside::WithJournal) {
            if let Err(f) = record_set_aside(&store, id, &why) {
                let text = self.note(&self.coded(f.code, &f.message));
                self.err(text);
            }
        }
        self.out(self.pick(
            format!(
                "Restore point {id} is set aside; its files stay for a look. What that change did \
                 is not rolled back by itself any more, and an undo does not go back to it. Check \
                 by hand: {CLEANUP_FIX_EN}"
            ),
            format!(
                "กันจุดย้อนกลับ {id} ไว้ต่างหากแล้ว ไฟล์ของมันยังเก็บไว้ให้ดู สิ่งที่การเปลี่ยนแปลงนั้นทำไว้ \
                 จะไม่ถูกย้อนเองอีก และการย้อนกลับจะไม่ย้อนไปถึงจุดนี้ ให้ตรวจด้วยมือ: {CLEANUP_FIX_TH}"
            ),
        ));
        EXIT_OK
    }

    fn doctor(&mut self, sys: System<'_>, ctx: &Ctx, json: bool) -> i32 {
        let checks = doctor(sys, ctx);
        if json {
            match serde_json::to_string_pretty(&serde_json::json!({ "checks": checks })) {
                Ok(text) => self.out(text),
                Err(e) => {
                    self.err(format!("cmdstudio: {e}"));
                    return EXIT_FAILED;
                }
            }
        } else {
            for c in &checks {
                let status = match c.status {
                    Status::Ok => self.pick("ok", "ผ่าน"),
                    Status::Warn => self.pick("warn", "เตือน"),
                    Status::Fail => self.pick("FAIL", "ไม่ผ่าน"),
                    Status::Skipped => self.pick("skip", "ข้าม"),
                };
                let message = self.pick(&c.message_en, &c.message_th);
                let code = c.code.map(|c| format!(" [{c}]")).unwrap_or_default();
                self.out(format!("{:<4} {status:<5} {message}{code}", c.id));
                for line in c.details.lines() {
                    self.out(format!("{:11}{line}", ""));
                }
            }
        }
        // An R entry that fails counts like a D check.
        if checks.iter().any(|c| c.status == Status::Fail) {
            EXIT_FAILED
        } else {
            EXIT_OK
        }
    }
}

/// The record of a restore point set aside by hand while its journal could not be read: one
/// step for the whole change, with the shared clean-up advice, as the core records one it sets
/// aside itself. A record that is already there, or cannot be read, is left as it is.
fn record_set_aside(store: &SnapshotStore<'_>, id: &str, why: &Failure) -> Result<(), Failure> {
    if !matches!(store.incomplete(id), Ok(None)) {
        return Ok(());
    }
    store.write_incomplete(&Incomplete {
        id: id.to_string(),
        code: why.code.to_string(),
        reason: format!(
            "restore point {id} was set aside by hand while its journal could not be read ({}), \
             so its change was not checked or rolled back",
            why.message
        ),
        steps: vec![Unrestored {
            op_index: 0,
            target: format!("every change of restore point {id}"),
            code: why.code.to_string(),
            message: why.message.clone(),
            fix: CLEANUP_FIX_EN.to_string(),
            fix_th: CLEANUP_FIX_TH.to_string(),
            restore_point: None,
        }],
    })
}

/// Milliseconds since 1970, for restore point ids.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// Runs one command on this machine (spec §5.7).
#[cfg(windows)]
pub fn run(args: &[OsString], io: &mut Io<'_>) -> i32 {
    use cmdstudio_core::adapters::program_data_dir;
    use cmdstudio_core::adapters::windows::{
        fonts::WinFonts, fs::WinFs, host::WinHost, process::WinProcess, registry::WinAutoRun,
        wt::WinWt,
    };
    let host = WinHost;
    let local = match host.local_app_data() {
        Ok(path) => path,
        Err(e) => {
            let mut say = Say::new(io, chosen_lang(&parse(args), &host));
            let text = say.coded(
                "E_MACHINE_READ",
                &format!("the user's local app data folder could not be found: {e}"),
            );
            say.err(text);
            return EXIT_FAILED;
        }
    };
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
        .unwrap_or_default();
    let defaults = Defaults {
        data_dir: local.join("ai.resincore.cmdstudio"),
        resources: exe_dir.join("resources"),
        now_ms: now_ms(),
    };
    // Only programs from the runtime folders may run (spec §5.10), each for 60 seconds at most.
    let data = parse(args)
        .ok()
        .and_then(|a| a.data_dir)
        .unwrap_or_else(|| defaults.data_dir.clone());
    let mut allowed = vec![data.join("runtime")];
    if let (Ok(pd), Ok(sid)) = (host.program_data(), host.user_sid()) {
        allowed.push(program_data_dir(&pd, &sid));
    }
    let (fs, autorun, fonts, wt) = (
        WinFs,
        WinAutoRun::real(),
        WinFonts::real(&local),
        WinWt::new(local.clone()),
    );
    let process = WinProcess::new(allowed, std::time::Duration::from_secs(60));
    let sys = System {
        fs: &fs,
        autorun: &autorun,
        fonts: &fonts,
        wt: &wt,
        process: &process,
        host: &host,
    };
    run_with(args, io, sys, &defaults)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmdstudio_core::adapters::fake::FakeMachine;
    use cmdstudio_core::adapters::{AutoRun, Fs, RegValue, WtLocation, WtVariant, REG_SZ};
    use std::path::Path;

    const WT: &str = r"C:\Users\tester\AppData\Local\Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json";
    const THEME: &str = r"C:\themes\terracotta.cmdtheme";

    fn machine() -> (FakeMachine, Defaults) {
        let mut m = FakeMachine::new();
        m.fs.create_dir_all(Path::new(
            r"C:\Users\tester\AppData\Local\Microsoft\Windows",
        ))
        .unwrap();
        m.fs.put(WT, "{\n    \"profiles\": { \"list\": [] }\n}\n");
        m.fs.put(
            THEME,
            include_bytes!("../../../presets/terracotta.cmdtheme"),
        );
        m.wt.locations = vec![WtLocation {
            variant: WtVariant::Store,
            installed: true,
            settings: PathBuf::from(WT),
        }];
        m.process.answer("oh-my-posh.exe", "print", 0, "> ");
        m.process.answer("oh-my-posh.exe", "version", 0, "31.5.0");
        let res = PathBuf::from(r"C:\Program Files\CMD Studio\resources");
        cmdstudio_core::resources::fake_resources(&m.fs, &res);
        let d = Defaults {
            data_dir: PathBuf::from(r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio"),
            resources: res,
            now_ms: 1_791_694_709_000,
        };
        (m, d)
    }

    fn cli(m: &FakeMachine, d: &Defaults, args: &[&str]) -> (i32, String, String) {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        let code = run_with(
            &args,
            &mut Io {
                out: &mut out,
                err: &mut err,
            },
            m.system(),
            d,
        );
        (
            code,
            String::from_utf8_lossy(&out).into_owned(),
            String::from_utf8_lossy(&err).into_owned(),
        )
    }

    fn later(d: &Defaults, ms: u64) -> Defaults {
        Defaults {
            now_ms: d.now_ms + ms,
            ..d.clone()
        }
    }

    #[test]
    fn apply_doctor_undo_round_trip() {
        let (m, d) = machine();
        let before = (m.fs.file(WT), m.autorun.snapshot());
        let (code, out, err) = cli(&m, &d, &["apply", THEME]);
        assert_eq!(code, EXIT_OK, "{out}{err}");
        assert!(out.contains("Plan for \"Terracotta\":\n  - "), "{out}");
        let (code, out, _) = cli(&m, &later(&d, 1), &["doctor", "--json"]);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["checks"].as_array().unwrap().len(), 11);
        assert!(v["checks"][0].get("messageTh").is_some());
        assert_eq!(code, EXIT_OK, "{out}");
        let (code, out, err) = cli(&m, &later(&d, 2), &["undo"]);
        assert_eq!(code, EXIT_OK, "{out}{err}");
        assert_eq!((m.fs.file(WT), m.autorun.snapshot()), before);
        assert_eq!(cli(&m, &later(&d, 3), &["undo"]).1, "Nothing to undo.\n");
    }

    #[test]
    fn invalid_input_exits_2() {
        let (m, d) = machine();
        assert_eq!(cli(&m, &d, &["apply"]).0, EXIT_INVALID);
        assert_eq!(cli(&m, &d, &["apply", r"C:\nope.cmdtheme"]).0, EXIT_INVALID);
        m.fs.put(THEME, b"not json");
        let (code, _, err) = cli(&m, &d, &["apply", THEME]);
        assert_eq!(code, EXIT_INVALID);
        assert!(err.starts_with("E_THEME_INVALID"), "{err}");
    }

    #[test]
    fn a_foreign_clink_needs_the_flag() {
        let (m, d) = machine();
        let other = r#""C:\clink\clink.bat" inject --autorun"#;
        m.autorun
            .write_user(&RegValue::from_text(REG_SZ, other))
            .unwrap();
        let before = m.autorun.snapshot();
        let (code, _, err) = cli(&m, &d, &["apply", THEME]);
        assert_eq!(code, EXIT_INVALID);
        assert!(err.starts_with("E_AUTORUN_FOREIGN_CLINK: "), "{err}");
        assert!(err.contains(&format!("\n  {other}\n")), "{err}");
        assert!(err.contains("--replace-other-clink"), "{err}");
        assert_eq!(m.autorun.snapshot(), before);
        assert_eq!(
            cli(&m, &d, &["apply", THEME, "--replace-other-clink"]).0,
            EXIT_OK
        );
    }

    #[test]
    fn an_autorun_cmd_rejects_needs_a_hand_edit_and_nothing_changes() {
        let (m, d) = machine();
        m.autorun
            .write_user(&RegValue::from_text(REG_SZ, "echo A |"))
            .unwrap();
        let before = (m.fs.file(WT), m.autorun.snapshot());
        let (code, _, err) = cli(&m, &d, &["apply", THEME]);
        assert_eq!(code, EXIT_FAILED, "{err}");
        assert!(err.starts_with("E_AUTORUN_WRITE: "), "{err}");
        assert!(err.contains("\n  echo A |\n"), "{err}");
        assert!(err.contains("Registry Editor"), "{err}");
        assert_eq!((m.fs.file(WT), m.autorun.snapshot()), before);
    }

    #[test]
    fn a_failed_verify_exits_1_and_a_failed_rollback_exits_3() {
        let (m, d) = machine();
        m.process
            .answer("oh-my-posh.exe", "print", 0, " CONFIG PARSE ERROR ");
        let (code, _, err) = cli(&m, &d, &["apply", THEME]);
        assert_eq!(code, EXIT_FAILED);
        assert!(err.starts_with("E_VERIFY_OMP"), "{err}");

        let (m, d) = machine();
        let init = d.data_dir.join(r"generated\init.lua");
        m.fs.put(&init, b"old");
        m.fs.fail_writes_to(&init);
        let (code, _, err) = cli(&m, &d, &["apply", THEME]);
        assert_eq!(code, EXIT_INCOMPLETE, "{err}");
        let target = format!(
            "could not put back: file {} (E_DATA_WRITE: ",
            init.display()
        );
        assert!(err.contains(&target), "{err}");
        let fix = format!(r"    fix: Copy {}\snapshots\", d.data_dir.display());
        assert!(err.contains(&fix), "{err}");
        assert!(
            err.contains(&format!(" over {}.\n", init.display())),
            "{err}"
        );
        // The next run, even in a new process, still says what to fix. Doctor says it in R3,
        // without the lock or a recovery, and writes nothing...
        let files = m.fs.snapshot();
        let (code, out, err) = cli(&m, &later(&d, 1), &["doctor"]);
        assert_eq!((code, err.as_str()), (EXIT_FAILED, ""), "{out}");
        assert!(out.contains("\nR3   FAIL  The last rollback"), "{out}");
        assert!(out.contains(&init.display().to_string()), "{out}");
        assert_eq!(m.fs.snapshot(), files);
        // ...and a command that changes the machine says it first, once, then refuses to undo
        // and says how to get out.
        let (code, out, err) = cli(&m, &later(&d, 2), &["undo"]);
        assert_eq!((code, out.as_str()), (EXIT_FAILED, ""), "{err}");
        assert!(
            err.starts_with("Note: the rollback of restore point "),
            "{err}"
        );
        assert_eq!(err.matches(&target).count(), 1, "{err}");
        assert!(
            err.contains("\nE_ROLLBACK_INCOMPLETE: the rollback of restore point "),
            "{err}"
        );
        assert!(err.contains("run `cmdstudio --cleanup`"), "{err}");
    }

    /// A restore point's manifest, for a transaction a test writes by hand.
    fn manifest(id: &str) -> String {
        format!(
            r#"{{"id": "{id}", "kind": "apply", "createdMs": 1, "theme": null, "target": "all",
                "summary": [], "undoes": null, "undoneBy": null}}"#
        )
    }

    #[test]
    fn every_command_first_rolls_back_a_run_a_crash_cut_off() {
        let (m, d) = machine();
        // What a crash leaves right after an apply started its journal.
        let point = d.data_dir.join(r"snapshots\0000000000001-aaaaaaaa");
        m.fs.put(
            point.join("manifest.json"),
            manifest("0000000000001-aaaaaaaa"),
        );
        m.fs.put(
            point.join("journal.json"),
            r#"{"id": "0000000000001-aaaaaaaa", "ops": [], "snapshots": [], "stage": {"state": "planned"}}"#,
        );
        // Doctor only reads: it reports the cut-off change (R1) and leaves it for the next
        // command that changes the machine.
        let files = m.fs.snapshot();
        let (code, out, err) = cli(&m, &d, &["doctor"]);
        assert_eq!((code, err.as_str()), (EXIT_OK, ""), "{out}");
        assert!(
            out.contains("\nR1   warn  An earlier change was cut off"),
            "{out}"
        );
        assert_eq!(m.fs.snapshot(), files);
        let (code, out, err) = cli(&m, &d, &["undo"]);
        assert_eq!(
            (code, out.as_str()),
            (EXIT_OK, "Nothing to undo.\n"),
            "{err}"
        );
        assert_eq!(
            err,
            "E_TXN_INTERRUPTED: the previous apply was interrupted before it changed anything; \
             the machine is as it was before it.\n"
        );
        // Once only.
        assert_eq!(cli(&m, &later(&d, 1), &["undo"]).2, "");
        assert!(!cli(&m, &later(&d, 2), &["doctor"]).1.contains("R1"));
    }

    #[test]
    fn another_run_holding_the_lock_stops_every_change_but_not_doctor() {
        let (m, d) = machine();
        assert_eq!(cli(&m, &d, &["apply", THEME]).0, EXIT_OK);
        let lock =
            m.fs.try_lock(&d.data_dir.join("cmdstudio.lock"))
                .unwrap()
                .unwrap();
        let before = (m.fs.snapshot(), m.autorun.snapshot());
        for args in [
            vec!["apply", THEME],
            vec!["undo"],
            vec!["--cleanup"],
            vec!["set-aside", "1791694709000-abcdef12"],
        ] {
            let (code, out, err) = cli(&m, &later(&d, 1), &args);
            assert_eq!(code, EXIT_FAILED, "{args:?}: {out}{err}");
            assert_eq!(
                err,
                format!("E_BUSY: {}\n", cmdstudio_core::actions::BUSY_EN),
                "{args:?}"
            );
            assert_eq!((m.fs.snapshot(), m.autorun.snapshot()), before, "{args:?}");
        }
        let (code, out, err) = cli(&m, &later(&d, 1), &["doctor"]);
        assert_eq!((code, err.as_str()), (EXIT_OK, ""), "{out}");
        assert!(out.contains("\nR1   warn  Another CMD Studio"), "{out}");
        drop(lock);
        assert_eq!(cli(&m, &later(&d, 2), &["undo"]).0, EXIT_OK);
    }

    #[test]
    fn thai_windows_gets_thai_and_lang_chooses_for_apply() {
        let (mut m, d) = machine();
        m.host.info.ui_language = 0x041E;
        let (code, out, _) = cli(&m, &d, &["--help"]);
        assert_eq!(code, EXIT_OK);
        assert!(out.starts_with("วิธีใช้:\n"), "{out}");
        let (code, _, err) = cli(&m, &d, &["frobnicate"]);
        assert_eq!(code, EXIT_INVALID);
        assert!(
            err.starts_with("cmdstudio: ไม่รู้จักคำสั่ง \"frobnicate\"\n\nวิธีใช้:"),
            "{err}"
        );
        let (code, out, err) = cli(&m, &d, &["apply", THEME]);
        assert_eq!(code, EXIT_OK, "{out}{err}");
        assert!(out.starts_with("แผนของธีม \"Terracotta\":\n  - "), "{out}");
        assert!(out.contains("\nใช้ธีม \"Terracotta\" แล้ว"), "{out}");
        let (code, out, _) = cli(&m, &later(&d, 1), &["doctor"]);
        assert_eq!(code, EXIT_OK, "{out}");
        assert!(out.contains("\nD10  ผ่าน  โปรแกรมทำงานแบบปกติ"), "{out}");
        // --lang picks the language of apply's messages too.
        let (code, out, _) = cli(&m, &later(&d, 2), &["apply", THEME, "--lang", "en"]);
        assert_eq!(code, EXIT_OK);
        assert!(out.starts_with("Plan for \"Terracotta\":\n"), "{out}");
    }

    #[test]
    fn what_to_fix_by_hand_comes_in_the_chosen_language() {
        let (m, d) = machine();
        let init = d.data_dir.join(r"generated\init.lua");
        m.fs.put(&init, b"old");
        m.fs.fail_writes_to(&init);
        let (code, _, err) = cli(&m, &d, &["apply", THEME, "--lang", "th"]);
        assert_eq!(code, EXIT_INCOMPLETE, "{err}");
        assert!(
            err.contains("\nE_ROLLBACK_INCOMPLETE: ย้อนกลับได้ไม่ครบ"),
            "{err}"
        );
        let fix = format!("    วิธีแก้: คัดลอก {}\\snapshots\\", d.data_dir.display());
        assert!(err.contains(&fix), "{err}");
        assert!(!err.contains("fix: Copy"), "{err}");
        // A code whose text comes from the core in English gets its Thai meaning first.
        assert!(
            err.starts_with("E_DATA_WRITE: เขียนลงโฟลเดอร์ข้อมูลของ CMD Studio ไม่ได้\n  รายละเอียด: "),
            "{err}"
        );
    }

    #[test]
    fn a_relative_folder_is_refused_before_anything_is_touched() {
        let (m, d) = machine();
        let before = m.fs.snapshot();
        for args in [
            vec!["doctor", "--data-dir", "data"],
            vec!["--cleanup", "--data-dir", r"..\x"],
            vec!["apply", THEME, "--resources", "res"],
        ] {
            let (code, out, err) = cli(&m, &d, &args);
            assert_eq!(code, EXIT_INVALID, "{args:?}: {out}");
            assert!(err.contains("needs a full path"), "{args:?}: {err}");
        }
        assert_eq!(m.fs.snapshot(), before);
    }

    #[test]
    fn missing_bundled_files_say_reinstall_and_keep_the_developer_hint_in_the_details() {
        let (m, d) = machine();
        let (code, _, err) = cli(&m, &d, &["apply", THEME, "--resources", r"C:\nowhere"]);
        assert_eq!(code, EXIT_FAILED, "{err}");
        let first = err.lines().next().unwrap();
        assert!(
            first.starts_with("E_RUNTIME_MISSING: ") && first.contains("Reinstall CMD Studio"),
            "{err}"
        );
        assert!(!first.contains("fetch-resources"), "{err}");
        assert!(
            err.lines()
                .any(|l| l.starts_with("  details: ") && l.contains("fetch-resources")),
            "{err}"
        );
    }

    #[test]
    fn cleanup_says_each_failed_step_with_its_code_and_a_second_run_finishes() {
        let (m, d) = machine();
        assert_eq!(cli(&m, &d, &["apply", THEME]).0, EXIT_OK);
        let generated = d.data_dir.join("generated");
        m.fs.fail_removes_of(&generated);
        let (code, out, err) = cli(&m, &later(&d, 1), &["--cleanup"]);
        assert_eq!(code, EXIT_INCOMPLETE, "{out}{err}");
        let failed = format!("FAILED Delete {}: E_DATA_WRITE: ", generated.display());
        assert!(err.contains(&failed), "{err}");
        assert!(err.contains("\n    fix: "), "{err}");
        assert!(err.contains("`cmdstudio --cleanup` again"), "{err}");
        assert!(
            out.starts_with("done   AutoRun and Windows Terminal's settings: ")
                && !out.contains("FAILED"),
            "{out}"
        );
        assert_eq!(m.autorun.read_user().unwrap(), None);
        m.fs.stop_failing(&generated);
        let (code, out, err) = cli(&m, &later(&d, 2), &["--cleanup"]);
        assert_eq!(code, EXIT_OK, "{out}{err}");
        assert!(!m.fs.exists(&d.data_dir));
    }

    #[test]
    fn a_cleanup_whose_revert_cannot_be_put_back_exits_3_and_deletes_nothing() {
        let (m, d) = machine();
        assert_eq!(cli(&m, &d, &["apply", THEME]).0, EXIT_OK);
        m.fs.fail_writes_to(WT);
        let (code, out, err) = cli(&m, &later(&d, 1), &["--cleanup"]);
        assert_eq!(code, EXIT_INCOMPLETE, "{out}{err}");
        assert!(
            err.contains(
                "\nE_ROLLBACK_INCOMPLETE: not everything could be put back. Fix these by hand:\n"
            ),
            "{err}"
        );
        assert!(
            err.contains(&format!("  could not put back: file {WT} (")),
            "{err}"
        );
        assert!(err.contains("\n    fix: "), "{err}");
        assert!(!out.contains("is deleted"), "{out}");
        assert!(m.fs.exists(&d.data_dir.join("state.json")));
        assert!(m.fs.exists(&d.data_dir.join("snapshots")));
    }

    #[test]
    fn cleanup_refuses_a_data_dir_that_is_not_cmd_studios() {
        let (m, d) = machine();
        let docs = r"C:\Users\tester\Documents";
        m.fs.put(format!(r"{docs}\notes.txt"), b"mine");
        let before = m.fs.snapshot();
        let (code, out, err) = cli(&m, &d, &["--cleanup", "--data-dir", docs]);
        assert_eq!(code, EXIT_INVALID, "{out}{err}");
        assert!(
            err.contains("does not look like CMD Studio's data folder"),
            "{err}"
        );
        assert_eq!(m.fs.snapshot(), before);
        // A folder that is absent, empty, or holds CMD Studio's files is cleaned up.
        let gone = r"C:\Users\tester\gone";
        assert_eq!(cli(&m, &d, &["--cleanup", "--data-dir", gone]).0, EXIT_OK);
        let other = r"C:\Users\tester\other";
        assert_eq!(
            cli(&m, &d, &["apply", THEME, "--data-dir", other]).0,
            EXIT_OK
        );
        let (code, out, err) = cli(&m, &later(&d, 1), &["--cleanup", "--data-dir", other]);
        assert_eq!(code, EXIT_OK, "{out}{err}");
        assert!(m.fs.file(format!(r"{docs}\notes.txt")).is_some());
    }

    #[test]
    fn a_restore_point_that_cannot_be_read_is_reported_once_per_command() {
        let (m, d) = machine();
        assert_eq!(cli(&m, &d, &["apply", THEME]).0, EXIT_OK);
        assert_eq!(cli(&m, &later(&d, 1), &["apply", THEME]).0, EXIT_OK);
        let snapshots = d.data_dir.join("snapshots");
        let first = m.fs.list_dir(&snapshots).unwrap()[0].path.clone();
        let id = first.file_name().unwrap().to_str().unwrap().to_string();
        m.fs.fail_reads_of(first.join("journal.json"));
        // Both the start of the CLI and the start of the apply find it; it is said once.
        let (code, out, err) = cli(&m, &later(&d, 2), &["apply", THEME]);
        assert_eq!(code, EXIT_OK, "{out}{err}");
        assert_eq!(err.lines().filter(|l| l.contains(&id)).count(), 1, "{err}");
        assert!(
            err.starts_with(&format!(
                "Note: E_SNAPSHOT_READ: restore point {id} can't be read right now ("
            )),
            "{err}"
        );
        // A damaged one is set aside, and said once too.
        m.fs.stop_failing(first.join("journal.json"));
        m.fs.put(first.join("manifest.json"), b"{ damaged");
        let (code, out, err) = cli(&m, &later(&d, 3), &["undo"]);
        assert_eq!(code, EXIT_OK, "{out}{err}");
        assert_eq!(err.lines().filter(|l| l.contains(&id)).count(), 1, "{err}");
        assert!(err.contains("It was set aside"), "{err}");
    }

    #[test]
    fn a_record_that_cannot_be_read_is_said_once_and_the_apply_goes_on() {
        let (m, d) = machine();
        let init = d.data_dir.join(r"generated\init.lua");
        m.fs.put(&init, b"old");
        m.fs.fail_writes_to(&init);
        assert_eq!(cli(&m, &d, &["apply", THEME]).0, EXIT_INCOMPLETE);
        m.fs.stop_failing(&init);
        let point = m.fs.list_dir(&d.data_dir.join("snapshots")).unwrap()[0]
            .path
            .clone();
        m.fs.put(point.join("incomplete.json"), b"{ damaged");
        let (code, out, err) = cli(&m, &later(&d, 1), &["apply", THEME]);
        assert_eq!(code, EXIT_OK, "{out}{err}");
        assert!(err.starts_with("Note: E_SNAPSHOT_READ: "), "{err}");
        assert_eq!(
            err.matches("incomplete.json is damaged").count(),
            1,
            "{err}"
        );
    }

    /// A transaction a crash cut off whose journal can't be read now, newer than any restore
    /// point the tests' commands make before it. Returns its id.
    fn crashed_and_unreadable(m: &FakeMachine, d: &Defaults) -> String {
        let id = "1791694709500-bbbbbbbb";
        let dir = d.data_dir.join("snapshots").join(id);
        m.fs.put(dir.join("manifest.json"), manifest(id));
        m.fs.put(
            dir.join("journal.json"),
            format!(
                r#"{{"id": "{id}", "ops": [], "snapshots": [], "stage": {{"state": "planned"}}}}"#
            ),
        );
        m.fs.fail_reads_of(dir.join("journal.json"));
        id.to_string()
    }

    #[test]
    fn a_journal_that_cannot_be_read_stops_every_change_and_set_aside_is_the_way_out() {
        let (m, d) = machine();
        assert_eq!(cli(&m, &d, &["apply", THEME]).0, EXIT_OK);
        let snapshots = d.data_dir.join("snapshots");
        let applied = m.fs.list_dir(&snapshots).unwrap()[0].path.clone();
        let applied = applied.file_name().unwrap().to_str().unwrap().to_string();
        let id = crashed_and_unreadable(&m, &d);
        let way_out = format!("`cmdstudio set-aside {id}`");
        let before = (m.fs.snapshot(), m.autorun.snapshot());
        // Doctor is not blocked: it reports it (R1), takes no lock and changes nothing.
        let (code, out, err) = cli(&m, &later(&d, 1), &["doctor"]);
        assert_eq!((code, err.as_str()), (EXIT_FAILED, ""), "{out}");
        assert!(
            out.contains("\nR1   FAIL  ") && out.contains(&way_out),
            "{out}"
        );
        let (code, out, _) = cli(&m, &later(&d, 1), &["doctor", "--json"]);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        let checks = v["checks"].as_array().unwrap();
        assert_eq!((code, checks.len()), (EXIT_FAILED, 12), "{out}");
        assert_eq!(checks[11]["id"], "R1");
        assert_eq!(checks[11]["code"], "E_SNAPSHOT_READ");
        assert!(checks[11]["messageTh"].as_str().unwrap().contains(&way_out));
        // Every command that changes the machine refuses, changes nothing and names the way out.
        for args in [vec!["apply", THEME], vec!["undo"], vec!["--cleanup"]] {
            let (code, out, err) = cli(&m, &later(&d, 1), &args);
            assert_eq!(code, EXIT_FAILED, "{args:?}: {out}{err}");
            assert!(
                err.contains("\nE_SNAPSHOT_READ: An earlier change could not be checked yet"),
                "{args:?}: {err}"
            );
            assert!(err.contains(&way_out), "{args:?}: {err}");
            assert_eq!((m.fs.snapshot(), m.autorun.snapshot()), before, "{args:?}");
        }
        // set-aside refuses a restore point that reads, and one that is not there.
        let (code, out, err) = cli(&m, &later(&d, 1), &["set-aside", &applied]);
        assert_eq!(code, EXIT_FAILED, "{out}{err}");
        assert!(err.contains("can be read, so it is not set aside"), "{err}");
        for missing in ["1791694709999-nothere0", r"..\..\x"] {
            let (code, _, err) = cli(&m, &later(&d, 1), &["set-aside", missing]);
            assert_eq!(code, EXIT_INVALID, "{missing}: {err}");
            assert!(err.contains("there is no restore point"), "{err}");
        }
        assert_eq!((m.fs.snapshot(), m.autorun.snapshot()), before);
        // It sets aside the one that can't be read, and says what that means.
        let (code, out, err) = cli(&m, &later(&d, 2), &["set-aside", &id]);
        assert_eq!(code, EXIT_OK, "{out}{err}");
        assert!(out.contains("is not rolled back by itself"), "{out}");
        assert!(out.contains("`cmdstudio --cleanup`"), "{out}");
        assert!(m
            .fs
            .exists(&snapshots.join(&id).join("journal.unreadable.json")));
        let (code, out, _) = cli(&m, &later(&d, 3), &["set-aside", &id]);
        assert_eq!(code, EXIT_OK);
        assert!(out.contains("already set aside"), "{out}");
        // From now on, what to check by hand is said: by doctor, and before every command.
        let (code, out, _) = cli(&m, &later(&d, 4), &["doctor"]);
        assert_eq!(code, EXIT_FAILED, "{out}");
        assert!(
            !out.contains("\nR1 ") && out.contains("\nR3   FAIL "),
            "{out}"
        );
        let (code, out, err) = cli(&m, &later(&d, 5), &["apply", THEME]);
        assert_eq!(code, EXIT_OK, "{out}{err}");
        assert!(
            err.starts_with(&format!(
                "Note: the rollback of restore point {id} did not finish."
            )),
            "{err}"
        );
        assert!(err.contains("cmdstudio --cleanup"), "{err}");
    }

    #[test]
    fn every_code_of_the_spec_has_a_thai_meaning() {
        let spec = include_str!("../../../docs/superpowers/specs/2026-10-10-cmd-studio-design.md");
        let codes: Vec<String> = spec
            .lines()
            .filter_map(|l| l.strip_prefix("| `E_"))
            .filter_map(|rest| rest.split('`').next())
            .map(|name| format!("E_{name}"))
            .collect();
        assert_eq!(codes.len(), 25, "{codes:?}");
        for code in codes {
            assert!(meaning_th(&code).is_some(), "{code}");
        }
    }

    #[test]
    fn cleanup_and_usage() {
        let (m, d) = machine();
        assert_eq!(cli(&m, &d, &["apply", THEME]).0, EXIT_OK);
        let (code, out, _) = cli(&m, &later(&d, 1), &["--cleanup"]);
        assert_eq!(code, EXIT_OK, "{out}");
        assert_eq!(m.autorun.read_user().unwrap(), None);
        let (code, out, _) = cli(&m, &d, &["--help"]);
        assert_eq!(code, EXIT_OK);
        assert!(out.starts_with("Usage:"));
        assert_eq!(cli(&m, &d, &["frobnicate"]).0, EXIT_INVALID);
    }
}
