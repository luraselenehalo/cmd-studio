//! "Revert everything" (the app's "ย้อนทุกอย่าง" button) and the uninstaller's `--cleanup`
//! (spec §6.3), key by key, so changes the user made after CMD Studio survive:
//!
//! 1. only our part of AutoRun goes;
//! 2. the cmd profile keys come back and the `CMD Studio` scheme goes;
//! 3. fonts CMD Studio installed go;
//! 4. (`--cleanup` only) app data except `themes\`, and the ProgramData folder.
//!
//! Both start as apply and undo do (`actions::begin`), under the same lock: a transaction a
//! crash cut off is rolled back first, and while one cannot be checked yet nothing changes and
//! nothing is deleted (`E_SNAPSHOT_READ`). Steps 1 and 2 are one transaction with a `revertAll`
//! restore point. The record of a rollback that did not finish is checked against the machine
//! after the revert: what is still CMD Studio's stays recorded, and what is not is said to be
//! put back. What could not be done is a failed [`Step`] with its code (spec §5.9) and, where a
//! person can finish it, how, in English and in Thai; it is never reported as done, and nothing
//! of the user's is deleted, nor the only record of what the user's settings were.
use crate::actions::{
    begin, delete_pending, delete_pending_and_save, next_id, record_incomplete, ActionError, Ctx,
    StartReport,
};
use crate::adapters::{
    program_data_dir, DirEntry, DirState, RegValue, System, REG_EXPAND_SZ, REG_SZ,
};
use crate::exec::{
    font_target, same_path, same_state, FixByHand, SystemExecutor, COMMAND_PROCESSOR_KEY, FONTS_KEY,
};
use crate::generate::wt::CMD_PROFILE_GUID;
use crate::plan::autorun::{is_foreign, remove_ours, scan};
use crate::plan::{Op, Paths, PlanError, NERD_FONT_FAMILY};
use crate::snapshot::{Incomplete, Manifest, SnapshotKind, SnapshotStore, Unrestored, KEEP};
use crate::state::{
    AutoRunRecord, Baseline, FontRecord, RegValueRecord, State, StateError, Was, WtFileBaseline,
};
use crate::txn::{self, Executor, Failure, Outcome};
use crate::verify::SystemVerifier;
use crate::wt_edit::{read_value, revert, WtError, WtKey};
use serde_json::{Map, Value};
use std::fmt::Display;
use std::path::Path;

/// One step's result, for the report and the exit code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    /// What the step is about, in English.
    pub what: String,
    pub ok: bool,
    /// What happened, in English. A failed step's starts with its code: `E_…: …`.
    pub detail: String,
    /// A failed step's stable code (spec §5.9); `None` for one that went well.
    pub code: Option<&'static str>,
    /// How a person finishes a failed step by hand, in English and in Thai, where there is a way
    /// to say it.
    pub fix: Option<FixByHand>,
}

/// What [`revert_all`] or [`cleanup`] did.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Report {
    /// What the start found and did (see [`StartReport`]): a crash rolled back, restore points
    /// that cannot be read, the record of a rollback that did not finish. Its `problems` are
    /// not kept here: each is a failed step. Each step of the record is a step of this report
    /// too, done when the revert left nothing of CMD Studio's in its target, failed (with the
    /// record's fix) when it did not.
    pub start: StartReport,
    /// The AutoRun and settings.json transaction; `None` when it did not run (nothing to change,
    /// no apply to mark and no record to settle, or the revert stopped before it).
    pub outcome: Option<Outcome>,
    pub steps: Vec<Step>,
}

impl Report {
    /// Every step went well, and the transaction, if it ran, committed.
    pub fn ok(&self) -> bool {
        self.steps.iter().all(|s| s.ok) && !self.rolled_back()
    }

    fn rolled_back(&self) -> bool {
        matches!(
            self.outcome,
            Some(Outcome::RolledBack { .. } | Outcome::RollbackIncomplete { .. })
        )
    }

    /// A report that begins with what the start could not do, each a failed step.
    fn new(mut start: StartReport) -> Report {
        let mut report = Report::default();
        for f in std::mem::take(&mut start.problems) {
            report.fail("Look at what earlier runs left", f.code, f.message, None);
        }
        report.start = start;
        report
    }

    fn done(&mut self, what: impl Into<String>, detail: impl Into<String>) {
        self.steps.push(Step {
            what: what.into(),
            ok: true,
            detail: detail.into(),
            code: None,
            fix: None,
        });
    }

    fn fail(
        &mut self,
        what: impl Into<String>,
        code: &'static str,
        message: impl Display,
        fix: Option<FixByHand>,
    ) {
        self.steps.push(Step {
            what: what.into(),
            ok: false,
            detail: format!("{code}: {message}"),
            code: Some(code),
            fix,
        });
    }
}

/// The commands of an AutoRun value, as cmd reads them.
fn commands(value: &str) -> Vec<String> {
    scan(value).parts.into_iter().map(|p| p.text).collect()
}

/// The other program's Clink commands that CMD Studio turned off and that are not back: the
/// commands of `original` that `plan::autorun` calls another Clink (`--replace-other-clink`
/// takes out exactly those), in their order, but for those in `now`. `original` is AutoRun as
/// it was before CMD Studio, or as a rollback was to put it back.
fn turned_off(original: Option<&AutoRunRecord>, profile: &str, now: &[String]) -> Vec<String> {
    original
        .and_then(|o| o.value.as_ref())
        .and_then(RegValueRecord::to_reg)
        .and_then(|v| v.text())
        .map(|text| {
            commands(&text)
                .into_iter()
                .filter(|c| is_foreign(c, profile) && !now.contains(c))
                .collect()
        })
        .unwrap_or_default()
}

/// What AutoRun becomes (spec §6.3 step 1: only our part goes). `Ok(None)`: leave it as it is.
/// `Ok(Some(None))`: delete the value. Our entries are taken out of the current value by the
/// planner's own editor (`plan::autorun::remove_ours`), so the planner and the cleanup never
/// disagree about what is ours, and every other byte stays. Then the other program's Clink
/// commands that CMD Studio turned off (see `turned_off`) come back in front, in their order.
/// When nothing but blanks is left, the value is deleted if it was not there before CMD Studio
/// (or nothing says it was), and kept, with what is left, if it was. A value with nothing to
/// take out and nothing to put back is left as it is. The value keeps its registry type when it
/// is written. A value that can't be edited safely is `E_AUTORUN_WRITE`, and is left for a hand
/// edit.
pub fn autorun_after_revert(
    current: Option<&str>,
    original: Option<&AutoRunRecord>,
    paths: &Paths,
) -> Result<Option<Option<String>>, PlanError> {
    // The planner refuses a profile path that is not Unicode, so no entry of ours holds one.
    let profile_path = paths.clink_profile();
    let Some(profile) = profile_path.to_str() else {
        return Ok(None);
    };
    let (rest, took_ours) = match current {
        Some(value) => {
            let edit = remove_ours(value, profile)?;
            (edit.value, edit.removed_ours)
        }
        None => (String::new(), false),
    };
    let now = current.map(commands).unwrap_or_default();
    let back = turned_off(original, profile, &now);
    if !took_ours && back.is_empty() {
        return Ok(None);
    }
    let front = back.join(" & ");
    let value = match (front.is_empty(), rest.trim().is_empty()) {
        (true, true) => {
            let existed = original.is_some_and(|o| o.value.is_some());
            return Ok(Some(existed.then_some(rest)));
        }
        (true, false) => rest,
        (false, true) => front,
        (false, false) => format!("{front} & {rest}"),
    };
    Ok(Some(Some(value)))
}

/// Undoes everything CMD Studio changed, key by key (the "ย้อนทุกอย่าง" button, spec §4.3),
/// under the lock that apply and undo take; the font files earlier runs left to delete are
/// tried again first. Refused, with nothing changed, while another run holds the lock
/// (`E_BUSY`), while a transaction a crash cut off cannot be checked yet (`E_SNAPSHOT_READ`),
/// and while state.json cannot be read now, is from a newer CMD Studio, or is damaged and
/// cannot be set aside (`E_STATE_INVALID`): such a state.json is never renamed or written over.
/// The record of a rollback that did not finish is settled after the revert (see [`Report`]):
/// what is still CMD Studio's goes into the record of the revert's own restore point, so later
/// starts keep reporting it.
pub fn revert_all(sys: System<'_>, ctx: &Ctx) -> Result<Report, ActionError> {
    let (_lock, start) = begin(sys, ctx)?;
    let mut report = Report::new(start);
    let (state, _) = load_state(sys, ctx, &mut report)?;
    revert_steps(sys, ctx, state, &mut report);
    Ok(report)
}

/// `cmdstudio --cleanup [--remove-themes]`: revert everything, then delete the app's data
/// (keeping `themes\` unless asked) and the user's ProgramData folder (spec §6.3 step 4), under
/// the lock that apply and undo take.
///
/// - It stops before deleting anything while another run holds the lock (`E_BUSY`) or a
///   transaction a crash cut off cannot be checked yet (`E_SNAPSHOT_READ`): its restore point
///   may be the only copy of the user's values. Restore points set aside never stop it.
/// - A revert whose transaction rolls back stops it too: nothing is deleted.
/// - A state.json that cannot be used stops only the revert (a failed step); the folders still
///   go, but the data folder stays whole while it holds a state.json from a newer CMD Studio.
/// - It never deletes the only record of what the user's settings were: while something is not
///   put back or cannot be known to be (state.json could not be used, or its AutoRun original
///   is damaged; a settings.json or a font or AutoRun is left as it is; a rollback record names
///   a target that is still CMD Studio's; the start could not read every record; a damaged
///   state.json set aside, a restore point set aside or that cannot be read, or a rollback
///   record that cannot be read is on disk), state.json, its damaged copies and the restore
///   points stay, and a failed step says so and how to finish by hand. Each of these is found
///   anew by every run, so a later `--cleanup` gives the same answer.
/// - The folders are worked out from `Paths` and the user's SID, not from state.json; a
///   ProgramData folder state.json names besides goes only when it is directly in the
///   ProgramData folder, private, and named like CMD Studio's. A folder that someone else may
///   change is left alone, and the report says why.
/// - The lock file goes last, once the lock is let go: Windows refuses to delete a file opened
///   the way the lock opens it.
pub fn cleanup(sys: System<'_>, ctx: &Ctx, remove_themes: bool) -> Result<Report, ActionError> {
    let (lock, start) = begin(sys, ctx)?;
    // What the start could not read, a record or a restore point, may be the only copy of the
    // user's values: its code, when there is such a thing.
    let unread = start
        .problems
        .first()
        .map(|f| f.code)
        .or_else(|| (!start.unreadable.is_empty()).then_some("E_SNAPSHOT_READ"));
    let mut report = Report::new(start);
    let mut state = None;
    // The code of what kept state.json from use, or damaged a part of it, when something did.
    let mut unusable = None;
    let mut newer_state = false;
    let unresolved;
    match load_state(sys, ctx, &mut report) {
        Ok((loaded, damaged)) => {
            let reverted = revert_steps(sys, ctx, loaded, &mut report);
            if damaged || reverted.damaged {
                unusable = Some("E_STATE_INVALID");
            }
            unresolved = reverted.unresolved;
            match reverted.state {
                Some(next) => state = Some(next),
                None => return Ok(report),
            }
        }
        Err(e) => {
            newer_state = matches!(e, ActionError::State(StateError::Newer { .. }));
            unusable = Some(e.code());
            report.fail(
                "Revert CMD Studio's changes",
                e.code(),
                format!("{e}; nothing was reverted"),
                None,
            );
            unresolved = settle_record(sys, ctx, None, &mut report);
        }
    }
    if let Some(state) = &mut state {
        pending_font_files(sys, ctx, state, &mut report);
    }
    let recorded = state
        .as_ref()
        .and_then(|s| s.baseline.program_data_dir.as_deref());
    remove_program_data(sys, recorded, &mut report);
    let data = &ctx.paths.data;
    if newer_state {
        report.fail(
            format!("Delete {}", data.display()),
            "E_STATE_INVALID",
            format!(
                "{} was kept: its state.json is from a newer CMD Studio and records what your \
                 settings were before CMD Studio",
                data.display()
            ),
            Some(newer_state_by_hand(data)),
        );
        return Ok(report);
    }
    let not_back = state
        .as_ref()
        .is_some_and(|s| holds_what_is_not_back(&s.baseline));
    // Looked for on disk by every run, so a later --cleanup gives the same answer.
    let in_doubt = records_in_doubt(sys, ctx);
    let keep_records =
        unusable.is_some() || in_doubt.is_some() || unread.is_some() || not_back || unresolved;
    remove_data(sys, ctx, remove_themes, keep_records, &mut report);
    if keep_records {
        report.fail(
            format!("Delete {}", data.display()),
            unusable
                .or(in_doubt)
                .or(unread)
                .unwrap_or("E_ROLLBACK_INCOMPLETE"),
            format!(
                "state.json and the restore points were kept in {}: not everything is known to \
                 be back as it was before CMD Studio, and they record what it was",
                data.display()
            ),
            Some(records_by_hand(data)),
        );
    }
    drop(lock);
    remove_lock_and_folder(sys, ctx, &mut report);
    Ok(report)
}

/// state.json for a revert, and whether it was damaged. One that cannot be read now, or is from
/// a newer CMD Studio, stops the revert and is left as it is: it may hold a baseline only it
/// knows. A damaged one is renamed `state.json.damaged-<ms>` and kept for a look, the report
/// says so in a failed step, and the revert goes on with no baseline; when that rename fails
/// it stops, so the damaged file is never written over. The font files earlier runs left to
/// delete are tried again, as at the start of apply and undo.
fn load_state(
    sys: System<'_>,
    ctx: &Ctx,
    report: &mut Report,
) -> Result<(State, bool), ActionError> {
    let file = ctx.paths.state_file();
    match State::load(sys.fs, &file) {
        Ok(mut state) => {
            let mut start = StartReport::default();
            delete_pending_and_save(sys, ctx, &mut state, &mut start);
            for f in start.problems {
                report.fail(
                    "Delete the font files left for later",
                    f.code,
                    f.message,
                    None,
                );
            }
            Ok((state, false))
        }
        Err(e @ StateError::Invalid(_)) => {
            let aside = file.with_extension(format!("json.damaged-{}", ctx.now_ms));
            if let Err(why) = sys.fs.rename(&file, &aside) {
                return Err(StateError::Invalid(format!(
                    "{e}. It could not be set aside as {} ({why}), so nothing was changed",
                    aside.display()
                ))
                .into());
            }
            report.fail(
                "Read state.json",
                e.code(),
                format!(
                    "{e}. It was renamed {} and kept for a look; without it, only CMD Studio's \
                     own AutoRun entries are taken out",
                    aside.display()
                ),
                None,
            );
            Ok((State::default(), true))
        }
        Err(e) => Err(e.into()),
    }
}

/// What [`revert_steps`] left.
struct Reverted {
    /// The state.json written; `None` when the transaction did not commit, and nothing else was
    /// touched.
    state: Option<State>,
    /// A step of the pending rollback record names a target that is still CMD Studio's.
    unresolved: bool,
    /// state.json's copy of the AutoRun original was damaged (see [`original_damaged`]).
    damaged: bool,
}

