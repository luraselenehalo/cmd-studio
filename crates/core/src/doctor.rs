//! Health checks D1–D11 (spec §5.8), then what doctor finds wrong with CMD Studio's own restore
//! points (R1–R3). Read only: doctor writes nothing, takes no lock and runs no recovery, so it
//! is never blocked by another run, by a restore point or by a state.json it cannot read; it
//! says what it found and goes on. A check says `ok` only after it really looked (spec §2.1
//! COE). What it could not look at is `skipped` with the reason, or `fail` with the read error
//! (`E_MACHINE_READ`): a read that failed is never taken for the problem a check looks for.
use crate::actions::{Ctx, BUSY_EN, BUSY_TH};
use crate::adapters::{DefaultTerminal, System};
use crate::exec::{CLEANUP_FIX_EN, CLEANUP_FIX_TH, COMMAND_PROCESSOR_KEY};
use crate::generate::wt::{commandline, CMD_PROFILE_GUID, SCHEME_NAME};
use crate::model::Target;
use crate::plan::autorun::{is_foreign, is_our_entry, runs_clink, scan};
use crate::plan::{autorun_entry, Component, OMP_VERSION};
use crate::resources::{check_bundle, sha256_hex, ResourceError};
use crate::snapshot::{SnapshotStore, UnreadableKind};
use crate::state::{Applied, State, StateError};
use crate::txn::{Failure, JournalStore};
use crate::verify::lua_error_line;
use crate::wt_edit::{check_parses, read_value, WtError, WtKey};
use serde::Serialize;
use serde_json::Value;
use std::ffi::OsStr;
use std::fmt::Display;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Ok,
    Warn,
    Fail,
    Skipped,
}

/// One check's result (spec §5.9: a stable code, Thai and English text, details).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub id: &'static str,
    pub status: Status,
    pub code: Option<&'static str>,
    pub message_th: String,
    pub message_en: String,
    pub details: String,
}

fn check(
    id: &'static str,
    status: Status,
    code: Option<&'static str>,
    th: impl Into<String>,
    en: impl Into<String>,
    details: impl Into<String>,
) -> Check {
    Check {
        id,
        status,
        code,
        message_th: th.into(),
        message_en: en.into(),
        details: details.into(),
    }
}

/// A check that could not read what it looks at: it says so, and what failed, with
/// `E_MACHINE_READ`. Never `ok`, and never the problem the check looks for.
fn unreadable(id: &'static str, what_th: &str, what_en: &str, error: impl Display) -> Check {
    // Thai runs its words together, but not into an English name or a closing parenthesis.
    let gap = if what_th.ends_with(|c: char| c.is_ascii()) {
        " "
    } else {
        ""
    };
    check(
        id,
        Status::Fail,
        Some("E_MACHINE_READ"),
        format!("อ่าน{what_th}{gap}ไม่ได้ จึงตรวจข้อนี้ไม่ได้ ลองตรวจอีกครั้งในอีกสักครู่"),
        format!("Could not read {what_en}, so this was not checked. Check again in a moment"),
        error.to_string(),
    )
}

/// The theme the checks D1–D4, D7 and D8 look at, or the `skipped` check that says why there
/// is none: no theme was applied, or state.json cannot be read (D9 reports that).
fn applied<'a>(
    id: &'static str,
    state: &'a Result<State, StateError>,
) -> Result<&'a Applied, Check> {
    match state {
        Ok(State {
            applied: Some(a), ..
        }) => Ok(a),
        Ok(_) => Err(check(
            id,
            Status::Skipped,
            None,
            "ยังไม่ได้ใช้ธีม",
            "No theme has been applied yet",
            "",
        )),
        Err(e) => Err(check(
            id,
            Status::Skipped,
            None,
            "ตรวจข้อนี้ไม่ได้ เพราะอ่านบันทึกของ CMD Studio (state.json) ไม่ได้ ดู D9",
            "Not checked: CMD Studio's record (state.json) can't be read (see D9)",
            e.to_string(),
        )),
    }
}

/// What `clink.log` last says about starting Clink in a cmd window (G19).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InjectSeen {
    Never,
    Ok,
    Failed(String),
}

pub fn last_inject(log: &str) -> InjectSeen {
    let mut seen = InjectSeen::Never;
    for line in log.lines() {
        if line.contains("Inject completed") {
            seen = InjectSeen::Ok;
        } else if line.contains("Unable to get DLL version") || line.contains("Failed to copy DLL")
        {
            seen = InjectSeen::Failed(line.trim().to_string());
        }
    }
    seen
}

/// Runs D1–D11, always all eleven and in this order, then adds one entry for each problem it
/// finds with CMD Studio's restore points (R1–R3, see `records`); none when there is none.
/// Read only.
pub fn doctor(sys: System<'_>, ctx: &Ctx) -> Vec<Check> {
    let state = State::load(sys.fs, &ctx.paths.state_file());
    let mut checks = vec![
        d1(sys, ctx, &state),
        d2(sys, ctx, &state),
        d3(sys, ctx, &state),
        d4(sys, ctx, &state),
        d5(sys, ctx),
        d6(sys),
        d7(sys, ctx, &state),
        d8(sys, &state),
        d9(sys, &state),
        d10(sys),
        d11(sys),
    ];
    checks.extend(records(sys, ctx));
    checks
}

/// What to do when a bundled program does not run: D1 says what is wrong with its files.
const SEE_D1_TH: &str = "ถ้า D1 ไม่ผ่านด้วย ให้แก้ตาม D1 ก่อน แล้วตรวจอีกครั้ง";
const SEE_D1_EN: &str = "If D1 fails too, do what it says first, then check again";

fn d1(sys: System<'_>, ctx: &Ctx, state: &Result<State, StateError>) -> Check {
    let a = match applied("D1", state) {
        Ok(a) => a,
        Err(skipped) => return skipped,
    };
    let m =
        match ctx.resources.manifest(sys.fs) {
            Ok(m) => m,
            Err(ResourceError::Io(e)) => {
                return unreadable(
                    "D1",
                    "รายการไฟล์ที่แนบมากับ CMD Studio",
                    "the list of files that come with CMD Studio",
                    e,
                )
            }
            // The app's own install is broken: only a new install brings the files back.
            Err(e) => return check(
                "D1",
                Status::Fail,
                Some(e.code()),
                "ไฟล์ที่แนบมากับ CMD Studio หายหรือเสีย ให้ติดตั้ง CMD Studio ใหม่",
                "The files that come with CMD Studio are missing or damaged. Reinstall CMD Studio",
                e.to_string(),
            ),
        };
    let launch = a.launch.to_clink();
    for (component, dir) in [
        (Component::Omp, ctx.paths.omp_dir()),
        (Component::Clink, launch.runtime_dir(&ctx.paths)),
    ] {
        // An apply installs a runtime folder that is gone, and never overwrites one that is
        // there (spec §5.4), so a changed folder must be deleted first.
        if !sys.fs.exists(&dir) {
            return check(
                "D1",
                Status::Fail,
                Some("E_RUNTIME_MISSING"),
                "โฟลเดอร์ส่วนเสริมหายไป ให้ใช้ธีมซ้ำ",
                "An add-on folder is gone. Apply the theme again",
                dir.display().to_string(),
            );
        }
        match check_bundle(sys.fs, &dir, m.bundle(component)) {
            Ok(()) => {}
            Err(ResourceError::Io(e)) => {
                return unreadable("D1", "ไฟล์ส่วนเสริม", "the add-on files", e)
            }
            Err(e) => {
                let dir = dir.display();
                return check(
                    "D1",
                    Status::Fail,
                    Some(e.code()),
                    format!(
                        "ไฟล์ส่วนเสริมหายหรือถูกแก้ ให้ปิดหน้าต่าง cmd และ Windows Terminal \
                         ทุกหน้าต่าง ลบโฟลเดอร์ {dir} แล้วใช้ธีมซ้ำ"
                    ),
                    format!(
                        "Add-on files are missing or changed. Close every cmd window and Windows \
                         Terminal, delete the folder {dir}, then apply the theme again"
                    ),
                    e.to_string(),
                );
            }
        }
    }
    check(
        "D1",
        Status::Ok,
        None,
        "ไฟล์ส่วนเสริมอยู่ครบและตรงกับที่แนบมา",
        "The add-on files are all there and unchanged",
        "",
    )
}

fn d2(sys: System<'_>, ctx: &Ctx, state: &Result<State, StateError>) -> Check {
    if let Err(skipped) = applied("D2", state) {
        return skipped;
    }
    match sys
        .process
        .run(&ctx.paths.omp_exe(), &[OsStr::new("version")])
    {
        Ok(out) if out.status == 0 && out.stdout.trim() == OMP_VERSION => check(
            "D2",
            Status::Ok,
            None,
            "ตัวสร้าง prompt ทำงานได้",
            "The prompt add-on runs",
            format!("oh-my-posh version: {OMP_VERSION}"),
        ),
        Ok(out) => check(
            "D2",
            Status::Fail,
            Some("E_RUNTIME_MISSING"),
            format!("ตัวสร้าง prompt ไม่ใช่รุ่นที่แนบมา {SEE_D1_TH}"),
            format!("The prompt add-on is not the bundled version. {SEE_D1_EN}"),
            format!(
                "oh-my-posh version printed {:?} (exit {})",
                clip(&out.stdout),
                out.status
            ),
        ),
        Err(e) => check(
            "D2",
            Status::Fail,
            Some("E_RUNTIME_MISSING"),
            format!("ตัวสร้าง prompt ทำงานไม่ได้ {SEE_D1_TH}"),
            format!("The prompt add-on does not run. {SEE_D1_EN}"),
            e.to_string(),
        ),
    }
}

/// The start of a program's output, for details: a few lines at most.
fn clip(text: &str) -> String {
    const MAX: usize = 300;
    let text = text.trim();
    let mut clipped: String = text.chars().take(MAX).collect();
    if text.chars().nth(MAX).is_some() {
        clipped.push_str("...");
    }
    clipped
}

