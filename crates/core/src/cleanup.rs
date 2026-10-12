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
//! restore point. What could not be done is a failed [`Step`] with its code (spec §5.9) and,
//! where a person can finish it, how, in English and in Thai; it is never reported as done, and
//! nothing of the user's is deleted.
use crate::actions::{
    begin, delete_pending, delete_pending_and_save, next_id, record_incomplete, ActionError, Ctx,
    StartReport,
};
use crate::adapters::{
    program_data_dir, DirEntry, DirState, RegValue, System, REG_EXPAND_SZ, REG_SZ,
};
use crate::exec::{same_path, FixByHand, SystemExecutor, COMMAND_PROCESSOR_KEY, FONTS_KEY};
use crate::plan::autorun::remove_ours;
use crate::plan::{Op, Paths, PlanError};
use crate::snapshot::{Incomplete, Manifest, SnapshotKind, SnapshotStore, KEEP};
use crate::state::{
    AutoRunRecord, AutoRunWritten, Baseline, FontRecord, State, StateError, WtFileBaseline,
};
use crate::txn::{self, Failure, Outcome};
use crate::verify::SystemVerifier;
use crate::wt_edit::{revert, WtError};
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
    /// not kept here: each is a failed step, and so is a record that is pending.
    pub start: StartReport,
    /// The AutoRun and settings.json transaction; `None` when it did not run (nothing to change
    /// and no apply to mark, or the revert stopped before it).
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

    /// A report that begins with what the start could not do and with a rollback that did not
    /// finish, each a failed step: an earlier change may still need a hand.
    fn new(mut start: StartReport) -> Report {
        let mut report = Report::default();
        for f in std::mem::take(&mut start.problems) {
            report.fail("Look at what earlier runs left", f.code, f.message, None);
        }
        if let Some(record) = &start.incomplete {
            report.fail(
                format!("The rollback of restore point {}", record.id),
                "E_ROLLBACK_INCOMPLETE",
                format!(
                    "the rollback of restore point {} did not put everything back ({}: {}); fix \
                     what its record lists by hand",
                    record.id, record.code, record.reason
                ),
                None,
            );
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

/// `value` without CMD Studio's entries, taken out by the planner's own editor
/// (`plan::autorun::remove_ours`), so the planner and the cleanup never disagree about what is
/// ours; `None` when nothing but blanks is left. Fails with `E_AUTORUN_WRITE` when the value
/// can't be edited safely and needs a hand edit.
fn without_ours(value: &str, profile: &str) -> Result<Option<String>, PlanError> {
    let edit = remove_ours(value, profile)?;
    Ok((!edit.value.trim().is_empty()).then_some(edit.value))
}

/// What AutoRun becomes (spec §6.3 step 1). `Ok(None)`: leave it as it is.
/// `Ok(Some(None))`: delete the value. When nobody changed AutoRun since CMD
/// Studio last wrote it, the original comes back (minus any entry of ours);
/// otherwise only our entries leave the current value. A value that can't be
/// edited safely is `E_AUTORUN_WRITE`, and is left for a hand edit; so is one whose original,
/// due back, cannot be read back as text (a record that is not hex, or a value that was not a
/// string): it is never deleted in its place.
pub fn autorun_after_revert(
    current: Option<&str>,
    original: Option<&AutoRunRecord>,
    written: Option<&AutoRunWritten>,
    paths: &Paths,
) -> Result<Option<Option<String>>, PlanError> {
    // The planner refuses a profile path that is not Unicode, so no entry of ours holds one.
    let profile_path = paths.clink_profile();
    let Some(profile) = profile_path.to_str() else {
        return Ok(None);
    };
    let untouched = written.is_some_and(|w| w.value.as_deref() == current);
    let target = match (original, untouched) {
        (Some(orig), true) => match &orig.value {
            None => None,
            Some(record) => match record.to_reg().and_then(|v| v.text()) {
                Some(text) => without_ours(&text, profile)?,
                // What was there before cannot be written back. An empty AutoRun has nothing of
                // ours to take out; anything else is a hand edit, never a delete in its place.
                None => match current {
                    None => return Ok(None),
                    Some(c) => {
                        return Err(PlanError::AutoRunNeedsHandEdit {
                            part: c.to_string(),
                        })
                    }
                },
            },
        },
        _ => match current {
            Some(c) => without_ours(c, profile)?,
            None => None,
        },
    };
    Ok((target.as_deref() != current).then_some(target))
}

/// Undoes everything CMD Studio changed, key by key (the "ย้อนทุกอย่าง" button, spec §4.3),
/// under the lock that apply and undo take; the font files earlier runs left to delete are
/// tried again first. Refused, with nothing changed, while another run holds the lock
/// (`E_BUSY`), while a transaction a crash cut off cannot be checked yet (`E_SNAPSHOT_READ`),
/// and while state.json cannot be read now, is from a newer CMD Studio, or is damaged and
/// cannot be set aside (`E_STATE_INVALID`): such a state.json is never renamed or written over.
pub fn revert_all(sys: System<'_>, ctx: &Ctx) -> Result<Report, ActionError> {
    let (_lock, start) = begin(sys, ctx)?;
    let mut report = Report::new(start);
    let state = load_state(sys, ctx, &mut report)?;
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
/// - A state.json that cannot be read stops only the revert (a failed step); the folders still
///   go, but the data folder stays whole while it holds a state.json from a newer CMD Studio.
/// - The folders are worked out from `Paths` and the user's SID, not from state.json; a
///   ProgramData folder state.json names besides goes only when it is private and named like
///   CMD Studio's. A folder that someone else may change is left alone, and the report says why.
/// - A restore point whose pending record says to copy files from it by hand is kept.
/// - The lock file goes last, once the lock is let go: Windows refuses to delete a file opened
///   the way the lock opens it.
pub fn cleanup(sys: System<'_>, ctx: &Ctx, remove_themes: bool) -> Result<Report, ActionError> {
    let (lock, start) = begin(sys, ctx)?;
    let mut report = Report::new(start);
    let mut state = None;
    let mut newer_state = false;
    match load_state(sys, ctx, &mut report) {
        Ok(loaded) => match revert_steps(sys, ctx, loaded, &mut report) {
            Some(next) => state = Some(next),
            None => return Ok(report),
        },
        Err(e) => {
            newer_state = matches!(e, ActionError::State(StateError::Newer { .. }));
            report.fail(
                "Revert CMD Studio's changes",
                e.code(),
                format!("{e}; nothing was reverted"),
                None,
            );
        }
    }
    if let Some(state) = &mut state {
        pending_font_files(sys, ctx, state, &mut report);
    }
    let recorded = state
        .as_ref()
        .and_then(|s| s.baseline.program_data_dir.as_deref());
    remove_program_data(sys, recorded, &mut report);
    if newer_state {
        let data = &ctx.paths.data;
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
    let keep = report
        .start
        .incomplete
        .as_ref()
        .filter(|record| fixes_copy_from_it(record, &ctx.paths))
        .map(|record| record.id.clone());
    remove_data(sys, ctx, remove_themes, keep.as_deref(), &mut report);
    drop(lock);
    remove_lock_and_folder(sys, ctx, &mut report);
    Ok(report)
}

/// state.json for a revert. One that cannot be read now, or is from a newer CMD Studio, stops
/// it and is left as it is: it may hold a baseline only it knows. A damaged one is renamed
/// `state.json.damaged-<ms>` and kept for a look, the report says so in a failed step, and the
/// revert goes on with no baseline; when that rename fails it stops, so the damaged file is
/// never written over. The font files earlier runs left to delete are tried again, as at the
/// start of apply and undo.
fn load_state(sys: System<'_>, ctx: &Ctx, report: &mut Report) -> Result<State, ActionError> {
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
            Ok(state)
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
            Ok(State::default())
        }
        Err(e) => Err(e.into()),
    }
}

/// Steps 1–3 with `state` loaded: the AutoRun and settings.json transaction, the applies marked
/// as reverted, the registry keys CMD Studio created, the fonts, then the new state.json. It has
/// no applied theme, and keeps what could not be put back yet (an AutoRun left for a hand edit,
/// a settings.json that cannot be read, a font that stays installed), the ProgramData folder
/// CMD Studio made, and the font files to delete later. `None` when the transaction did not
/// commit: then nothing else was touched.
fn revert_steps(sys: System<'_>, ctx: &Ctx, state: State, report: &mut Report) -> Option<State> {
    let base = &state.baseline;
    let mut ops = Vec::new();
    let mut left = Baseline {
        program_data_dir: base.program_data_dir.clone(),
        ..Baseline::default()
    };
    let autorun_done = autorun_step(sys, &ctx.paths, base, &mut ops, report);
    if !autorun_done {
        left.autorun = base.autorun.clone();
        left.autorun_written = base.autorun_written.clone();
    }
    for file in &base.wt {
        if !settings_step(sys, file, &mut ops, report) {
            left.wt.push(file.clone());
        }
    }
    if !transaction(sys, ctx, ops, report) {
        return None;
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
    Some(next)
}

/// Step 1: queues what AutoRun becomes. Whether AutoRun needs nothing more: a change is queued,
/// or none is needed. A value that cannot be read, or is left for a hand edit, is a failed step.
fn autorun_step(
    sys: System<'_>,
    paths: &Paths,
    base: &Baseline,
    ops: &mut Vec<Op>,
    report: &mut Report,
) -> bool {
    const WHAT: &str = "Take CMD Studio's part out of AutoRun";
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
    match autorun_after_revert(
        current.as_deref(),
        base.autorun.as_ref(),
        base.autorun_written.as_ref(),
        paths,
    ) {
        Ok(Some(value)) => {
            ops.push(Op::SetAutoRun { value });
            true
        }
        Ok(None) => true,
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
/// not read, is left as it is and said.
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
                None,
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
                Some(settings_by_hand(path)),
            );
            false
        }
    }
}

/// The AutoRun and settings.json changes as one transaction with a `revertAll` restore point
/// (verified by read-back; a rollback that cannot finish is recorded), then every apply marked
/// as reverted by it: an undo after this would bring back a theme the user removed. It runs
/// when there is something to change or an apply to mark, so a machine the user already put
/// back by hand gets its applies marked too. Whether it committed, or had nothing to do.
fn transaction(sys: System<'_>, ctx: &Ctx, ops: Vec<Op>, report: &mut Report) -> bool {
    const MARK: &str = "Mark the themes applied before as reverted";
    let root = ctx.paths.snapshots();
    let store = SnapshotStore::new(sys.fs, &root);
    let (applies, unread) = applies_to_mark(&store);
    for f in unread {
        report.fail(MARK, f.code, f.message, None);
    }
    if ops.is_empty() && applies.is_empty() {
        return true;
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
    if !committed {
        return false;
    }
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
    true
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
/// SID, and one state.json names besides, if it is named like CMD Studio's.
fn remove_program_data(sys: System<'_>, recorded: Option<&Path>, report: &mut Report) {
    let host = sys.host;
    let own = match (host.program_data(), host.user_sid()) {
        (Ok(pd), Ok(sid)) => Some(program_data_dir(&pd, &sid)),
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
    if named_like_ours(dir) {
        remove_private(sys, dir, report);
    } else {
        report.done(
            format!("Remove {}", dir.display()),
            "left alone: state.json names it as CMD Studio's ProgramData folder, but it is not \
             named like one",
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

/// Whether the hand fixes of `record` copy files from its restore point (`Copy <payload> over
/// …`): deleting the restore point would leave them nothing to copy.
fn fixes_copy_from_it(record: &Incomplete, paths: &Paths) -> bool {
    let dir = paths.snapshots().join(&record.id).display().to_string();
    record
        .steps
        .iter()
        .any(|s| s.fix.contains(&dir) || s.fix_th.contains(&dir))
}

/// Step 4 for the data folder: every entry but the lock file this run holds, `themes\` unless
/// asked, and the restore point `keep` (whose record's fixes copy from it). Entries are deleted
/// one by one, so the held lock file stops nothing; what cannot be deleted is a failed step.
fn remove_data(
    sys: System<'_>,
    ctx: &Ctx,
    remove_themes: bool,
    keep: Option<&str>,
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
    let (lock, themes, snapshots) = (
        ctx.paths.lock_file(),
        ctx.paths.themes(),
        ctx.paths.snapshots(),
    );
    for entry in &entries {
        if same_path(&entry.path, &lock) || (!remove_themes && same_path(&entry.path, &themes)) {
            continue;
        }
        match keep {
            Some(id) if same_path(&entry.path, &snapshots) => {
                remove_restore_points_but(sys, &snapshots, id, report);
            }
            _ => remove_entry(sys, entry, report),
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

/// Every restore point but `id`, which is kept and said.
fn remove_restore_points_but(sys: System<'_>, snapshots: &Path, id: &str, report: &mut Report) {
    let kept = snapshots.join(id);
    match sys.fs.list_dir(snapshots) {
        Ok(entries) => {
            for entry in entries.iter().filter(|e| !same_path(&e.path, &kept)) {
                remove_entry(sys, entry, report);
            }
        }
        Err(e) => report.fail(
            format!("Delete {}", snapshots.display()),
            "E_MACHINE_READ",
            format!("what it holds could not be listed ({e}), so nothing in it was deleted"),
            Some(folder_by_hand(snapshots)),
        ),
    }
    report.fail(
        format!("Delete {}", kept.display()),
        "E_ROLLBACK_INCOMPLETE",
        format!(
            "{} was kept: the record of restore point {id} says to copy files from it by hand",
            kept.display()
        ),
        Some(record_folder_by_hand(id, &kept)),
    );
}

/// After the lock is let go: the lock file, then the data folder when nothing is left in it
/// (`themes\`, or what could not be deleted, keeps it).
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
    if sys.fs.list_dir(data).is_ok_and(|left| left.is_empty()) {
        if let Err(e) = sys.fs.remove_dir(data) {
            report.fail(
                format!("Delete {}", data.display()),
                "E_DATA_WRITE",
                e,
                Some(folder_by_hand(data)),
            );
        }
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

/// A settings.json Windows Terminal cannot read, left as it is.
fn settings_by_hand(file: &Path) -> FixByHand {
    let file = file.display();
    FixByHand {
        en: format!(
            "Fix {file} so that Windows Terminal can read it, then revert again; or delete the \
             color scheme named \"CMD Studio\" from it and set the Command Prompt profile back \
             as you want it."
        ),
        th: format!(
            "แก้ไฟล์ {file} ให้ Windows Terminal อ่านได้ แล้วสั่งย้อนทุกอย่างอีกครั้ง \
             หรือลบชุดสี (scheme) ชื่อ \"CMD Studio\" ออกจากไฟล์นี้ \
             แล้วตั้งโปรไฟล์ Command Prompt กลับเป็นแบบที่ต้องการเอง"
        ),
    }
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

/// The restore point a pending record's fixes copy from, kept.
fn record_folder_by_hand(id: &str, dir: &Path) -> FixByHand {
    let dir = dir.display();
    FixByHand {
        en: format!(
            "First fix by hand what the record of restore point {id} lists: its fixes copy \
             files from {dir}. Then delete {dir}."
        ),
        th: format!(
            "ให้แก้ด้วยมือตามรายการในบันทึกของจุดย้อนกลับ {id} ก่อน \
             (วิธีแก้ในบันทึกนั้นคัดลอกไฟล์จาก {dir}) แล้วจึงลบ {dir}"
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
        let wrote = |t: &str| AutoRunWritten {
            value: Some(t.to_string()),
        };
        let foreign = r#""C:\clink\clink.bat" inject --autorun"#;
        // Untouched since our write: the original comes back, a foreign Clink that the apply
        // took out included.
        assert_eq!(
            autorun_after_revert(
                Some(&ours),
                Some(&rec(foreign)),
                Some(&wrote(&ours)),
                &paths
            ),
            Ok(Some(Some(foreign.to_string())))
        );
        assert_eq!(
            autorun_after_revert(Some(&ours), Some(&absent), Some(&wrote(&ours)), &paths),
            Ok(Some(None))
        );
        // Changed by someone after us: only our parts leave, the older form too, and every
        // other byte stays.
        let changed = format!("{ours} & echo a ^& b & {older} & echo z");
        assert_eq!(
            autorun_after_revert(Some(&changed), Some(&absent), Some(&wrote(&ours)), &paths),
            Ok(Some(Some("echo a ^& b & echo z".into())))
        );
        // No state at all: still only our part leaves; nothing of ours means no change.
        assert_eq!(
            autorun_after_revert(Some(&ours), None, None, &paths),
            Ok(Some(None))
        );
        assert_eq!(
            autorun_after_revert(Some("echo hi"), None, None, &paths),
            Ok(None)
        );
        assert_eq!(autorun_after_revert(None, None, None, &paths), Ok(None));
        // A command that holds our profile path without being one of our entries is left for
        // a hand edit: cutting it out would take other text with it.
        let err =
            autorun_after_revert(Some(&format!("echo ^& {ours}")), None, None, &paths).unwrap_err();
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

    /// The fake disk, but `dir` cannot be listed.
    struct Unlistable<'a> {
        fs: &'a FakeFs,
        dir: PathBuf,
    }

    impl Fs for Unlistable<'_> {
        fn read(&self, path: &Path) -> Result<Option<Vec<u8>>, AdapterError> {
            self.fs.read(path)
        }
        fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<(), AdapterError> {
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
            if path == self.dir {
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
        // Nothing says which of its changes ran, so the user is told; ours are taken out.
        assert!(
            !failed(&report, "E_ROLLBACK_INCOMPLETE").is_empty(),
            "{report:?}"
        );
        assert_eq!(m.autorun.read_user().unwrap(), None);
        assert!(!m.fs.exists(&ctx.paths.data), "{report:?}");
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
            // which it works out without state.json, and keeps the data folder only while it
            // holds a state.json from a newer CMD Studio.
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
                assert!(!m.fs.exists(&ctx.paths.data), "{report:?}");
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
    fn cleanup_keeps_the_restore_point_whose_record_says_what_to_copy_back() {
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        let id = apply_that_cannot_roll_back(&m, &later(&ctx, 1));
        let report = cleanup(m.system(), &later(&ctx, 2), true).unwrap();
        assert!(!report.ok());
        let record = report.start.incomplete.as_ref().unwrap();
        assert_eq!(record.id, id);
        let payload = SnapshotStore::new(&m.fs, &ctx.paths.snapshots())
            .payload_path(&id, record.steps[0].op_index);
        assert!(
            m.fs.exists(&payload),
            "the copy its fix names is still there"
        );
        let kept = ctx.paths.snapshots().join(&id).display().to_string();
        assert!(
            failed(&report, "E_ROLLBACK_INCOMPLETE")
                .iter()
                .any(|s| s.detail.contains(&kept) && s.fix.is_some()),
            "{report:?}"
        );
        assert!(!m.fs.exists(&ctx.paths.runtime_root()));
        assert!(!m.fs.exists(&ctx.paths.state_file()));
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
        let fs = Unlistable {
            fs: &m.fs,
            dir: ctx.paths.data.clone(),
        };
        let sys = System {
            fs: &fs,
            ..m.system()
        };
        let report = cleanup(sys, &later(&ctx, 1), true).unwrap();
        assert!(!report.ok());
        assert!(!failed(&report, "E_MACHINE_READ").is_empty(), "{report:?}");
        assert!(m.fs.exists(&ctx.paths.runtime_root()));
    }

    #[test]
    fn an_original_autorun_that_cannot_be_read_back_needs_a_hand_edit_and_is_never_deleted() {
        let paths = Paths {
            data: PathBuf::from(r"C:\d"),
        };
        let ours = autorun_entry(&ClinkLaunch::Direct(paths.clink_exe()), &paths).unwrap();
        let wrote = AutoRunWritten {
            value: Some(ours.clone()),
        };
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
            let e = autorun_after_revert(Some(&ours), Some(&original), Some(&wrote), &paths)
                .unwrap_err();
            assert_eq!(e.code(), "E_AUTORUN_WRITE");
            // With nothing of ours in AutoRun there is nothing to take out.
            let deleted = AutoRunWritten { value: None };
            assert_eq!(
                autorun_after_revert(None, Some(&original), Some(&deleted), &paths),
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
        let ours = m.autorun.read_user().unwrap();
        let entry = ours.as_ref().unwrap().text().unwrap();
        let report = revert_all(m.system(), &later(&ctx, 1)).unwrap();
        let [step] = failed(&report, "E_AUTORUN_WRITE")[..] else {
            panic!("{report:?}")
        };
        let fix = step.fix.as_ref().unwrap();
        for text in [&fix.en, &fix.th] {
            assert!(
                text.contains(COMMAND_PROCESSOR_KEY) && text.contains(&entry),
                "{text}"
            );
        }
        assert_eq!(m.autorun.read_user().unwrap(), ours, "left, never deleted");
        assert_eq!(m.fs.file(WT).unwrap(), SETTINGS.as_bytes());
        assert_eq!(
            state(&m, &ctx).baseline.autorun,
            s.baseline.autorun,
            "kept for the next revert"
        );
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
        // A rollback that did not finish, with nothing run since: the user is told, and its
        // record is in the report.
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        let id = apply_that_cannot_roll_back(&m, &later(&ctx, 1));
        let report = revert_all(m.system(), &later(&ctx, 2)).unwrap();
        assert_eq!(
            report.start.incomplete.as_ref().map(|r| r.id.as_str()),
            Some(id.as_str())
        );
        let [step] = failed(&report, "E_ROLLBACK_INCOMPLETE")[..] else {
            panic!("{report:?}")
        };
        assert!(step.detail.contains(&id), "{step:?}");
        assert!(!report.ok());

        // Its record cannot be read: said, not dropped.
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
}