/// Steps 1–3 with `state` loaded: the AutoRun and settings.json transaction, the applies marked
/// as reverted, the registry keys CMD Studio created, the fonts, the new state.json, and the
/// pending rollback record settled. The new state.json has no applied theme, and keeps what
/// could not be put back yet (the AutoRun original while AutoRun is left as it is, a
/// settings.json left as it is, a font that stays installed), the ProgramData folder CMD Studio
/// made, and the font files to delete later.
fn revert_steps(sys: System<'_>, ctx: &Ctx, state: State, report: &mut Report) -> Reverted {
    let base = &state.baseline;
    let mut ops = Vec::new();
    let mut left = Baseline {
        program_data_dir: base.program_data_dir.clone(),
        ..Baseline::default()
    };
    let damaged = original_damaged(base);
    if damaged {
        report.fail(
            AUTORUN_STEP,
            "E_STATE_INVALID",
            "what AutoRun held before CMD Studio cannot be read back from state.json (it is not \
             hex), so a Clink of another program that CMD Studio turned off, if there was one, \
             is not turned back on; CMD Studio's own part is taken out all the same",
            Some(autorun_original_by_hand(sys, ctx)),
        );
    }
    let autorun_done = autorun_step(sys, &ctx.paths, base, &mut ops, report);
    if !autorun_done || damaged {
        // Only the original: it says which other Clink to turn back on, and whether the value
        // was there before, once AutoRun can be edited. A damaged one stays while it is
        // damaged, so that every later --cleanup keeps the restore point that holds it whole.
        left.autorun = base.autorun.clone();
    }
    for file in &base.wt {
        if !settings_step(sys, file, &mut ops, report) {
            left.wt.push(file.clone());
        }
    }
    let ran = transaction(sys, ctx, ops, report);
    if !ran.committed {
        return Reverted {
            state: None,
            unresolved: settle_record(sys, ctx, ran.id.as_deref(), report),
            damaged,
        };
    }
    if autorun_done && base.autorun.as_ref().is_some_and(|a| !a.key_existed) {
        command_processor_key(sys, report);
    }
    let mut next = State {
        applied: None,
        baseline: left,
        pending_deletes: state.pending_deletes.clone(),
        ..State::default()
    };
    uninstall_fonts(sys, base, &mut next, report);
    save_state(sys, ctx, &next, report);
    let unresolved = settle_record(sys, ctx, ran.id.as_deref(), report);
    Reverted {
        state: Some(next),
        unresolved,
        damaged,
    }
}

/// What the AutoRun steps are about.
const AUTORUN_STEP: &str = "Take CMD Studio's part out of AutoRun";

/// Step 1: queues what AutoRun becomes. Whether AutoRun needs nothing more: a change is queued,
/// or none is needed. A value that cannot be read, or is left for a hand edit, is a failed step.
fn autorun_step(
    sys: System<'_>,
    paths: &Paths,
    base: &Baseline,
    ops: &mut Vec<Op>,
    report: &mut Report,
) -> bool {
    const WHAT: &str = AUTORUN_STEP;
    let profile = paths.clink_profile().display().to_string();
    let value = match sys.autorun.read_user() {
        Ok(value) => value,
        Err(e) => {
            report.fail(
                WHAT,
                "E_MACHINE_READ",
                format!("AutoRun could not be read ({e}), so it was left as it is"),
                None,
            );
            return false;
        }
    };
    let current = match &value {
        None => None,
        Some(v) => match v.text() {
            Some(text) => Some(text),
            // Not a string value: cmd does not run it, so nothing of ours runs from it.
            None if v.kind != REG_SZ && v.kind != REG_EXPAND_SZ => return true,
            None => {
                report.fail(
                    WHAT,
                    "E_AUTORUN_WRITE",
                    "AutoRun is a string value but not valid text, so CMD Studio cannot tell \
                     its part from the rest; it was left as it is",
                    Some(autorun_by_hand(&lossy_text(v), &profile)),
                );
                return false;
            }
        },
    };
    match autorun_after_revert(current.as_deref(), base.autorun.as_ref(), paths) {
        Ok(next) => {
            if let Some(value) = next {
                ops.push(Op::SetAutoRun { value });
            }
            true
        }
        Err(e) => {
            let part = match &e {
                PlanError::AutoRunNeedsHandEdit { part } => part.clone(),
                _ => current.unwrap_or_default(),
            };
            report.fail(
                WHAT,
                e.code(),
                format!("{e}. It was left as it is"),
                Some(autorun_by_hand(&part, &profile)),
            );
            false
        }
    }
}

/// Whether state.json's copy of what AutoRun held before CMD Studio is damaged (not hex). The
/// restore point of the apply that recorded it still holds a whole copy.
fn original_damaged(base: &Baseline) -> bool {
    base.autorun
        .as_ref()
        .and_then(|a| a.value.as_ref())
        .is_some_and(|record| record.to_reg().is_none())
}

/// Where AutoRun as it was before CMD Studio still is, when state.json's copy is damaged: the
/// oldest apply that changed AutoRun keeps it as that step's prior state. Names the restore
/// point, its file, and the value when it can be read, and how to put back by hand a Clink of
/// another program that CMD Studio turned off.
fn autorun_original_by_hand(sys: System<'_>, ctx: &Ctx) -> FixByHand {
    let key = COMMAND_PROCESSOR_KEY;
    let root = ctx.paths.snapshots();
    let store = SnapshotStore::new(sys.fs, &root);
    let found = store.ids().unwrap_or_default().into_iter().find_map(|id| {
        if store.manifest(&id).ok()?.kind != SnapshotKind::Apply {
            return None;
        }
        let journal = store.journal(&id).ok()?;
        let index = journal
            .ops
            .iter()
            .position(|o| matches!(o, Op::SetAutoRun { .. }))?;
        let prior = journal
            .snapshots
            .iter()
            .find(|s| s.op_index == index)?
            .prior
            .clone()?;
        let record = serde_json::from_slice::<AutoRunRecord>(&prior).ok();
        let payload = store.payload_path(&id, index);
        Some((id, payload, record))
    });
    let Some((id, payload, record)) = found else {
        return FixByHand {
            en: format!(
                "No restore point holds a copy that can be read either. In Registry Editor, open \
                 {key} and look at AutoRun: if a Clink of another program used to start from it, \
                 add its command back by hand."
            ),
            th: format!(
                "ไม่มีจุดย้อนกลับใดเก็บสำเนาที่อ่านได้ไว้เช่นกัน เปิด Registry Editor (regedit) ไปที่ {key} \
                 แล้วดูค่า AutoRun ถ้าเคยมี Clink ของโปรแกรมอื่นเริ่มจากค่านี้ ให้เพิ่มคำสั่งนั้นกลับเข้าไปเอง"
            ),
        };
    };
    let payload = payload.display();
    let was = record.map(|r| r.value.map(|v| v.to_reg().and_then(|v| v.text())));
    let (was_en, was_th) = match was {
        Some(None) => (
            "there was no AutoRun value".to_string(),
            "ตอนนั้นไม่มีค่า AutoRun".to_string(),
        ),
        Some(Some(Some(text))) => (
            format!("AutoRun was: {text}"),
            format!("ค่า AutoRun ตอนนั้นคือ: {text}"),
        ),
        _ => (
            "its value is kept there as hex".to_string(),
            "ค่าในไฟล์นั้นเก็บเป็นเลขฐานสิบหก".to_string(),
        ),
    };
    FixByHand {
        en: format!(
            "Restore point {id} keeps what AutoRun held before CMD Studio, in {payload}; \
             {was_en}. In Registry Editor, open {key}: if a command of another program's Clink \
             from it is missing from AutoRun now, add it back by hand. While state.json's copy \
             is damaged, CMD Studio says this again and --cleanup keeps its records."
        ),
        th: format!(
            "จุดย้อนกลับ {id} เก็บค่า AutoRun ก่อนใช้ CMD Studio ไว้ในไฟล์ {payload} \
             {was_th} เปิด Registry Editor (regedit) ไปที่ {key} ถ้าคำสั่งของ Clink \
             ของโปรแกรมอื่นจากค่านั้นหายไปจาก AutoRun ให้เพิ่มกลับเข้าไปเอง \
             ระหว่างที่สำเนาใน state.json ยังเสียอยู่ CMD Studio จะแจ้งข้อนี้อีก \
             และ --cleanup จะเก็บบันทึกไว้"
        ),
    }
}