fn d3(sys: System<'_>, ctx: &Ctx, state: &Result<State, StateError>) -> Check {
    let a = match applied("D3", state) {
        Ok(a) => a,
        Err(skipped) => return skipped,
    };
    let profile = ctx.paths.clink_profile();
    let launch = a.launch.to_clink();
    let args = [
        OsStr::new("--profile"),
        profile.as_os_str(),
        OsStr::new("set"),
        OsStr::new("clink.logo"),
    ];
    let out = match sys.process.run(&launch.exe(), &args) {
        Ok(out) => out,
        Err(e) => {
            return check(
                "D3",
                Status::Fail,
                Some("E_RUNTIME_MISSING"),
                format!("ส่วนเสริมของ cmd ทำงานไม่ได้ {SEE_D1_TH}"),
                format!("The cmd add-on does not run. {SEE_D1_EN}"),
                e.to_string(),
            )
        }
    };
    if let Some(line) = lua_error_line(&format!("{}\n{}", out.stdout, out.stderr)) {
        return check(
            "D3",
            Status::Fail,
            Some("E_VERIFY_CLINK"),
            "ส่วนเสริมของ cmd โหลดสคริปต์ของธีมไม่ได้ ให้ใช้ธีมซ้ำ",
            "The cmd add-on can't load the theme's scripts. Apply the theme again",
            clip(line),
        );
    }
    // `set` always exits 0 (G5): any other status means Clink did not run as it should, as the
    // verifier also says.
    if out.status != 0 {
        let said = if out.stderr.trim().is_empty() {
            &out.stdout
        } else {
            &out.stderr
        };
        return check(
            "D3",
            Status::Fail,
            Some("E_VERIFY_CLINK"),
            "ส่วนเสริมของ cmd ทำงานผิดปกติ ให้ใช้ธีมซ้ำ",
            "The cmd add-on did not run as it should. Apply the theme again",
            format!("clink exited {}: {}", out.status, clip(said)),
        );
    }
    let log = match sys.fs.read(&profile.join("clink.log")) {
        Ok(log) => log.unwrap_or_default(),
        Err(e) => {
            return unreadable(
                "D3",
                "ไฟล์บันทึก clink.log ของส่วนเสริม cmd",
                "the cmd add-on's log (clink.log)",
                e,
            )
        }
    };
    let key = COMMAND_PROCESSOR_KEY;
    match last_inject(&log_since(&log, a.clink_log_len)) {
        InjectSeen::Ok => check(
            "D3",
            Status::Ok,
            None,
            "ส่วนเสริมของ cmd โหลดสคริปต์ของธีมได้ และเปิดใน cmd ได้จริง",
            "The cmd add-on loads the theme's scripts and starts in cmd",
            "",
        ),
        InjectSeen::Failed(line) => check(
            "D3",
            Status::Fail,
            Some("E_CLINK_PATH"),
            "ส่วนเสริมของ cmd เปิดจากโฟลเดอร์ของมันไม่ได้ cmd จึงขึ้นข้อความผิดพลาด",
            "The cmd add-on can't start from its folder, so cmd shows an error",
            line,
        ),
        // Ruling P68: a value cmd refuses whole can pass the planner's reading, and then
        // nothing of AutoRun runs. Only a real cmd window shows that, so it is said here.
        InjectSeen::Never if a.target == Target::All => check(
            "D3",
            Status::Warn,
            Some("E_THEME_NOT_LOADED"),
            format!(
                "ยังไม่มีหน้าต่าง cmd ไหนโหลดธีมตั้งแต่ใช้ธีม ให้เปิดหน้าต่าง cmd ใหม่แล้วตรวจอีกครั้ง \
                 ถ้ายังขึ้นแบบนี้ แปลว่า cmd หยุดก่อนถึงส่วนของ CMD Studio ในการตั้งค่าเริ่มต้นของ cmd \
                 ให้แก้ด้วยมือ: เปิด Registry Editor (regedit) ไปที่ {key} แก้ค่า AutoRun ให้ cmd \
                 รันได้ แล้วใช้ธีมซ้ำ"
            ),
            format!(
                "No cmd window has loaded the theme since it was applied. Open a new cmd window, \
                 then check again. If this stays, cmd stops before it reaches CMD Studio's part \
                 of its start-up setting. Fix that by hand: in Registry Editor, open {key} and \
                 correct the AutoRun value so that cmd can run it, then apply the theme again"
            ),
            "clink.log has no inject record since the theme was applied",
        ),
        InjectSeen::Never => check(
            "D3",
            Status::Warn,
            Some("E_THEME_NOT_LOADED"),
            "ยังไม่มีแท็บ cmd ใน Windows Terminal แท็บไหนโหลดธีมตั้งแต่ใช้ธีม \
             ให้เปิดแท็บ cmd ใหม่ใน Windows Terminal แล้วตรวจอีกครั้ง ถ้ายังขึ้นแบบนี้ ให้ใช้ธีมซ้ำ",
            "No cmd tab in Windows Terminal has loaded the theme since it was applied. Open a new \
             cmd tab there, then check again. If this stays, apply the theme again",
            "clink.log has no inject record since the theme was applied",
        ),
    }
}

/// What Clink wrote to `clink.log` after it held `len_at_apply` bytes: the record of every cmd
/// window started since the theme was applied. A log that is shorter now was started over, so
/// all of it is new.
fn log_since(log: &[u8], len_at_apply: u64) -> String {
    let start = usize::try_from(len_at_apply).unwrap_or(usize::MAX);
    String::from_utf8_lossy(log.get(start..).unwrap_or(log)).into_owned()
}

fn d4(sys: System<'_>, ctx: &Ctx, state: &Result<State, StateError>) -> Check {
    let a = match applied("D4", state) {
        Ok(a) => a,
        Err(skipped) => return skipped,
    };
    if a.target == Target::Wt {
        return check(
            "D4",
            Status::Skipped,
            None,
            "ใช้ธีมเฉพาะใน Windows Terminal",
            "The theme is set for Windows Terminal only",
            "",
        );
    }
    let launch = a.launch.to_clink();
    let entry = match autorun_entry(&launch, &ctx.paths) {
        Ok(e) => e,
        Err(e) => {
            return check(
                "D4",
                Status::Fail,
                Some(e.code()),
                "ที่อยู่ของส่วนเสริมใช้กับ cmd ไม่ได้",
                "The add-on's path can't be used by cmd",
                e.to_string(),
            )
        }
    };
    // A value that is not text (not REG_SZ or REG_EXPAND_SZ) runs nothing, as no value does.
    let current = match sys.autorun.read_user() {
        Ok(value) => value.and_then(|v| v.text()).unwrap_or_default(),
        Err(e) => return unreadable("D4", "ค่า AutoRun ของ cmd", "cmd's AutoRun value", e),
    };
    let key = COMMAND_PROCESSOR_KEY;
    let read = scan(&current);
    if read.rejected {
        return check(
            "D4",
            Status::Fail,
            Some("E_AUTORUN_WRITE"),
            format!(
                "หน้าต่าง cmd ใหม่จะไม่ได้ธีม เพราะ cmd รันการตั้งค่าเริ่มต้นนี้ไม่ได้เลย ให้แก้ด้วยมือ: \
                 เปิด Registry Editor (regedit) ไปที่ {key} แก้ค่า AutoRun ให้ cmd รันได้ แล้วใช้ธีมซ้ำ"
            ),
            format!(
                "New cmd windows won't get the theme: cmd can't run this start-up setting at all. \
                 Fix it by hand: in Registry Editor, open {key} and correct the AutoRun value so \
                 that cmd can run it, then apply the theme again"
            ),
            current,
        );
    }
    let profile = ctx.paths.clink_profile();
    let profile = profile.to_str().unwrap_or_default();
    if !read.parts.iter().any(|p| is_our_entry(&p.text, profile)) {
        return check(
            "D4",
            Status::Fail,
            Some("E_AUTORUN_WRITE"),
            "หน้าต่าง cmd ใหม่จะไม่ได้ธีม เพราะการตั้งค่าที่เปิดธีมหายไป ให้ใช้ธีมซ้ำ",
            "New cmd windows won't get the theme: the setting that loads it is gone. Apply the \
             theme again",
            current,
        );
    }
    // Ours goes first, in parentheses (spec §5.4): anywhere else another program's `if` can
    // keep it from running.
    if !read
        .parts
        .first()
        .is_some_and(|p| p.text.eq_ignore_ascii_case(&entry))
    {
        return check(
            "D4",
            Status::Fail,
            Some("E_AUTORUN_WRITE"),
            "การตั้งค่าที่เปิดธีมไม่ได้อยู่หน้าสุด หรือเป็นของรุ่นเก่า ให้ใช้ธีมซ้ำ",
            "The setting that loads the theme is not first, or is an older one. Apply the theme \
             again",
            current,
        );
    }
    if !sys.fs.exists(&launch.exe()) {
        return check(
            "D4",
            Status::Fail,
            Some("E_RUNTIME_MISSING"),
            "การตั้งค่าชี้ไปที่ส่วนเสริมที่ไม่มีอยู่แล้ว ให้ใช้ธีมซ้ำ",
            "The setting points to an add-on that is gone. Apply the theme again",
            launch.exe().display().to_string(),
        );
    }
    check(
        "D4",
        Status::Ok,
        None,
        "การตั้งค่าที่เปิดธีมอยู่หน้าสุด และชี้ไปที่ส่วนเสริมที่มีอยู่จริง",
        "The setting that loads the theme comes first and points to an add-on that is there",
        "",
    )
}

/// D5 reads AutoRun as the planner does (`plan::autorun`): in the user's value, the commands it
/// would ask the user about turning off (`is_foreign`: they inject Clink and are not ours); in
/// the machine-wide one, every command that injects Clink, as the planner's warning counts
/// them. A command that only mentions Clink is another program's own.
fn d5(sys: System<'_>, ctx: &Ctx) -> Check {
    let text = |v: Option<crate::adapters::RegValue>| v.and_then(|v| v.text()).unwrap_or_default();
    let (user, machine) = match (sys.autorun.read_user(), sys.autorun.read_machine()) {
        (Ok(user), Ok(machine)) => (text(user), text(machine)),
        (Err(e), _) | (_, Err(e)) => {
            return unreadable("D5", "ค่า AutoRun ของ cmd", "cmd's AutoRun values", e)
        }
    };
    let profile = ctx.paths.clink_profile();
    // The planner refuses a profile path that is not Unicode, so no entry of ours holds one.
    let other_clink = |command: &str| match profile.to_str() {
        Some(profile) => is_foreign(command, profile),
        None => runs_clink(command),
    };
    let in_user: Vec<String> = scan(&user)
        .parts
        .into_iter()
        .filter(|p| other_clink(&p.text))
        .map(|p| format!("HKCU: {}", p.text))
        .collect();
    let in_machine: Vec<String> = scan(&machine)
        .parts
        .into_iter()
        .filter(|p| runs_clink(&p.text))
        .map(|p| format!("HKLM: {}", p.text))
        .collect();
    let details = in_user
        .iter()
        .chain(&in_machine)
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    if !in_user.is_empty() {
        return check(
            "D5",
            Status::Warn,
            Some("E_AUTORUN_FOREIGN_CLINK"),
            "มีส่วนเสริมของ cmd ตัวอื่นตั้งไว้ prompt ของธีมอาจไม่ขึ้น \
             ให้ใช้ธีมซ้ำแล้วเลือก \"ใช้ของ CMD Studio แทน\"",
            "Another cmd add-on is set up; the theme's prompt may not show. Apply the theme again \
             and choose to use CMD Studio's instead",
            details,
        );
    }
    if !in_machine.is_empty() {
        return check(
            "D5",
            Status::Warn,
            Some("E_AUTORUN_FOREIGN_CLINK"),
            "มีส่วนเสริมของ cmd ที่ติดตั้งไว้ทั้งเครื่อง prompt ของธีมอาจไม่ทำงาน \
             แต่สีใน Windows Terminal ยังใช้ได้",
            "A cmd add-on is set up for the whole machine; the theme's prompt may not work, but \
             the colors in Windows Terminal still do",
            details,
        );
    }
    check(
        "D5",
        Status::Ok,
        None,
        "ไม่มีส่วนเสริมของ cmd ตัวอื่น",
        "No other cmd add-on is set up",
        "",
    )
}

/// The text of a Windows Terminal settings file for check `id`; `Ok(None)` when it is not there.
fn settings_text(id: &'static str, sys: System<'_>, path: &Path) -> Result<Option<String>, Check> {
    match sys.fs.read(path) {
        Ok(None) => Ok(None),
        Ok(Some(bytes)) => String::from_utf8(bytes)
            .map(Some)
            .map_err(|e| wt_parse(id, path, &WtError::Parse(e.to_string()))),
        Err(e) => Err(unreadable(
            id,
            "การตั้งค่าของ Windows Terminal",
            "Windows Terminal's settings",
            e,
        )),
    }
}

/// A settings file CMD Studio can't work with (`E_WT_PARSE`): text Windows Terminal can't read,
/// or a shape CMD Studio refuses, such as a key it owns written twice.
fn wt_parse(id: &'static str, path: &Path, e: &WtError) -> Check {
    check(
        id,
        Status::Fail,
        Some(e.code()),
        "การตั้งค่าของ Windows Terminal (settings.json) อ่านไม่ได้ หรือมีรูปแบบที่ CMD Studio ใช้ไม่ได้ \
         ให้แก้ settings.json ด้วยมือ",
        "Windows Terminal's settings (settings.json) can't be read, or have a shape CMD Studio \
         can't work with. Fix settings.json by hand",
        format!("{}: {e}", path.display()),
    )
}

fn d6(sys: System<'_>) -> Check {
    let locations = sys.wt.locations();
    let mut present = 0usize;
    for l in locations.iter().filter(|l| sys.fs.exists(&l.settings)) {
        let text = match settings_text("D6", sys, &l.settings) {
            Ok(Some(text)) => text,
            Ok(None) => continue,
            Err(c) => return c,
        };
        if let Err(e) = check_parses(&text) {
            return wt_parse("D6", &l.settings, &e);
        }
        present += 1;
    }
    if present == 0 {
        return if locations.iter().any(|l| l.installed) {
            check(
                "D6",
                Status::Warn,
                Some("E_WT_NOT_FOUND"),
                "Windows Terminal ยังไม่เคยเปิด ให้เปิดสักครั้งแล้วใช้ธีมซ้ำ",
                "Windows Terminal has never been opened. Open it once, then apply the theme again",
                "settings.json does not exist yet",
            )
        } else {
            check(
                "D6",
                Status::Warn,
                Some("E_WT_NOT_FOUND"),
                "ไม่พบ Windows Terminal",
                "Windows Terminal is not installed",
                "",
            )
        };
    }
    check(
        "D6",
        Status::Ok,
        None,
        "พบ Windows Terminal และอ่านการตั้งค่าได้",
        "Windows Terminal is installed and its settings can be read",
        format!("{present} settings file(s)"),
    )
}

fn d7(sys: System<'_>, ctx: &Ctx, state: &Result<State, StateError>) -> Check {
    let a = match applied("D7", state) {
        Ok(a) => a,
        Err(skipped) => return skipped,
    };
    // What the planner writes: plain cmd.exe for every cmd window (lesson 2), the `/k … inject`
    // line of the theme's own Clink for Windows Terminal only.
    let line = commandline(
        a.target,
        &a.launch.to_clink().exe(),
        &ctx.paths.clink_profile(),
    );
    let mut looked = false;
    for l in sys
        .wt
        .locations()
        .into_iter()
        .filter(|l| sys.fs.exists(&l.settings))
    {
        let text = match settings_text("D7", sys, &l.settings) {
            Ok(Some(text)) => text,
            Ok(None) => continue,
            Err(c) => return c,
        };
        if let Err(c) = d7_file(&l.settings, &text, a, &line) {
            return c;
        }
        looked = true;
    }
    if !looked {
        return check(
            "D7",
            Status::Skipped,
            None,
            "ไม่พบการตั้งค่าของ Windows Terminal (ดู D6)",
            "Windows Terminal's settings were not found (see D6)",
            "",
        );
    }
    check(
        "D7",
        Status::Ok,
        None,
        "หน้าต่าง cmd ใน Windows Terminal ใช้สีของธีม",
        "cmd in Windows Terminal uses the theme's colors",
        "",
    )
}

/// One settings file for D7. A key it cannot read is `E_WT_PARSE` with the reason (such as a
/// key written twice), never taken for a value that is not the theme's.
fn d7_file(path: &Path, text: &str, a: &Applied, line: &str) -> Result<(), Check> {
    let read = |key: WtKey| read_value(text, &key).map_err(|e| wt_parse("D7", path, &e));
    let profile_key = |key: &str| WtKey::Profile { key: key.into() };
    let p = path.display();
    let bad = |details: String| {
        check(
            "D7",
            Status::Fail,
            Some("E_WT_WRITE"),
            "การตั้งค่า cmd ใน Windows Terminal ไม่ตรงกับธีม ให้ใช้ธีมซ้ำ",
            "cmd's settings in Windows Terminal don't match the theme. Apply the theme again",
            details,
        )
    };
    let scheme = read(profile_key("colorScheme"))?;
    if scheme != Some(Value::String(SCHEME_NAME.into())) || read(WtKey::Scheme)?.is_none() {
        return Err(bad(format!(
            "{p}: the cmd profile does not use the {SCHEME_NAME} scheme"
        )));
    }
    let written = read(profile_key("commandline"))?;
    if written.as_ref().and_then(Value::as_str) != Some(line) {
        return Err(bad(format!(
            "{p}: commandline is {written:?}, not {line:?}"
        )));
    }
    if a.set_default {
        let default = read(WtKey::DefaultProfile)?;
        if !default
            .as_ref()
            .and_then(Value::as_str)
            .is_some_and(|g| g.eq_ignore_ascii_case(CMD_PROFILE_GUID))
        {
            return Err(bad(format!("{p}: defaultProfile is {default:?}")));
        }
    }
    Ok(())
}

fn d8(sys: System<'_>, state: &Result<State, StateError>) -> Check {
    let a = match applied("D8", state) {
        Ok(a) => a,
        Err(skipped) => return skipped,
    };
    let Some(face) = &a.wt_font else {
        return check(
            "D8",
            Status::Skipped,
            None,
            "ธีมไม่ได้ตั้งฟอนต์ เพราะตอนใช้ธีมไม่พบไฟล์ตั้งค่าของ Windows Terminal",
            "The theme set no font, because no Windows Terminal settings file was found when it \
             was applied",
            "",
        );
    };
    // v1 counts fonts GDI lists; fonts packaged inside Windows Terminal are not counted (spec
    // §5.4, decided in Phase 1b). The planner falls back to the bundled font anyway. GDI may
    // spell a family in another letter case than the theme does.
    let face = face.trim();
    let known = "GDI fonts only; fonts inside Windows Terminal's package are not counted";
    match sys.fonts.families() {
        Ok(f) if f.iter().any(|n| n.trim().eq_ignore_ascii_case(face)) => check(
            "D8",
            Status::Ok,
            None,
            "ฟอนต์ของธีมติดตั้งอยู่",
            "The theme's font is installed",
            format!("{face} ({known})"),
        ),
        Ok(_) => check(
            "D8",
            Status::Fail,
            Some("E_FONT_INSTALL"),
            "ไม่พบฟอนต์ของธีม Windows Terminal จะใช้ฟอนต์อื่นแทนโดยไม่เตือน ให้ใช้ธีมซ้ำ",
            "The theme's font is missing, and Windows Terminal silently uses another. Apply the \
             theme again",
            format!("{face} ({known})"),
        ),
        Err(e) => unreadable("D8", "รายชื่อฟอนต์ของ Windows", "the list of Windows fonts", e),
    }
}

fn d9(sys: System<'_>, state: &Result<State, StateError>) -> Check {
    let s = match state {
        Ok(s) => s,
        Err(e) => {
            let (th, en) = match e {
                StateError::Read(_) | StateError::Io(_) => (
                    "อ่านบันทึกของ CMD Studio (state.json) ไม่ได้ในตอนนี้ ลองตรวจอีกครั้งในอีกสักครู่",
                    "CMD Studio's record (state.json) can't be read right now. Check again in a \
                     moment",
                ),
                StateError::Newer { .. } => (
                    "บันทึกของ CMD Studio (state.json) มาจาก CMD Studio รุ่นที่ใหม่กว่า \
                     ให้ใช้รุ่นนั้น หรืออัปเดต CMD Studio",
                    "CMD Studio's record (state.json) was written by a newer CMD Studio. Use that \
                     version, or update CMD Studio",
                ),
                // "Revert everything" is a button of the app. On the command line `--cleanup`
                // reverts the same way, but with a damaged state.json it keeps the records (the
                // damaged copy and the restore points), so it ends with a failed step and does
                // so on every later run (`cleanup::cleanup`).
                StateError::Invalid(_) => (
                    "บันทึกของ CMD Studio (state.json) เสียหรือถูกแก้ ให้ใช้ \"ย้อนทุกอย่าง\" ในแอป \
                     แล้วใช้ธีมใหม่ ในบรรทัดคำสั่ง `cmdstudio --cleanup` จะย้อนแบบเดียวกัน \
                     แต่เก็บบันทึกของ CMD Studio ไว้ (state.json ที่เสียกับจุดย้อนกลับ) \
                     และจบด้วยขั้นที่ไม่สำเร็จซึ่งบอกว่าต้องทำอะไรต่อ",
                    "CMD Studio's record (state.json) is damaged or was edited. Use Revert \
                     everything in the app, then apply a theme again. On the command line, \
                     `cmdstudio --cleanup` reverts the same way, but keeps CMD Studio's records \
                     (the damaged state.json and the restore points) and ends with a failed step \
                     that says what to do",
                ),
            };
            return check("D9", Status::Fail, Some(e.code()), th, en, e.to_string());
        }
    };
    let Some(a) = &s.applied else {
        return check(
            "D9",
            Status::Skipped,
            None,
            "ยังไม่ได้ใช้ธีม",
            "No theme has been applied yet",
            "",
        );
    };
    let mut changed = Vec::new();
    for (path, hash) in &a.generated {
        match sys.fs.read(path) {
            Ok(now) => {
                if now.map(|b| sha256_hex(&b)).as_deref() != Some(hash.as_str()) {
                    changed.push(path.display().to_string());
                }
            }
            Err(e) => {
                return unreadable(
                    "D9",
                    "ไฟล์ที่ CMD Studio สร้าง",
                    "a file CMD Studio generated",
                    e,
                )
            }
        }
    }
    if !changed.is_empty() {
        return check(
            "D9",
            Status::Warn,
            Some("E_GENERATED_EDITED"),
            "มีคนแก้หรือลบไฟล์ที่ CMD Studio สร้าง ให้ใช้ธีมซ้ำเพื่อให้กลับเป็นของธีม",
            "A file CMD Studio generated was edited or deleted. Apply the theme again to restore \
             it",
            changed.join("\n"),
        );
    }
    check(
        "D9",
        Status::Ok,
        None,
        "ไฟล์ที่โปรแกรมสร้างยังเหมือนเดิม",
        "The files CMD Studio generated are unchanged",
        "",
    )
}