/// The text of a string value that is not valid UTF-16, with U+FFFD in place of what is not, to
/// show it.
fn lossy_text(value: &RegValue) -> String {
    let (pairs, _) = value.bytes.as_chunks::<2>();
    let units: Vec<u16> = pairs
        .iter()
        .map(|p| u16::from_le_bytes(*p))
        .take_while(|u| *u != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

/// Step 2 for one settings.json: queues its text with CMD Studio's keys put back
/// (`wt_edit::revert`). Whether the file needs nothing more: queued, nothing to change, or
/// gone. CMD Studio never creates a settings.json, so one that is gone is not made again, and
/// `revert` never gives an empty text. One that cannot be read, or that Windows Terminal could
/// not read, is left as it is and said, with what CMD Studio set in it to take out by hand.
fn settings_step(
    sys: System<'_>,
    file: &WtFileBaseline,
    ops: &mut Vec<Op>,
    report: &mut Report,
) -> bool {
    let path = &file.settings;
    let what = format!("Put back {}", path.display());
    let bytes = match sys.fs.read(path) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => {
            report.done(what, "the file is gone; nothing to put back");
            return true;
        }
        Err(e) => {
            report.fail(
                what,
                "E_MACHINE_READ",
                format!("it could not be read ({e}), so it was left as it is"),
                Some(settings_by_hand(
                    file,
                    "CMD Studio could not read it, so it left it as it is.",
                    "CMD Studio อ่านไฟล์นี้ไม่ได้ จึงยังไม่ได้แก้",
                )),
            );
            return false;
        }
    };
    let reverted = String::from_utf8(bytes)
        .map_err(|e| WtError::Parse(e.to_string()))
        .and_then(|text| {
            revert(
                &text,
                &file.originals_map(),
                &file.written_map(),
                &file.created,
            )
            .map(|new| (text, new))
        });
    match reverted {
        Ok((old, new)) => {
            if old != new {
                ops.push(Op::WriteFile {
                    path: path.clone(),
                    contents: new,
                });
            }
            true
        }
        Err(e) => {
            report.fail(
                what,
                e.code(),
                format!("{e}; it was left as it is"),
                Some(settings_by_hand(
                    file,
                    "Windows Terminal cannot read it as it is, so CMD Studio left it as it is; \
                     fix that first.",
                    "Windows Terminal อ่านไฟล์นี้ในสภาพนี้ไม่ได้ CMD Studio จึงยังไม่ได้แก้ \
                     ให้แก้ไฟล์ให้อ่านได้ก่อน",
                )),
            );
            false
        }
    }
}

/// How [`transaction`] went.
struct Ran {
    /// The `revertAll` restore point, when it ran.
    id: Option<String>,
    /// It committed, or had nothing to do.
    committed: bool,
}

/// The AutoRun and settings.json changes as one transaction with a `revertAll` restore point
/// (verified by read-back; a rollback that cannot finish is recorded), then every apply marked
/// as reverted by it: an undo after this would bring back a theme the user removed. It runs
/// when there is something to change, an apply to mark, or a pending rollback record, so a
/// machine the user already put back by hand gets its applies marked too, and the record is
/// settled into a restore point newer than its own.
fn transaction(sys: System<'_>, ctx: &Ctx, ops: Vec<Op>, report: &mut Report) -> Ran {
    const MARK: &str = "Mark the themes applied before as reverted";
    let root = ctx.paths.snapshots();
    let store = SnapshotStore::new(sys.fs, &root);
    let (applies, unread) = applies_to_mark(&store);
    for f in unread {
        report.fail(MARK, f.code, f.message, None);
    }
    if ops.is_empty() && applies.is_empty() && report.start.incomplete.is_none() {
        return Ran {
            id: None,
            committed: true,
        };
    }
    let id = next_id(&store, ctx.now_ms);
    let manifest = Manifest {
        id: id.clone(),
        kind: SnapshotKind::RevertAll,
        created_ms: ctx.now_ms,
        theme: None,
        target: None,
        summary: vec!["Revert everything".into()],
        undoes: None,
        undone_by: None,
    };
    let mut journal = store.begin(manifest);
    let mut exec = SystemExecutor::new(sys, &ctx.resources);
    let mut verifier = SystemVerifier::new(sys, &ctx.paths, &ctx.resources, None);
    let outcome = txn::run(&id, ops, &mut exec, &mut verifier, &mut journal);
    if let Err(f) = record_incomplete(&store, &id, &outcome) {
        report.fail(
            "Record what the rollback could not put back",
            f.code,
            f.message,
            None,
        );
    }
    let committed = outcome == Outcome::Committed;
    report.outcome = Some(outcome);
    if committed {
        if let Err(f) = store.mark_undone(&applies, &id) {
            report.fail(
                MARK,
                f.code,
                format!(
                    "{}; until they are, an undo may put back what restore points {} changed",
                    f.message,
                    applies.join(", ")
                ),
                None,
            );
        }
        // Failing to prune costs disk space only.
        let _ = store.prune(KEEP);
    }
    Ran {
        id: Some(id),
        committed,
    }
}

/// The applies that no undo or revert reversed yet, and a failure for each restore point whose
/// manifest cannot be read now and that was not set aside: it may be one of them, and an undo
/// would go over it once it can be read.
fn applies_to_mark(store: &SnapshotStore<'_>) -> (Vec<String>, Vec<Failure>) {
    let ids = match store.ids() {
        Ok(ids) => ids,
        Err(f) => return (Vec::new(), vec![f]),
    };
    let mut applies = Vec::new();
    let mut unread = Vec::new();
    for id in ids {
        match store.manifest(&id) {
            Ok(m) => {
                if m.kind == SnapshotKind::Apply && m.undone_by.is_none() {
                    applies.push(id);
                }
            }
            Err(f) => unread.push((id, f)),
        }
    }
    if unread.is_empty() {
        return (applies, Vec::new());
    }
    // A restore point that was set aside is never undone, so it needs no mark.
    let failures = match store.unreadable() {
        Ok(listed) => unread
            .into_iter()
            .filter(|(id, _)| listed.iter().any(|u| u.id == *id))
            .map(|(id, f)| Failure {
                code: f.code,
                message: format!(
                    "restore point {id} cannot be read now, so it is not marked as reverted, and \
                     an undo may put back what it changed once it can be read: {}",
                    f.message
                ),
            })
            .collect(),
        Err(f) => vec![f],
    };
    (applies, failures)
}

/// The record of a rollback that did not finish, found when this run started: each of its steps
/// is checked against the machine now. A step whose target holds nothing of CMD Studio's any
/// more is put back; one whose target still does is a failed step with the record's own fix,
/// and is carried over into the record of `revert`, the restore point this run made, which is
/// now the newest, so that later starts keep reporting it. Whether any step is still not put
/// back.
fn settle_record(sys: System<'_>, ctx: &Ctx, revert: Option<&str>, report: &mut Report) -> bool {
    let Some(record) = report.start.incomplete.clone() else {
        return false;
    };
    let root = ctx.paths.snapshots();
    let store = SnapshotStore::new(sys.fs, &root);
    let mut left = Vec::new();
    for step in &record.steps {
        let from = step
            .restore_point
            .clone()
            .unwrap_or_else(|| record.id.clone());
        let what = format!("{} (restore point {from})", step.target);
        match still_ours(sys, ctx, &store, &from, step) {
            None => report.done(
                what,
                format!(
                    "put back: what the rollback of restore point {from} left of CMD Studio's \
                     is gone"
                ),
            ),
            Some(fix) => {
                report.fail(
                    what,
                    "E_ROLLBACK_INCOMPLETE",
                    format!(
                        "the rollback of restore point {from} could not put it back ({}: {}), and \
                         it still holds CMD Studio's change",
                        step.code, step.message
                    ),
                    Some(fix),
                );
                left.push(Unrestored {
                    restore_point: Some(from),
                    ..step.clone()
                });
            }
        }
    }
    if let Some(id) = revert {
        carry_over(&store, id, &record, &left, report);
    }
    !left.is_empty()
}

/// Writes the steps still not put back into the record of restore point `id`, after its own
/// steps if its rollback did not finish either.
fn carry_over(
    store: &SnapshotStore<'_>,
    id: &str,
    pending: &Incomplete,
    left: &[Unrestored],
    report: &mut Report,
) {
    const WHAT: &str = "Keep the record of what is still not put back";
    if left.is_empty() {
        return;
    }
    let own = match store.latest_incomplete() {
        Ok(record) => record.filter(|r| r.id == id),
        Err(f) => {
            report.fail(WHAT, f.code, f.message, None);
            return;
        }
    };
    let (code, reason, mut steps) = match own {
        Some(record) => (record.code, record.reason, record.steps),
        None => (pending.code.clone(), pending.reason.clone(), Vec::new()),
    };
    steps.extend(left.iter().cloned());
    let record = Incomplete {
        id: id.to_string(),
        code,
        reason,
        steps,
    };
    if let Err(f) = store.write_incomplete(&record) {
        report.fail(WHAT, f.code, f.message, None);
    }
}

/// Whether the target of a step of a rollback record still holds a change of CMD Studio's,
/// judged now: `Some` with how to take it out by hand when it does (or when that cannot be
/// read), `None` when it does not. The step's op comes from the journal of restore point
/// `from`; when that cannot be read, CMD Studio's traces on the whole machine are looked for.
fn still_ours(
    sys: System<'_>,
    ctx: &Ctx,
    store: &SnapshotStore<'_>,
    from: &str,
    step: &Unrestored,
) -> Option<FixByHand> {
    let journal = store.journal(from).ok();
    let found = journal.as_ref().and_then(|j| {
        let op = j.ops.get(step.op_index)?;
        let prior = j
            .snapshots
            .iter()
            .find(|s| s.op_index == step.op_index)
            .map(|s| s.prior.clone());
        Some((op, prior))
    });
    let Some((op, prior)) = found else {
        return traces_left(sys, ctx);
    };
    op_still_ours(sys, ctx, op, prior.as_ref()).then(|| FixByHand {
        en: step.fix.clone(),
        th: if step.fix_th.is_empty() {
            step.fix.clone()
        } else {
            step.fix_th.clone()
        },
    })
}

/// Whether `op`'s target still holds CMD Studio's change, against `prior`, what the rollback was
/// to put back. CMD Studio's own files and runtime folders are not the user's, and nothing runs
/// them once the theme is reverted.
fn op_still_ours(sys: System<'_>, ctx: &Ctx, op: &Op, prior: Option<&Option<Vec<u8>>>) -> bool {
    let wt_file = |path: &Path| {
        sys.wt
            .locations()
            .iter()
            .any(|l| same_path(&l.settings, path))
    };
    match op {
        Op::InstallRuntime { dir, .. } => !cmd_studios_own(sys, ctx, dir),
        Op::WriteFile { path, .. } if cmd_studios_own(sys, ctx, path) => false,
        // A settings.json a revert of everything wrote: CMD Studio's part is its scheme.
        Op::WriteFile { path, .. } if wt_file(path) => settings_still_ours(sys, path, None, None),
        Op::WriteFile { .. } => true,
        Op::SetAutoRun { .. } => {
            let original = prior
                .and_then(|p| p.as_deref())
                .and_then(|bytes| serde_json::from_slice::<AutoRunRecord>(bytes).ok());
            autorun_still_ours(sys, &ctx.paths, original.as_ref())
        }
        Op::WtApply {
            settings,
            profile,
            set_default,
            ..
        } => settings_still_ours(
            sys,
            settings,
            Some((profile, *set_default)),
            prior.and_then(|p| p.as_deref()),
        ),
        Op::InstallFont { .. } => {
            let Some(prior) = prior else {
                return true;
            };
            let mut exec = SystemExecutor::new(sys, &ctx.resources);
            exec.capture(op)
                .map_or(true, |now| !same_state(op, &now, prior))
        }
    }
}

/// Whether `path` is in CMD Studio's data folder or the user's ProgramData folder.
fn cmd_studios_own(sys: System<'_>, ctx: &Ctx, path: &Path) -> bool {
    if path.starts_with(&ctx.paths.data) {
        return true;
    }
    match (sys.host.program_data(), sys.host.user_sid()) {
        (Ok(pd), Ok(sid)) => path.starts_with(program_data_dir(&pd, &sid)),
        _ => false,
    }
}

/// Whether AutoRun still holds an entry of ours, or lacks another program's Clink that
/// `original` held (one CMD Studio turned off). One that cannot be read counts as holding ours.
fn autorun_still_ours(sys: System<'_>, paths: &Paths, original: Option<&AutoRunRecord>) -> bool {
    let profile_path = paths.clink_profile();
    let Some(profile) = profile_path.to_str() else {
        return false;
    };
    let current = match sys.autorun.read_user() {
        Ok(None) => None,
        Ok(Some(v)) => match v.text() {
            Some(text) => Some(text),
            None if v.kind != REG_SZ && v.kind != REG_EXPAND_SZ => None,
            None => return true,
        },
        Err(_) => return true,
    };
    if let Some(value) = &current {
        match remove_ours(value, profile) {
            Ok(edit) if !edit.removed_ours => {}
            _ => return true,
        }
    }
    let now = current.as_deref().map(commands).unwrap_or_default();
    !turned_off(original, profile, &now).is_empty()
}

/// Whether `settings` still holds CMD Studio's change: a scheme named `CMD Studio`, or, given
/// the op's profile values (and whether it set the default profile), a key that holds what CMD
/// Studio wrote and held something else in `prior`. A file that is gone holds nothing; one that
/// cannot be read counts as holding the change.
fn settings_still_ours(
    sys: System<'_>,
    settings: &Path,
    op: Option<(&Map<String, Value>, bool)>,
    prior: Option<&[u8]>,
) -> bool {
    let now = match sys.fs.read(settings) {
        Ok(None) => return false,
        Ok(Some(bytes)) => match String::from_utf8(bytes) {
            Ok(text) => text,
            Err(_) => return true,
        },
        Err(_) => return true,
    };
    if !matches!(read_value(&now, &WtKey::Scheme), Ok(None)) {
        return true;
    }
    let Some((profile, set_default)) = op else {
        return false;
    };
    let was = prior.and_then(|bytes| std::str::from_utf8(bytes).ok());
    let left = |key: WtKey, wrote: &Value| {
        let Ok(holds) = read_value(&now, &key) else {
            return true;
        };
        let held = was.and_then(|w| read_value(w, &key).ok().flatten());
        holds.as_ref() == Some(wrote) && held.as_ref() != Some(wrote)
    };
    for (key, wrote) in profile {
        let ours = match wrote {
            Value::Object(subs) => subs.iter().any(|(sub, v)| {
                left(
                    WtKey::ProfileSub {
                        key: key.clone(),
                        sub: sub.clone(),
                    },
                    v,
                )
            }),
            v => left(WtKey::Profile { key: key.clone() }, v),
        };
        if ours {
            return true;
        }
    }
    set_default
        && left(
            WtKey::DefaultProfile,
            &Value::String(CMD_PROFILE_GUID.into()),
        )
}

/// CMD Studio's traces on the whole machine, for a record step whose op cannot be known (its
/// journal cannot be read): an entry of ours in AutoRun, a `CMD Studio` scheme in a settings
/// file, the bundled font registered to CMD Studio's file. `None` when there is none; otherwise
/// what is left and how to take it out by hand.
fn traces_left(sys: System<'_>, ctx: &Ctx) -> Option<FixByHand> {
    let mut en = Vec::new();
    let mut th = Vec::new();
    if autorun_still_ours(sys, &ctx.paths, None) {
        let current = match sys.autorun.read_user() {
            Ok(Some(v)) => v.text().unwrap_or_else(|| lossy_text(&v)),
            _ => String::from("(it could not be read)"),
        };
        let fix = autorun_by_hand(&current, &ctx.paths.clink_profile().display().to_string());
        en.push(fix.en);
        th.push(fix.th);
    }
    for location in sys.wt.locations() {
        let file = &location.settings;
        if sys.fs.exists(file) && settings_still_ours(sys, file, None, None) {
            let path = file.display();
            en.push(format!(
                "In {path}, delete the color scheme named \"CMD Studio\" and set the Command \
                 Prompt profile (guid {CMD_PROFILE_GUID}) back as you want it."
            ));
            th.push(format!(
                "ในไฟล์ {path} ให้ลบชุดสี (scheme) ชื่อ \"CMD Studio\" \
                 แล้วตั้งโปรไฟล์ Command Prompt (guid {CMD_PROFILE_GUID}) กลับเป็นแบบที่ต้องการเอง"
            ));
        }
    }
    if let Some((_, file, value_name)) = font_target(sys, &ctx.resources, NERD_FONT_FAMILY) {
        let ours = match sys.fonts.registration(&value_name) {
            Ok(Some(named)) => same_path(Path::new(&named), &file),
            Ok(None) => false,
            Err(_) => true,
        };
        if ours {
            let fix = font_by_hand(&FontRecord {
                family: NERD_FONT_FAMILY.to_string(),
                file,
                value_name,
            });
            en.push(fix.en);
            th.push(fix.th);
        }
    }
    if en.is_empty() {
        return None;
    }
    Some(FixByHand {
        en: format!(
            "CMD Studio cannot tell which of its changes this left: its journal cannot be read. \
             This is still CMD Studio's: {}",
            en.join(" ")
        ),
        th: format!(
            "CMD Studio บอกไม่ได้ว่าการเปลี่ยนแปลงนี้เหลืออะไรไว้ เพราะอ่านบันทึกขั้นตอนของมันไม่ได้ \
             สิ่งที่ยังเป็นของ CMD Studio คือ: {}",
            th.join(" ")
        ),
    })
}

/// The `Command Processor` key, which CMD Studio created: deleted once it holds nothing.
fn command_processor_key(sys: System<'_>, report: &mut Report) {
    const WHAT: &str = "Remove the Command Processor key CMD Studio created";
    if let Err(e) = sys.autorun.delete_user_key_if_empty() {
        report.fail(WHAT, "E_AUTORUN_WRITE", e, None);
        return;
    }
    match sys.autorun.user_key_exists() {
        Ok(false) => report.done(WHAT, "removed"),
        Ok(true) => report.done(WHAT, "kept: it holds other values"),
        Err(e) => report.fail(
            WHAT,
            "E_MACHINE_READ",
            format!("whether it is gone could not be read ({e})"),
            None,
        ),
    }
}

/// Step 3: the fonts CMD Studio installed. One that stays installed stays in the baseline for
/// the next revert, and so does whether CMD Studio made the registration key; once no font of
/// CMD Studio's is left, a key it made is deleted when empty.
fn uninstall_fonts(sys: System<'_>, base: &Baseline, next: &mut State, report: &mut Report) {
    for font in &base.fonts {
        if !uninstall_font(sys, font, next, report) {
            next.baseline.note_font(font.clone());
        }
    }
    if !next.baseline.fonts.is_empty() {
        next.baseline.fonts_key_existed = base.fonts_key_existed;
    } else if base.fonts_key_existed == Some(false) {
        fonts_key(sys, report);
    }
}

/// One font CMD Studio installed, in the order G12 proved (spec §5.4): out of the session's
/// font table, its registration, a broadcast of the change, its file. Whether CMD Studio is done
/// with it: uninstalled (its file deleted, or, while Windows Terminal holds it, noted for a
/// later run to delete, which is no failure), or registered again to a file CMD Studio did not
/// install, which is not CMD Studio's to remove. One whose registration cannot be read or
/// deleted stays installed, as a failed step.
fn uninstall_font(
    sys: System<'_>,
    font: &FontRecord,
    next: &mut State,
    report: &mut Report,
) -> bool {
    let what = format!("Uninstall font {}", font.family);
    match sys.fonts.registration(&font.value_name) {
        Err(e) => {
            report.fail(
                what,
                "E_MACHINE_READ",
                format!(
                    "its registration {} could not be read ({e}), so it was left installed for \
                     the next revert",
                    font.value_name
                ),
                None,
            );
            return false;
        }
        Ok(Some(named)) if !same_path(Path::new(&named), &font.file) => {
            report.done(
                what,
                format!(
                    "left as it is: its registration {} now names {named}, a file CMD Studio did \
                     not install",
                    font.value_name
                ),
            );
            return true;
        }
        Ok(_) => {}
    }
    // An unload that fails leaves the font in this session only; the delete below then fails
    // too, and the file is noted.
    let _ = sys.fonts.unload(&font.file);
    if let Err(e) = sys.fonts.unregister(&font.value_name) {
        report.fail(
            what,
            "E_FONT_INSTALL",
            format!(
                "its registration could not be deleted ({e}), so it stays installed for the next \
                 revert"
            ),
            Some(font_by_hand(font)),
        );
        return false;
    }
    // The broadcast after the registration changed: `unload` ends with one.
    let _ = sys.fonts.unload(&font.file);
    if sys.fs.remove_file(&font.file).is_err() && sys.fs.exists(&font.file) {
        next.note_pending_delete(font.file.clone());
        report.done(
            what,
            format!(
                "uninstalled; its file {} is still in use (Windows Terminal may hold it), and a \
                 later run deletes it",
                font.file.display()
            ),
        );
    } else {
        report.done(what, "removed");
    }
    true
}

/// The per-user font registration key, which CMD Studio created: deleted once it holds nothing.
fn fonts_key(sys: System<'_>, report: &mut Report) {
    const WHAT: &str = "Remove the per-user Fonts key CMD Studio created";
    if let Err(e) = sys.fonts.delete_registration_key_if_empty() {
        report.fail(WHAT, "E_FONT_INSTALL", e, None);
        return;
    }
    match sys.fonts.registration_key_exists() {
        Ok(false) => report.done(WHAT, "removed"),
        Ok(true) => report.done(WHAT, "kept: other fonts are registered in it"),
        Err(e) => report.fail(
            WHAT,
            "E_MACHINE_READ",
            format!("whether it is gone could not be read ({e})"),
            None,
        ),
    }
}

/// Writes the state.json a revert leaves (see [`revert_steps`]). When that fails, the old one
/// still lists what CMD Studio changed, so the next revert goes over the same steps again,
/// which changes nothing that is back already, and notes a held font file then.
fn save_state(sys: System<'_>, ctx: &Ctx, next: &State, report: &mut Report) {
    let saved = next
        .to_bytes()
        .map_err(|e| (e.code(), e.to_string()))
        .and_then(|bytes| {
            sys.fs
                .write_atomic(&ctx.paths.state_file(), &bytes)
                .map_err(|e| ("E_DATA_WRITE", e.to_string()))
        });
    if let Err((code, why)) = saved {
        report.fail(
            "Save state.json",
            code,
            format!(
                "state.json could not be saved ({why}); it still lists what CMD Studio changed, \
                 so revert again to finish"
            ),
            None,
        );
    }
}

/// Whether a baseline a revert left still holds something it could not put back: an AutoRun
/// left as it is, a settings.json, a font.
fn holds_what_is_not_back(baseline: &Baseline) -> bool {
    baseline.autorun.is_some() || !baseline.wt.is_empty() || !baseline.fonts.is_empty()
}

/// `--cleanup`: the font files left for a later delete are tried once more. Nothing deletes one
/// once CMD Studio is gone, so one still there is a failed step with how to delete it by hand.
fn pending_font_files(sys: System<'_>, ctx: &Ctx, state: &mut State, report: &mut Report) {
    let mut problems = Vec::new();
    delete_pending(sys, ctx, state, &mut problems);
    for f in problems {
        report.fail(
            "Delete the font files left for later",
            f.code,
            f.message,
            None,
        );
    }
    for file in &state.pending_deletes {
        report.fail(
            format!("Delete {}", file.display()),
            "E_FONT_INSTALL",
            "the font is uninstalled, but its file could not be deleted (Windows Terminal may \
             hold it), and nothing deletes it once CMD Studio is gone",
            Some(held_file_by_hand(file)),
        );
    }
}

/// Step 4 for the ProgramData folders: the user's own, worked out from the known folder and the
/// SID, and one state.json names besides, when it is directly in the ProgramData folder and
/// named like CMD Studio's.
fn remove_program_data(sys: System<'_>, recorded: Option<&Path>, report: &mut Report) {
    let host = sys.host;
    let program_data = host.program_data();
    let own = match (program_data.as_ref(), host.user_sid().as_ref()) {
        (Ok(pd), Ok(sid)) => Some(program_data_dir(pd, sid)),
        (Err(e), _) | (_, Err(e)) => {
            report.fail(
                "Remove CMD Studio's ProgramData folder",
                "E_MACHINE_READ",
                format!("where it is could not be read ({e}), so it was left alone"),
                None,
            );
            None
        }
    };
    if let Some(dir) = &own {
        remove_private(sys, dir, report);
    }
    let Some(dir) = recorded else {
        return;
    };
    if own.as_deref().is_some_and(|own| same_path(own, dir)) {
        return;
    }
    let in_program_data = match (&program_data, dir.parent()) {
        (Ok(pd), Some(parent)) => same_path(pd, parent),
        _ => false,
    };
    if in_program_data && named_like_ours(dir) {
        remove_private(sys, dir, report);
    } else {
        report.done(
            format!("Remove {}", dir.display()),
            "left alone: state.json names it as CMD Studio's ProgramData folder, but it is not \
             a folder of that name directly in the ProgramData folder",
        );
    }
}

/// Whether `dir` is named as `program_data_dir` names CMD Studio's folders, in any letter case.
fn named_like_ours(dir: &Path) -> bool {
    let prefix = program_data_dir(Path::new(""), "");
    match (dir.file_name().and_then(|n| n.to_str()), prefix.to_str()) {
        (Some(name), Some(prefix)) => {
            name.len() > prefix.len()
                && name
                    .to_ascii_lowercase()
                    .starts_with(&prefix.to_ascii_lowercase())
        }
        _ => false,
    }
}

/// Deletes `dir` only when it is private to this user (spec §5.4 step 3): a folder that someone
/// else may change, or that is a link to another place, is left alone, and the step says why.
fn remove_private(sys: System<'_>, dir: &Path, report: &mut Report) {
    let what = format!("Remove {}", dir.display());
    match sys.host.private_dir_state(dir) {
        Ok(DirState::Absent) => {}
        Ok(DirState::Private) => match sys.fs.remove_dir_all(dir) {
            Ok(()) => report.done(what, "removed"),
            Err(e) => report.fail(what, "E_DATA_WRITE", e, Some(folder_by_hand(dir))),
        },
        Ok(DirState::Unsafe(why)) => report.done(what, format!("left alone: {why}")),
        Err(e) => report.fail(
            what,
            "E_MACHINE_READ",
            format!("who may change it could not be read ({e}), so it was left alone"),
            None,
        ),
    }
}

/// Whether `path`, in the data folder, is one of the records `--cleanup` keeps while something
/// is not put back: state.json, a damaged state.json set aside, the restore points.
fn is_record(ctx: &Ctx, path: &Path) -> bool {
    is_damaged_copy(path)
        || same_path(path, &ctx.paths.state_file())
        || same_path(path, &ctx.paths.snapshots())
}

/// Whether `path` is a damaged state.json that a revert set aside (`state.json.damaged-<ms>`).
fn is_damaged_copy(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.to_ascii_lowercase().starts_with("state.json.damaged-"))
}