/// `packaged` is conservative: a package query that fails reads as packaged (Task 5), so this
/// never says `ok` on a guess.
fn d10(sys: System<'_>) -> Check {
    if sys.host.system_info().packaged {
        return check(
            "D10",
            Status::Fail,
            Some("E_APP_PACKAGED"),
            "โปรแกรมทำงานอยู่ในแพ็กเกจ MSIX (หรือ Windows ยืนยันไม่ได้ว่าไม่ได้อยู่ในแพ็กเกจ) \
             สิ่งที่โปรแกรมเขียนจึงไม่มีผลกับโปรแกรมอื่น ให้ติดตั้ง CMD Studio ด้วยตัวติดตั้งปกติ",
            "The app runs inside an MSIX package (or Windows could not tell that it does not), so \
             what it writes is invisible to other programs. Install CMD Studio with its own \
             installer",
            "spec §2.2 lesson 1",
        );
    }
    check(
        "D10",
        Status::Ok,
        None,
        "โปรแกรมทำงานแบบปกติ ไม่ได้อยู่ในแพ็กเกจ",
        "The app runs as a normal program, not inside a package",
        "",
    )
}

fn d11(sys: System<'_>) -> Check {
    let info = sys.host.system_info();
    // A fact the host could not read is not an answer (`SystemInfo::unreadable`).
    let unread = |fact: &str| {
        info.unreadable
            .iter()
            .find_map(|u| u.strip_prefix(fact))
            .map(|why| why.trim_start_matches(':').trim().to_string())
    };
    if let Some(why) = unread("build") {
        return check(
            "D11",
            Status::Skipped,
            None,
            format!("ตรวจข้อนี้ไม่ได้ เพราะอ่านรุ่นของ Windows ไม่ได้: {why}"),
            format!("Not checked: could not read the Windows build: {why}"),
            "",
        );
    }
    if info.build.is_none_or(|b| b < 22000) {
        return check(
            "D11",
            Status::Skipped,
            None,
            "มีเฉพาะใน Windows 11",
            "Windows 11 only",
            "",
        );
    }
    if let Some(why) = unread("default terminal") {
        return check(
            "D11",
            Status::Skipped,
            None,
            format!("ตรวจข้อนี้ไม่ได้ เพราะอ่านไม่ได้ว่าแอปไหนเปิดหน้าต่าง cmd (แอปเทอร์มินัลเริ่มต้น): {why}"),
            format!(
                "Not checked: could not read which app opens cmd windows (the default terminal): \
                 {why}"
            ),
            "",
        );
    }
    // Information only (spec §5.8): which app opens cmd windows started from Run or Start.
    // Only Windows Terminal gives them the theme's colors (G7).
    let wt_installed = sys.wt.locations().iter().any(|l| l.installed);
    let (th, en, colors) = match info.default_terminal {
        DefaultTerminal::WindowsTerminal => ("Windows Terminal", "Windows Terminal", true),
        DefaultTerminal::WindowsTerminalPreview => {
            ("Windows Terminal Preview", "Windows Terminal Preview", true)
        }
        DefaultTerminal::LetWindowsDecide if wt_installed => (
            "Windows Terminal (Windows เลือกให้)",
            "Windows Terminal (chosen by Windows)",
            true,
        ),
        DefaultTerminal::LetWindowsDecide | DefaultTerminal::Conhost => {
            ("หน้าต่าง cmd แบบเก่า", "the classic console window", false)
        }
        DefaultTerminal::Other => ("แอปอื่น", "another app", false),
    };
    let (th, en) = if colors {
        (
            format!("แอปเทอร์มินัลเริ่มต้นคือ {th} หน้าต่าง cmd ที่เปิดจาก Run หรือเมนู Start จึงได้สีของธีม"),
            format!(
                "The default terminal app is {en}, so cmd windows opened from Run or Start get \
                 the theme's colors"
            ),
        )
    } else {
        (
            format!("แอปเทอร์มินัลเริ่มต้นคือ {th} หน้าต่าง cmd ที่เปิดจาก Run หรือเมนู Start จึงไม่ได้สีของธีม"),
            format!(
                "The default terminal app is {en}, so cmd windows opened from Run or Start don't \
                 get the theme's colors"
            ),
        )
    };
    check(
        "D11",
        Status::Ok,
        None,
        th,
        en,
        format!("{:?}", info.default_terminal),
    )
}

/// What doctor finds wrong with CMD Studio's restore points, one entry for each problem, read
/// as the next command that changes the machine would find it (`actions::begin`), without the
/// recovery, the lock or anything set aside:
/// - R1: another run is changing the machine now (`E_BUSY`); or the newest change was cut off
///   and waits to be rolled back (`E_TXN_INTERRUPTED`), or cannot be checked, because its
///   restore point cannot be read (`E_SNAPSHOT_READ`, with `cmdstudio set-aside` as the way out
///   when it may never be read);
/// - R2: other restore points that cannot be read (`E_SNAPSHOT_READ`);
/// - R3: the record of a rollback that did not finish, with what to fix by hand
///   (`E_ROLLBACK_INCOMPLETE`), or a record that cannot be read (`E_SNAPSHOT_READ`).
fn records(sys: System<'_>, ctx: &Ctx) -> Vec<Check> {
    // The file a running command holds is opened for that command alone, so it cannot be read:
    // a read that fails is that command (or a scanner that has it open, as BUSY_EN says).
    // While it runs, its own restore point is unfinished, so nothing more is said.
    let lock = ctx.paths.lock_file();
    if let Err(e) = sys.fs.read(&lock) {
        return vec![check(
            "R1",
            Status::Warn,
            Some("E_BUSY"),
            BUSY_TH,
            BUSY_EN,
            e.to_string(),
        )];
    }
    let root = ctx.paths.snapshots();
    let store = SnapshotStore::new(sys.fs, &root);
    let listed = store
        .ids()
        .and_then(|ids| store.unreadable().map(|unreadable| (ids, unreadable)));
    let (ids, unreadable) = match listed {
        Ok(listed) => listed,
        Err(f) => {
            return vec![check(
                "R2",
                Status::Fail,
                Some(f.code),
                "อ่านรายการจุดย้อนกลับไม่ได้ จึงตรวจจุดย้อนกลับไม่ได้ ลองตรวจอีกครั้งในอีกสักครู่",
                "The restore points could not be listed, so they were not checked. Check again \
                 in a moment",
                f.message,
            )]
        }
    };
    let mut found = Vec::new();
    // The restore point R1 speaks for, so that R2 does not list it again.
    let mut said = None;
    match store.recovery().load_unfinished() {
        Ok(None) => {}
        Ok(Some(j)) => found.push(check(
            "R1",
            Status::Warn,
            Some("E_TXN_INTERRUPTED"),
            "การเปลี่ยนแปลงครั้งก่อนถูกตัดกลางทาง (โปรแกรมหยุดทำงานก่อนเสร็จ) และยังไม่ได้ย้อนกลับ \
             CMD Studio จะย้อนให้เองเมื่อเปลี่ยนการตั้งค่าครั้งถัดไป \
             (ใช้ธีม ย้อนกลับ ย้อนทุกอย่าง หรือ --cleanup) \
             ระหว่างนี้ผลตรวจข้างบนอาจเห็นการเปลี่ยนแปลงที่ทำไปครึ่งเดียว",
            "An earlier change was cut off (the app stopped) and is not rolled back yet. CMD \
             Studio rolls it back the next time it changes something (apply, undo, Revert \
             everything or --cleanup); until then the checks above may see it half done",
            format!("restore point {}", j.id),
        )),
        Err(f) => {
            let id = ids.last().cloned().unwrap_or_default();
            let kind = unreadable.iter().find(|u| u.id == id).map(|u| u.kind);
            found.push(cannot_recover(&id, kind, &f));
            said = Some(id);
        }
    }
    let others: Vec<String> = unreadable
        .iter()
        .filter(|u| said.as_ref() != Some(&u.id))
        .map(|u| {
            let then = match u.kind {
                UnreadableKind::CouldNotRead => {
                    "can't be read right now; it is left as it is, and an undo does not go past it"
                }
                UnreadableKind::Missing | UnreadableKind::Damaged => {
                    "is damaged or incomplete; it is set aside the next time CMD Studio changes \
                     something"
                }
            };
            format!("{}: {then} ({})", u.id, u.failure.message)
        })
        .collect();
    if !others.is_empty() {
        let n = others.len();
        found.push(check(
            "R2",
            Status::Warn,
            Some("E_SNAPSHOT_READ"),
            format!("อ่านจุดย้อนกลับไม่ได้ {n} จุด การย้อนกลับจะไม่ข้ามจุดเหล่านี้ ดูรายละเอียดของแต่ละจุด"),
            format!(
                "{n} restore point(s) can't be read, and an undo does not go past them. See the \
                 details for each"
            ),
            others.join("\n"),
        ));
    }
    match store.latest_incomplete() {
        Ok(None) => {}
        Ok(Some(record)) => {
            let n = record.steps.len();
            let mut lines = vec![format!("{}: {}", record.code, record.reason)];
            for s in &record.steps {
                let th = if s.fix_th.is_empty() {
                    &s.fix
                } else {
                    &s.fix_th
                };
                lines.push(format!(
                    "{}: {}: {}\n  fix: {}\n  แก้: {th}",
                    s.target, s.code, s.message, s.fix
                ));
            }
            found.push(check(
                "R3",
                Status::Fail,
                Some("E_ROLLBACK_INCOMPLETE"),
                format!(
                    "การย้อนกลับครั้งล่าสุด (จุดย้อนกลับ {}) ทำได้ไม่ครบ ยังมี {n} อย่างที่ต้องแก้ด้วยมือตามรายละเอียด \
                     แล้วใช้ธีมใหม่ หรือรันคำสั่ง `cmdstudio --cleanup`",
                    record.id
                ),
                format!(
                    "The last rollback (restore point {}) did not finish: {n} thing(s) must be \
                     fixed by hand, as the details say. Then apply a theme again, or run \
                     `cmdstudio --cleanup`",
                    record.id
                ),
                lines.join("\n"),
            ));
        }
        Err(f) => found.push(check(
            "R3",
            Status::Fail,
            Some(f.code),
            "อ่านบันทึกว่าการย้อนกลับครั้งล่าสุดเหลืออะไรให้แก้ด้วยมือไม่ได้ ลองตรวจอีกครั้งในอีกสักครู่",
            "The record of what the last rollback left to fix by hand can't be read. Check again \
             in a moment",
            f.message,
        )),
    }
    found
}