/// What on disk says the records may hold the only copy of the user's values, looked for anew
/// by every `--cleanup`, so that a later one gives the same answer: a damaged state.json set
/// aside, a restore point set aside or that cannot be read, the record of a rollback that cannot
/// be read. Its code, when there is such a thing; a listing that fails counts as one.
fn records_in_doubt(sys: System<'_>, ctx: &Ctx) -> Option<&'static str> {
    let damaged_copy = sys
        .fs
        .list_dir(&ctx.paths.data)
        .is_ok_and(|entries| entries.iter().any(|e| is_damaged_copy(&e.path)));
    if damaged_copy {
        return Some("E_STATE_INVALID");
    }
    let root = ctx.paths.snapshots();
    let store = SnapshotStore::new(sys.fs, &root);
    let ids = match store.ids() {
        Ok(ids) => ids,
        Err(f) => return Some(f.code),
    };
    match store.unreadable() {
        Ok(found) if !found.is_empty() => return Some("E_SNAPSHOT_READ"),
        Ok(_) => {}
        Err(f) => return Some(f.code),
    }
    ids.iter().find_map(|id| {
        if store.is_set_aside(id) {
            Some("E_SNAPSHOT_READ")
        } else {
            store.incomplete(id).err().map(|f| f.code)
        }
    })
}

/// Step 4 for the data folder: every entry but the lock file this run holds, `themes\` unless
/// asked, and, with `keep_records`, the records (see [`is_record`]). Entries are deleted one by
/// one, so the held lock file stops nothing; what cannot be deleted is a failed step.
fn remove_data(
    sys: System<'_>,
    ctx: &Ctx,
    remove_themes: bool,
    keep_records: bool,
    report: &mut Report,
) {
    let data = &ctx.paths.data;
    let entries = match sys.fs.list_dir(data) {
        Ok(entries) => entries,
        Err(e) => {
            report.fail(
                format!("Delete {}", data.display()),
                "E_MACHINE_READ",
                format!("what it holds could not be listed ({e}), so nothing in it was deleted"),
                Some(folder_by_hand(data)),
            );
            return;
        }
    };
    let (lock, themes) = (ctx.paths.lock_file(), ctx.paths.themes());
    for entry in &entries {
        let kept = same_path(&entry.path, &lock)
            || (!remove_themes && same_path(&entry.path, &themes))
            || (keep_records && is_record(ctx, &entry.path));
        if !kept {
            remove_entry(sys, entry, report);
        }
    }
}

fn remove_entry(sys: System<'_>, entry: &DirEntry, report: &mut Report) {
    let removed = if entry.is_dir {
        sys.fs.remove_dir_all(&entry.path)
    } else {
        sys.fs.remove_file(&entry.path)
    };
    if let Err(e) = removed {
        report.fail(
            format!("Delete {}", entry.path.display()),
            "E_DATA_WRITE",
            e,
            Some(folder_by_hand(&entry.path)),
        );
    }
}

/// After the lock is let go: the lock file, then the data folder when nothing is left in it
/// (`themes\`, the records, or what could not be deleted, keeps it).
fn remove_lock_and_folder(sys: System<'_>, ctx: &Ctx, report: &mut Report) {
    let lock = ctx.paths.lock_file();
    if let Err(e) = sys.fs.remove_file(&lock) {
        report.fail(
            format!("Delete {}", lock.display()),
            "E_DATA_WRITE",
            e,
            Some(folder_by_hand(&lock)),
        );
    }
    let data = &ctx.paths.data;
    match sys.fs.list_dir(data) {
        Ok(left) if left.is_empty() => {
            if let Err(e) = sys.fs.remove_dir(data) {
                report.fail(
                    format!("Delete {}", data.display()),
                    "E_DATA_WRITE",
                    e,
                    Some(folder_by_hand(data)),
                );
            }
        }
        Ok(_) => {}
        Err(e) => report.fail(
            format!("Delete {}", data.display()),
            "E_MACHINE_READ",
            format!("whether anything is left in it could not be listed ({e}), so it stays"),
            Some(folder_by_hand(data)),
        ),
    }
}

/// Taking CMD Studio's part out of AutoRun by hand, when it is in `command`.
fn autorun_by_hand(command: &str, profile: &str) -> FixByHand {
    let key = COMMAND_PROCESSOR_KEY;
    FixByHand {
        en: format!(
            "In Registry Editor, open {key} and look at the AutoRun value. Take out only CMD \
             Studio's part, the command that starts Clink with --profile \"{profile}\", and keep \
             everything else as it is. It is in: {command}"
        ),
        th: format!(
            "เปิด Registry Editor (regedit) ไปที่ {key} แล้วดูค่า AutoRun \
             ให้เอาออกเฉพาะส่วนของ CMD Studio คือคำสั่งที่เริ่ม Clink ด้วย --profile \"{profile}\" \
             และเก็บส่วนอื่นไว้ตามเดิม ส่วนนี้อยู่ใน: {command}"
        ),
    }
}

/// A settings.json left as it is: what CMD Studio set in it, to take out by hand, after `why`.
/// Each key goes back to what it held before CMD Studio, unless the user has changed it since.
fn settings_by_hand(file: &WtFileBaseline, why_en: &str, why_th: &str) -> FixByHand {
    let mut profile_en = Vec::new();
    let mut profile_th = Vec::new();
    let mut top_en = Vec::new();
    let mut top_th = Vec::new();
    for original in &file.originals {
        let (name, top) = match &original.key {
            WtKey::Scheme => continue,
            WtKey::DefaultProfile => ("\"defaultProfile\"".to_string(), true),
            WtKey::Profile { key } => (format!("\"{key}\""), false),
            WtKey::ProfileSub { key, sub } => (format!("\"{key}\".\"{sub}\""), false),
        };
        let (en, th) = match &original.original {
            Was::Absent => (format!("delete {name}"), format!("ลบ {name}")),
            Was::Present { value } => (
                format!("set {name} to {value}"),
                format!("ตั้ง {name} เป็น {value}"),
            ),
        };
        if top {
            top_en.push(en);
            top_th.push(th);
        } else {
            profile_en.push(en);
            profile_th.push(th);
        }
    }
    let path = file.settings.display();
    let guid = CMD_PROFILE_GUID;
    let mut en = format!(
        "{why_en} By hand, in {path}: delete the color scheme named \"CMD Studio\", and put back \
         each key below that still holds what CMD Studio set."
    );
    let mut th = format!(
        "{why_th} ให้แก้ด้วยมือในไฟล์ {path}: ลบชุดสี (scheme) ชื่อ \"CMD Studio\" \
         และคืนค่าแต่ละคีย์ข้างล่างที่ยังเป็นค่าที่ CMD Studio ตั้งไว้"
    );
    if !profile_en.is_empty() {
        en.push_str(&format!(
            " In the Command Prompt profile (guid {guid}): {}.",
            profile_en.join("; ")
        ));
        th.push_str(&format!(
            " ในโปรไฟล์ Command Prompt (guid {guid}): {}",
            profile_th.join("; ")
        ));
    }
    if !top_en.is_empty() {
        en.push_str(&format!(" At the top level: {}.", top_en.join("; ")));
        th.push_str(&format!(" ที่ระดับบนสุด: {}", top_th.join("; ")));
    }
    FixByHand { en, th }
}

/// A font file nothing will delete.
fn held_file_by_hand(file: &Path) -> FixByHand {
    let file = file.display();
    FixByHand {
        en: format!(
            "Close Windows Terminal, then delete {file}; if Windows says the file is in use, \
             sign out and back in first."
        ),
        th: format!(
            "ปิด Windows Terminal แล้วลบไฟล์ {file} \
             ถ้า Windows บอกว่าไฟล์ถูกใช้อยู่ ให้ออกจากระบบแล้วเข้าใหม่ก่อน"
        ),
    }
}

/// A font whose registration CMD Studio could not delete.
fn font_by_hand(font: &FontRecord) -> FixByHand {
    let value = &font.value_name;
    let file = held_file_by_hand(&font.file);
    FixByHand {
        en: format!(
            "In Registry Editor, open {FONTS_KEY} and delete the value {value}. Then: {}",
            file.en
        ),
        th: format!(
            "เปิด Registry Editor (regedit) ไปที่ {FONTS_KEY} แล้วลบค่า {value} จากนั้น{}",
            file.th
        ),
    }
}

/// A file or folder CMD Studio could not delete.
fn folder_by_hand(path: &Path) -> FixByHand {
    let path = path.display();
    FixByHand {
        en: format!("Close every cmd window and Windows Terminal, then delete {path}."),
        th: format!("ปิดหน้าต่าง cmd และ Windows Terminal ทุกหน้าต่าง แล้วลบ {path}"),
    }
}

/// state.json and the restore points, kept because not everything is back.
fn records_by_hand(data: &Path) -> FixByHand {
    let data = data.display();
    FixByHand {
        en: format!(
            "CMD Studio kept state.json and its restore points in {data}: they record what your \
             settings were before CMD Studio. Do by hand what the failed steps above say, or \
             install CMD Studio again and use Revert everything; then delete {data}."
        ),
        th: format!(
            "CMD Studio เก็บไฟล์ state.json และจุดย้อนกลับไว้ใน {data} \
             เพราะเป็นที่บันทึกค่าเดิมก่อนใช้ CMD Studio ให้ทำตามขั้นที่ล้มเหลวข้างบนด้วยมือ \
             หรือติดตั้ง CMD Studio ใหม่แล้วใช้ \"ย้อนทุกอย่าง\" จากนั้นจึงลบ {data}"
        ),
    }
}

/// A data folder kept for the state.json of a newer CMD Studio.
fn newer_state_by_hand(data: &Path) -> FixByHand {
    let data = data.display();
    FixByHand {
        en: format!(
            "Install the newer CMD Studio that wrote its state.json and take out CMD Studio's \
             changes with it; or, if you do not need what it recorded, delete {data} by hand."
        ),
        th: format!(
            "ติดตั้ง CMD Studio รุ่นใหม่กว่าที่เขียนไฟล์ state.json นี้ แล้วใช้รุ่นนั้นเอาสิ่งที่ CMD Studio \
             เปลี่ยนออก หรือถ้าไม่ต้องการค่าที่บันทึกไว้แล้ว ให้ลบ {data} เอง"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::testing::{machine, request, SETTINGS, WT};
    use crate::actions::{prepare_apply, run_apply, undo, UndoResult};
    use crate::adapters::{AutoRun, Fonts, Fs, RegValue, REG_EXPAND_SZ, REG_SZ};
    use crate::model::Target;
    use crate::plan::{autorun_entry, ClinkLaunch, NERD_FONT_FAMILY};
    use crate::presets::terracotta;
    use std::path::PathBuf;

    fn apply(m: &crate::adapters::fake::FakeMachine, ctx: &Ctx, target: Target) {
        let p = prepare_apply(m.system(), ctx, &terracotta(), &request(target)).unwrap();
        assert_eq!(run_apply(m.system(), ctx, p), Outcome::Committed);
    }

    fn later(ctx: &Ctx, ms: u64) -> Ctx {
        Ctx {
            now_ms: ctx.now_ms + ms,
            ..ctx.clone()
        }
    }

    #[test]
    fn revert_all_on_an_untouched_machine_gives_the_original_back() {
        let (m, ctx) = machine();
        let conda = RegValue::from_text(REG_EXPAND_SZ, r#""%USERPROFILE%\conda_hook.bat""#);
        m.autorun.write_user(&conda).unwrap();
        apply(&m, &ctx, Target::All);
        let report = revert_all(m.system(), &later(&ctx, 5)).unwrap();
        assert!(report.ok(), "{report:?}");
        assert_eq!(m.autorun.read_user().unwrap(), Some(conda));
        assert_eq!(m.fs.file(WT).unwrap(), SETTINGS.as_bytes());
        assert!(!m
            .fonts
            .families()
            .unwrap()
            .contains(&NERD_FONT_FAMILY.to_string()));
        assert_eq!(
            undo(m.system(), &later(&ctx, 6)).unwrap().result,
            UndoResult::NothingToUndo
        );
    }

    #[test]
    fn revert_all_keeps_what_the_user_changed_after_us() {
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        let ours = m.autorun.read_user().unwrap().unwrap().text().unwrap();
        m.autorun
            .write_user(&RegValue::from_text(REG_SZ, &format!("{ours} & echo mine")))
            .unwrap();
        let text = String::from_utf8(m.fs.file(WT).unwrap())
            .unwrap()
            .replace("\"opacity\": 96", "\"opacity\": 70");
        m.fs.put(WT, text);
        revert_all(m.system(), &later(&ctx, 5)).unwrap();
        assert_eq!(
            m.autorun.read_user().unwrap().unwrap().text().as_deref(),
            Some("echo mine")
        );
        let after = String::from_utf8(m.fs.file(WT).unwrap()).unwrap();
        assert!(after.contains("\"opacity\": 70") && !after.contains("CMD Studio"));
    }

    #[test]
    fn autorun_rule_table() {
        let paths = Paths {
            data: PathBuf::from(r"C:\d"),
        };
        let ours = autorun_entry(&ClinkLaunch::Direct(paths.clink_exe()), &paths).unwrap();
        // our entry as builds before ruling P59 wrote it, without the parentheses
        let older = r#"if exist "C:\d\runtime\clink-1.9.0\clink_x64.exe" "C:\d\runtime\clink-1.9.0\clink_x64.exe" inject --autorun --quiet --profile "C:\d\clink-profile""#;
        let rec = |t: &str| AutoRunRecord {
            key_existed: true,
            value: Some(RegValueRecord::from_reg(&RegValue::from_text(REG_SZ, t))),
        };
        let absent = AutoRunRecord {
            key_existed: true,
            value: None,
        };
        let foreign = r#""C:\clink\clink.bat" inject --autorun"#;
        // A foreign Clink that the apply took out comes back, in front.
        assert_eq!(
            autorun_after_revert(Some(&ours), Some(&rec(foreign)), &paths),
            Ok(Some(Some(foreign.to_string())))
        );
        assert_eq!(
            autorun_after_revert(
                Some(&format!("{ours} & echo mine")),
                Some(&rec(&format!("{foreign} & echo old"))),
                &paths
            ),
            Ok(Some(Some(format!("{foreign} & echo mine"))))
        );
        // ...but only one that is not there already.
        assert_eq!(
            autorun_after_revert(
                Some(&format!("{ours} & {foreign}")),
                Some(&rec(foreign)),
                &paths
            ),
            Ok(Some(Some(foreign.to_string())))
        );
        // Not there before CMD Studio, nothing left: the value goes. There before: it stays,
        // with what is left.
        assert_eq!(
            autorun_after_revert(Some(&ours), Some(&absent), &paths),
            Ok(Some(None))
        );
        assert_eq!(
            autorun_after_revert(Some(&ours), Some(&rec("echo old")), &paths),
            Ok(Some(Some(String::new())))
        );
        // Only our parts leave, the older form too, and every other byte stays, whatever the
        // value was before.
        let changed = format!("{ours} & echo a ^& b & {older} & echo z");
        assert_eq!(
            autorun_after_revert(Some(&changed), Some(&absent), &paths),
            Ok(Some(Some("echo a ^& b & echo z".into())))
        );
        assert_eq!(
            autorun_after_revert(Some(&format!("{ours} & echo mine")), Some(&absent), &paths),
            Ok(Some(Some("echo mine".into())))
        );
        // No state at all: still only our part leaves; nothing of ours means no change, even
        // for a value that is empty.
        assert_eq!(
            autorun_after_revert(Some(&ours), None, &paths),
            Ok(Some(None))
        );
        assert_eq!(
            autorun_after_revert(Some("echo hi"), None, &paths),
            Ok(None)
        );
        assert_eq!(autorun_after_revert(Some(" "), None, &paths), Ok(None));
        assert_eq!(autorun_after_revert(None, None, &paths), Ok(None));
        // A command that holds our profile path without being one of our entries is left for
        // a hand edit: cutting it out would take other text with it.
        let err = autorun_after_revert(Some(&format!("echo ^& {ours}")), None, &paths).unwrap_err();
        assert_eq!(err.code(), "E_AUTORUN_WRITE");
    }

    #[test]
    fn an_autorun_that_needs_a_hand_edit_is_reported_and_left_alone() {
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        let ours = m.autorun.read_user().unwrap().unwrap().text().unwrap();
        let tangled = RegValue::from_text(REG_SZ, &format!("echo ^& {ours}"));
        m.autorun.write_user(&tangled).unwrap();
        let report = revert_all(m.system(), &later(&ctx, 5)).unwrap();
        assert!(!report.ok());
        assert!(
            report
                .steps
                .iter()
                .any(|s| !s.ok && s.detail.starts_with("E_AUTORUN_WRITE")),
            "{report:?}"
        );
        assert_eq!(m.autorun.read_user().unwrap(), Some(tangled));
        // the rest is still put back
        assert_eq!(m.fs.file(WT).unwrap(), SETTINGS.as_bytes());
    }

    #[test]
    fn cleanup_deletes_data_but_keeps_themes_unless_asked() {
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        m.fs.put(ctx.paths.themes().join("mine.cmdtheme"), b"{}");
        let report = cleanup(m.system(), &later(&ctx, 5), false).unwrap();
        assert!(report.ok(), "{report:?}");
        assert!(m.fs.exists(&ctx.paths.themes().join("mine.cmdtheme")));
        assert!(!m.fs.exists(&ctx.paths.runtime_root()));
        assert!(!m.fs.exists(&ctx.paths.state_file()));
        cleanup(m.system(), &later(&ctx, 6), true).unwrap();
        assert!(!m.fs.exists(&ctx.paths.data));
        assert_eq!(m.autorun.read_user().unwrap(), None);
    }

    #[test]
    fn cleanup_removes_a_private_program_data_folder_and_leaves_a_foreign_one() {
        let (m, ctx) = machine();
        let pd = PathBuf::from(r"C:\ProgramData\ai.resincore.cmdstudio-S-1-5-21-1-2-3-1001");
        m.fs.put(pd.join(r"clink-1.9.34\clink_x64.exe"), b"x");
        m.host
            .set_dir_state(&pd, crate::adapters::DirState::Private);
        cleanup(m.system(), &ctx, false).unwrap();
        assert!(!m.fs.exists(&pd));
        m.fs.put(pd.join("trap.exe"), b"x");
        m.host.set_dir_state(
            &pd,
            crate::adapters::DirState::Unsafe("owned by S-1-5-21-9".into()),
        );
        let report = cleanup(m.system(), &later(&ctx, 1), false).unwrap();
        assert!(m.fs.exists(&pd.join("trap.exe")));
        assert!(report
            .steps
            .iter()
            .any(|s| s.detail.starts_with("left alone")));
    }

    #[test]
    fn a_damaged_state_file_still_removes_our_autorun_part() {
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        m.fs.put(ctx.paths.state_file(), b"garbage");
        let report = revert_all(m.system(), &later(&ctx, 5)).unwrap();
        assert!(!report.ok());
        assert_eq!(m.autorun.read_user().unwrap(), None);
    }

    // ---- The rulings that postdate the brief, and the shipped APIs. ----

    use crate::actions::{pending_incomplete, Prepared};
    use crate::adapters::fake::{FakeAutoRun, FakeFonts, FakeFs, FakeMachine};
    use crate::adapters::{AdapterError, DirEntry, DirState, FsLock};
    use crate::exec::COMMAND_PROCESSOR_KEY;
    use crate::resources::NERD_FONT_FILE;
    use crate::snapshot::SnapshotStore;
    use crate::state::{Baseline, RegValueRecord, State};
    use std::path::Path;

    fn apply_id(m: &FakeMachine, ctx: &Ctx, target: Target) -> String {
        let p = prepare_apply(m.system(), ctx, &terracotta(), &request(target)).unwrap();
        let id = p.manifest.id.clone();
        assert_eq!(run_apply(m.system(), ctx, p), Outcome::Committed);
        id
    }

    fn state(m: &FakeMachine, ctx: &Ctx) -> State {
        State::load(&m.fs, &ctx.paths.state_file()).unwrap()
    }

    fn font_file(m: &FakeMachine) -> PathBuf {
        m.fonts.user_fonts_dir().join(NERD_FONT_FILE)
    }

    fn font_value() -> String {
        format!("{NERD_FONT_FAMILY} (TrueType)")
    }

    fn registered(m: &FakeMachine) -> bool {
        m.fonts.registration(&font_value()).unwrap().is_some()
    }

    fn own_program_data(m: &FakeMachine) -> PathBuf {
        program_data_dir(&m.host.program_data, &m.host.sid)
    }

    /// Every file on the fake disk but the lock file, AutoRun and the fonts.
    fn everything(m: &FakeMachine, ctx: &Ctx) -> impl PartialEq + std::fmt::Debug {
        let mut files = m.fs.snapshot().0;
        files.remove(&ctx.paths.lock_file());
        (files, m.autorun.snapshot(), m.fonts.snapshot())
    }

    /// The failed steps with `code`. Every failed step of the report carries its code, and its
    /// detail starts with it.
    fn failed<'a>(report: &'a Report, code: &str) -> Vec<&'a Step> {
        for s in report.steps.iter().filter(|s| !s.ok) {
            let c = s
                .code
                .unwrap_or_else(|| panic!("a failed step without a code: {s:?}"));
            assert!(s.detail.starts_with(&format!("{c}: ")), "{s:?}");
        }
        report
            .steps
            .iter()
            .filter(|s| !s.ok && s.code == Some(code))
            .collect()
    }

    /// An apply cut off after `done` ops, as a crash leaves it.
    fn crash_after(m: &FakeMachine, ctx: &Ctx, p: &Prepared, done: usize) {
        use crate::txn::{Executor, Journal, JournalStore, Snapshot, Stage};
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

    /// An apply whose rollback could not put init.lua back; its id.
    fn apply_that_cannot_roll_back(m: &FakeMachine, ctx: &Ctx) -> String {
        let init = ctx.paths.generated().join("init.lua");
        if m.fs.file(&init).is_none() {
            m.fs.put(&init, b"old");
        }
        m.fs.fail_writes_to(&init);
        let p = prepare_apply(m.system(), ctx, &terracotta(), &request(Target::All)).unwrap();
        let id = p.manifest.id.clone();
        assert!(matches!(
            run_apply(m.system(), ctx, p),
            Outcome::RollbackIncomplete { .. }
        ));
        m.fs.stop_failing(&init);
        id
    }

    /// An apply whose rollback could not put settings.json back, on a machine CMD Studio had
    /// not changed before: it fails at AutoRun after settings.json was written, and neither can
    /// be written again. Its state.json was never written, so no baseline knows the file. Its id.
    fn apply_whose_settings_cannot_go_back(m: &FakeMachine, ctx: &Ctx) -> String {
        m.autorun.fail_writes();
        let fs = HookedFs {
            fs: &m.fs,
            list_fails: None,
            writes_once: Some(PathBuf::from(WT)),
            wrote: std::cell::Cell::new(false),
        };
        let sys = System {
            fs: &fs,
            ..m.system()
        };
        let p = prepare_apply(sys, ctx, &terracotta(), &request(Target::All)).unwrap();
        let id = p.manifest.id.clone();
        let out = run_apply(sys, ctx, p);
        assert!(
            matches!(&out, Outcome::RollbackIncomplete { failed, .. } if failed.len() == 2),
            "{out:?}"
        );
        assert!(String::from_utf8(m.fs.file(WT).unwrap())
            .unwrap()
            .contains("CMD Studio"));
        id
    }

    /// The fake disk, but `list_fails` cannot be listed, and `writes_once` can be written once.
    struct HookedFs<'a> {
        fs: &'a FakeFs,
        list_fails: Option<PathBuf>,
        writes_once: Option<PathBuf>,
        wrote: std::cell::Cell<bool>,
    }

    impl<'a> HookedFs<'a> {
        fn unlistable(fs: &'a FakeFs, dir: &Path) -> Self {
            HookedFs {
                fs,
                list_fails: Some(dir.to_path_buf()),
                writes_once: None,
                wrote: std::cell::Cell::new(false),
            }
        }
    }

    impl Fs for HookedFs<'_> {
        fn read(&self, path: &Path) -> Result<Option<Vec<u8>>, AdapterError> {
            self.fs.read(path)
        }
        fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<(), AdapterError> {
            if self.writes_once.as_deref() == Some(path) && self.wrote.replace(true) {
                return Err(AdapterError::new(
                    path.display().to_string(),
                    "denied (test)",
                ));
            }
            self.fs.write_atomic(path, bytes)
        }
        fn remove_file(&self, path: &Path) -> Result<(), AdapterError> {
            self.fs.remove_file(path)
        }
        fn exists(&self, path: &Path) -> bool {
            self.fs.exists(path)
        }
        fn is_dir(&self, path: &Path) -> bool {
            self.fs.is_dir(path)
        }
        fn create_dir_all(&self, path: &Path) -> Result<(), AdapterError> {
            self.fs.create_dir_all(path)
        }
        fn remove_dir(&self, path: &Path) -> Result<(), AdapterError> {
            self.fs.remove_dir(path)
        }
        fn remove_dir_all(&self, path: &Path) -> Result<(), AdapterError> {
            self.fs.remove_dir_all(path)
        }
        fn rename(&self, from: &Path, to: &Path) -> Result<(), AdapterError> {
            self.fs.rename(from, to)
        }
        fn list_dir(&self, path: &Path) -> Result<Vec<DirEntry>, AdapterError> {
            if self.list_fails.as_deref() == Some(path) {
                return Err(AdapterError::new(
                    path.display().to_string(),
                    "denied (test)",
                ));
            }
            self.fs.list_dir(path)
        }
        fn try_lock(&self, path: &Path) -> Result<Option<FsLock>, AdapterError> {
            self.fs.try_lock(path)
        }
    }

    /// The fake AutoRun, but the user's value cannot be read.
    struct AutoRunReadFails<'a>(&'a FakeAutoRun);

    impl AutoRun for AutoRunReadFails<'_> {
        fn user_key_exists(&self) -> Result<bool, AdapterError> {
            self.0.user_key_exists()
        }
        fn read_user(&self) -> Result<Option<RegValue>, AdapterError> {
            Err(AdapterError::new("AutoRun", "denied (test)"))
        }
        fn write_user(&self, value: &RegValue) -> Result<(), AdapterError> {
            self.0.write_user(value)
        }
        fn delete_user(&self) -> Result<(), AdapterError> {
            self.0.delete_user()
        }
        fn delete_user_key_if_empty(&self) -> Result<(), AdapterError> {
            self.0.delete_user_key_if_empty()
        }
        fn read_machine(&self) -> Result<Option<RegValue>, AdapterError> {
            self.0.read_machine()
        }
    }

    /// The fake fonts, but no registration can be deleted.
    struct UnregisterFails<'a>(&'a FakeFonts);

    impl Fonts for UnregisterFails<'_> {
        fn families(&self) -> Result<Vec<String>, AdapterError> {
            self.0.families()
        }
        fn user_fonts_dir(&self) -> PathBuf {
            self.0.user_fonts_dir()
        }
        fn registration(&self, value_name: &str) -> Result<Option<String>, AdapterError> {
            self.0.registration(value_name)
        }
        fn register(&self, value_name: &str, file: &str) -> Result<(), AdapterError> {
            self.0.register(value_name, file)
        }
        fn unregister(&self, value_name: &str) -> Result<(), AdapterError> {
            Err(AdapterError::new(value_name, "denied (test)"))
        }
        fn registration_key_exists(&self) -> Result<bool, AdapterError> {
            self.0.registration_key_exists()
        }
        fn delete_registration_key_if_empty(&self) -> Result<(), AdapterError> {
            self.0.delete_registration_key_if_empty()
        }
        fn load(&self, file: &Path) -> Result<(), AdapterError> {
            self.0.load(file)
        }
        fn unload(&self, file: &Path) -> Result<(), AdapterError> {
            self.0.unload(file)
        }
    }

    #[test]
    fn revert_all_and_cleanup_are_busy_while_another_run_holds_the_lock() {
        // One lock for every command that changes the machine; a second run fails fast.
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        let p = prepare_apply(
            m.system(),
            &later(&ctx, 1),
            &terracotta(),
            &request(Target::Wt),
        )
        .unwrap();
        let before = everything(&m, &ctx);
        let errors = [
            revert_all(m.system(), &later(&ctx, 2)).unwrap_err(),
            cleanup(m.system(), &later(&ctx, 3), true).unwrap_err(),
        ];
        for e in errors {
            assert_eq!(e.code(), "E_BUSY", "{e:?}");
        }
        assert_eq!(everything(&m, &ctx), before, "nothing changed");
        drop(p);
        let report = revert_all(m.system(), &later(&ctx, 4)).unwrap();
        assert!(report.ok(), "{report:?}");
        assert!(
            m.fs.try_lock(&ctx.paths.lock_file()).unwrap().is_some(),
            "revert_all lets the lock go"
        );
        // --cleanup lets it go too, and then takes the lock file away with the folder.
        let report = cleanup(m.system(), &later(&ctx, 5), true).unwrap();
        assert!(report.ok(), "{report:?}");
        assert!(!m.fs.exists(&ctx.paths.data));
    }

    #[test]
    fn a_crash_that_cannot_be_checked_yet_stops_revert_all_and_cleanup_before_anything_changes() {
        // The crashed transaction's restore point may be the only copy of the user's values:
        // nothing is reverted over it, and nothing is deleted, until it can be read.
        let (m, ctx) = machine();
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        let set_autorun = p
            .ops
            .iter()
            .position(|o| matches!(o, Op::SetAutoRun { .. }))
            .unwrap();
        crash_after(&m, &ctx, &p, set_autorun + 1);
        let id = p.manifest.id.clone();
        drop(p);
        let journal = ctx.paths.snapshots().join(&id).join("journal.json");
        m.fs.fail_reads_of(&journal);
        let crashed = everything(&m, &ctx);
        let errors = [
            revert_all(m.system(), &later(&ctx, 1)).unwrap_err(),
            cleanup(m.system(), &later(&ctx, 2), true).unwrap_err(),
        ];
        for e in errors {
            assert_eq!(e.code(), "E_SNAPSHOT_READ", "{e:?}");
            assert!(e.to_string().contains("try again"), "{e}");
        }
        assert_eq!(everything(&m, &ctx), crashed, "nothing changed");

        // A journal that will never be read again is set aside, and --cleanup goes on.
        m.fs.stop_failing(&journal);
        m.fs.put(&journal, b"{ damaged");
        let report = cleanup(m.system(), &later(&ctx, 3), true).unwrap();
        assert!(
            report.start.unreadable.iter().any(|u| u.point.id == id),
            "{report:?}"
        );
        // Nothing says which of its changes ran. Our AutoRun entry is taken out, but its
        // settings.json and font are still CMD Studio's, and no baseline knows them: the user
        // is told what is left and how to take it out, and the records stay.
        assert_eq!(m.autorun.read_user().unwrap(), None);
        let fails = failed(&report, "E_ROLLBACK_INCOMPLETE");
        assert!(
            fails.iter().any(|s| s.fix.as_ref().is_some_and(|f| {
                f.en.contains(WT) && f.en.contains("CMD Studio") && f.th.contains(WT)
            })),
            "{report:?}"
        );
        assert!(m.fs.exists(&ctx.paths.snapshots().join(&id)), "{report:?}");
        assert!(m.fs.exists(&ctx.paths.state_file()));
        assert!(!m.fs.exists(&ctx.paths.runtime_root()));
    }

    #[test]
    fn a_state_file_that_is_newer_or_cannot_be_read_stops_the_revert_and_is_never_touched() {
        let newer = br#"{"version": 2, "applied": null, "baseline": {}, "futureField": 1}"#;
        for newer_one in [true, false] {
            let (m, ctx) = machine();
            apply(&m, &ctx, Target::All);
            let file = ctx.paths.state_file();
            if newer_one {
                m.fs.put(&file, newer);
            } else {
                m.fs.fail_reads_of(&file);
            }
            let before = everything(&m, &ctx);
            let e = revert_all(m.system(), &later(&ctx, 1)).unwrap_err();
            assert_eq!(e.code(), "E_STATE_INVALID", "{e:?}");
            assert_eq!(
                everything(&m, &ctx),
                before,
                "nothing changed, state.json included"
            );

            // --cleanup's revert stops the same way. It still removes the ProgramData folder,
            // which it works out without state.json; it keeps the whole data folder while it
            // holds a state.json from a newer CMD Studio, and state.json with the restore points
            // when it cannot be read.
            let pd = own_program_data(&m);
            m.fs.put(pd.join("clink_x64.exe"), b"x");
            m.host.set_dir_state(&pd, DirState::Private);
            let autorun = m.autorun.snapshot();
            let report = cleanup(m.system(), &later(&ctx, 2), true).unwrap();
            assert!(!report.ok());
            assert!(!failed(&report, "E_STATE_INVALID").is_empty(), "{report:?}");
            assert!(!m.fs.exists(&pd), "{report:?}");
            assert_eq!(m.autorun.snapshot(), autorun, "nothing reverted");
            if newer_one {
                assert_eq!(m.fs.file(&file).as_deref(), Some(&newer[..]));
                assert!(
                    m.fs.exists(&ctx.paths.runtime_root()),
                    "the data folder is kept"
                );
            } else {
                // It may hold the only record of the user's settings: it stays, with the
                // restore points, and the rest of the data folder goes.
                assert!(m.fs.exists(&file), "{report:?}");
                assert!(m.fs.exists(&ctx.paths.snapshots()), "{report:?}");
                assert!(!m.fs.exists(&ctx.paths.runtime_root()), "{report:?}");
                assert!(
                    report.steps.iter().any(|s| !s.ok
                        && s.code == Some("E_STATE_INVALID")
                        && s.fix.as_ref().is_some_and(|f| f.en.contains("state.json"))),
                    "{report:?}"
                );
            }
        }
    }

    #[test]
    fn without_a_state_file_only_our_autorun_entries_go() {
        let (m, ctx) = machine();
        let ours = autorun_entry(&ClinkLaunch::Direct(ctx.paths.clink_exe()), &ctx.paths).unwrap();
        m.autorun
            .write_user(&RegValue::from_text(REG_SZ, &format!("{ours} & echo x")))
            .unwrap();
        let report = revert_all(m.system(), &ctx).unwrap();
        assert!(report.ok(), "{report:?}");
        assert_eq!(
            m.autorun.read_user().unwrap().unwrap().text().as_deref(),
            Some("echo x")
        );
        assert_eq!(m.fs.file(WT).unwrap(), SETTINGS.as_bytes());
        assert_eq!(state(&m, &ctx), State::default());
    }

    #[test]
    fn a_damaged_state_file_is_set_aside_and_one_that_cannot_be_is_never_written_over() {
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        let file = ctx.paths.state_file();
        m.fs.put(&file, b"garbage");
        // The rename that sets it aside fails.
        m.fs.fail_removes_of(&file);
        let before = everything(&m, &ctx);
        let e = revert_all(m.system(), &later(&ctx, 1)).unwrap_err();
        assert_eq!(e.code(), "E_STATE_INVALID", "{e:?}");
        assert!(e.to_string().contains("state.json.damaged-"), "{e}");
        assert_eq!(everything(&m, &ctx), before, "nothing changed");

        // Once it can be renamed, it is kept beside a fresh state.json, and the report says so.
        m.fs.stop_failing(&file);
        let ctx2 = later(&ctx, 2);
        let report = revert_all(m.system(), &ctx2).unwrap();
        let aside = ctx
            .paths
            .data
            .join(format!("state.json.damaged-{}", ctx2.now_ms));
        assert_eq!(m.fs.file(&aside).as_deref(), Some(&b"garbage"[..]));
        let [step] = failed(&report, "E_STATE_INVALID")[..] else {
            panic!("{report:?}")
        };
        assert!(
            step.detail.contains(&aside.display().to_string()),
            "{step:?}"
        );
        assert_eq!(state(&m, &ctx).applied, None);
    }

    #[test]
    fn a_font_file_still_in_use_is_noted_for_later_and_earlier_notes_are_kept() {
        let (m, ctx) = machine();
        let earlier =
            PathBuf::from(r"C:\Users\tester\AppData\Local\Microsoft\Windows\Fonts\old.ttf");
        m.fs.put(&earlier, b"old");
        m.fs.fail_removes_of(&earlier);
        apply(&m, &ctx, Target::All);
        let mut s = state(&m, &ctx);
        s.note_pending_delete(earlier.clone());
        m.fs.put(ctx.paths.state_file(), s.to_bytes().unwrap());
        let file = font_file(&m);
        m.fs.fail_removes_of(&file);
        let report = revert_all(m.system(), &later(&ctx, 1)).unwrap();
        // Uninstalled; a file Windows Terminal still holds is no failure: a later run deletes it.
        assert!(report.ok(), "{report:?}");
        assert!(!registered(&m));
        assert!(m.fs.exists(&file));
        assert_eq!(
            state(&m, &ctx).pending_deletes,
            vec![earlier.clone(), file.clone()]
        );
        // Free now: the next revert deletes both.
        m.fs.stop_failing(&file);
        m.fs.stop_failing(&earlier);
        revert_all(m.system(), &later(&ctx, 2)).unwrap();
        assert!(!m.fs.exists(&file) && !m.fs.exists(&earlier));
        assert!(state(&m, &ctx).pending_deletes.is_empty());
    }

    #[test]
    fn cleanup_deletes_the_font_files_left_for_later_and_reports_one_still_held() {
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        let file = font_file(&m);
        m.fs.fail_removes_of(&file);
        let report = cleanup(m.system(), &later(&ctx, 1), true).unwrap();
        assert!(!registered(&m));
        assert!(m.fs.exists(&file));
        // Nothing deletes it once CMD Studio is gone: the user is told how, in both languages.
        let [step] = failed(&report, "E_FONT_INSTALL")[..] else {
            panic!("{report:?}")
        };
        let fix = step.fix.as_ref().unwrap();
        let name = file.display().to_string();
        assert!(fix.en.contains(&name) && fix.th.contains(&name), "{fix:?}");
        assert!(!report.ok());

        // One that is free by then is deleted.
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        let file = font_file(&m);
        m.fs.fail_removes_of(&file);
        revert_all(m.system(), &later(&ctx, 1)).unwrap();
        assert_eq!(state(&m, &ctx).pending_deletes, vec![file.clone()]);
        m.fs.stop_failing(&file);
        let report = cleanup(m.system(), &later(&ctx, 2), true).unwrap();
        assert!(report.ok(), "{report:?}");
        assert!(!m.fs.exists(&file));
    }

    #[test]
    fn the_registry_keys_cmd_studio_created_go_once_empty_and_others_stay() {
        let (m, ctx) = machine();
        m.autorun.set_key_exists(false);
        m.fonts.set_key_exists(false);
        apply(&m, &ctx, Target::All);
        assert!(m.autorun.user_key_exists().unwrap());
        assert!(m.fonts.registration_key_exists().unwrap());
        let report = revert_all(m.system(), &later(&ctx, 1)).unwrap();
        assert!(report.ok(), "{report:?}");
        assert!(!m.autorun.user_key_exists().unwrap());
        assert!(!m.fonts.registration_key_exists().unwrap());

        // They were there before: they stay, even empty.
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        let report = revert_all(m.system(), &later(&ctx, 1)).unwrap();
        assert!(report.ok(), "{report:?}");
        assert!(m.autorun.user_key_exists().unwrap());
        assert!(m.fonts.registration_key_exists().unwrap());
    }

    #[test]
    fn a_font_that_cannot_be_uninstalled_stays_in_the_baseline_for_the_next_revert() {
        // Its registration cannot be read.
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        m.fonts.fail_registration_of(&font_value());
        let report = revert_all(m.system(), &later(&ctx, 1)).unwrap();
        let [step] = failed(&report, "E_MACHINE_READ")[..] else {
            panic!("{report:?}")
        };
        assert!(step.what.contains(NERD_FONT_FAMILY), "{step:?}");
        assert_eq!(state(&m, &ctx).baseline.fonts.len(), 1);
        assert!(m.fs.exists(&font_file(&m)));

        // Its registration cannot be deleted. The next revert that can, uninstalls it.
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        let fonts = UnregisterFails(&m.fonts);
        let sys = System {
            fonts: &fonts,
            ..m.system()
        };
        let report = revert_all(sys, &later(&ctx, 1)).unwrap();
        let [step] = failed(&report, "E_FONT_INSTALL")[..] else {
            panic!("{report:?}")
        };
        let fix = step.fix.as_ref().unwrap();
        assert!(
            fix.en.contains(&font_value()) && fix.th.contains(&font_value()),
            "{fix:?}"
        );
        assert!(registered(&m));
        assert_eq!(state(&m, &ctx).baseline.fonts.len(), 1);
        let report = revert_all(m.system(), &later(&ctx, 2)).unwrap();
        assert!(report.ok(), "{report:?}");
        assert!(!registered(&m) && !m.fs.exists(&font_file(&m)));
        assert!(state(&m, &ctx).baseline.fonts.is_empty());
    }

    #[test]
    fn a_font_registered_again_to_another_file_is_not_cmd_studios_to_remove() {
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        let theirs = r"C:\Users\tester\AppData\Local\Microsoft\Windows\Fonts\CaskaydiaCove_0.ttf";
        m.fonts.register(&font_value(), theirs).unwrap();
        let report = revert_all(m.system(), &later(&ctx, 1)).unwrap();
        assert!(report.ok(), "{report:?}");
        assert_eq!(
            m.fonts.registration(&font_value()).unwrap().as_deref(),
            Some(theirs)
        );
        assert!(state(&m, &ctx).baseline.fonts.is_empty());
    }

    #[test]
    fn a_settings_file_that_is_gone_damaged_or_unreadable_is_left_as_it_is_and_said() {
        // Gone: nothing to put back, and it is not made again.
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        m.fs.remove_file(Path::new(WT)).unwrap();
        let report = revert_all(m.system(), &later(&ctx, 1)).unwrap();
        assert!(report.ok(), "{report:?}");
        assert_eq!(m.fs.file(WT), None);
        assert!(state(&m, &ctx).baseline.wt.is_empty());

        for (damage, code) in [
            ("not json", "E_WT_PARSE"),
            ("not utf-8", "E_WT_PARSE"),
            ("unreadable", "E_MACHINE_READ"),
        ] {
            let (m, ctx) = machine();
            apply(&m, &ctx, Target::All);
            match damage {
                "not json" => m.fs.put(WT, b"{ \"profiles\": [ "),
                "not utf-8" => m.fs.put(WT, b"{ \xff }"),
                _ => m.fs.fail_reads_of(WT),
            }
            let file_before = m.fs.file(WT);
            let report = revert_all(m.system(), &later(&ctx, 1)).unwrap();
            let [step] = failed(&report, code)[..] else {
                panic!("{damage}: {report:?}")
            };
            assert!(step.what.contains("settings.json"), "{step:?}");
            if code == "E_WT_PARSE" {
                assert!(step.fix.is_some(), "{step:?}");
            }
            assert_eq!(m.fs.file(WT), file_before, "{damage}: left as it is");
            // The rest is put back, and the file's baseline is kept for when it can be read.
            assert_eq!(m.autorun.read_user().unwrap(), None, "{damage}");
            assert_eq!(state(&m, &ctx).baseline.wt.len(), 1, "{damage}");
        }
    }

    /// The fake AutoRun, but its first change fails, as when another program has the key open
    /// for a moment; the rollback's write goes through.
    struct FirstWriteFails<'a> {
        autorun: &'a FakeAutoRun,
        failed: std::cell::Cell<bool>,
    }

    impl<'a> FirstWriteFails<'a> {
        fn new(autorun: &'a FakeAutoRun) -> Self {
            FirstWriteFails {
                autorun,
                failed: std::cell::Cell::new(false),
            }
        }
        fn first(&self) -> Result<(), AdapterError> {
            if self.failed.replace(true) {
                Ok(())
            } else {
                Err(AdapterError::new("AutoRun", "in use (test)"))
            }
        }
    }

    impl AutoRun for FirstWriteFails<'_> {
        fn user_key_exists(&self) -> Result<bool, AdapterError> {
            self.autorun.user_key_exists()
        }
        fn read_user(&self) -> Result<Option<RegValue>, AdapterError> {
            self.autorun.read_user()
        }
        fn write_user(&self, value: &RegValue) -> Result<(), AdapterError> {
            self.first()?;
            self.autorun.write_user(value)
        }
        fn delete_user(&self) -> Result<(), AdapterError> {
            self.first()?;
            self.autorun.delete_user()
        }
        fn delete_user_key_if_empty(&self) -> Result<(), AdapterError> {
            self.autorun.delete_user_key_if_empty()
        }
        fn read_machine(&self) -> Result<Option<RegValue>, AdapterError> {
            self.autorun.read_machine()
        }
    }

    #[test]
    fn a_revert_that_fails_is_rolled_back_whole_and_cleanup_deletes_nothing() {
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        let parts = |m: &FakeMachine| {
            (
                m.fs.file(WT),
                m.fs.file(ctx.paths.state_file()),
                m.autorun.snapshot(),
                m.fonts.snapshot(),
            )
        };
        let before = parts(&m);
        let autorun = FirstWriteFails::new(&m.autorun);
        let sys = System {
            autorun: &autorun,
            ..m.system()
        };
        let report = revert_all(sys, &later(&ctx, 1)).unwrap();
        assert!(
            matches!(
                report.outcome,
                Some(Outcome::RolledBack {
                    code: "E_AUTORUN_WRITE",
                    ..
                })
            ),
            "{report:?}"
        );
        assert!(!report.ok());
        assert_eq!(
            parts(&m),
            before,
            "settings, state.json and fonts as they were"
        );
        let autorun = FirstWriteFails::new(&m.autorun);
        let sys = System {
            autorun: &autorun,
            ..m.system()
        };
        let report = cleanup(sys, &later(&ctx, 2), true).unwrap();
        assert!(
            matches!(report.outcome, Some(Outcome::RolledBack { .. })),
            "{report:?}"
        );
        assert!(m.fs.exists(&ctx.paths.runtime_root()));
        assert_eq!(parts(&m), before);
    }

    #[test]
    fn a_revert_whose_rollback_cannot_finish_says_what_to_fix() {
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        m.fs.fail_writes_to(WT);
        let report = revert_all(m.system(), &later(&ctx, 1)).unwrap();
        assert!(
            matches!(report.outcome, Some(Outcome::RollbackIncomplete { .. })),
            "{report:?}"
        );
        assert!(!report.ok());
        let record = pending_incomplete(m.system(), &ctx).unwrap().unwrap();
        assert_eq!(record.steps[0].target, format!("file {WT}"));
    }

    #[test]
    fn after_a_revert_no_undo_brings_a_theme_back() {
        // Two applies: both are marked.
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        apply(&m, &later(&ctx, 1), Target::Wt);
        revert_all(m.system(), &later(&ctx, 2)).unwrap();
        assert_eq!(
            undo(m.system(), &later(&ctx, 3)).unwrap().result,
            UndoResult::NothingToUndo
        );

        // Nothing left to change (the user put it back by hand): still marked.
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        m.fs.put(WT, SETTINGS);
        m.autorun.delete_user().unwrap();
        let report = revert_all(m.system(), &later(&ctx, 1)).unwrap();
        assert!(report.ok(), "{report:?}");
        assert_eq!(
            undo(m.system(), &later(&ctx, 2)).unwrap().result,
            UndoResult::NothingToUndo
        );

        // A mark that cannot be written is said: an undo would still go over that apply.
        let (m, ctx) = machine();
        let id = apply_id(&m, &ctx, Target::All);
        m.fs.fail_writes_to(ctx.paths.snapshots().join(&id).join("manifest.json"));
        let report = revert_all(m.system(), &later(&ctx, 1)).unwrap();
        assert_eq!(report.outcome, Some(Outcome::Committed), "{report:?}");
        let [step] = failed(&report, "E_SNAPSHOT_WRITE")[..] else {
            panic!("{report:?}")
        };
        assert!(step.detail.contains(&id), "{step:?}");
        assert_eq!(
            m.fs.file(WT).unwrap(),
            SETTINGS.as_bytes(),
            "the revert stands"
        );
    }

    #[test]
    fn an_apply_whose_restore_point_cannot_be_read_now_is_said_to_stay_unmarked() {
        // Locked now: it cannot be marked, and an undo would go over it once it can be read.
        let (m, ctx) = machine();
        let id = apply_id(&m, &ctx, Target::All);
        m.fs.fail_reads_of(ctx.paths.snapshots().join(&id).join("manifest.json"));
        let report = revert_all(m.system(), &later(&ctx, 1)).unwrap();
        assert_eq!(report.outcome, Some(Outcome::Committed), "{report:?}");
        let [step] = failed(&report, "E_SNAPSHOT_READ")[..] else {
            panic!("{report:?}")
        };
        assert!(step.detail.contains(&id), "{step:?}");
        assert_eq!(m.fs.file(WT).unwrap(), SETTINGS.as_bytes());

        // Damaged for good: the start sets it aside, and an undo never goes over it.
        let (m, ctx) = machine();
        let id = apply_id(&m, &ctx, Target::All);
        m.fs.put(ctx.paths.snapshots().join(&id).join("manifest.json"), b"{");
        let report = revert_all(m.system(), &later(&ctx, 1)).unwrap();
        assert!(report.ok(), "{report:?}");
    }

    #[test]
    fn cleanup_deletes_around_its_own_lock_and_says_what_another_program_holds() {
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        // A cmd window that runs Clink holds its program.
        let held = ctx.paths.clink_exe();
        let in_use = m.fs.try_lock(&held).unwrap().unwrap();
        let report = cleanup(m.system(), &later(&ctx, 1), true).unwrap();
        assert!(!report.ok());
        let [step] = failed(&report, "E_DATA_WRITE")[..] else {
            panic!("{report:?}")
        };
        assert!(step.fix.is_some(), "{step:?}");
        // Everything else is gone, the lock file and state.json included; what is left is in
        // the runtime folder, the held file with it.
        let left: Vec<PathBuf> =
            m.fs.snapshot()
                .0
                .into_keys()
                .filter(|p| p.starts_with(&ctx.paths.data))
                .collect();
        assert!(left.contains(&held), "{left:?}");
        assert!(
            left.iter().all(|p| p.starts_with(ctx.paths.runtime_root())),
            "{left:?}"
        );
        drop(in_use);
        let report = cleanup(m.system(), &later(&ctx, 2), true).unwrap();
        assert!(report.ok(), "{report:?}");
        assert!(!m.fs.exists(&ctx.paths.data));
    }

    #[test]
    fn cleanup_keeps_the_records_while_a_rollback_record_is_not_put_back() {
        // The record's fix copies settings.json back from its restore point, and nothing else
        // knows what the file held: --cleanup keeps state.json and the restore points.
        let (m, ctx) = machine();
        let id = apply_whose_settings_cannot_go_back(&m, &ctx);
        let report = cleanup(m.system(), &later(&ctx, 1), true).unwrap();
        assert!(!report.ok());
        let settings_step = report
            .start
            .incomplete
            .as_ref()
            .unwrap()
            .steps
            .iter()
            .find(|s| s.target.contains("Windows Terminal"))
            .unwrap()
            .op_index;
        let payload =
            SnapshotStore::new(&m.fs, &ctx.paths.snapshots()).payload_path(&id, settings_step);
        let fails = failed(&report, "E_ROLLBACK_INCOMPLETE");
        assert!(
            fails
                .iter()
                .any(|s| s.fix.as_ref().is_some_and(|f| f.en.contains("copy"))),
            "the record's own fix: {report:?}"
        );
        assert!(m.fs.exists(&ctx.paths.state_file()));
        assert!(m.fs.exists(&ctx.paths.snapshots()));
        assert!(
            m.fs.exists(&payload),
            "the copy its fix names is still there"
        );
        assert!(!m.fs.exists(&ctx.paths.runtime_root()));
        assert!(!m.fs.exists(&ctx.paths.lock_file()));
    }

    #[test]
    fn a_program_data_folder_state_json_names_goes_only_when_it_is_private_and_named_like_ours() {
        let other_sid = PathBuf::from(r"C:\ProgramData\ai.resincore.cmdstudio-S-1-5-21-9-9-9-1001");
        let unrelated = PathBuf::from(r"C:\ProgramData\SomeoneElse");
        let cases = [
            (&other_sid, DirState::Private, true),
            (
                &other_sid,
                DirState::Unsafe("owned by S-1-5-21-7".into()),
                false,
            ),
            (&unrelated, DirState::Private, false),
        ];
        for (dir, dir_state, goes) in cases {
            let (m, ctx) = machine();
            m.fs.put(dir.join("clink_x64.exe"), b"x");
            m.host.set_dir_state(dir, dir_state.clone());
            let s = State {
                baseline: Baseline {
                    program_data_dir: Some(dir.clone()),
                    ..Baseline::default()
                },
                ..State::default()
            };
            m.fs.put(ctx.paths.state_file(), s.to_bytes().unwrap());
            let report = cleanup(m.system(), &ctx, true).unwrap();
            assert_eq!(!m.fs.exists(dir), goes, "{dir:?} {dir_state:?}: {report:?}");
            if !goes {
                assert!(
                    report.steps.iter().any(|s| s.ok
                        && s.what.contains(&dir.display().to_string())
                        && s.detail.starts_with("left alone")),
                    "{report:?}"
                );
            }
        }
    }

    #[test]
    fn cleanup_that_cannot_list_the_data_folder_says_so() {
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        let fs = HookedFs::unlistable(&m.fs, &ctx.paths.data);
        let sys = System {
            fs: &fs,
            ..m.system()
        };
        let report = cleanup(sys, &later(&ctx, 1), true).unwrap();
        assert!(!report.ok());
        // Both listings say so: the one before deleting, and the one after, for the folder.
        assert_eq!(failed(&report, "E_MACHINE_READ").len(), 2, "{report:?}");
        assert!(m.fs.exists(&ctx.paths.runtime_root()));
    }

    #[test]
    fn an_original_autorun_that_cannot_be_read_back_is_said_and_autorun_is_never_deleted() {
        let paths = Paths {
            data: PathBuf::from(r"C:\d"),
        };
        let ours = autorun_entry(&ClinkLaunch::Direct(paths.clink_exe()), &paths).unwrap();
        let undecodable = [
            RegValueRecord {
                kind: REG_SZ,
                data_hex: "zz".into(),
            },
            RegValueRecord::from_reg(&RegValue {
                kind: 4,
                bytes: vec![1, 0, 0, 0],
            }),
        ];
        for value in undecodable {
            let original = AutoRunRecord {
                key_existed: true,
                value: Some(value),
            };
            // The value was there before CMD Studio: our part goes, and it stays, empty.
            assert_eq!(
                autorun_after_revert(Some(&ours), Some(&original), &paths),
                Ok(Some(Some(String::new())))
            );
            // With nothing of ours in AutoRun there is nothing to take out.
            assert_eq!(
                autorun_after_revert(None, Some(&original), &paths),
                Ok(None)
            );
        }

        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        let mut s = state(&m, &ctx);
        s.baseline.autorun = Some(AutoRunRecord {
            key_existed: true,
            value: Some(RegValueRecord {
                kind: REG_SZ,
                data_hex: "zz".into(),
            }),
        });
        m.fs.put(ctx.paths.state_file(), s.to_bytes().unwrap());
        let report = revert_all(m.system(), &later(&ctx, 1)).unwrap();
        // Our part goes. AutoRun existed before CMD Studio, so it stays, empty: never deleted.
        assert_eq!(
            m.autorun.read_user().unwrap().unwrap().text().as_deref(),
            Some("")
        );
        // What it held cannot be read back, so whether another program's Clink should come
        // back is not known: said.
        let [step] = failed(&report, "E_STATE_INVALID")[..] else {
            panic!("{report:?}")
        };
        assert!(step.what.contains("AutoRun"), "{step:?}");
        assert_eq!(m.fs.file(WT).unwrap(), SETTINGS.as_bytes());
    }

    #[test]
    fn an_autorun_value_cmd_runs_that_is_not_text_is_left_for_a_hand_edit() {
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        // REG_SZ, but not UTF-16: an unpaired surrogate.
        let broken = RegValue {
            kind: REG_SZ,
            bytes: vec![0x00, 0xD8, 0x41, 0x00, 0, 0],
        };
        m.autorun.write_user(&broken).unwrap();
        let report = revert_all(m.system(), &later(&ctx, 1)).unwrap();
        let [step] = failed(&report, "E_AUTORUN_WRITE")[..] else {
            panic!("{report:?}")
        };
        assert!(
            step.fix
                .as_ref()
                .unwrap()
                .en
                .contains(COMMAND_PROCESSOR_KEY),
            "{step:?}"
        );
        assert_eq!(m.autorun.read_user().unwrap(), Some(broken));

        // A value of another type is no command, and cmd does not run it: it stays, unsaid.
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        let dword = RegValue {
            kind: 4,
            bytes: vec![1, 0, 0, 0],
        };
        m.autorun.write_user(&dword).unwrap();
        let report = revert_all(m.system(), &later(&ctx, 1)).unwrap();
        assert!(report.ok(), "{report:?}");
        assert_eq!(m.autorun.read_user().unwrap(), Some(dword));
    }

    #[test]
    fn an_autorun_that_cannot_be_read_is_said_and_the_rest_is_still_put_back() {
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        let ours = m.autorun.read_user().unwrap();
        let autorun = AutoRunReadFails(&m.autorun);
        let sys = System {
            autorun: &autorun,
            ..m.system()
        };
        let report = revert_all(sys, &later(&ctx, 1)).unwrap();
        let [step] = failed(&report, "E_MACHINE_READ")[..] else {
            panic!("{report:?}")
        };
        assert!(step.what.contains("AutoRun"), "{step:?}");
        assert_eq!(m.autorun.read_user().unwrap(), ours);
        assert_eq!(m.fs.file(WT).unwrap(), SETTINGS.as_bytes());
        assert!(
            state(&m, &ctx).baseline.autorun.is_some(),
            "kept for the next revert"
        );
    }

    #[test]
    fn what_the_start_found_is_a_failed_step_with_its_code() {
        // The record of a rollback that did not finish cannot be read: said, not dropped.
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        let id = apply_that_cannot_roll_back(&m, &later(&ctx, 1));
        m.fs.fail_reads_of(ctx.paths.snapshots().join(&id).join("incomplete.json"));
        let report = revert_all(m.system(), &later(&ctx, 2)).unwrap();
        assert!(
            failed(&report, "E_SNAPSHOT_READ")
                .iter()
                .any(|s| s.detail.contains("incomplete.json")),
            "{report:?}"
        );
        assert!(report.start.problems.is_empty(), "moved into the steps");
        assert!(!report.ok());
    }

    // ---- Only our part of AutoRun goes (spec §6.3 step 1). ----

    fn autorun_text(m: &FakeMachine) -> Option<String> {
        m.autorun.read_user().unwrap().and_then(|v| v.text())
    }

    #[test]
    fn a_second_apply_after_the_user_changed_autorun_loses_nothing_of_theirs() {
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        let ours = autorun_text(&m).unwrap();
        m.autorun
            .write_user(&RegValue::from_text(REG_SZ, &format!("echo mine & {ours}")))
            .unwrap();
        apply(&m, &later(&ctx, 1), Target::All);
        assert_eq!(autorun_text(&m), Some(format!("{ours} & echo mine")));
        let report = revert_all(m.system(), &later(&ctx, 2)).unwrap();
        assert!(report.ok(), "{report:?}");
        assert_eq!(autorun_text(&m).as_deref(), Some("echo mine"));
    }

    #[test]
    fn a_hand_edit_after_a_refused_revert_is_kept_through_the_next_apply() {
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        let ours = autorun_text(&m).unwrap();
        m.autorun
            .write_user(&RegValue::from_text(REG_SZ, &format!("echo ^& {ours}")))
            .unwrap();
        let report = revert_all(m.system(), &later(&ctx, 1)).unwrap();
        assert!(!failed(&report, "E_AUTORUN_WRITE").is_empty(), "{report:?}");
        // The user takes our part out by hand, keeping a command of their own, and applies
        // again.
        m.autorun
            .write_user(&RegValue::from_text(REG_SZ, "echo mine"))
            .unwrap();
        apply(&m, &later(&ctx, 2), Target::All);
        let report = revert_all(m.system(), &later(&ctx, 3)).unwrap();
        assert!(report.ok(), "{report:?}");
        assert_eq!(autorun_text(&m).as_deref(), Some("echo mine"));
    }

    #[test]
    fn a_clink_cmd_studio_turned_off_comes_back_and_what_the_user_added_stays() {
        let foreign = r#""C:\clink\clink.bat" inject --autorun"#;
        let original = format!(r#"{foreign} & "%USERPROFILE%\conda_hook.bat""#);
        let replace = crate::plan::Request {
            replace_foreign_clink: true,
            ..request(Target::All)
        };
        for user_adds in [false, true] {
            let (m, ctx) = machine();
            let before = RegValue::from_text(REG_EXPAND_SZ, &original);
            m.autorun.write_user(&before).unwrap();
            let p = prepare_apply(m.system(), &ctx, &terracotta(), &replace).unwrap();
            assert_eq!(run_apply(m.system(), &ctx, p), Outcome::Committed);
            let now = autorun_text(&m).unwrap();
            assert!(!now.contains("clink.bat"), "turned off: {now}");
            if user_adds {
                m.autorun
                    .write_user(&RegValue::from_text(
                        REG_EXPAND_SZ,
                        &format!("{now} & echo mine"),
                    ))
                    .unwrap();
            }
            let report = revert_all(m.system(), &later(&ctx, 1)).unwrap();
            assert!(report.ok(), "{report:?}");
            let after = m.autorun.read_user().unwrap().unwrap();
            assert_eq!(after.kind, REG_EXPAND_SZ, "the value stays expandable");
            let want = if user_adds {
                format!("{original} & echo mine")
            } else {
                original.clone()
            };
            assert_eq!(after.text(), Some(want), "user_adds: {user_adds}");
            if !user_adds {
                assert_eq!(after, before, "byte for byte");
            }
        }
    }

    // ---- --cleanup never deletes the only copy of the user's values. ----

    #[test]
    fn cleanup_keeps_the_records_when_a_settings_file_is_left_as_it_is() {
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        // Windows Terminal, or a scanner, has it open for a moment.
        m.fs.fail_reads_of(WT);
        let report = cleanup(m.system(), &later(&ctx, 1), true).unwrap();
        assert!(!report.ok());
        let [step] = failed(&report, "E_MACHINE_READ")[..] else {
            panic!("{report:?}")
        };
        // Its fix says what CMD Studio set there, so it can be taken out by hand.
        let fix = step.fix.as_ref().unwrap();
        for text in [&fix.en, &fix.th] {
            assert!(
                text.contains("\"CMD Studio\"")
                    && text.contains("\"opacity\"")
                    && text.contains(crate::generate::wt::CMD_PROFILE_GUID),
                "{text}"
            );
        }
        // What knows the file's values stays, and says so.
        assert!(m.fs.exists(&ctx.paths.state_file()));
        assert!(m.fs.exists(&ctx.paths.snapshots()));
        assert_eq!(state(&m, &ctx).baseline.wt.len(), 1);
        assert!(
            failed(&report, "E_ROLLBACK_INCOMPLETE")
                .iter()
                .any(|s| s.fix.as_ref().is_some_and(|f| f.en.contains("state.json"))),
            "{report:?}"
        );
        assert!(!m.fs.exists(&ctx.paths.runtime_root()));
        assert!(!m.fs.exists(&ctx.paths.lock_file()));
    }

    #[test]
    fn cleanup_keeps_a_damaged_state_file_it_set_aside() {
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        m.fs.put(ctx.paths.state_file(), b"garbage");
        let ctx1 = later(&ctx, 1);
        let report = cleanup(m.system(), &ctx1, true).unwrap();
        assert!(!report.ok());
        let aside = ctx
            .paths
            .data
            .join(format!("state.json.damaged-{}", ctx1.now_ms));
        assert_eq!(
            m.fs.file(&aside).as_deref(),
            Some(&b"garbage"[..]),
            "{report:?}"
        );
        assert!(m.fs.exists(&ctx.paths.snapshots()));
        assert!(!m.fs.exists(&ctx.paths.runtime_root()));
        let kept = failed(&report, "E_STATE_INVALID");
        assert!(
            kept.iter()
                .any(|s| s.what.starts_with("Delete") && s.fix.is_some()),
            "{report:?}"
        );
        // The next --cleanup finds the damaged copy on disk and gives the same answer.
        let again = cleanup(m.system(), &later(&ctx, 2), true).unwrap();
        assert_same_records_kept(&m, &ctx, &report, &again);
        assert!(m.fs.exists(&aside));
    }

    // ---- A rollback record is never dropped by a revert. ----

    #[test]
    fn a_rollback_record_the_revert_cannot_clear_is_listed_and_kept_until_it_is() {
        let (m, ctx) = machine();
        let id = apply_whose_settings_cannot_go_back(&m, &ctx);
        assert_eq!(
            pending_incomplete(m.system(), &ctx).unwrap().map(|r| r.id),
            Some(id)
        );
        for n in 1..=2 {
            let report = revert_all(m.system(), &later(&ctx, n)).unwrap();
            assert!(!report.ok(), "{report:?}");
            // Its settings.json step is listed with the record's own fix. AutoRun holds nothing
            // of ours, so that step is put back.
            let [step] = failed(&report, "E_ROLLBACK_INCOMPLETE")[..] else {
                panic!("{report:?}")
            };
            assert!(step.what.contains(WT), "{step:?}");
            assert!(
                step.fix
                    .as_ref()
                    .is_some_and(|f| f.en.contains("copy") && !f.th.is_empty()),
                "{step:?}"
            );
            if n == 1 {
                assert!(
                    report
                        .steps
                        .iter()
                        .any(|s| s.ok && s.what.contains("AutoRun")),
                    "{report:?}"
                );
            }
            // Later starts still find it.
            let record = pending_incomplete(m.system(), &ctx).unwrap().unwrap();
            assert!(
                record.steps.iter().any(|s| s.target.contains(WT)),
                "{record:?}"
            );
        }
        // Put back by hand: the next revert finds it so, says so, and the record ends.
        m.fs.put(WT, SETTINGS);
        let report = revert_all(m.system(), &later(&ctx, 3)).unwrap();
        assert!(report.ok(), "{report:?}");
        assert!(
            report.steps.iter().any(|s| s.ok && s.what.contains(WT)),
            "{report:?}"
        );
        assert_eq!(pending_incomplete(m.system(), &ctx).unwrap(), None);
    }

    #[test]
    fn a_rollback_record_the_revert_clears_is_said_so_and_ends() {
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        let id = apply_that_cannot_roll_back(&m, &later(&ctx, 1));
        let report = revert_all(m.system(), &later(&ctx, 2)).unwrap();
        assert!(report.ok(), "{report:?}");
        let init = ctx.paths.generated().join("init.lua").display().to_string();
        assert!(
            report
                .steps
                .iter()
                .any(|s| s.ok && s.what.contains(&init) && s.what.contains(&id)),
            "{report:?}"
        );
        assert_eq!(pending_incomplete(m.system(), &ctx).unwrap(), None);
        let again = revert_all(m.system(), &later(&ctx, 3)).unwrap();
        assert!(again.ok(), "{again:?}");
        assert_eq!(again.start.incomplete, None);
    }

    #[test]
    fn a_recorded_program_data_folder_must_sit_in_program_data_and_be_named_like_ours() {
        let cases = [
            (
                r"C:\ProgramData\AI.RESINCORE.CMDSTUDIO-S-1-5-21-9-9-9-1001",
                true,
            ),
            (
                r"C:\Elsewhere\ai.resincore.cmdstudio-S-1-5-21-9-9-9-1001",
                false,
            ),
            (r"C:\ProgramData\ai.resincore.cmdstudio-", false),
        ];
        for (dir, goes) in cases {
            let dir = PathBuf::from(dir);
            let (m, ctx) = machine();
            m.fs.put(dir.join("clink_x64.exe"), b"x");
            m.host.set_dir_state(&dir, DirState::Private);
            let s = State {
                baseline: Baseline {
                    program_data_dir: Some(dir.clone()),
                    ..Baseline::default()
                },
                ..State::default()
            };
            m.fs.put(ctx.paths.state_file(), s.to_bytes().unwrap());
            let report = cleanup(m.system(), &ctx, true).unwrap();
            assert_eq!(!m.fs.exists(&dir), goes, "{dir:?}: {report:?}");
            if !goes {
                assert!(
                    report
                        .steps
                        .iter()
                        .any(|s| s.ok && s.detail.starts_with("left alone")),
                    "{report:?}"
                );
            }
        }
    }

    /// --cleanup kept state.json and the restore points, said so with how to finish, and
    /// deleted the rest.
    fn assert_records_kept(m: &FakeMachine, ctx: &Ctx, report: &Report) {
        assert!(!report.ok(), "{report:?}");
        assert!(m.fs.exists(&ctx.paths.state_file()), "{report:?}");
        assert!(m.fs.exists(&ctx.paths.snapshots()), "{report:?}");
        let data = format!("Delete {}", ctx.paths.data.display());
        assert!(
            report.steps.iter().any(|s| !s.ok
                && s.what == data
                && s.fix
                    .as_ref()
                    .is_some_and(|f| f.en.contains("state.json") && f.th.contains("state.json"))),
            "{report:?}"
        );
        assert!(!m.fs.exists(&ctx.paths.runtime_root()), "{report:?}");
        assert!(!m.fs.exists(&ctx.paths.lock_file()), "{report:?}");
    }

    /// The failed step that says what --cleanup kept in the data folder.
    fn kept_step(ctx: &Ctx, report: &Report) -> Step {
        let data = format!("Delete {}", ctx.paths.data.display());
        report
            .steps
            .iter()
            .find(|s| !s.ok && s.what == data)
            .cloned()
            .unwrap_or_else(|| panic!("nothing kept: {report:?}"))
    }

    /// A later --cleanup kept the records too, and said the same.
    fn assert_same_records_kept(m: &FakeMachine, ctx: &Ctx, first: &Report, again: &Report) {
        assert_records_kept(m, ctx, again);
        assert_eq!(kept_step(ctx, again), kept_step(ctx, first));
    }

    /// Where restore point `id` keeps the prior state of its first op that `pick` chooses.
    fn payload_of(ctx: &Ctx, m: &FakeMachine, id: &str, pick: fn(&Op) -> bool) -> PathBuf {
        let root = ctx.paths.snapshots();
        let store = SnapshotStore::new(&m.fs, &root);
        let index = store
            .journal(id)
            .unwrap()
            .ops
            .iter()
            .position(pick)
            .unwrap();
        store.payload_path(id, index)
    }

    #[test]
    fn cleanup_keeps_the_records_while_a_font_or_autorun_is_left_as_it_is() {
        // A font whose registration cannot be read stays installed.
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        m.fonts.fail_registration_of(&font_value());
        let report = cleanup(m.system(), &later(&ctx, 1), true).unwrap();
        assert!(
            m.fonts.snapshot().0.contains_key(&font_value()),
            "still registered"
        );
        assert_records_kept(&m, &ctx, &report);
        assert_eq!(state(&m, &ctx).baseline.fonts.len(), 1);

        // AutoRun needs a hand edit.
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        let ours = autorun_text(&m).unwrap();
        m.autorun
            .write_user(&RegValue::from_text(REG_SZ, &format!("echo ^& {ours}")))
            .unwrap();
        let report = cleanup(m.system(), &later(&ctx, 1), true).unwrap();
        assert!(!failed(&report, "E_AUTORUN_WRITE").is_empty(), "{report:?}");
        assert_records_kept(&m, &ctx, &report);
        assert!(state(&m, &ctx).baseline.autorun.is_some());
    }

    #[test]
    fn cleanup_keeps_the_records_when_the_autorun_original_cannot_be_read_back() {
        // The apply turned off another Clink. state.json's copy of what AutoRun held is not
        // hex, but the apply's restore point still holds it whole.
        let (m, ctx) = machine();
        m.autorun
            .write_user(&RegValue::from_text(
                REG_SZ,
                r#""C:\clink\clink.bat" inject --autorun"#,
            ))
            .unwrap();
        let replace = crate::plan::Request {
            replace_foreign_clink: true,
            ..request(Target::All)
        };
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &replace).unwrap();
        let id = p.manifest.id.clone();
        assert_eq!(run_apply(m.system(), &ctx, p), Outcome::Committed);
        let mut s = state(&m, &ctx);
        s.baseline.autorun = Some(AutoRunRecord {
            key_existed: true,
            value: Some(RegValueRecord {
                kind: REG_SZ,
                data_hex: "zz".into(),
            }),
        });
        m.fs.put(ctx.paths.state_file(), s.to_bytes().unwrap());
        let prior = payload_of(&ctx, &m, &id, |o| matches!(o, Op::SetAutoRun { .. }));
        let report = cleanup(m.system(), &later(&ctx, 1), true).unwrap();
        assert_records_kept(&m, &ctx, &report);
        let held: AutoRunRecord = serde_json::from_slice(&m.fs.file(&prior).unwrap()).unwrap();
        assert_eq!(
            held.value
                .and_then(|v| v.to_reg())
                .and_then(|v| v.text())
                .as_deref(),
            Some(r#""C:\clink\clink.bat" inject --autorun"#),
            "the restore point still holds what AutoRun was"
        );
        assert!(
            report.steps.iter().any(|s| !s.ok
                && s.code == Some("E_STATE_INVALID")
                && s.what.starts_with("Delete")),
            "{report:?}"
        );
        // The AutoRun step says where the original is, and what it was.
        let autorun_step = |report: &Report| {
            report
                .steps
                .iter()
                .find(|s| s.code == Some("E_STATE_INVALID") && s.what.contains("AutoRun"))
                .cloned()
                .unwrap_or_else(|| panic!("{report:?}"))
        };
        let step = autorun_step(&report);
        let fix = step.fix.as_ref().unwrap();
        for text in [&fix.en, &fix.th] {
            assert!(
                text.contains(&id)
                    && text.contains(&prior.display().to_string())
                    && text.contains("clink.bat")
                    && text.contains(COMMAND_PROCESSOR_KEY),
                "{text}"
            );
        }
        // state.json keeps the damaged original while it is damaged, so the next --cleanup
        // gives the same answer.
        assert!(state(&m, &ctx).baseline.autorun.is_some());
        let again = cleanup(m.system(), &later(&ctx, 2), true).unwrap();
        assert_same_records_kept(&m, &ctx, &report, &again);
        assert_eq!(autorun_step(&again), step);
        assert!(m.fs.exists(&prior));
    }

    #[test]
    fn cleanup_keeps_the_records_when_the_start_could_not_read_them_all() {
        // The record of a rollback that did not finish is damaged: nothing says what it left,
        // and the restore point holds the only copy of the user's settings.json.
        let (m, ctx) = machine();
        let id = apply_whose_settings_cannot_go_back(&m, &ctx);
        m.fs.put(
            ctx.paths.snapshots().join(&id).join("incomplete.json"),
            b"{ damaged",
        );
        let copy = payload_of(&ctx, &m, &id, |o| matches!(o, Op::WtApply { .. }));
        let report = cleanup(m.system(), &later(&ctx, 1), true).unwrap();
        assert!(!failed(&report, "E_SNAPSHOT_READ").is_empty(), "{report:?}");
        assert_records_kept(&m, &ctx, &report);
        assert_eq!(
            m.fs.file(&copy).as_deref(),
            Some(SETTINGS.as_bytes()),
            "the only copy of the user's settings.json is still there"
        );
        // The next start reads its own newer restore point, but the damaged record is still on
        // disk: the same answer.
        let again = cleanup(m.system(), &later(&ctx, 2), true).unwrap();
        assert_same_records_kept(&m, &ctx, &report, &again);
        assert!(m.fs.exists(&copy));

        // A restore point the start finds damaged, and sets aside.
        let (m, ctx) = machine();
        let id = apply_id(&m, &ctx, Target::All);
        m.fs.put(ctx.paths.snapshots().join(&id).join("manifest.json"), b"{");
        let report = cleanup(m.system(), &later(&ctx, 1), true).unwrap();
        assert!(
            report.start.unreadable.iter().any(|u| u.point.id == id),
            "{report:?}"
        );
        assert_records_kept(&m, &ctx, &report);
        assert!(
            report.steps.iter().any(|s| !s.ok
                && s.code == Some("E_SNAPSHOT_READ")
                && s.what.starts_with("Delete")),
            "{report:?}"
        );
        // Only that start reports it; the next --cleanup finds it set aside on disk.
        let again = cleanup(m.system(), &later(&ctx, 2), true).unwrap();
        assert!(again.start.unreadable.is_empty(), "{again:?}");
        assert_same_records_kept(&m, &ctx, &report, &again);
        assert!(m.fs.exists(&ctx.paths.snapshots().join(&id)));
    }

    #[test]
    fn a_clean_cleanup_still_removes_everything_the_first_time() {
        // A rollback record the revert clears, and an earlier one that a later apply ended,
        // keep nothing.
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        apply_that_cannot_roll_back(&m, &later(&ctx, 1));
        apply(&m, &later(&ctx, 2), Target::Wt);
        apply_that_cannot_roll_back(&m, &later(&ctx, 3));
        let report = cleanup(m.system(), &later(&ctx, 4), true).unwrap();
        assert!(report.ok(), "{report:?}");
        assert!(!m.fs.exists(&ctx.paths.data), "{report:?}");
        assert_eq!(autorun_text(&m), None);
        assert_eq!(m.fs.file(WT).unwrap(), SETTINGS.as_bytes());
    }
}