/// R1 for a newest restore point whose journal cannot be read, so that the change it holds
/// cannot be checked or rolled back: what the next command that changes the machine does about
/// it (`actions::begin`).
fn cannot_recover(id: &str, kind: Option<UnreadableKind>, f: &Failure) -> Check {
    match kind {
        // It will not mend: the next start sets it aside and records one step to fix by hand.
        Some(UnreadableKind::Missing | UnreadableKind::Damaged) => check(
            "R1",
            Status::Warn,
            Some(f.code),
            format!(
                "จุดย้อนกลับ {id} ของการเปลี่ยนแปลงที่ถูกตัดกลางทางเสียหาย จึงย้อนกลับเองไม่ได้ \
                 ครั้งถัดไปที่ CMD Studio เปลี่ยนการตั้งค่า มันจะกันจุดนี้ไว้ต่างหาก \
                 แล้วต้องตรวจด้วยมือ: {CLEANUP_FIX_TH}"
            ),
            format!(
                "The restore point {id} of a change that was cut off is damaged, so it can't be \
                 rolled back by itself. The next time CMD Studio changes something, it sets that \
                 point aside; then check by hand: {CLEANUP_FIX_EN}"
            ),
            f.message.clone(),
        ),
        // It may be read later: until then every change is refused (`E_SNAPSHOT_READ`).
        Some(UnreadableKind::CouldNotRead) | None => check(
            "R1",
            Status::Fail,
            Some(f.code),
            format!(
                "ยังตรวจการเปลี่ยนแปลงครั้งก่อนไม่ได้ เพราะตอนนี้อ่านจุดย้อนกลับ {id} ของมันไม่ได้ \
                 ระหว่างนี้ CMD Studio จะไม่เปลี่ยนการตั้งค่าใดๆ (ใช้ธีม ย้อนกลับ ย้อนทุกอย่าง \
                 และ --cleanup จะไม่ทำงาน) ลองใหม่อีกครั้งในอีกสักครู่ ถ้าจุดนี้อ่านไม่ได้ตลอด ให้รันคำสั่ง \
                 `cmdstudio set-aside {id}` เพื่อกันจุดนี้ไว้ต่างหาก แต่สิ่งที่การเปลี่ยนแปลงนั้นทำไว้ \
                 จะไม่ถูกย้อนเอง ต้องแก้ด้วยมือ"
            ),
            format!(
                "An earlier change could not be checked yet: its restore point {id} can't be \
                 read right now. Until it can, CMD Studio changes nothing (apply, undo, Revert \
                 everything and --cleanup are refused). Try again in a moment. If it never \
                 becomes readable, \
                 run `cmdstudio set-aside {id}` to set it aside; what that change did is then \
                 not rolled back by itself, and must be fixed by hand"
            ),
            f.message.clone(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::testing::{machine, request, WT};
    use crate::actions::{prepare_apply, run_apply, Prepared, BUSY_EN};
    use crate::adapters::fake::FakeMachine;
    use crate::adapters::{AutoRun, Fonts, Fs, RegValue, REG_SZ};
    use crate::exec::SystemExecutor;
    use crate::plan::NERD_FONT_FAMILY;
    use crate::presets::terracotta;
    use crate::resources::NERD_FONT_FILE;
    use crate::snapshot::SnapshotStore;
    use crate::txn::{Executor, Journal, JournalStore, Outcome, Snapshot, Stage};
    use std::path::PathBuf;

    fn by_id(checks: &[Check], id: &str) -> Check {
        checks.iter().find(|c| c.id == id).cloned().unwrap()
    }

    #[test]
    fn a_fresh_machine_skips_what_needs_a_theme() {
        let (m, ctx) = machine();
        let checks = doctor(m.system(), &ctx);
        assert_eq!(
            checks.iter().map(|c| c.id).collect::<Vec<_>>(),
            ["D1", "D2", "D3", "D4", "D5", "D6", "D7", "D8", "D9", "D10", "D11"]
        );
        for id in ["D1", "D2", "D3", "D4", "D7", "D8", "D9"] {
            assert_eq!(by_id(&checks, id).status, Status::Skipped, "{id}");
        }
        assert_eq!(by_id(&checks, "D6").status, Status::Ok);
        assert_eq!(by_id(&checks, "D10").status, Status::Ok);
    }

    #[test]
    fn after_a_good_apply_everything_checked_is_ok() {
        let (m, ctx) = machine();
        m.process.answer("oh-my-posh.exe", "version", 0, "31.5.0\n");
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        assert_eq!(run_apply(m.system(), &ctx, p), Outcome::Committed);
        put_log(
            &m,
            &ctx,
            "5f50 initialise_clink 237 Inject completed at 2026/10/10 22:39:52.599\n",
        );
        let checks = doctor(m.system(), &ctx);
        for c in &checks {
            assert!(matches!(c.status, Status::Ok | Status::Skipped), "{c:?}");
            if c.status == Status::Ok {
                assert_eq!(c.code, None);
            }
        }
        assert_eq!(by_id(&checks, "D3").status, Status::Ok);
    }

    #[test]
    fn only_a_cmd_window_started_since_the_apply_counts_for_d3() {
        // Ruling P68: AutoRun can stop before our entry in ways the planner can't see; a real
        // cmd window shows it. What Clink logged before the apply says nothing about this theme.
        let (m, ctx) = machine();
        let inject = "5f50 initialise_clink 237 Inject completed at 2026/10/10 22:39:52.599\n";
        put_log(&m, &ctx, inject);
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        assert_eq!(run_apply(m.system(), &ctx, p), Outcome::Committed);
        let d3 = by_id(&doctor(m.system(), &ctx), "D3");
        assert_eq!(
            (d3.status, d3.code),
            (Status::Warn, Some("E_THEME_NOT_LOADED"))
        );
        assert!(d3.message_en.contains("Open a new cmd window"), "{d3:?}");
        assert!(d3.message_en.contains("Registry Editor"), "{d3:?}");
        // a cmd window started now writes its own record after the old one
        put_log(&m, &ctx, &format!("{inject}{inject}"));
        assert_eq!(by_id(&doctor(m.system(), &ctx), "D3").status, Status::Ok);
        // a log that is shorter than it was at the apply was started over: all of it is new
        put_log(&m, &ctx, "1 x Inject completed\n");
        assert_eq!(by_id(&doctor(m.system(), &ctx), "D3").status, Status::Ok);
        put_log(&m, &ctx, "1 x nothing\n");
        assert_eq!(by_id(&doctor(m.system(), &ctx), "D3").status, Status::Warn);
    }

    #[test]
    fn our_entry_must_come_first_in_autorun() {
        let (m, ctx) = machine();
        m.process.answer("oh-my-posh.exe", "version", 0, "31.5.0\n");
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        assert_eq!(run_apply(m.system(), &ctx, p), Outcome::Committed);
        assert_eq!(by_id(&doctor(m.system(), &ctx), "D4").status, Status::Ok);
        let ours = m.autorun.read_user().unwrap().unwrap().text().unwrap();
        // another program put its command in front of ours
        let behind = RegValue::from_text(REG_SZ, &format!("if exist x.bat call x.bat & {ours}"));
        m.autorun.write_user(&behind).unwrap();
        let d4 = by_id(&doctor(m.system(), &ctx), "D4");
        assert_eq!(
            (d4.status, d4.code),
            (Status::Fail, Some("E_AUTORUN_WRITE"))
        );
        assert!(d4.message_en.contains("not first"), "{d4:?}");
        // a value cmd can't run at all
        let broken = RegValue::from_text(REG_SZ, &format!("{ours} & & echo x"));
        m.autorun.write_user(&broken).unwrap();
        let d4 = by_id(&doctor(m.system(), &ctx), "D4");
        assert_eq!(d4.code, Some("E_AUTORUN_WRITE"));
        assert!(d4.message_en.contains("Registry Editor"), "{d4:?}");
    }

    #[test]
    fn problems_are_reported_with_their_codes() {
        let (m, ctx) = machine();
        m.process.answer("oh-my-posh.exe", "version", 0, "31.4.0\n");
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        assert_eq!(run_apply(m.system(), &ctx, p), Outcome::Committed);
        put_log(&m, &ctx, "copy_dll 122 Failed to copy DLL to 'C:\\x'\ncheck_dll_version 154 Unable to get DLL version for 'C:\\??\\clink_dll_x64.dll'\n");
        m.autorun
            .write_user(&RegValue::from_text(
                REG_SZ,
                r#""C:\clink\clink.bat" inject --autorun"#,
            ))
            .unwrap();
        m.fs.put(ctx.paths.generated().join("init.lua"), b"-- edited by hand");
        m.fs.put(WT, b"{ broken");
        let checks = doctor(m.system(), &ctx);
        let code = |id| (by_id(&checks, id).status, by_id(&checks, id).code);
        assert_eq!(code("D2"), (Status::Fail, Some("E_RUNTIME_MISSING")));
        assert_eq!(code("D3"), (Status::Fail, Some("E_CLINK_PATH")));
        assert_eq!(code("D4"), (Status::Fail, Some("E_AUTORUN_WRITE")));
        assert_eq!(code("D5"), (Status::Warn, Some("E_AUTORUN_FOREIGN_CLINK")));
        assert_eq!(code("D6"), (Status::Fail, Some("E_WT_PARSE")));
        assert_eq!(code("D9"), (Status::Warn, Some("E_GENERATED_EDITED")));
    }

    #[test]
    fn inject_log_reading_takes_the_last_word() {
        assert_eq!(last_inject(""), InjectSeen::Never);
        assert_eq!(
            last_inject("x Failed to copy DLL\ny Inject completed at t\n"),
            InjectSeen::Ok
        );
        assert!(matches!(
            last_inject("Inject completed\nUnable to get DLL version for 'C:\\??'"),
            InjectSeen::Failed(_)
        ));
    }

    #[test]
    fn json_is_camel_case() {
        let c = check("D1", Status::Ok, None, "ก", "a", "");
        let text = serde_json::to_string(&c).unwrap();
        assert_eq!(
            text,
            r#"{"id":"D1","status":"ok","code":null,"messageTh":"ก","messageEn":"a","details":""}"#
        );
    }

    #[test]
    fn windows_terminal_never_opened_is_a_warning_with_a_way_out() {
        // Review Focus 1: a fresh Windows 11 has WT installed but no settings.json yet.
        let (m, ctx) = machine();
        m.fs.remove_file(std::path::Path::new(WT)).unwrap();
        let d6 = by_id(&doctor(m.system(), &ctx), "D6");
        assert_eq!((d6.status, d6.code), (Status::Warn, Some("E_WT_NOT_FOUND")));
        assert!(d6.message_en.contains("Open it once"), "{d6:?}");
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        assert!(p
            .plan
            .warnings
            .contains(&crate::plan::Warning::NoWindowsTerminal));
        assert!(!p
            .ops
            .iter()
            .any(|o| matches!(o, crate::plan::Op::WtApply { .. })));
        assert!(!m.fs.exists(std::path::Path::new(WT)), "never created");
    }

    #[test]
    fn d11_is_information_only() {
        let (mut m, ctx) = machine();
        m.host.info.default_terminal = DefaultTerminal::Conhost;
        let d11 = by_id(&doctor(m.system(), &ctx), "D11");
        assert_eq!((d11.status, d11.code), (Status::Ok, None));
        assert!(d11.message_en.contains("classic console"), "{d11:?}");
        m.host.info.build = Some(19045);
        assert_eq!(
            by_id(&doctor(m.system(), &ctx), "D11").status,
            Status::Skipped
        );
    }

    /// Writes `clink.log` in the Clink profile, as Clink does when it starts in cmd.
    fn put_log(m: &crate::adapters::fake::FakeMachine, ctx: &Ctx, text: &str) {
        m.fs.put(ctx.paths.clink_profile().join("clink.log"), text);
    }

    /// The line Clink 1.9.34 logs after a good inject (`spikes/clink/tmp/profile/clink.log`).
    const INJECTED: &str =
        "5f50 initialise_clink 237 Inject completed at 2026/10/10 22:39:52.599\n";

    /// A machine with the theme applied for `target`, and a cmd window started since.
    fn themed(target: Target) -> (FakeMachine, Ctx) {
        let (m, ctx) = machine();
        m.process.answer("oh-my-posh.exe", "version", 0, "31.5.0\n");
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &request(target)).unwrap();
        assert_eq!(run_apply(m.system(), &ctx, p), Outcome::Committed);
        put_log(&m, &ctx, INJECTED);
        (m, ctx)
    }

    fn later(ctx: &Ctx, ms: u64) -> Ctx {
        Ctx {
            now_ms: ctx.now_ms + ms,
            ..ctx.clone()
        }
    }

    /// A file of restore point `id`.
    fn point(ctx: &Ctx, id: &str, file: &str) -> PathBuf {
        ctx.paths.snapshots().join(id).join(file)
    }

    /// Everything doctor could change on the fake machine: every file and folder, AutoRun, the
    /// font registrations and loaded fonts.
    fn everything(m: &FakeMachine) -> impl PartialEq + std::fmt::Debug {
        (m.fs.snapshot(), m.autorun.snapshot(), m.fonts.snapshot())
    }

    /// Runs doctor and checks what holds for every result: D1–D11 first, in order; every text
    /// in Thai and in English (spec §5.9); `ok` never with a code, and every warning or failure
    /// with one; and nothing on the machine changed.
    fn run(m: &FakeMachine, ctx: &Ctx) -> Vec<Check> {
        let before = everything(m);
        let checks = doctor(m.system(), ctx);
        assert_eq!(everything(m), before, "doctor only reads");
        let ids: Vec<&str> = checks.iter().map(|c| c.id).collect();
        assert_eq!(
            ids.get(..11).unwrap(),
            ["D1", "D2", "D3", "D4", "D5", "D6", "D7", "D8", "D9", "D10", "D11"]
        );
        let thai = |s: &str| s.chars().any(|c| ('\u{0e00}'..='\u{0e7f}').contains(&c));
        for c in &checks {
            assert!(thai(&c.message_th), "no Thai text: {c:?}");
            assert!(!c.message_en.is_empty() && !thai(&c.message_en), "{c:?}");
            match c.status {
                Status::Ok => assert_eq!(c.code, None, "{c:?}"),
                Status::Warn | Status::Fail => assert!(c.code.is_some(), "{c:?}"),
                Status::Skipped => {}
            }
        }
        checks
    }

    /// Captures every op of `p` and runs the first `done`, journaling each step as the engine
    /// does, then stops: what a crash leaves.
    fn crash_after(m: &FakeMachine, ctx: &Ctx, p: &Prepared, done: usize) {
        let root = ctx.paths.snapshots();
        let store = SnapshotStore::new(m.system().fs, &root);
        let mut journal = store.begin(p.manifest.clone());
        let mut exec = SystemExecutor::new(m.system(), &ctx.resources);
        let snapshots = p
            .ops
            .iter()
            .enumerate()
            .map(|(i, op)| Snapshot {
                op_index: i,
                prior: exec.capture(op).unwrap(),
            })
            .collect();
        let mut j = Journal {
            id: p.manifest.id.clone(),
            ops: p.ops.clone(),
            snapshots,
            stage: Stage::Snapshotted,
        };
        journal.save(&j).unwrap();
        for (i, op) in p.ops.iter().take(done).enumerate() {
            exec.apply(op).unwrap();
            j.stage = Stage::Applying { done: i + 1 };
            journal.save(&j).unwrap();
        }
    }

    #[test]
    fn a_failed_autorun_read_is_never_ok() {
        // Ruling Q8 (COE): what could not be read is said, never passed off as "ok" or as the
        // problem the check looks for.
        for theme in [false, true] {
            let (m, ctx) = if theme {
                themed(Target::All)
            } else {
                machine()
            };
            m.autorun.fail_reads();
            let checks = run(&m, &ctx);
            let mut ids = vec!["D5"];
            if theme {
                ids.push("D4");
            }
            for id in ids {
                let c = by_id(&checks, id);
                assert_eq!(
                    (c.status, c.code),
                    (Status::Fail, Some("E_MACHINE_READ")),
                    "{c:?}"
                );
                assert!(c.details.contains("read denied (injected)"), "{c:?}");
                assert!(c.message_en.starts_with("Could not read"), "{c:?}");
            }
        }
    }

    #[test]
    fn d7_shows_why_a_settings_file_cannot_be_used() {
        // Task 7 review: a file that writes a key CMD Studio owns twice passes D6 (Windows
        // Terminal reads it), and D7 says why CMD Studio can't, not "does not use the scheme".
        let (m, ctx) = themed(Target::All);
        let text = String::from_utf8(m.fs.file(WT).unwrap()).unwrap();
        let twice = text.replacen(
            "\"defaultProfile\"",
            "\"defaultProfile\": \"{61c54bbd-c2c6-5271-96e7-009a87ff44bf}\",\n    \"defaultProfile\"",
            1,
        );
        assert_ne!(twice, text);
        m.fs.put(WT, twice);
        let checks = run(&m, &ctx);
        assert_eq!(by_id(&checks, "D6").status, Status::Ok);
        let d7 = by_id(&checks, "D7");
        assert_eq!(
            (d7.status, d7.code),
            (Status::Fail, Some("E_WT_PARSE")),
            "{d7:?}"
        );
        assert!(
            d7.details
                .contains("\"defaultProfile\" twice; fix settings.json by hand"),
            "{d7:?}"
        );
        assert!(
            d7.message_en.contains("Fix settings.json by hand"),
            "{d7:?}"
        );
    }

    #[test]
    fn d8_matches_the_font_family_in_any_letter_case() {
        // Task 6: GDI may spell the family otherwise than the theme does.
        let (m, ctx) = machine();
        m.fonts.knows_file(NERD_FONT_FILE, "CASKAYDIACOVE NF");
        m.process.answer("oh-my-posh.exe", "version", 0, "31.5.0\n");
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        assert_eq!(run_apply(m.system(), &ctx, p), Outcome::Committed);
        assert!(!m
            .fonts
            .families()
            .unwrap()
            .contains(&NERD_FONT_FAMILY.to_string()));
        let d8 = by_id(&run(&m, &ctx), "D8");
        assert_eq!((d8.status, d8.code), (Status::Ok, None), "{d8:?}");
    }

    #[test]
    fn d11_says_what_it_could_not_read() {
        // Ruling Q21: a fact that could not be read is not an answer.
        let (mut m, ctx) = machine();
        m.host.info.build = None;
        m.host.info.unreadable = vec!["build: access denied (injected)".into()];
        let d11 = by_id(&run(&m, &ctx), "D11");
        assert_eq!((d11.status, d11.code), (Status::Skipped, None));
        assert!(
            d11.message_en
                .contains("could not read the Windows build: access denied (injected)"),
            "{d11:?}"
        );
        assert!(
            d11.message_th.contains("access denied (injected)"),
            "{d11:?}"
        );

        m.host.info.build = Some(26100);
        m.host.info.unreadable = vec!["default terminal: access denied (injected)".into()];
        for wt in [true, false] {
            m.wt.locations.iter_mut().for_each(|l| l.installed = wt);
            let d11 = by_id(&run(&m, &ctx), "D11");
            assert_eq!((d11.status, d11.code), (Status::Skipped, None), "{d11:?}");
            assert!(d11.message_en.contains("default terminal"), "{d11:?}");
            assert!(
                d11.message_en.contains("access denied (injected)"),
                "{d11:?}"
            );
            assert!(!d11.message_en.contains("classic console"), "{d11:?}");
            assert!(!d11.message_en.contains("chosen by Windows"), "{d11:?}");
        }
    }

    #[test]
    fn d10_is_never_ok_when_windows_cannot_tell() {
        // Task 5: a failed package query reads as packaged, so D10 never says "ok" on a guess.
        let (mut m, ctx) = machine();
        m.host.info.packaged = true;
        let d10 = by_id(&run(&m, &ctx), "D10");
        assert_eq!(
            (d10.status, d10.code),
            (Status::Fail, Some("E_APP_PACKAGED"))
        );
        assert!(d10.message_en.contains("could not tell"), "{d10:?}");
    }

    #[test]
    fn an_unreadable_state_file_is_reported_and_the_rest_goes_on() {
        // Rulings Q3/Q9: doctor is never blocked by it, and never says "no theme" for it.
        for how in ["locked", "damaged", "newer"] {
            let (m, ctx) = themed(Target::All);
            let file = ctx.paths.state_file();
            match how {
                "locked" => m.fs.fail_reads_of(&file),
                "damaged" => m.fs.put(&file, b"{ not json"),
                _ => m.fs.put(&file, br#"{"version": 99}"#),
            }
            let checks = run(&m, &ctx);
            let d9 = by_id(&checks, "D9");
            assert_eq!(
                (d9.status, d9.code),
                (Status::Fail, Some("E_STATE_INVALID")),
                "{how}"
            );
            assert!(d9.message_en.contains("state.json"), "{how}: {d9:?}");
            for id in ["D1", "D2", "D3", "D4", "D7", "D8"] {
                let c = by_id(&checks, id);
                assert_eq!(c.status, Status::Skipped, "{how}: {c:?}");
                assert!(c.message_en.contains("state.json"), "{how}: {c:?}");
                assert!(!c.message_en.contains("No theme"), "{how}: {c:?}");
            }
            for id in ["D5", "D6", "D10", "D11"] {
                assert_eq!(by_id(&checks, id).status, Status::Ok, "{how}: {id}");
            }
        }
    }

    #[test]
    fn a_damaged_state_file_names_the_command_line_way_out_too() {
        // "Revert everything" is a button of the app; the CLI's way is `--cleanup`.
        let (m, ctx) = themed(Target::All);
        m.fs.put(ctx.paths.state_file(), b"{ not json");
        let d9 = by_id(&run(&m, &ctx), "D9");
        for text in [&d9.message_en, &d9.message_th] {
            assert!(text.contains("`cmdstudio --cleanup`"), "{d9:?}");
        }
        assert!(
            d9.message_en.contains("Revert everything in the app"),
            "{d9:?}"
        );
        assert!(d9.message_th.contains("\"ย้อนทุกอย่าง\" ในแอป"), "{d9:?}");
        // What --cleanup does then: it keeps the records (the damaged copy and the restore
        // points) and ends with a failed step; it does not delete the data.
        assert!(
            d9.message_en.contains("keeps CMD Studio's records")
                && d9.message_en.contains("failed step"),
            "{d9:?}"
        );
        assert!(!d9.message_en.contains("deletes"), "{d9:?}");
        assert!(
            d9.message_th.contains("เก็บบันทึกของ CMD Studio ไว้")
                && d9.message_th.contains("ขั้นที่ไม่สำเร็จ"),
            "{d9:?}"
        );
    }

    #[test]
    fn restore_points_that_cannot_be_read_are_reported_and_block_nothing() {
        // Ruling Q3: reported, never in the way of the checks.
        for locked in [false, true] {
            let (m, ctx) = themed(Target::All);
            let p = prepare_apply(
                m.system(),
                &later(&ctx, 1),
                &terracotta(),
                &request(Target::All),
            )
            .unwrap();
            assert_eq!(
                run_apply(m.system(), &later(&ctx, 1), p),
                Outcome::Committed
            );
            let root = ctx.paths.snapshots();
            let first = SnapshotStore::new(m.system().fs, &root).ids().unwrap()[0].clone();
            let journal = point(&ctx, &first, "journal.json");
            if locked {
                m.fs.fail_reads_of(&journal);
            } else {
                m.fs.put(&journal, b"{ damaged");
            }
            let checks = run(&m, &ctx);
            assert_eq!(checks.len(), 12, "{checks:#?}");
            assert_eq!(by_id(&checks, "D1").status, Status::Ok);
            let r2 = by_id(&checks, "R2");
            assert_eq!(
                (r2.status, r2.code),
                (Status::Warn, Some("E_SNAPSHOT_READ")),
                "{r2:?}"
            );
            assert!(r2.details.contains(&first), "{r2:?}");
            let then = if locked { "right now" } else { "set aside" };
            assert!(r2.details.contains(then), "{r2:?}");
        }
    }

    #[test]
    fn a_crash_that_cannot_be_checked_yet_is_reported_with_its_way_out() {
        // Ruling Q39: doctor runs no recovery and takes no lock; it says what waits, and how out.
        let (m, ctx) = machine();
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        crash_after(&m, &ctx, &p, 3);
        let id = p.manifest.id.clone();
        drop(p);
        let journal = point(&ctx, &id, "journal.json");

        m.fs.fail_reads_of(&journal);
        let checks = run(&m, &ctx);
        assert_eq!(
            checks.len(),
            12,
            "the same point is not listed twice: {checks:#?}"
        );
        let r1 = by_id(&checks, "R1");
        assert_eq!(
            (r1.status, r1.code),
            (Status::Fail, Some("E_SNAPSHOT_READ")),
            "{r1:?}"
        );
        let way_out = format!("cmdstudio set-aside {id}");
        assert!(r1.message_en.contains(&way_out), "{r1:?}");
        assert!(r1.message_th.contains(&way_out), "{r1:?}");
        assert!(r1.message_en.contains("Try again"), "{r1:?}");
        // Every command that changes the machine starts the same way, Revert everything too.
        let refused = "(apply, undo, Revert everything and --cleanup are refused)";
        assert!(r1.message_en.contains(refused), "{r1:?}");
        assert!(r1.message_th.contains("ย้อนทุกอย่าง"), "{r1:?}");

        // Readable again: still not rolled back by doctor, only reported.
        m.fs.stop_failing(&journal);
        let r1 = by_id(&run(&m, &ctx), "R1");
        assert_eq!(
            (r1.status, r1.code),
            (Status::Warn, Some("E_TXN_INTERRUPTED")),
            "{r1:?}"
        );
        assert!(r1.details.contains(&id), "{r1:?}");
        let rolls_back = "(apply, undo, Revert everything or --cleanup)";
        assert!(r1.message_en.contains(rolls_back), "{r1:?}");
        assert!(r1.message_th.contains("ย้อนทุกอย่าง"), "{r1:?}");
        assert!(
            m.fs.exists(&ctx.paths.omp_dir()),
            "the crash is still there"
        );

        // Damaged: the next change sets it aside; doctor says so.
        m.fs.put(&journal, b"{ damaged");
        let r1 = by_id(&run(&m, &ctx), "R1");
        assert_eq!(
            (r1.status, r1.code),
            (Status::Warn, Some("E_SNAPSHOT_READ")),
            "{r1:?}"
        );
        assert!(r1.message_en.contains("sets that point aside"), "{r1:?}");
    }

    #[test]
    fn a_rollback_that_did_not_finish_is_reported_with_how_to_fix_it() {
        // Spec §5.5: said after a restart too, with the fix by hand in both languages.
        let (m, ctx) = machine();
        let init = ctx.paths.generated().join("init.lua");
        m.fs.put(&init, b"old");
        m.fs.fail_writes_to(&init);
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        assert!(matches!(
            run_apply(m.system(), &ctx, p),
            Outcome::RollbackIncomplete { .. }
        ));
        let record = crate::actions::pending_incomplete(m.system(), &ctx)
            .unwrap()
            .unwrap();
        let step = &record.steps[0];
        let r3 = by_id(&run(&m, &ctx), "R3");
        assert_eq!(
            (r3.status, r3.code),
            (Status::Fail, Some("E_ROLLBACK_INCOMPLETE")),
            "{r3:?}"
        );
        assert!(r3.details.contains(&step.fix), "{r3:?}");
        assert!(r3.details.contains(&step.fix_th), "{r3:?}");
        assert!(r3.message_en.contains("cmdstudio --cleanup"), "{r3:?}");

        m.fs.fail_reads_of(point(&ctx, &record.id, "incomplete.json"));
        let r3 = by_id(&run(&m, &ctx), "R3");
        assert_eq!(
            (r3.status, r3.code),
            (Status::Fail, Some("E_SNAPSHOT_READ")),
            "{r3:?}"
        );
    }

    #[test]
    fn doctor_only_reads_and_never_waits_for_the_lock() {
        // A fresh machine: not even the data folder appears.
        let (m, ctx) = machine();
        assert_eq!(run(&m, &ctx).len(), 11);
        assert!(!m.fs.exists(&ctx.paths.data));

        // After an apply: the bundled programs are run with fixed arguments, and only these.
        let (m, ctx) = themed(Target::All);
        let calls = m.process.calls().len();
        run(&m, &ctx);
        let args: Vec<String> = m.process.calls()[calls..]
            .iter()
            .map(|c| c.args.join(" "))
            .collect();
        let profile = ctx.paths.clink_profile();
        assert_eq!(
            args,
            [
                "version".to_string(),
                format!("--profile {} set clink.logo", profile.display())
            ]
        );

        // Another run holds the lock: doctor neither waits nor takes it, and says so.
        let held = m.fs.try_lock(&ctx.paths.lock_file()).unwrap().unwrap();
        let checks = run(&m, &ctx);
        assert_eq!(by_id(&checks, "D1").status, Status::Ok);
        let r1 = by_id(&checks, "R1");
        assert_eq!(
            (r1.status, r1.code),
            (Status::Warn, Some("E_BUSY")),
            "{r1:?}"
        );
        assert_eq!(r1.message_en, BUSY_EN);
        drop(held);
        assert_eq!(run(&m, &ctx).len(), 11);
    }

    #[test]
    fn read_failures_are_never_reported_as_the_problem_checked_for() {
        // COE: a file that could not be read is not missing, edited or broken.
        type BreakIt = fn(&FakeMachine, &Ctx);
        let cases: [(&str, BreakIt); 6] = [
            ("D1", |m, ctx| m.fs.fail_reads_of(ctx.paths.omp_exe())),
            ("D3", |m, ctx| {
                m.fs.fail_reads_of(ctx.paths.clink_profile().join("clink.log"))
            }),
            ("D6", |m, _| m.fs.fail_reads_of(WT)),
            ("D7", |m, _| m.fs.fail_reads_of(WT)),
            ("D8", |m, _| m.fonts.fail_families()),
            ("D9", |m, ctx| {
                m.fs.fail_reads_of(ctx.paths.generated().join("init.lua"))
            }),
        ];
        for (id, break_it) in cases {
            let (m, ctx) = themed(Target::All);
            break_it(&m, &ctx);
            let c = by_id(&run(&m, &ctx), id);
            assert_eq!(
                (c.status, c.code),
                (Status::Fail, Some("E_MACHINE_READ")),
                "{c:?}"
            );
            assert!(c.details.contains("(injected)"), "{c:?}");
        }
    }

    #[test]
    fn d3_counts_a_clink_that_exits_badly() {
        // `clink set` always exits 0 (G5), as the verifier checks too.
        let (m, ctx) = themed(Target::All);
        m.process.answer("clink_x64.exe", "--profile", 1, "");
        let d3 = by_id(&run(&m, &ctx), "D3");
        assert_eq!(
            (d3.status, d3.code),
            (Status::Fail, Some("E_VERIFY_CLINK")),
            "{d3:?}"
        );
        assert!(d3.details.contains("exited 1"), "{d3:?}");
    }

    #[test]
    fn d1_says_how_to_get_the_add_on_back() {
        // A changed file in a runtime folder: an apply never overwrites the folder, so the way
        // back is to delete it first.
        let (m, ctx) = themed(Target::All);
        m.fs.put(ctx.paths.omp_exe(), b"changed");
        let d1 = by_id(&run(&m, &ctx), "D1");
        assert_eq!(
            (d1.status, d1.code),
            (Status::Fail, Some("E_RUNTIME_MISSING"))
        );
        let dir = ctx.paths.omp_dir().display().to_string();
        assert!(
            d1.message_en.contains(&format!("delete the folder {dir}")),
            "{d1:?}"
        );
        assert!(d1.message_th.contains(&dir), "{d1:?}");
        let p = prepare_apply(
            m.system(),
            &later(&ctx, 1),
            &terracotta(),
            &request(Target::All),
        )
        .unwrap();
        assert!(matches!(
            run_apply(m.system(), &later(&ctx, 1), p),
            Outcome::RolledBack {
                code: "E_RUNTIME_MISSING",
                ..
            }
        ));

        // A runtime folder that is gone comes back with the next apply.
        m.fs.remove_dir_all(&ctx.paths.omp_dir()).unwrap();
        let d1 = by_id(&run(&m, &ctx), "D1");
        assert_eq!(d1.code, Some("E_RUNTIME_MISSING"));
        assert!(d1.message_en.contains("Apply the theme again"), "{d1:?}");
        assert!(!d1.message_en.contains("delete"), "{d1:?}");
        let p = prepare_apply(
            m.system(),
            &later(&ctx, 2),
            &terracotta(),
            &request(Target::All),
        )
        .unwrap();
        assert_eq!(
            run_apply(m.system(), &later(&ctx, 2), p),
            Outcome::Committed
        );
        assert_eq!(by_id(&run(&m, &ctx), "D1").status, Status::Ok);

        // CMD Studio's own bundled files are gone: only a new install brings them back.
        m.fs.remove_file(&ctx.resources.root.join("manifest.json"))
            .unwrap();
        let d1 = by_id(&run(&m, &ctx), "D1");
        assert_eq!(
            (d1.status, d1.code),
            (Status::Fail, Some("E_RUNTIME_MISSING"))
        );
        assert!(d1.message_en.contains("Reinstall CMD Studio"), "{d1:?}");
    }

    #[test]
    fn wt_only_mode_checks_its_own_commandline() {
        let (m, ctx) = themed(Target::Wt);
        let checks = run(&m, &ctx);
        assert_eq!(by_id(&checks, "D4").status, Status::Skipped);
        assert_eq!(by_id(&checks, "D7").status, Status::Ok);

        // A `/k … inject` line that starts another Clink than the theme's.
        let text = String::from_utf8(m.fs.file(WT).unwrap()).unwrap();
        let stale = text.replace(
            r"clink-1.9.34\\clink_x64.exe",
            r"clink-1.9.0\\clink_x64.exe",
        );
        assert_ne!(stale, text);
        m.fs.put(WT, stale);
        let d7 = by_id(&run(&m, &ctx), "D7");
        assert_eq!(
            (d7.status, d7.code),
            (Status::Fail, Some("E_WT_WRITE")),
            "{d7:?}"
        );

        // No cmd tab since the apply: open one; AutoRun is not the way here.
        put_log(&m, &ctx, "1 x nothing\n");
        let d3 = by_id(&run(&m, &ctx), "D3");
        assert_eq!(
            (d3.status, d3.code),
            (Status::Warn, Some("E_THEME_NOT_LOADED"))
        );
        assert!(d3.message_en.contains("new cmd tab"), "{d3:?}");
        assert!(!d3.message_en.contains("Registry Editor"), "{d3:?}");
    }

    #[test]
    fn d5_names_every_other_clink_and_where_it_is() {
        let (m, ctx) = themed(Target::All);
        let ours = m.autorun.read_user().unwrap().unwrap().text().unwrap();
        // A command that only mentions Clink is another program's own.
        let mention = r#"doskey /macrofile="C:\tools\clink\macros.txt""#;
        m.autorun
            .write_user(&RegValue::from_text(REG_SZ, &format!("{ours} & {mention}")))
            .unwrap();
        assert_eq!(by_id(&run(&m, &ctx), "D5").status, Status::Ok);

        let user = r#""C:\clink\clink.bat" inject --autorun"#;
        let machine_wide = r#""C:\Program Files\clink\clink_x64.exe" inject --autorun"#;
        m.autorun
            .write_user(&RegValue::from_text(REG_SZ, &format!("{ours} & {user}")))
            .unwrap();
        m.autorun
            .set_machine(Some(RegValue::from_text(REG_SZ, machine_wide)));
        let d5 = by_id(&run(&m, &ctx), "D5");
        assert_eq!(
            (d5.status, d5.code),
            (Status::Warn, Some("E_AUTORUN_FOREIGN_CLINK"))
        );
        assert_eq!(d5.details, format!("HKCU: {user}\nHKLM: {machine_wide}"));
        assert!(d5.message_en.contains("Apply the theme again"), "{d5:?}");

        m.autorun
            .write_user(&RegValue::from_text(REG_SZ, &ours))
            .unwrap();
        let d5 = by_id(&run(&m, &ctx), "D5");
        assert_eq!(d5.code, Some("E_AUTORUN_FOREIGN_CLINK"));
        assert_eq!(d5.details, format!("HKLM: {machine_wide}"));
        assert!(d5.message_en.contains("whole machine"), "{d5:?}");
    }

    #[test]
    fn every_problem_is_said_in_thai_and_english() {
        // Spec §5.9: `run` checks both texts, the code of every warning and failure, and that
        // nothing changed. These are the results the other tests reach through `doctor` alone.
        let statuses = |checks: &[Check]| -> Vec<(&'static str, Status)> {
            checks.iter().map(|c| (c.id, c.status)).collect()
        };
        let not_ok = |checks: &[Check], ids: &[&str]| {
            for id in ids {
                let c = by_id(checks, id);
                assert!(matches!(c.status, Status::Warn | Status::Fail), "{c:?}");
            }
        };

        // The problems of `problems_are_reported_with_their_codes`, and a font that is gone.
        let (mut m, ctx) = themed(Target::All);
        m.process.answer("oh-my-posh.exe", "version", 0, "31.4.0\n");
        put_log(
            &m,
            &ctx,
            "check_dll_version 154 Unable to get DLL version for 'C:\\??'\n",
        );
        m.autorun
            .write_user(&RegValue::from_text(
                REG_SZ,
                r#""C:\clink\clink.bat" inject"#,
            ))
            .unwrap();
        m.fs.put(ctx.paths.generated().join("init.lua"), b"-- edited by hand");
        m.fs.put(WT, b"{ broken");
        m.fonts = crate::adapters::fake::FakeFonts::new(r"C:\f", &["Arial"]);
        let checks = run(&m, &ctx);
        not_ok(&checks, &["D2", "D3", "D4", "D5", "D6", "D7", "D8", "D9"]);

        // A Lua error, a clink that does not start, AutoRun that cmd refuses or with ours
        // behind another command, and the runtime gone.
        let (m, ctx) = themed(Target::All);
        m.process.answer(
            "clink_x64.exe",
            "--profile",
            0,
            "C:\\d\\generated\\init.lua:9: attempt to call a nil value\n",
        );
        let ours = m.autorun.read_user().unwrap().unwrap().text().unwrap();
        m.autorun
            .write_user(&RegValue::from_text(REG_SZ, &format!("{ours} & & x")))
            .unwrap();
        not_ok(&run(&m, &ctx), &["D3", "D4"]);
        m.autorun
            .write_user(&RegValue::from_text(REG_SZ, &format!("echo x & {ours}")))
            .unwrap();
        m.fs.remove_dir_all(&ctx.paths.clink_dir()).unwrap();
        m.fs.remove_dir_all(&ctx.paths.omp_dir()).unwrap();
        not_ok(&run(&m, &ctx), &["D1", "D4"]);
        m.autorun
            .write_user(&RegValue::from_text(REG_SZ, &ours))
            .unwrap();
        let d4 = by_id(&run(&m, &ctx), "D4");
        assert_eq!(d4.code, Some("E_RUNTIME_MISSING"), "{d4:?}");

        // No Windows Terminal at all, and each default terminal D11 can name.
        let (mut m, ctx) = machine();
        m.fs.remove_file(std::path::Path::new(WT)).unwrap();
        m.wt.locations.iter_mut().for_each(|l| l.installed = false);
        assert_eq!(by_id(&run(&m, &ctx), "D6").code, Some("E_WT_NOT_FOUND"));
        for terminal in [
            DefaultTerminal::WindowsTerminal,
            DefaultTerminal::WindowsTerminalPreview,
            DefaultTerminal::LetWindowsDecide,
            DefaultTerminal::Conhost,
            DefaultTerminal::Other,
        ] {
            m.host.info.default_terminal = terminal;
            let d11 = by_id(&run(&m, &ctx), "D11");
            assert_eq!(d11.status, Status::Ok, "{d11:?}");
        }

        // An apply with no Windows Terminal: what needs it is skipped, saying why.
        m.process.answer("oh-my-posh.exe", "version", 0, "31.5.0\n");
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        assert_eq!(run_apply(m.system(), &ctx, p), Outcome::Committed);
        let checks = run(&m, &ctx);
        assert!(
            statuses(&checks).contains(&("D7", Status::Skipped))
                && statuses(&checks).contains(&("D8", Status::Skipped)),
            "{checks:#?}"
        );
    }

    #[test]
    fn d8_says_no_settings_file_was_there_when_the_theme_set_no_font() {
        // Windows Terminal installed but never opened has no settings.json: the planner wrote
        // no font, though Windows Terminal is installed.
        let (m, ctx) = machine();
        m.fs.remove_file(std::path::Path::new(WT)).unwrap();
        m.process.answer("oh-my-posh.exe", "version", 0, "31.5.0\n");
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        assert_eq!(run_apply(m.system(), &ctx, p), Outcome::Committed);
        let d8 = by_id(&run(&m, &ctx), "D8");
        assert_eq!((d8.status, d8.code), (Status::Skipped, None));
        assert!(
            d8.message_en
                .contains("no Windows Terminal settings file was found"),
            "{d8:?}"
        );
        assert!(!d8.message_en.contains("not installed"), "{d8:?}");
    }

    /// Settings.json after `themed`, with `from` replaced by `to` once (which must be there).
    fn edit_wt(m: &FakeMachine, from: &str, to: &str) {
        let text = String::from_utf8(m.fs.file(WT).unwrap()).unwrap();
        let edited = text.replacen(from, to, 1);
        assert_ne!(edited, text, "{from} is not in settings.json");
        m.fs.put(WT, edited);
    }

    #[test]
    fn d7_checks_the_scheme_itself_the_default_profile_and_the_plain_commandline() {
        let wrong = |m: &FakeMachine, ctx: &Ctx, what: &str| {
            let d7 = by_id(&run(m, ctx), "D7");
            assert_eq!(
                (d7.status, d7.code),
                (Status::Fail, Some("E_WT_WRITE")),
                "{what}: {d7:?}"
            );
            assert!(d7.details.contains(what), "{what}: {d7:?}");
        };
        // The profile names the scheme, but the scheme is gone.
        let (m, ctx) = themed(Target::All);
        edit_wt(&m, r#""name": "CMD Studio""#, r#""name": "Someone else's""#);
        wrong(&m, &ctx, "does not use the CMD Studio scheme");

        // The user chose cmd as the default profile, and something changed it back.
        let (m, ctx) = themed(Target::All);
        edit_wt(
            &m,
            r#""defaultProfile": "{0caa0dad-35be-5f56-a8ff-afceeeaa6101}""#,
            r#""defaultProfile": "{61c54bbd-c2c6-5271-96e7-009a87ff44bf}""#,
        );
        wrong(&m, &ctx, "defaultProfile");
        // Only checked when the user chose it.
        let (m, ctx) = machine();
        m.process.answer("oh-my-posh.exe", "version", 0, "31.5.0\n");
        let mut not_default = request(Target::All);
        not_default.set_wt_default_profile = false;
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &not_default).unwrap();
        assert_eq!(run_apply(m.system(), &ctx, p), Outcome::Committed);
        assert_eq!(by_id(&run(&m, &ctx), "D7").status, Status::Ok);

        // Every cmd window: anything after cmd.exe loses the colors of windows opened from Run
        // (spec §2.2 lesson 2).
        let (m, ctx) = themed(Target::All);
        edit_wt(
            &m,
            r#""commandline": "%SystemRoot%\\System32\\cmd.exe""#,
            r#""commandline": "%SystemRoot%\\System32\\cmd.exe /k echo hi""#,
        );
        wrong(&m, &ctx, "commandline is");
    }

    #[test]
    fn d9_counts_a_generated_file_that_was_deleted() {
        let (m, ctx) = themed(Target::All);
        let init = ctx.paths.generated().join("init.lua");
        m.fs.remove_file(&init).unwrap();
        let d9 = by_id(&run(&m, &ctx), "D9");
        assert_eq!(
            (d9.status, d9.code),
            (Status::Warn, Some("E_GENERATED_EDITED"))
        );
        assert_eq!(d9.details, init.display().to_string());
    }
}
