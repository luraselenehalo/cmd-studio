//! The commands behind the CLI (spec §5.7) and, later, the app's IPC: apply, undo, and
//! recovery when a run starts. Each one is a transaction with its own restore point
//! (spec §5.5).
use crate::adapters::{AdapterError, Arch, Fs, FsLock, System};
use crate::detect::{detect, DetectError, Detected};
use crate::exec::{
    fix_by_hand, font_target, op_label, same_path, sweep, SystemExecutor, UndoExecutor,
    UndoVerifier, CLEANUP_FIX_EN, CLEANUP_FIX_TH,
};
use crate::generate::lua::Lang;
use crate::model::Theme;
use crate::plan::{
    plan, Op, Paths, Plan, PlanError, Request, CLINK_VERSION, NERD_FONT_FAMILY, OMP_VERSION,
};
use crate::resources::{sha256_hex, ResourceError, Resources};
use crate::snapshot::{
    new_id, Incomplete, Manifest, SnapshotKind, SnapshotStore, ThemeRef, Unreadable,
    UnreadableKind, Unrestored, KEEP,
};
use crate::state::{Applied, AutoRunRecord, FontRecord, RegValueRecord, State, StateError};
use crate::txn::{self, Failure, Outcome};
use crate::verify::SystemVerifier;
use crate::wt_edit::{apply_edit, WtEdit, WtError};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

impl Paths {
    /// `<data>\cmdstudio.lock`: held by every command that changes the machine.
    pub fn lock_file(&self) -> PathBuf {
        self.data.join("cmdstudio.lock")
    }
}

/// Where things are, and the clock.
#[derive(Clone, Debug)]
pub struct Ctx {
    pub paths: Paths,
    pub resources: Resources,
    pub now_ms: u64,
}

/// Why a command stopped. Each carries a stable code (spec §5.9); a read never carries a
/// write code.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ActionError {
    #[error("CMD Studio supports x64 Windows only; this machine is ARM64")]
    Unsupported,
    #[error(transparent)]
    Resources(#[from] ResourceError),
    #[error(transparent)]
    Detect(#[from] DetectError),
    #[error(transparent)]
    State(#[from] StateError),
    #[error(transparent)]
    Plan(#[from] PlanError),
    #[error("{path}: {error}")]
    Wt { path: PathBuf, error: WtError },
    /// A restore point could not be read (`E_SNAPSHOT_READ`) or written (`E_SNAPSHOT_WRITE`):
    /// the failure keeps the code of what went wrong.
    #[error("{}", .0.message)]
    Snapshot(Failure),
    /// Another CMD Studio holds the lock: it is changing this computer right now.
    #[error("{}", BUSY_EN)]
    Busy,
    /// The lock file could not be made or opened.
    #[error("could not open CMD Studio's lock file: {0}")]
    Lock(AdapterError),
    /// Undo is refused while the rollback of the newest transaction did not finish: undoing the
    /// apply before it would hide the record of what is left to fix by hand. The record says
    /// what, in English and in Thai.
    #[error(
        "the rollback of restore point {} did not finish, so nothing is undone. Fix what it \
         lists by hand first; then apply a theme, or run `cmdstudio --cleanup`",
        .0.id
    )]
    PendingIncomplete(Incomplete),
    /// A transaction a crash may have cut off could not be checked: its restore point cannot be
    /// read right now. Nothing is changed until it can be, and recovered.
    #[error(
        "An earlier change could not be checked yet: its restore point cannot be read right now \
         ({reason}). Nothing was changed; try again in a moment"
    )]
    RecoveryPending { reason: String },
}

/// `E_BUSY` in English (spec §5.9). A virus scanner or an indexer that has the lock file open
/// for a moment looks the same as another CMD Studio, so both are named.
pub const BUSY_EN: &str = "Another CMD Studio is changing this computer right now, or another \
                           program has its file cmdstudio.lock open. Try again in a moment.";
/// `E_BUSY` in Thai.
pub const BUSY_TH: &str = "มี CMD Studio อีกตัวกำลังเปลี่ยนการตั้งค่าของเครื่องนี้อยู่ \
                           หรือมีโปรแกรมอื่นเปิดไฟล์ cmdstudio.lock ของ CMD Studio ค้างไว้ \
                           ลองใหม่อีกครั้งในอีกสักครู่";

impl ActionError {
    /// Stable code shown to users (spec §5.9).
    pub fn code(&self) -> &'static str {
        match self {
            ActionError::Unsupported => "E_UNSUPPORTED_PLATFORM",
            ActionError::Resources(e) => e.code(),
            ActionError::Detect(e) => e.code(),
            ActionError::State(e) => e.code(),
            ActionError::Plan(e) => e.code(),
            ActionError::Wt { error, .. } => error.code(),
            ActionError::Snapshot(f) => f.code,
            ActionError::Busy => "E_BUSY",
            ActionError::Lock(_) => "E_DATA_WRITE",
            ActionError::PendingIncomplete(_) => "E_ROLLBACK_INCOMPLETE",
            ActionError::RecoveryPending { .. } => "E_SNAPSHOT_READ",
        }
    }

    /// The Thai text of the errors this module words itself (spec §5.9: every problem the user
    /// sees has Thai and English). `None` for an error passed on from another module, whose
    /// text is English and whose code's Thai meaning is in spec §5.9.
    pub fn message_th(&self) -> Option<String> {
        match self {
            ActionError::Unsupported => {
                Some("CMD Studio ใช้ได้กับ Windows แบบ x64 เท่านั้น แต่เครื่องนี้เป็น ARM64".to_string())
            }
            ActionError::Busy => Some(BUSY_TH.to_string()),
            ActionError::Lock(e) => Some(format!("เปิดไฟล์ล็อกของ CMD Studio ไม่ได้: {e}")),
            ActionError::PendingIncomplete(record) => Some(format!(
                "การย้อนกลับของจุดย้อนกลับ {} ทำได้ไม่ครบ จึงยังย้อนกลับไม่ได้ \
                 ให้แก้รายการที่ค้างด้วยมือก่อน แล้วใช้ธีมใหม่ หรือรันคำสั่ง `cmdstudio --cleanup`",
                record.id
            )),
            ActionError::RecoveryPending { reason } => Some(format!(
                "ยังตรวจการเปลี่ยนแปลงครั้งก่อนไม่ได้ เพราะตอนนี้อ่านจุดย้อนกลับของมันไม่ได้ \
                 ({reason}) จึงยังไม่ได้เปลี่ยนอะไร ลองใหม่อีกครั้งในอีกสักครู่"
            )),
            ActionError::Resources(_)
            | ActionError::Detect(_)
            | ActionError::State(_)
            | ActionError::Plan(_)
            | ActionError::Wt { .. }
            | ActionError::Snapshot(_) => None,
        }
    }
}

impl From<Failure> for ActionError {
    fn from(f: Failure) -> Self {
        ActionError::Snapshot(f)
    }
}

/// Takes CMD Studio's lock: one command at a time may change the machine, whether the app or
/// the CLI runs it. The data folder that holds the lock file is made first. Fails fast with
/// `E_BUSY` while another run holds it; never waits.
pub(crate) fn lock(sys: System<'_>, ctx: &Ctx) -> Result<FsLock, ActionError> {
    sys.fs
        .create_dir_all(&ctx.paths.data)
        .map_err(DetectError::Write)?;
    match sys.fs.try_lock(&ctx.paths.lock_file()) {
        Ok(Some(lock)) => Ok(lock),
        Ok(None) => Err(ActionError::Busy),
        Err(e) => Err(ActionError::Lock(e)),
    }
}

/// An apply that is planned but has not touched the machine yet. It holds the lock from
/// `prepare_apply` until `run_apply` ends, so nothing else changes the machine, or the
/// state.json it was planned from, in between; dropping it lets the lock go.
#[derive(Debug)]
pub struct Prepared {
    pub plan: Plan,
    pub detected: Detected,
    /// The plan's ops, then the write of the new state.json.
    pub ops: Vec<Op>,
    pub state_after: State,
    pub manifest: Manifest,
    /// What the start of this apply found and did, before it planned (see [`StartReport`]).
    pub start: StartReport,
    /// The data folder and the bundled files it was planned with, whose lock it holds.
    paths: Paths,
    resources: Resources,
    lock: FsLock,
}

fn lang_code(lang: Lang) -> &'static str {
    match lang {
        Lang::Th => "th",
        Lang::En => "en",
    }
}

/// Takes the lock and does what every command does first (see [`StartReport`]; the report is
/// in [`Prepared::start`]), deletes the font files an earlier run left to delete, then detects,
/// plans and works out the new state.json without changing anything else. A settings.json that
/// does not parse stops it here (spec §5.4: never touch that file), and so does a state.json it
/// cannot plan from. Every error lets the lock go; on success the [`Prepared`] keeps it.
pub fn prepare_apply(
    sys: System<'_>,
    ctx: &Ctx,
    theme: &Theme,
    req: &Request,
) -> Result<Prepared, ActionError> {
    ctx.resources.manifest(sys.fs)?;
    let (lock, mut start) = begin(sys, ctx)?;
    let detected = detect(sys, &ctx.paths)?;
    if detected.info.arch == Arch::Arm64 {
        return Err(ActionError::Unsupported);
    }
    // Any state.json that cannot be planned from stops the apply, and is left as it is: one
    // from a newer CMD Studio or one that cannot be read now, or a damaged one.
    let mut state = State::load(sys.fs, &ctx.paths.state_file())?;
    delete_pending_and_save(sys, ctx, &mut state, &mut start);
    let plan = plan(theme, req, &detected.machine, &ctx.paths)?;
    let root = ctx.paths.snapshots();
    let id = next_id(&SnapshotStore::new(sys.fs, &root), ctx.now_ms);
    let mut generated = BTreeMap::new();
    let mut wt_font = None;

    for op in &plan.ops {
        match op {
            Op::WtApply {
                settings,
                scheme,
                profile,
                set_default,
            } => {
                // Reading it is reading the machine; only text Windows Terminal cannot read is
                // E_WT_PARSE.
                let gone = || AdapterError::new(settings.display().to_string(), "the file is gone");
                let bytes = sys
                    .fs
                    .read(settings)
                    .map_err(DetectError::Read)?
                    .ok_or_else(|| DetectError::Read(gone()))?;
                let wt = |error| ActionError::Wt {
                    path: settings.clone(),
                    error,
                };
                let text =
                    String::from_utf8(bytes).map_err(|e| wt(WtError::Parse(e.to_string())))?;
                let edited = apply_edit(
                    &text,
                    &WtEdit {
                        scheme,
                        profile,
                        set_default: *set_default,
                    },
                )
                .map_err(wt)?;
                state.baseline.note_wt(settings, &edited);
                wt_font = profile
                    .get("font")
                    .and_then(|f| f.get("face"))
                    .and_then(|f| f.as_str())
                    .map(str::to_string);
            }
            Op::SetAutoRun { value } => {
                let autorun = sys.autorun;
                let key_existed = autorun.user_key_exists().map_err(DetectError::Read)?;
                let current = autorun
                    .read_user()
                    .map_err(DetectError::Read)?
                    .as_ref()
                    .map(RegValueRecord::from_reg);
                state.baseline.note_autorun(
                    &AutoRunRecord {
                        key_existed,
                        value: current,
                    },
                    value.as_deref(),
                );
            }
            Op::InstallFont { family } => {
                if let Some((_, file, value_name)) = font_target(sys, &ctx.resources, family) {
                    // Installed again, its file is no longer one to delete later.
                    state.pending_deletes.retain(|p| !same_path(p, &file));
                    // Before CMD Studio's first font: whether the registration key it will add
                    // the font to is there yet, so that revert all deletes only a key it made.
                    if state.baseline.fonts.is_empty() {
                        let existed = sys
                            .fonts
                            .registration_key_exists()
                            .map_err(DetectError::Read)?;
                        state.baseline.fonts_key_existed = Some(existed);
                    }
                    state.baseline.note_font(FontRecord {
                        family: family.clone(),
                        file,
                        value_name,
                    });
                }
            }
            Op::InstallRuntime { dir, .. } => {
                let pd = &detected.program_data.0;
                if dir.parent() == Some(pd.as_path())
                    && !sys.fs.exists(pd)
                    && state.baseline.program_data_dir.is_none()
                {
                    state.baseline.program_data_dir = Some(pd.clone());
                }
            }
            // Doctor D9 compares the files in generated (spec §5.8).
            Op::WriteFile { path, contents } if path.starts_with(ctx.paths.generated()) => {
                generated.insert(path.clone(), sha256_hex(contents.as_bytes()));
            }
            Op::WriteFile { .. } => {}
        }
    }

    // Doctor D3 reads what Clink logs after this point. A log that is missing, or cannot be
    // read now, counts as empty: D3 then reads all of it, so an old error may show again, but
    // a new one is never skipped.
    let clink_log = ctx.paths.clink_profile().join("clink.log");
    let clink_log_len = sys
        .fs
        .read(&clink_log)
        .ok()
        .flatten()
        .map_or(0, |bytes| u64::try_from(bytes.len()).unwrap_or(u64::MAX));
    state.applied = Some(Applied {
        theme_id: theme.id,
        theme_name: theme.name.clone(),
        target: req.target,
        set_default: req.set_wt_default_profile,
        lang: lang_code(req.lang).to_string(),
        launch: (&plan.launch).into(),
        omp_version: OMP_VERSION.to_string(),
        clink_version: CLINK_VERSION.to_string(),
        generated,
        wt_font,
        clink_log_len,
        snapshot_id: id.clone(),
        applied_at_ms: ctx.now_ms,
    });
    // `to_bytes` writes JSON, which is UTF-8; a failure here is a failure to produce the file.
    let state_text = String::from_utf8(state.to_bytes()?)
        .map_err(|e| StateError::Io(format!("state.json could not be produced: {e}")))?;
    let mut ops = plan.ops.clone();
    ops.push(Op::WriteFile {
        path: ctx.paths.state_file(),
        contents: state_text,
    });
    let manifest = Manifest {
        id,
        kind: SnapshotKind::Apply,
        created_ms: ctx.now_ms,
        theme: Some(ThemeRef {
            id: theme.id,
            name: theme.name.clone(),
        }),
        target: Some(req.target),
        summary: plan.summary_en.iter().map(|s| s.to_string()).collect(),
        undoes: None,
        undone_by: None,
    };
    Ok(Prepared {
        plan,
        detected,
        ops,
        state_after: state,
        manifest,
        start,
        paths: ctx.paths.clone(),
        resources: ctx.resources.clone(),
        lock,
    })
}

/// Runs a prepared apply as one transaction, its new state.json the last op. After it ends, the
/// font files its rollback uninstalled but could not delete are noted in state.json for a later
/// delete; a rollback that could neither delete nor note one ends `RollbackIncomplete` with that
/// font among what to fix by hand. A rollback that did not finish is recorded in the restore
/// point (`incomplete.json`). Prunes old restore points after a commit. The lock that
/// `prepare_apply` took is let go when this returns. Given a data folder or bundled files other
/// than those it was prepared with, it changes nothing and ends `RolledBack` (`E_DATA_WRITE`).
pub fn run_apply(sys: System<'_>, ctx: &Ctx, prepared: Prepared) -> Outcome {
    let Prepared {
        plan,
        ops,
        manifest,
        paths,
        resources,
        lock: _lock,
        ..
    } = prepared;
    // The ops, the lock and the state.json it writes belong to the data folder it was planned
    // in; another one would get a restore point and a state.json that no lock protects.
    if ctx.paths != paths || ctx.resources != resources {
        return Outcome::RolledBack {
            code: "E_DATA_WRITE",
            reason: format!(
                "nothing was changed: this apply was prepared for the data folder {} and the \
                 bundled files in {}, but was run with {} and {}",
                paths.data.display(),
                resources.root.display(),
                ctx.paths.data.display(),
                ctx.resources.root.display()
            ),
        };
    }
    let root = ctx.paths.snapshots();
    let store = SnapshotStore::new(sys.fs, &root);
    let id = manifest.id.clone();
    let mut journal = store.begin(manifest);
    let mut exec = SystemExecutor::new(sys, &ctx.resources);
    let mut verifier = SystemVerifier::new(sys, &ctx.paths, &ctx.resources, Some(plan.launch));
    let outcome = txn::run(&id, ops.clone(), &mut exec, &mut verifier, &mut journal);
    // Only a rollback uninstalls a font (`SystemExecutor::apply` never does), so a committed
    // apply holds no file and `after_held` has nothing to give back for one.
    let (outcome, _) = after_held(sys, ctx, &ops, &exec.held_files(), outcome);
    // Best effort: the outcome itself already says what was not put back.
    let _ = record_incomplete(&store, &id, &outcome);
    if outcome == Outcome::Committed {
        // Failing to prune costs disk space only.
        let _ = store.prune(KEEP);
    }
    outcome
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UndoResult {
    NothingToUndo,
    Done { undid: String, outcome: Outcome },
}

/// What `undo` did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UndoReport {
    /// What the start of this undo found and did (see [`StartReport`]).
    pub start: StartReport,
    pub result: UndoResult,
    /// What the undo could not do besides its transaction, each with its code; the result
    /// stands. For example: an undo that is done but whose apply could not be marked undone.
    pub problems: Vec<Failure>,
}

/// Puts back what the newest apply that is not undone yet changed, byte for byte (spec §4.4
/// "ย้อนกลับ", CLI `undo`), as a transaction of its own over that apply's ops in reverse, so a
/// failed undo rolls back to the themed state. Takes the lock and does what every command does
/// first. Refused (and nothing changes) while the rollback of the newest transaction did not
/// finish (`E_ROLLBACK_INCOMPLETE`) or its record cannot be read, while state.json is from a
/// newer CMD Studio or cannot be read (`E_STATE_INVALID`), and before a restore point it cannot
/// read (`E_SNAPSHOT_READ`). The font files it uninstalled but could not delete are noted in
/// state.json for a later delete.
pub fn undo(sys: System<'_>, ctx: &Ctx) -> Result<UndoReport, ActionError> {
    let (_lock, mut start) = begin(sys, ctx)?;
    let root = ctx.paths.snapshots();
    let store = SnapshotStore::new(sys.fs, &root);
    // `latest_undoable` skips an apply that did not commit, so with a rollback that did not
    // finish it would undo the apply before that one, and the undo, being newer, would hide
    // the record of what is left to fix. A record that cannot be read stops it the same way.
    if let Some(record) = store.latest_incomplete()? {
        return Err(ActionError::PendingIncomplete(record));
    }
    // state.json is one of the targets an undo puts back. One from a newer CMD Studio, or one
    // that cannot be read now, is never overwritten; a damaged one is replaced by the
    // state.json the apply found, as before.
    match State::load(sys.fs, &ctx.paths.state_file()) {
        Ok(mut state) => delete_pending_and_save(sys, ctx, &mut state, &mut start),
        Err(e @ StateError::Invalid(_)) => start.problems.push(Failure {
            code: e.code(),
            message: format!("{e}; the font files it may list for a later delete were not tried"),
        }),
        Err(e) => return Err(e.into()),
    }
    let mut problems = Vec::new();
    let Some((applied, journal)) = store.latest_undoable()? else {
        return Ok(UndoReport {
            start,
            result: UndoResult::NothingToUndo,
            problems,
        });
    };
    let priors: Vec<(Op, Option<Vec<u8>>)> = journal
        .snapshots
        .iter()
        .filter_map(|s| {
            journal
                .ops
                .get(s.op_index)
                .map(|op| (op.clone(), s.prior.clone()))
        })
        .collect();
    let ops: Vec<Op> = journal.ops.iter().rev().cloned().collect();
    let id = next_id(&store, ctx.now_ms);
    let manifest = Manifest {
        id: id.clone(),
        kind: SnapshotKind::Undo,
        created_ms: ctx.now_ms,
        theme: applied.theme.clone(),
        target: applied.target,
        summary: vec![format!("Undo {}", applied.id)],
        undoes: Some(applied.id.clone()),
        undone_by: None,
    };
    let mut store_txn = store.begin(manifest);
    let mut exec = UndoExecutor::new(SystemExecutor::new(sys, &ctx.resources), priors.clone());
    let mut verifier = UndoVerifier::new(SystemExecutor::new(sys, &ctx.resources), priors);
    let outcome = txn::run(&id, ops.clone(), &mut exec, &mut verifier, &mut store_txn);
    // Into the state.json the transaction left: the apply's own, put back, after a commit.
    let (outcome, unnoted) = after_held(sys, ctx, &ops, &exec.held_files(), outcome);
    problems.extend(unnoted);
    if let Err(f) = record_incomplete(&store, &id, &outcome) {
        problems.push(f);
    }
    if outcome == Outcome::Committed {
        // The machine is back; only the restore point's note of it failed. Said, not turned
        // into an error that would hide the outcome.
        if let Err(f) = store.mark_undone(std::slice::from_ref(&applied.id), &id) {
            problems.push(Failure {
                code: f.code,
                message: format!(
                    "the undo is done, but restore point {} could not be marked undone, so the \
                     next undo goes over it again: {}",
                    applied.id, f.message
                ),
            });
        }
        // Failing to prune costs disk space only.
        let _ = store.prune(KEEP);
    }
    Ok(UndoReport {
        start,
        result: UndoResult::Done {
            undid: applied.id,
            outcome,
        },
        problems,
    })
}

/// What a command found and did when it started, before its own work.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StartReport {
    /// How the rollback of a transaction a crash cut off ended; `None` when there was none.
    pub recovered: Option<Outcome>,
    /// What the newest transaction could not put back, when it ended `RollbackIncomplete` and
    /// nothing has run since: it may still need a hand (spec §5.5).
    pub incomplete: Option<Incomplete>,
    /// Restore points that cannot be read, oldest first, and what was done about each. One that
    /// was set aside is not listed again; one left for a later try is listed by every start.
    pub unreadable: Vec<UnreadablePoint>,
    /// What the start could not do or could not find out, each with its code: the restore
    /// points could not be listed, `incomplete.json` could not be read, a record could not be
    /// written. The command still runs.
    pub problems: Vec<Failure>,
    /// A transaction may still need rolling back: recovery could not read the newest restore
    /// point's journal now, and left it for a later start (`recovered` is then
    /// `RollbackIncomplete` with no failed ops). While it is, a command that changes the machine
    /// is refused with `E_SNAPSHOT_READ`, because its own restore point would become the newest
    /// and recovery would never look at the crashed one again. `recover_at_start` reports it and
    /// goes on. Doctor never runs it: doctor only reads, and reports a waiting transaction from
    /// the restore points themselves (`doctor` R1).
    pub recovery_pending: bool,
}

/// A restore point that cannot be read, found when a command started.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnreadablePoint {
    /// Its id, what is wrong (missing, damaged, or not readable now) and the read failure.
    pub point: Unreadable,
    pub handled: Handled,
}

/// What a start did about a restore point it cannot read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Handled {
    /// A file is missing or damaged and will not come back: its journal was renamed
    /// `journal.unreadable.json` (kept for a look), so later commands, `--cleanup` and doctor go
    /// on without it.
    SetAside,
    /// The read itself failed (a lock, a scan, a permission) and may work later: left as it
    /// is. An undo does not go past it meanwhile.
    LeftForLater,
    /// Setting it aside failed.
    SetAsideFailed(Failure),
}

/// Takes the lock and does what every command that changes the machine does first (see
/// [`recover_at_start`]). The lock is held until the returned guard is dropped. Refused with
/// `E_SNAPSHOT_READ` while a transaction waits to be rolled back
/// ([`StartReport::recovery_pending`]): a new transaction would become the newest restore point,
/// and recovery would never look at the waiting one again. apply, undo and Task 14's revert-all
/// start here.
pub(crate) fn begin(sys: System<'_>, ctx: &Ctx) -> Result<(FsLock, StartReport), ActionError> {
    let (lock, report) = begin_anyway(sys, ctx)?;
    if report.recovery_pending {
        let reason = match &report.recovered {
            Some(Outcome::RollbackIncomplete { reason, .. }) => reason.clone(),
            _ => String::new(),
        };
        return Err(ActionError::RecoveryPending { reason });
    }
    Ok((lock, report))
}

/// The id of a new restore point, made under the lock: never older than the newest there is, so
/// restore points keep the order they were made in when the clock goes back. Undo and recovery
/// go by that order. Only ids of the form `new_id` makes count.
pub(crate) fn next_id(store: &SnapshotStore<'_>, now_ms: u64) -> String {
    let newest = store
        .ids()
        .unwrap_or_default()
        .iter()
        .filter_map(|id| {
            let (ms, _) = id.split_once('-')?;
            if ms.len() != 13 || !ms.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            ms.parse::<u64>().ok()
        })
        .max();
    new_id(newest.map_or(now_ms, |n| now_ms.max(n.saturating_add(1))))
}

/// [`begin`] without the refusal: for [`recover_at_start`], which only reports a transaction that
/// waits to be rolled back. A caller that goes on to run a transaction of its own buries the
/// waiting one (see [`begin`]).
pub(crate) fn begin_anyway(
    sys: System<'_>,
    ctx: &Ctx,
) -> Result<(FsLock, StartReport), ActionError> {
    let lock = lock(sys, ctx)?;
    Ok((lock, start(sys, ctx)))
}

/// The start of a command, under the lock:
/// 1. a transaction a crash cut off is rolled back (spec §5.5);
/// 2. restore points that cannot be read are reported, and those that will not mend set aside;
///    after the recovery, which may still roll back one listed for its manifest alone;
/// 3. a recovery that could not put everything back is recorded in its restore point, so it is
///    still reported after a restart;
/// 4. half-made runtime folders are deleted;
/// 5. the record of a rollback that did not finish, if nothing has run since, is read.
///
/// Nothing here stops the command: what could not be done is in `problems`.
fn start(sys: System<'_>, ctx: &Ctx) -> StartReport {
    let root = ctx.paths.snapshots();
    let store = SnapshotStore::new(sys.fs, &root);
    let mut report = StartReport::default();
    // Recovery works on the newest restore point; the lock keeps another from appearing.
    let newest = store.ids().ok().and_then(|mut ids| ids.pop());
    let mut exec = SystemExecutor::new(sys, &ctx.resources);
    let recovered = txn::recover(&mut exec, &mut store.recovery()).map(|outcome| {
        // Font files the rollback uninstalled but could not delete, noted for a later run.
        let held = exec.held_files();
        let ops = match (&newest, held.is_empty()) {
            (Some(id), false) => store.journal(id).map(|j| j.ops).unwrap_or_default(),
            _ => Vec::new(),
        };
        let (outcome, unnoted) = after_held(sys, ctx, &ops, &held, outcome);
        report.problems.extend(unnoted);
        outcome
    });

    match store.unreadable() {
        Ok(found) => {
            for point in found {
                let handled = match point.kind {
                    UnreadableKind::CouldNotRead => Handled::LeftForLater,
                    UnreadableKind::Missing | UnreadableKind::Damaged => {
                        match store.set_aside(&point.id) {
                            Ok(()) => Handled::SetAside,
                            Err(f) => Handled::SetAsideFailed(f),
                        }
                    }
                };
                report.unreadable.push(UnreadablePoint { point, handled });
            }
        }
        Err(f) => report.problems.push(f),
    }

    let newest_set_aside = newest.as_ref().is_some_and(|id| {
        report
            .unreadable
            .iter()
            .any(|u| u.point.id == *id && u.handled == Handled::SetAside)
    });
    if let Some(outcome @ Outcome::RollbackIncomplete { failed, .. }) = &recovered {
        // With no failed ops, the journal itself could not be read and nothing was restored.
        // That is recorded only once the restore point is set aside: one left for a later try
        // is recovered again at the next start, and a record would outlive that recovery.
        // Until then the transaction is still waiting to be rolled back.
        if failed.is_empty() && !newest_set_aside {
            report.recovery_pending = true;
        } else if let Some(id) = &newest {
            if let Err(f) = record_incomplete(&store, id, outcome) {
                report.problems.push(f);
            }
        }
    }
    sweep(sys, &ctx.paths);
    match store.latest_incomplete() {
        Ok(record) => report.incomplete = record,
        Err(f) => report.problems.push(f),
    }
    report.recovered = recovered;
    report
}

/// What every command that changes the machine does first (apply, undo, revert-all and
/// `--cleanup` do it through `begin`; the CLI may run it on its own before one of them, never
/// before doctor or `--help`, which only read and must not take the lock): takes the lock,
/// rolls back a transaction a crash cut off (spec §5.5), reports restore points that cannot be
/// read and sets aside those that will not mend, deletes half-made runtime folders, and finds
/// what a rollback left for a hand to fix. `E_BUSY` while another run holds the lock: a
/// recovery then could roll back that run's transaction under it.
pub fn recover_at_start(sys: System<'_>, ctx: &Ctx) -> Result<StartReport, ActionError> {
    let (_lock, report) = begin_anyway(sys, ctx)?;
    Ok(report)
}

/// What the newest transaction could not put back, if it ended `RollbackIncomplete` and
/// nothing has run since. A record that cannot be read is an error (`E_SNAPSHOT_READ`), never
/// "nothing": it is the one record that says what to fix by hand.
pub fn pending_incomplete(sys: System<'_>, ctx: &Ctx) -> Result<Option<Incomplete>, ActionError> {
    let root = ctx.paths.snapshots();
    Ok(SnapshotStore::new(sys.fs, &root).latest_incomplete()?)
}

/// Writes `incomplete.json` into restore point `id` when `outcome` is `RollbackIncomplete`:
/// each op that was not put back, its error, and how to fix it by hand in English and in Thai,
/// so that this survives a restart (spec §5.5). A step whose op the journal cannot tell is
/// named `step <n>`, with the shared clean-up advice as its fix; an outcome with no failed ops
/// (its journal could not be read at all) gets one such step for the whole change. The outcome
/// itself already says the rollback is incomplete, so a caller that cannot report a failed
/// write may drop it.
pub(crate) fn record_incomplete(
    store: &SnapshotStore<'_>,
    id: &str,
    outcome: &Outcome,
) -> Result<(), Failure> {
    let Outcome::RollbackIncomplete {
        code,
        reason,
        failed,
    } = outcome
    else {
        return Ok(());
    };
    let journal = store.journal(id).ok();
    let mut steps: Vec<Unrestored> = failed
        .iter()
        .map(|(index, failure)| {
            let op = journal.as_ref().and_then(|j| j.ops.get(*index));
            let snapshot = journal
                .as_ref()
                .and_then(|j| j.snapshots.iter().find(|s| s.op_index == *index));
            let (fix, fix_th) = match (op, snapshot) {
                (Some(op), Some(s)) => {
                    let fix = fix_by_hand(op, &s.prior, &store.payload_path(id, *index));
                    (fix.en, fix.th)
                }
                _ => (CLEANUP_FIX_EN.to_string(), CLEANUP_FIX_TH.to_string()),
            };
            Unrestored {
                op_index: *index,
                target: op.map_or_else(|| format!("step {index}"), op_label),
                code: failure.code.to_string(),
                message: failure.message.clone(),
                fix,
                fix_th,
                restore_point: None,
            }
        })
        .collect();
    if failed.is_empty() {
        // The journal itself could not be read, so nothing says which ops ran: the whole change
        // is the one step (from op 0), with the shared clean-up advice.
        steps.push(Unrestored {
            op_index: 0,
            target: format!("every change of restore point {id}"),
            code: code.to_string(),
            message: reason.clone(),
            fix: CLEANUP_FIX_EN.to_string(),
            fix_th: CLEANUP_FIX_TH.to_string(),
            restore_point: None,
        });
    }
    store.write_incomplete(&Incomplete {
        id: id.to_string(),
        code: code.to_string(),
        reason: reason.clone(),
        steps,
    })
}

fn state_failure(e: &StateError) -> Failure {
    Failure {
        code: e.code(),
        message: e.to_string(),
    }
}

/// Writes state.json outside any transaction.
fn save_state(fs: &dyn Fs, paths: &Paths, state: &State) -> Result<(), Failure> {
    let bytes = state.to_bytes().map_err(|e| state_failure(&e))?;
    fs.write_atomic(&paths.state_file(), &bytes)
        .map_err(|e| Failure {
            code: "E_DATA_WRITE",
            message: format!("could not save state.json: {e}"),
        })
}

/// Tries again to delete the font files `state.pending_deletes` lists: a file that is gone
/// leaves the list, and one still in use stays for a later run. A file that the bundled font's
/// registration names again is installed, not CMD Studio's to delete: it leaves the list and
/// stays; one whose registration cannot be read now is not touched, and the failed read goes
/// into `problems`. Returns whether the list changed. Writes nothing itself.
pub(crate) fn delete_pending(
    sys: System<'_>,
    ctx: &Ctx,
    state: &mut State,
    problems: &mut Vec<Failure>,
) -> bool {
    if state.pending_deletes.is_empty() {
        return false;
    }
    let ours = font_target(sys, &ctx.resources, NERD_FONT_FAMILY);
    let before = state.pending_deletes.clone();
    state.pending_deletes.retain(|file| {
        if let Some((_, dest, value_name)) = &ours {
            if same_path(file, dest) {
                match sys.fonts.registration(value_name) {
                    Ok(Some(named)) if same_path(Path::new(&named), file) => return false,
                    Ok(_) => {}
                    Err(e) => {
                        problems.push(Failure {
                            code: "E_MACHINE_READ",
                            message: format!(
                                "{} stays on the list of font files to delete later: whether a \
                                 font registration names it could not be read ({e})",
                                file.display()
                            ),
                        });
                        return true;
                    }
                }
            }
        }
        sys.fs.remove_file(file).is_err() && sys.fs.exists(file)
    });
    state.pending_deletes != before
}

/// The start's pending deletes for a command that changes the machine (apply, undo, and Task
/// 14's revert-all): tried again, and the shorter list saved outside any transaction. A failed
/// save is a problem of the start, not of the command: the files it no longer lists are gone,
/// and trying them again is harmless.
pub(crate) fn delete_pending_and_save(
    sys: System<'_>,
    ctx: &Ctx,
    state: &mut State,
    start: &mut StartReport,
) {
    if delete_pending(sys, ctx, state, &mut start.problems) {
        if let Err(f) = save_state(sys.fs, &ctx.paths, state) {
            start.problems.push(f);
        }
    }
}

/// Notes font files that were uninstalled but are still in use (`held_files`) in state.json's
/// pending deletes, in a write of its own after the transaction: the transaction's own
/// state.json op is rolled back with the rest. A state.json that cannot be read, or is from a
/// newer CMD Studio, is not written.
pub(crate) fn note_held(fs: &dyn Fs, paths: &Paths, held: &[PathBuf]) -> Result<(), Failure> {
    if held.is_empty() {
        return Ok(());
    }
    let mut state = State::load(fs, &paths.state_file()).map_err(|e| state_failure(&e))?;
    let before = state.pending_deletes.len();
    for file in held {
        state.note_pending_delete(file.clone());
    }
    if state.pending_deletes.len() == before {
        return Ok(());
    }
    save_state(fs, paths, &state)
}

/// What to say about a held font file that could not be noted for a later delete.
fn unnoted(file: &Path, why: &Failure) -> Failure {
    Failure {
        code: why.code,
        message: format!(
            "{} was uninstalled but is still in use, and could not be noted for a later delete \
             ({}: {}); delete it by hand once Windows Terminal is closed",
            file.display(),
            why.code,
            why.message
        ),
    }
}

/// A rollback that left `held` font files neither deleted nor noted for later did not finish
/// putting them away (spec §5.5): each becomes a failed op of the outcome, the `InstallFont` op
/// of `ops` it belongs to, so that `record_incomplete` keeps how to delete it by hand. A file no
/// op of `ops` names is told in the reason instead. `code` and `reason` stay the rollback's
/// cause.
fn rollback_with_unnoted(
    sys: System<'_>,
    ctx: &Ctx,
    ops: &[Op],
    held: &[PathBuf],
    why: &Failure,
    (code, mut reason, mut failed): (&'static str, String, Vec<(usize, Failure)>),
) -> Outcome {
    for file in held {
        let op = ops.iter().position(|op| match op {
            Op::InstallFont { family } => font_target(sys, &ctx.resources, family)
                .is_some_and(|(_, dest, _)| same_path(&dest, file)),
            _ => false,
        });
        match op {
            Some(index) => failed.push((index, unnoted(file, why))),
            None => reason.push_str(&format!("; {}", unnoted(file, why).message)),
        }
    }
    Outcome::RollbackIncomplete {
        code,
        reason,
        failed,
    }
}

/// Notes the font files a transaction's executor holds, after it ended (any outcome). When that
/// fails, a rollback's outcome says so (see [`rollback_with_unnoted`]); a committed one has no
/// failed ops, so one failure per file comes back for the caller to report.
fn after_held(
    sys: System<'_>,
    ctx: &Ctx,
    ops: &[Op],
    held: &[PathBuf],
    outcome: Outcome,
) -> (Outcome, Vec<Failure>) {
    let Err(why) = note_held(sys.fs, &ctx.paths, held) else {
        return (outcome, Vec::new());
    };
    match outcome {
        Outcome::RolledBack { code, reason } => (
            rollback_with_unnoted(sys, ctx, ops, held, &why, (code, reason, Vec::new())),
            Vec::new(),
        ),
        Outcome::RollbackIncomplete {
            code,
            reason,
            failed,
        } => (
            rollback_with_unnoted(sys, ctx, ops, held, &why, (code, reason, failed)),
            Vec::new(),
        ),
        Outcome::Committed => (outcome, held.iter().map(|f| unnoted(f, &why)).collect()),
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use crate::adapters::fake::FakeMachine;
    use crate::adapters::{Fs, WtLocation, WtVariant};
    use crate::model::Target;
    use crate::plan::NERD_FONT_FAMILY;
    use crate::resources::fake_resources;
    use crate::resources::NERD_FONT_FILE;
    use std::path::Path;

    pub(crate) const WT: &str = r"C:\Users\tester\AppData\Local\Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json";
    pub(crate) const SETTINGS: &str = include_str!("../tests/fixtures/wt/default.json");

    /// A machine with Windows Terminal, an AutoRun from another program, and
    /// OMP/Clink answering like the real ones.
    pub(crate) fn machine() -> (FakeMachine, Ctx) {
        let mut m = FakeMachine::new();
        m.fs.create_dir_all(Path::new(
            r"C:\Users\tester\AppData\Local\Microsoft\Windows",
        ))
        .unwrap_or(());
        m.fs.put(WT, SETTINGS);
        m.wt.locations = vec![WtLocation {
            variant: WtVariant::Store,
            installed: true,
            settings: PathBuf::from(WT),
        }];
        m.fonts = crate::adapters::fake::FakeFonts::new(
            r"C:\Users\tester\AppData\Local\Microsoft\Windows\Fonts",
            &["Arial"],
        );
        m.fonts.knows_file(NERD_FONT_FILE, NERD_FONT_FAMILY);
        m.process.answer("oh-my-posh.exe", "print", 0, "> ");
        let resources = fake_resources(&m.fs, Path::new(r"C:\Program Files\CMD Studio\resources"));
        let ctx = Ctx {
            paths: Paths {
                data: PathBuf::from(r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio"),
            },
            resources,
            now_ms: 1_791_694_709_000,
        };
        (m, ctx)
    }

    pub(crate) fn request(target: Target) -> Request {
        Request {
            target,
            set_wt_default_profile: true,
            lang: Lang::En,
            replace_foreign_clink: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{machine, request, WT};
    use super::*;
    use crate::adapters::{AutoRun, Fonts, Fs, RegValue, REG_EXPAND_SZ, REG_SZ};
    use crate::model::Target;
    use crate::plan::NERD_FONT_FAMILY;
    use crate::presets::terracotta;
    use crate::txn::{Journal, JournalStore, Stage};

    fn everything(m: &crate::adapters::fake::FakeMachine) -> impl PartialEq + std::fmt::Debug {
        (
            m.fs.file(WT),
            m.autorun.snapshot(),
            m.fonts.snapshot(),
            m.fonts.families().unwrap(),
        )
    }

    /// Every file on the fake disk but the lock file, which CMD Studio keeps in its own data
    /// folder (made, like the folder, by the first command that takes the lock).
    fn files_but_the_lock(
        m: &crate::adapters::fake::FakeMachine,
        ctx: &Ctx,
    ) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
        let mut files = m.fs.snapshot().0;
        files.remove(&ctx.paths.lock_file());
        files
    }

    #[test]
    fn apply_then_undo_is_byte_identical() {
        // The spec §8 Phase 1 criterion, on the fake machine.
        let (m, ctx) = machine();
        m.autorun
            .write_user(&RegValue::from_text(
                REG_EXPAND_SZ,
                r#""%USERPROFILE%\conda_hook.bat""#,
            ))
            .unwrap();
        let before = everything(&m);
        let prepared =
            prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        assert!(prepared
            .ops
            .iter()
            .any(|o| matches!(o, Op::InstallFont { .. })));
        assert_eq!(run_apply(m.system(), &ctx, prepared), Outcome::Committed);
        assert_ne!(everything(&m), before);
        // Ours goes first, in parentheses, and the other program's command follows unchanged
        // (spec §5.4); the value keeps its REG_EXPAND_SZ type.
        let text = m.autorun.read_user().unwrap().unwrap();
        assert_eq!(text.kind, REG_EXPAND_SZ);
        let value = text.text().unwrap();
        assert!(value.starts_with("(if exist \""), "{value}");
        assert!(
            value.ends_with(r#") & "%USERPROFILE%\conda_hook.bat""#),
            "{value}"
        );
        assert!(m
            .fonts
            .families()
            .unwrap()
            .contains(&NERD_FONT_FAMILY.to_string()));
        let state = State::load(m.system().fs, &ctx.paths.state_file()).unwrap();
        assert_eq!(
            state.applied.as_ref().unwrap().wt_font.as_deref(),
            Some(NERD_FONT_FAMILY)
        );

        let ctx2 = Ctx {
            now_ms: ctx.now_ms + 1,
            ..ctx.clone()
        };
        let UndoResult::Done { outcome, .. } = undo(m.system(), &ctx2).unwrap().result else {
            panic!("nothing undone")
        };
        assert_eq!(outcome, Outcome::Committed);
        assert_eq!(everything(&m), before);
        assert_eq!(m.system().fs.read(&ctx.paths.state_file()).unwrap(), None);
        assert_eq!(
            undo(m.system(), &ctx2).unwrap().result,
            UndoResult::NothingToUndo
        );
    }

    #[test]
    fn two_applies_undo_one_at_a_time() {
        let (m, ctx) = machine();
        let before = everything(&m);
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        assert_eq!(run_apply(m.system(), &ctx, p), Outcome::Committed);
        let after_first = everything(&m);
        let ctx2 = Ctx {
            now_ms: ctx.now_ms + 10,
            ..ctx.clone()
        };
        let p = prepare_apply(m.system(), &ctx2, &terracotta(), &request(Target::Wt)).unwrap();
        assert_eq!(run_apply(m.system(), &ctx2, p), Outcome::Committed);
        let ctx3 = Ctx {
            now_ms: ctx.now_ms + 20,
            ..ctx.clone()
        };
        undo(m.system(), &ctx3).unwrap();
        assert_eq!(everything(&m), after_first);
        let ctx4 = Ctx {
            now_ms: ctx.now_ms + 30,
            ..ctx.clone()
        };
        undo(m.system(), &ctx4).unwrap();
        assert_eq!(everything(&m), before);
    }

    #[test]
    fn a_broken_settings_file_stops_the_apply_before_anything_changes() {
        let (m, ctx) = machine();
        m.fs.put(WT, b"{ \"profiles\": [ ");
        let before = files_but_the_lock(&m, &ctx);
        let err =
            prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap_err();
        assert_eq!(err.code(), "E_WT_PARSE");
        assert_eq!(files_but_the_lock(&m, &ctx), before);
        assert_eq!(m.autorun.read_user().unwrap(), None);
    }

    #[test]
    fn arm64_is_refused() {
        let (mut m, ctx) = machine();
        m.host.info.arch = Arch::Arm64;
        assert_eq!(
            prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All))
                .unwrap_err()
                .code(),
            "E_UNSUPPORTED_PLATFORM"
        );
    }

    #[test]
    fn a_failed_verify_leaves_the_machine_as_it_was() {
        let (m, ctx) = machine();
        m.process
            .answer("oh-my-posh.exe", "print", 0, " CONFIG PARSE ERROR ");
        let before = everything(&m);
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        assert!(matches!(
            run_apply(m.system(), &ctx, p),
            Outcome::RolledBack {
                code: "E_VERIFY_OMP",
                ..
            }
        ));
        assert_eq!(everything(&m), before);
        assert_eq!(
            undo(m.system(), &ctx).unwrap().result,
            UndoResult::NothingToUndo
        );
    }

    #[test]
    fn a_crash_mid_apply_is_rolled_back_at_the_next_start() {
        let (m, ctx) = machine();
        let before = everything(&m);
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        let (ops, manifest) = (p.ops.clone(), p.manifest.clone());
        // Simulate a crash: capture and run ops by hand, stop after three.
        let root = ctx.paths.snapshots();
        let store = SnapshotStore::new(m.system().fs, &root);
        let mut journal = store.begin(manifest.clone());
        let mut exec = SystemExecutor::new(m.system(), &ctx.resources);
        use crate::txn::Executor;
        let snapshots = ops
            .iter()
            .enumerate()
            .map(|(i, op)| crate::txn::Snapshot {
                op_index: i,
                prior: exec.capture(op).unwrap(),
            })
            .collect();
        let mut j = Journal {
            id: manifest.id.clone(),
            ops: ops.clone(),
            snapshots,
            stage: Stage::Snapshotted,
        };
        journal.save(&j).unwrap();
        for (i, op) in ops.iter().take(3).enumerate() {
            exec.apply(op).unwrap();
            j.stage = Stage::Applying { done: i + 1 };
            journal.save(&j).unwrap();
        }
        // The crash ends the process, and with it the apply's lock.
        drop(p);
        let start = recover_at_start(m.system(), &ctx).unwrap();
        assert!(matches!(
            start.recovered,
            Some(Outcome::RolledBack {
                code: "E_TXN_INTERRUPTED",
                ..
            })
        ));
        assert_eq!(start.incomplete, None);
        assert_eq!(everything(&m), before);
        assert!(!m.system().fs.exists(&ctx.paths.omp_dir()));
        assert_eq!(
            recover_at_start(m.system(), &ctx).unwrap(),
            StartReport::default()
        );
    }

    fn later(ctx: &Ctx, ms: u64) -> Ctx {
        Ctx {
            now_ms: ctx.now_ms + ms,
            ..ctx.clone()
        }
    }

    /// Captures every op of `p` and runs the first `done`, journaling each step as the engine
    /// does, then stops: what a crash leaves.
    fn crash_after(m: &crate::adapters::fake::FakeMachine, ctx: &Ctx, p: &Prepared, done: usize) {
        use crate::txn::Executor;
        let root = ctx.paths.snapshots();
        let store = SnapshotStore::new(m.system().fs, &root);
        let mut journal = store.begin(p.manifest.clone());
        let mut exec = SystemExecutor::new(m.system(), &ctx.resources);
        let snapshots = p
            .ops
            .iter()
            .enumerate()
            .map(|(i, op)| crate::txn::Snapshot {
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
    fn a_second_run_while_one_holds_the_lock_is_busy_and_changes_nothing() {
        // Ruling Q10: one lock for every command that changes the machine; a second fails fast.
        let (m, ctx) = machine();
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        // The first run is cut off half-way, but its process still lives and holds the lock.
        crash_after(&m, &ctx, &p, 3);
        let half = (everything(&m), files_but_the_lock(&m, &ctx));
        assert!(m.fs.exists(&ctx.paths.omp_dir()));

        let busy = |e: ActionError| {
            assert_eq!(e.code(), "E_BUSY", "{e:?}");
            assert_eq!(e.to_string(), BUSY_EN);
            assert_eq!(e.message_th().as_deref(), Some(BUSY_TH));
        };
        // A scanner or an indexer that has the file open looks the same, so the text names the
        // file and says it may be another program.
        for text in [BUSY_EN, BUSY_TH] {
            assert!(text.contains("cmdstudio.lock"), "{text}");
        }
        assert!(BUSY_EN.contains("another program"), "{BUSY_EN}");
        busy(
            prepare_apply(
                m.system(),
                &later(&ctx, 1),
                &terracotta(),
                &request(Target::All),
            )
            .unwrap_err(),
        );
        busy(undo(m.system(), &later(&ctx, 2)).unwrap_err());
        // A recovery that ran now would roll back the first run's transaction under it.
        busy(recover_at_start(m.system(), &later(&ctx, 3)).unwrap_err());
        assert_eq!(
            (everything(&m), files_but_the_lock(&m, &ctx)),
            half,
            "nothing changed"
        );

        // Once the first run is gone, the next start rolls its transaction back.
        drop(p);
        let start = recover_at_start(m.system(), &later(&ctx, 4)).unwrap();
        assert!(
            matches!(start.recovered, Some(Outcome::RolledBack { .. })),
            "{start:?}"
        );
        assert!(!m.fs.exists(&ctx.paths.omp_dir()));
    }

    #[test]
    fn the_lock_is_let_go_on_every_exit_path() {
        let (m, ctx) = machine();
        let lock = ctx.paths.lock_file();
        let free = || {
            let held = m.fs.try_lock(&lock).unwrap();
            assert!(held.is_some(), "the lock is still held");
        };
        // A refused apply.
        m.fs.put(WT, b"{ \"profiles\": [ ");
        prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap_err();
        free();
        m.fs.put(WT, super::testing::SETTINGS);
        // A prepared apply holds it until it runs or is dropped.
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        assert!(
            m.fs.try_lock(&lock).unwrap().is_none(),
            "held while prepared"
        );
        drop(p);
        free();
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        assert_eq!(run_apply(m.system(), &ctx, p), Outcome::Committed);
        free();
        // Undo and recovery, done or refused.
        undo(m.system(), &later(&ctx, 1)).unwrap();
        free();
        recover_at_start(m.system(), &later(&ctx, 2)).unwrap();
        free();
        let root = ctx.paths.snapshots();
        let newest = SnapshotStore::new(&m.fs, &root)
            .ids()
            .unwrap()
            .pop()
            .unwrap();
        m.fs.fail_reads_of(ctx.paths.snapshots().join(newest).join("manifest.json"));
        let e = undo(m.system(), &later(&ctx, 3)).unwrap_err();
        assert_eq!(e.code(), "E_SNAPSHOT_READ", "{e:?}");
        free();
        assert_eq!(m.fs.file(&lock), Some(Vec::new()), "the file stays");
    }

    #[test]
    fn a_lock_file_that_cannot_be_opened_is_a_write_failure() {
        let (m, ctx) = machine();
        m.fs.fail_writes_to(ctx.paths.lock_file());
        let e = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap_err();
        assert_eq!(e.code(), "E_DATA_WRITE", "{e:?}");
        assert!(e.to_string().contains("cmdstudio.lock"), "{e}");
        assert_eq!(recover_at_start(m.system(), &ctx).unwrap_err(), e);
        assert_eq!(m.autorun.read_user().unwrap(), None);
    }

    fn apply(m: &crate::adapters::fake::FakeMachine, ctx: &Ctx, target: Target) -> String {
        let p = prepare_apply(m.system(), ctx, &terracotta(), &request(target)).unwrap();
        let id = p.manifest.id.clone();
        assert_eq!(run_apply(m.system(), ctx, p), Outcome::Committed);
        id
    }

    fn point(ctx: &Ctx, id: &str, file: &str) -> PathBuf {
        ctx.paths.snapshots().join(id).join(file)
    }

    fn ids_and_handling(report: &StartReport) -> Vec<(String, UnreadableKind, Handled)> {
        report
            .unreadable
            .iter()
            .map(|u| (u.point.id.clone(), u.point.kind, u.handled.clone()))
            .collect()
    }

    #[test]
    fn unreadable_restore_points_are_reported_and_only_those_that_stay_broken_set_aside() {
        // Rulings Q3/Q5/Q28: at the start of a command, after crash recovery.
        let (m, ctx) = machine();
        let first = apply(&m, &ctx, Target::All);
        let second = apply(&m, &later(&ctx, 1), Target::Wt);
        // The first one's journal cannot be read right now (a lock, say); a payload file of the
        // second is gone for good.
        m.fs.fail_reads_of(point(&ctx, &first, "journal.json"));
        m.fs.remove_file(&point(&ctx, &second, r"payload\0.bin"))
            .unwrap();

        let start = recover_at_start(m.system(), &later(&ctx, 2)).unwrap();
        assert_eq!(start.recovered, None);
        assert_eq!(
            ids_and_handling(&start),
            vec![
                (
                    first.clone(),
                    UnreadableKind::CouldNotRead,
                    Handled::LeftForLater
                ),
                (second.clone(), UnreadableKind::Missing, Handled::SetAside),
            ]
        );
        assert!(start
            .unreadable
            .iter()
            .all(|u| u.point.failure.code == "E_SNAPSHOT_READ"));
        assert!(m.fs.exists(&point(&ctx, &first, "journal.json")), "left");
        assert!(
            m.fs.exists(&point(&ctx, &second, "journal.unreadable.json")),
            "set aside, kept for a look"
        );

        // Reported once: the next start lists only the one that may still pass.
        let again = recover_at_start(m.system(), &later(&ctx, 3)).unwrap();
        assert_eq!(
            ids_and_handling(&again),
            vec![(
                first.clone(),
                UnreadableKind::CouldNotRead,
                Handled::LeftForLater
            )]
        );
        // An undo does not go past a restore point it cannot read: the second apply was set
        // aside, so the first is next, and it cannot be read now.
        let e = undo(m.system(), &later(&ctx, 4)).unwrap_err();
        assert_eq!(e.code(), "E_SNAPSHOT_READ", "{e:?}");
        assert!(e.to_string().contains(&first), "{e}");
        // Once it can be read, it is undone.
        m.fs.stop_failing(point(&ctx, &first, "journal.json"));
        let undone = undo(m.system(), &later(&ctx, 5)).unwrap();
        assert!(undone.start.unreadable.is_empty(), "{undone:?}");
        assert!(matches!(
            undone.result,
            UndoResult::Done { ref undid, outcome: Outcome::Committed } if *undid == first
        ));
    }

    #[test]
    fn apply_and_undo_report_what_their_own_start_found() {
        let (m, ctx) = machine();
        let first = apply(&m, &ctx, Target::All);
        m.fs.put(point(&ctx, &first, "manifest.json"), b"not json");
        let p = prepare_apply(
            m.system(),
            &later(&ctx, 1),
            &terracotta(),
            &request(Target::Wt),
        )
        .unwrap();
        assert_eq!(
            ids_and_handling(&p.start),
            vec![(first.clone(), UnreadableKind::Damaged, Handled::SetAside)]
        );
        let second = p.manifest.id.clone();
        assert_eq!(
            run_apply(m.system(), &later(&ctx, 1), p),
            Outcome::Committed
        );

        m.fs.put(point(&ctx, &second, "manifest.json"), b"[]");
        let undone = undo(m.system(), &later(&ctx, 2)).unwrap();
        assert_eq!(
            ids_and_handling(&undone.start),
            vec![(second, UnreadableKind::Damaged, Handled::SetAside)]
        );
        // Both applies were set aside, so there is nothing left to undo.
        assert_eq!(undone.result, UndoResult::NothingToUndo);
    }

    #[test]
    fn a_crash_is_rolled_back_before_its_restore_point_is_set_aside() {
        // A restore point listed for its manifest alone still has a journal that reads: recover,
        // then set aside (snapshot::unreadable's own note).
        let (m, ctx) = machine();
        let before = everything(&m);
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        crash_after(&m, &ctx, &p, 3);
        let id = p.manifest.id.clone();
        drop(p);
        m.fs.put(point(&ctx, &id, "manifest.json"), b"{");
        let start = recover_at_start(m.system(), &later(&ctx, 1)).unwrap();
        assert!(
            matches!(
                start.recovered,
                Some(Outcome::RolledBack {
                    code: "E_TXN_INTERRUPTED",
                    ..
                })
            ),
            "{start:?}"
        );
        assert_eq!(everything(&m), before);
        assert!(!m.fs.exists(&ctx.paths.omp_dir()));
        assert_eq!(
            ids_and_handling(&start),
            vec![(id, UnreadableKind::Damaged, Handled::SetAside)]
        );
        assert_eq!(start.incomplete, None);
    }

    #[test]
    fn a_journal_that_cannot_be_read_is_recorded_only_once_it_is_set_aside() {
        let (m, ctx) = machine();
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        crash_after(&m, &ctx, &p, 3);
        let id = p.manifest.id.clone();
        drop(p);

        // Locked now: reported, left, and tried again at the next start.
        m.fs.fail_reads_of(point(&ctx, &id, "journal.json"));
        let start = recover_at_start(m.system(), &later(&ctx, 1)).unwrap();
        assert!(
            matches!(
                &start.recovered,
                Some(Outcome::RollbackIncomplete { code: "E_SNAPSHOT_READ", failed, .. })
                    if failed.is_empty()
            ),
            "{start:?}"
        );
        assert_eq!(
            ids_and_handling(&start),
            vec![(
                id.clone(),
                UnreadableKind::CouldNotRead,
                Handled::LeftForLater
            )]
        );
        assert_eq!(start.incomplete, None, "no record: it is tried again");
        assert!(!m.fs.exists(&point(&ctx, &id, "incomplete.json")));
        m.fs.stop_failing(point(&ctx, &id, "journal.json"));
        let start = recover_at_start(m.system(), &later(&ctx, 2)).unwrap();
        assert!(
            matches!(start.recovered, Some(Outcome::RolledBack { .. })),
            "{start:?}"
        );
        assert!(!m.fs.exists(&ctx.paths.omp_dir()));

        // Damaged: reported, set aside, and recorded, so it is still said after a restart.
        let (m, ctx) = machine();
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        crash_after(&m, &ctx, &p, 3);
        let id = p.manifest.id.clone();
        drop(p);
        m.fs.put(point(&ctx, &id, "journal.json"), b"{ damaged");
        let start = recover_at_start(m.system(), &later(&ctx, 1)).unwrap();
        assert!(matches!(
            start.recovered,
            Some(Outcome::RollbackIncomplete {
                code: "E_SNAPSHOT_READ",
                ..
            })
        ));
        assert_eq!(
            ids_and_handling(&start),
            vec![(id.clone(), UnreadableKind::Damaged, Handled::SetAside)]
        );
        let record = start.incomplete.unwrap();
        assert_eq!(
            (record.id.as_str(), record.code.as_str()),
            (id.as_str(), "E_SNAPSHOT_READ")
        );
        // Nothing says which ops ran, so the record's one step is the whole change, with the
        // shared clean-up advice.
        assert_eq!(record.steps.len(), 1, "{record:?}");
        let step = &record.steps[0];
        assert!(step.target.contains(&id), "{step:?}");
        assert_eq!(step.code, "E_SNAPSHOT_READ");
        assert_eq!(
            (step.fix.as_str(), step.fix_th.as_str()),
            (CLEANUP_FIX_EN, CLEANUP_FIX_TH)
        );
        assert!(
            !start.recovery_pending,
            "set aside: it does not stop later commands"
        );
        let later_start = recover_at_start(m.system(), &later(&ctx, 2)).unwrap();
        assert_eq!(
            later_start.recovered, None,
            "set aside: not recovered again"
        );
        assert_eq!(later_start.incomplete, Some(record));
    }

    #[test]
    fn a_crash_that_cannot_be_checked_yet_stops_every_change_until_it_can() {
        // Going on would make the new transaction the newest restore point, and recovery only
        // ever looks at the newest: the crashed one would never be rolled back.
        let (m, ctx) = machine();
        let mine = RegValue::from_text(REG_SZ, "echo mine");
        m.autorun.write_user(&mine).unwrap();
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        let set_autorun = p
            .ops
            .iter()
            .position(|o| matches!(o, Op::SetAutoRun { .. }))
            .unwrap();
        crash_after(&m, &ctx, &p, set_autorun + 1);
        let id = p.manifest.id.clone();
        drop(p);
        assert_ne!(m.autorun.read_user().unwrap(), Some(mine.clone()));
        let journal = point(&ctx, &id, "journal.json");
        m.fs.fail_reads_of(&journal);
        let crashed = (everything(&m), files_but_the_lock(&m, &ctx));

        // The start that `recover_at_start` runs says so and goes on.
        let start = recover_at_start(m.system(), &later(&ctx, 1)).unwrap();
        assert!(start.recovery_pending, "{start:?}");
        // A change is refused, and nothing changes.
        let refused = |e: ActionError| {
            assert_eq!(e.code(), "E_SNAPSHOT_READ", "{e:?}");
            assert!(e.to_string().contains("try again"), "{e}");
            assert!(e.message_th().is_some());
        };
        refused(
            prepare_apply(
                m.system(),
                &later(&ctx, 2),
                &terracotta(),
                &request(Target::All),
            )
            .unwrap_err(),
        );
        refused(undo(m.system(), &later(&ctx, 3)).unwrap_err());
        assert_eq!((everything(&m), files_but_the_lock(&m, &ctx)), crashed);

        // Once the journal can be read, the next start rolls the crash back.
        m.fs.stop_failing(&journal);
        let start = recover_at_start(m.system(), &later(&ctx, 4)).unwrap();
        assert!(
            matches!(start.recovered, Some(Outcome::RolledBack { .. })),
            "{start:?}"
        );
        assert!(!start.recovery_pending);
        assert_eq!(m.autorun.read_user().unwrap(), Some(mine));
        apply(&m, &later(&ctx, 5), Target::All);
    }

    #[test]
    fn restore_points_keep_their_order_when_the_clock_goes_back() {
        let (m, ctx) = machine();
        let before = everything(&m);
        let first = apply(&m, &ctx, Target::All);
        let after_first = everything(&m);
        let back = |ms: u64| Ctx {
            now_ms: ctx.now_ms - 5_000 + ms,
            ..ctx.clone()
        };
        let second = apply(&m, &back(0), Target::Wt);
        assert!(second > first, "{second} is not after {first}");
        let undone = |ctx: &Ctx| match undo(m.system(), ctx).unwrap().result {
            UndoResult::Done { undid, outcome } => {
                assert_eq!(outcome, Outcome::Committed);
                undid
            }
            UndoResult::NothingToUndo => panic!("nothing undone"),
        };
        assert_eq!(undone(&back(1)), second);
        assert_eq!(everything(&m), after_first);
        assert_eq!(undone(&back(2)), first);
        assert_eq!(everything(&m), before);
    }

    #[test]
    fn run_apply_refuses_a_data_folder_other_than_the_one_it_was_prepared_in() {
        let (m, ctx) = machine();
        let before = (everything(&m), files_but_the_lock(&m, &ctx));
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        let elsewhere = Ctx {
            paths: Paths {
                data: PathBuf::from(r"C:\elsewhere"),
            },
            ..ctx.clone()
        };
        let out = run_apply(m.system(), &elsewhere, p);
        assert!(
            matches!(&out, Outcome::RolledBack { code: "E_DATA_WRITE", reason } if reason.contains(r"C:\elsewhere")),
            "{out:?}"
        );
        assert_eq!((everything(&m), files_but_the_lock(&m, &ctx)), before);
        assert!(!m.fs.exists(Path::new(r"C:\elsewhere")));
        assert!(
            m.fs.try_lock(&ctx.paths.lock_file()).unwrap().is_some(),
            "the lock is let go"
        );
    }

    #[test]
    fn a_file_no_op_names_is_told_in_the_reason() {
        let (m, ctx) = machine();
        let stray = PathBuf::from(r"C:\Users\tester\AppData\Local\Microsoft\Windows\Fonts\x.ttf");
        let why = Failure {
            code: "E_DATA_WRITE",
            message: "denied".into(),
        };
        let out = rollback_with_unnoted(
            m.system(),
            &ctx,
            &[Op::SetAutoRun { value: None }],
            std::slice::from_ref(&stray),
            &why,
            ("E_VERIFY_OMP", "cause".into(), Vec::new()),
        );
        let Outcome::RollbackIncomplete {
            code,
            reason,
            failed,
        } = out
        else {
            panic!("{out:?}")
        };
        assert_eq!(code, "E_VERIFY_OMP");
        assert!(failed.is_empty());
        assert!(reason.starts_with("cause; "), "{reason}");
        assert!(reason.contains(&stray.display().to_string()), "{reason}");
        assert!(reason.contains("E_DATA_WRITE: denied"), "{reason}");
    }

    /// The fake disk, but `incomplete.json` can never be written.
    struct NoRecords<'a>(&'a crate::adapters::fake::FakeFs);

    impl Fs for NoRecords<'_> {
        fn read(&self, path: &Path) -> Result<Option<Vec<u8>>, crate::adapters::AdapterError> {
            self.0.read(path)
        }
        fn write_atomic(
            &self,
            path: &Path,
            bytes: &[u8],
        ) -> Result<(), crate::adapters::AdapterError> {
            if path.file_name() == Some(std::ffi::OsStr::new("incomplete.json")) {
                return Err(crate::adapters::AdapterError::new(
                    path.display().to_string(),
                    "denied (test)",
                ));
            }
            self.0.write_atomic(path, bytes)
        }
        fn remove_file(&self, path: &Path) -> Result<(), crate::adapters::AdapterError> {
            self.0.remove_file(path)
        }
        fn exists(&self, path: &Path) -> bool {
            self.0.exists(path)
        }
        fn is_dir(&self, path: &Path) -> bool {
            self.0.is_dir(path)
        }
        fn create_dir_all(&self, path: &Path) -> Result<(), crate::adapters::AdapterError> {
            self.0.create_dir_all(path)
        }
        fn remove_dir(&self, path: &Path) -> Result<(), crate::adapters::AdapterError> {
            self.0.remove_dir(path)
        }
        fn remove_dir_all(&self, path: &Path) -> Result<(), crate::adapters::AdapterError> {
            self.0.remove_dir_all(path)
        }
        fn rename(&self, from: &Path, to: &Path) -> Result<(), crate::adapters::AdapterError> {
            self.0.rename(from, to)
        }
        fn list_dir(
            &self,
            path: &Path,
        ) -> Result<Vec<crate::adapters::DirEntry>, crate::adapters::AdapterError> {
            self.0.list_dir(path)
        }
        fn try_lock(
            &self,
            path: &Path,
        ) -> Result<Option<crate::adapters::FsLock>, crate::adapters::AdapterError> {
            self.0.try_lock(path)
        }
    }

    /// The fake fonts, but whether the registration key exists cannot be read.
    struct KeyUnreadable<'a>(&'a crate::adapters::fake::FakeFonts);

    impl Fonts for KeyUnreadable<'_> {
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
            self.0.unregister(value_name)
        }
        fn registration_key_exists(&self) -> Result<bool, AdapterError> {
            Err(AdapterError::new("Fonts key", "denied (test)"))
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
    fn a_fonts_key_that_cannot_be_read_stops_the_apply_with_a_read_code() {
        // Whether CMD Studio makes the key is recorded before its first font, so revert all
        // deletes only a key it made; not knowing it, the apply does not go on.
        let (m, ctx) = machine();
        let before = everything(&m);
        let fonts = KeyUnreadable(&m.fonts);
        let sys = System {
            fonts: &fonts,
            ..m.system()
        };
        let e = prepare_apply(sys, &ctx, &terracotta(), &request(Target::All)).unwrap_err();
        assert_eq!(e.code(), "E_MACHINE_READ", "{e:?}");
        assert!(e.to_string().contains("Fonts key"), "{e}");
        assert_eq!(everything(&m), before);
    }

    #[test]
    fn an_undo_whose_record_cannot_be_written_says_so() {
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        // The undo cannot delete init.lua, and its rollback cannot write it back.
        let init = ctx.paths.generated().join("init.lua");
        m.fs.fail_removes_of(&init);
        m.fs.fail_writes_to(&init);
        let fs = NoRecords(&m.fs);
        let sys = System {
            fs: &fs,
            ..m.system()
        };
        let undone = undo(sys, &later(&ctx, 1)).unwrap();
        assert!(
            matches!(
                undone.result,
                UndoResult::Done {
                    outcome: Outcome::RollbackIncomplete { .. },
                    ..
                }
            ),
            "{undone:?}"
        );
        assert_eq!(
            undone.problems.iter().map(|f| f.code).collect::<Vec<_>>(),
            ["E_SNAPSHOT_WRITE"],
            "{undone:?}"
        );
        assert!(undone.problems[0].message.contains("incomplete.json"));
    }

    /// An apply whose rollback cannot put init.lua back: `RollbackIncomplete`, with a record.
    fn apply_that_cannot_roll_back(m: &crate::adapters::fake::FakeMachine, ctx: &Ctx) -> PathBuf {
        let init = ctx.paths.generated().join("init.lua");
        if m.fs.file(&init).is_none() {
            m.fs.put(&init, b"old");
        }
        m.fs.fail_writes_to(&init);
        let p = prepare_apply(m.system(), ctx, &terracotta(), &request(Target::All)).unwrap();
        assert!(matches!(
            run_apply(m.system(), ctx, p),
            Outcome::RollbackIncomplete { .. }
        ));
        init
    }

    #[test]
    fn undo_is_refused_while_a_rollback_that_did_not_finish_needs_a_hand() {
        // Undoing the apply before it would hide the record of what the newer one left.
        let (m, ctx) = machine();
        let first = apply(&m, &ctx, Target::All);
        let init = apply_that_cannot_roll_back(&m, &later(&ctx, 1));
        let before = (everything(&m), files_but_the_lock(&m, &ctx));
        let record = pending_incomplete(m.system(), &ctx).unwrap().unwrap();

        let e = undo(m.system(), &later(&ctx, 2)).unwrap_err();
        assert_eq!(e.code(), "E_ROLLBACK_INCOMPLETE");
        assert_eq!(e, ActionError::PendingIncomplete(record.clone()));
        let step = &record.steps[0];
        assert_eq!(step.target, format!("file {}", init.display()));
        assert!(
            step.fix.starts_with("Copy ") && step.fix_th.starts_with("คัดลอก "),
            "{step:?}"
        );
        let text = e.to_string();
        assert!(text.contains(&record.id), "{text}");
        assert!(e.message_th().unwrap().contains(&record.id));
        assert_eq!(
            (everything(&m), files_but_the_lock(&m, &ctx)),
            before,
            "nothing was undone"
        );

        // A newer transaction ends it: the record is no longer the newest news.
        m.fs.stop_failing(&init);
        let third = apply(&m, &later(&ctx, 3), Target::Wt);
        let UndoResult::Done { undid, outcome } = undo(m.system(), &later(&ctx, 4)).unwrap().result
        else {
            panic!("nothing undone")
        };
        assert_eq!((undid, outcome), (third, Outcome::Committed));
        let UndoResult::Done { undid, .. } = undo(m.system(), &later(&ctx, 5)).unwrap().result
        else {
            panic!("nothing undone")
        };
        assert_eq!(undid, first);
    }

    #[test]
    fn a_record_that_cannot_be_read_is_reported_not_dropped() {
        // Ruling Q9: incomplete.json is the one record that says what to fix by hand.
        let (m, ctx) = machine();
        apply_that_cannot_roll_back(&m, &ctx);
        let record = pending_incomplete(m.system(), &ctx).unwrap().unwrap();
        let file = point(&ctx, &record.id, "incomplete.json");
        for (damage, kind) in [(true, "damaged"), (false, "locked")] {
            if damage {
                m.fs.put(&file, b"{ not json");
            } else {
                m.fs.fail_reads_of(&file);
            }
            let start = recover_at_start(m.system(), &later(&ctx, 1)).unwrap();
            assert_eq!(start.incomplete, None, "{kind}");
            assert!(
                start
                    .problems
                    .iter()
                    .any(|f| f.code == "E_SNAPSHOT_READ" && f.message.contains("incomplete.json")),
                "{kind}: {start:?}"
            );
            let e = pending_incomplete(m.system(), &ctx).unwrap_err();
            assert_eq!(e.code(), "E_SNAPSHOT_READ", "{kind}");
            // Not knowing whether a rollback is pending, undo does not go on.
            let e = undo(m.system(), &later(&ctx, 2)).unwrap_err();
            assert_eq!(e.code(), "E_SNAPSHOT_READ", "{kind}: {e:?}");
        }
    }

    #[test]
    fn a_state_file_that_is_newer_or_cannot_be_read_stops_apply_and_undo_untouched() {
        // Ruling Q26: never overwrite or rename it; it may hold a baseline only it knows.
        let newer = br#"{"version": 2, "applied": null, "baseline": {}, "futureField": 1}"#;
        for newer_one in [true, false] {
            let (m, ctx) = machine();
            apply(&m, &ctx, Target::All);
            let state_file = ctx.paths.state_file();
            if newer_one {
                m.fs.put(&state_file, newer);
            } else {
                m.fs.fail_reads_of(&state_file);
            }
            let before = (everything(&m), files_but_the_lock(&m, &ctx));
            let e = prepare_apply(
                m.system(),
                &later(&ctx, 1),
                &terracotta(),
                &request(Target::Wt),
            )
            .unwrap_err();
            assert_eq!(e.code(), "E_STATE_INVALID", "{e:?}");
            let e = undo(m.system(), &later(&ctx, 2)).unwrap_err();
            assert_eq!(e.code(), "E_STATE_INVALID", "{e:?}");
            if newer_one {
                assert!(e.to_string().contains("newer CMD Studio"), "{e}");
            }
            assert_eq!(
                (everything(&m), files_but_the_lock(&m, &ctx)),
                before,
                "nothing changed, state.json included"
            );
        }
    }

    #[test]
    fn a_damaged_state_file_stops_an_apply_but_not_an_undo() {
        // Ruling Q26 leaves a damaged one to the brief: an apply plans from it, so it stops; an
        // undo puts back the state.json its apply found, which replaces the damaged one.
        let (m, ctx) = machine();
        let before = everything(&m);
        apply(&m, &ctx, Target::All);
        m.fs.put(ctx.paths.state_file(), b"garbage");
        let e = prepare_apply(
            m.system(),
            &later(&ctx, 1),
            &terracotta(),
            &request(Target::Wt),
        )
        .unwrap_err();
        assert_eq!(e.code(), "E_STATE_INVALID");
        let undone = undo(m.system(), &later(&ctx, 2)).unwrap();
        assert!(matches!(
            undone.result,
            UndoResult::Done {
                outcome: Outcome::Committed,
                ..
            }
        ));
        assert_eq!(everything(&m), before);
        assert_eq!(m.fs.file(ctx.paths.state_file()), None);
        // The font files it may list for a later delete could not be read: said, not dropped.
        assert!(
            undone
                .start
                .problems
                .iter()
                .any(|f| f.code == "E_STATE_INVALID"),
            "{undone:?}"
        );
    }

    fn font_file(m: &crate::adapters::fake::FakeMachine) -> PathBuf {
        m.fonts
            .user_fonts_dir()
            .join(crate::resources::NERD_FONT_FILE)
    }

    fn pending(m: &crate::adapters::fake::FakeMachine, ctx: &Ctx) -> Vec<PathBuf> {
        State::load(&m.fs, &ctx.paths.state_file())
            .unwrap()
            .pending_deletes
    }

    fn registered(m: &crate::adapters::fake::FakeMachine) -> bool {
        m.fonts
            .registration(&format!("{NERD_FONT_FAMILY} (TrueType)"))
            .unwrap()
            .is_some()
    }

    #[test]
    fn a_font_file_still_in_use_after_an_undo_is_deleted_by_a_later_run() {
        // Rulings Q4/Q25b: noted in state.json outside the undo's transaction, tried again at
        // the start of the next apply, undo or revert-all, and dropped from the list once gone.
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        let file = font_file(&m);
        m.fs.fail_removes_of(&file);
        let undone = undo(m.system(), &later(&ctx, 1)).unwrap();
        assert!(matches!(
            undone.result,
            UndoResult::Done {
                outcome: Outcome::Committed,
                ..
            }
        ));
        assert!(undone.problems.is_empty(), "{undone:?}");
        assert!(!registered(&m), "the font is uninstalled");
        assert!(m.fs.exists(&file), "but its file is still held");
        assert_eq!(pending(&m, &ctx), vec![file.clone()]);

        // Still held at the next start: kept.
        undo(m.system(), &later(&ctx, 2)).unwrap();
        assert!(m.fs.exists(&file));
        assert_eq!(pending(&m, &ctx), vec![file.clone()]);
        // Free now: deleted, and off the list.
        m.fs.stop_failing(&file);
        let p = prepare_apply(
            m.system(),
            &later(&ctx, 3),
            &terracotta(),
            &request(Target::All),
        )
        .unwrap();
        assert!(!m.fs.exists(&file));
        assert_eq!(pending(&m, &ctx), Vec::<PathBuf>::new());
        assert!(p.state_after.pending_deletes.is_empty());
    }

    #[test]
    fn an_apply_that_installs_the_font_again_takes_its_file_off_the_list() {
        let (m, ctx) = machine();
        apply(&m, &ctx, Target::All);
        let file = font_file(&m);
        m.fs.fail_removes_of(&file);
        undo(m.system(), &later(&ctx, 1)).unwrap();
        assert_eq!(pending(&m, &ctx), vec![file.clone()]);
        // The next apply installs the font from the same file, which is still held.
        let p = prepare_apply(
            m.system(),
            &later(&ctx, 2),
            &terracotta(),
            &request(Target::All),
        )
        .unwrap();
        assert!(p.ops.iter().any(|o| matches!(o, Op::InstallFont { .. })));
        assert!(p.state_after.pending_deletes.is_empty());
        assert_eq!(
            run_apply(m.system(), &later(&ctx, 2), p),
            Outcome::Committed
        );
        assert!(registered(&m));
        assert_eq!(pending(&m, &ctx), Vec::<PathBuf>::new());
        // A later start does not delete the installed font's file.
        m.fs.stop_failing(&file);
        drop(
            prepare_apply(
                m.system(),
                &later(&ctx, 3),
                &terracotta(),
                &request(Target::Wt),
            )
            .unwrap(),
        );
        assert!(m.fs.exists(&file));
    }

    #[test]
    fn a_pending_file_that_a_registration_names_again_is_not_deleted() {
        // Not CMD Studio's to delete any more; and one whose registration cannot be read is
        // kept for a later try.
        let (m, ctx) = machine();
        let file = font_file(&m);
        let value = format!("{NERD_FONT_FAMILY} (TrueType)");
        m.fs.put(&file, b"font-bytes");
        let state = State {
            pending_deletes: vec![file.clone()],
            ..State::default()
        };
        m.fs.put(ctx.paths.state_file(), state.to_bytes().unwrap());
        m.fonts.fail_registration_of(&value);
        let undone = undo(m.system(), &ctx).unwrap();
        assert!(m.fs.exists(&file));
        assert_eq!(pending(&m, &ctx), vec![file.clone()], "kept for later");
        // The read that failed is said.
        let [problem] = undone.start.problems.as_slice() else {
            panic!("{undone:?}")
        };
        assert_eq!(problem.code, "E_MACHINE_READ");
        assert!(
            problem.message.contains(&file.display().to_string()),
            "{problem:?}"
        );

        let (m, ctx) = machine();
        m.fs.put(&file, b"font-bytes");
        m.fs.put(ctx.paths.state_file(), state.to_bytes().unwrap());
        m.fonts.register(&value, file.to_str().unwrap()).unwrap();
        undo(m.system(), &ctx).unwrap();
        assert!(m.fs.exists(&file), "a registered font's file stays");
        assert_eq!(
            pending(&m, &ctx),
            Vec::<PathBuf>::new(),
            "and is off the list"
        );
    }

    #[test]
    fn a_font_file_still_in_use_after_a_rollback_is_noted() {
        let (m, ctx) = machine();
        m.process
            .answer("oh-my-posh.exe", "print", 0, " CONFIG PARSE ERROR ");
        let file = font_file(&m);
        m.fs.fail_removes_of(&file);
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        assert!(matches!(
            run_apply(m.system(), &ctx, p),
            Outcome::RolledBack {
                code: "E_VERIFY_OMP",
                ..
            }
        ));
        assert!(!registered(&m));
        assert_eq!(pending(&m, &ctx), vec![file]);
    }

    #[test]
    fn a_crash_recovery_notes_a_font_file_still_in_use() {
        let (m, ctx) = machine();
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        let font_op = p
            .ops
            .iter()
            .position(|o| matches!(o, Op::InstallFont { .. }))
            .unwrap();
        crash_after(&m, &ctx, &p, font_op + 1);
        drop(p);
        assert!(registered(&m));
        let file = font_file(&m);
        m.fs.fail_removes_of(&file);
        let start = recover_at_start(m.system(), &later(&ctx, 1)).unwrap();
        assert!(
            matches!(start.recovered, Some(Outcome::RolledBack { .. })),
            "{start:?}"
        );
        assert!(start.problems.is_empty(), "{start:?}");
        assert!(!registered(&m));
        assert_eq!(pending(&m, &ctx), vec![file]);
    }

    #[test]
    fn a_held_font_file_that_cannot_be_noted_leaves_the_rollback_incomplete() {
        // The file is neither deleted nor noted for later: the rollback did not finish putting
        // it away, so the user is told to delete it by hand, after a restart too.
        let (m, ctx) = machine();
        let file = font_file(&m);
        m.fs.fail_removes_of(&file);
        // state.json can be written neither by the apply (which fails and rolls back) nor
        // afterwards.
        m.fs.fail_writes_to(ctx.paths.state_file());
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        let font_op = p
            .ops
            .iter()
            .position(|o| matches!(o, Op::InstallFont { .. }))
            .unwrap();
        let out = run_apply(m.system(), &ctx, p);
        let Outcome::RollbackIncomplete { code, failed, .. } = &out else {
            panic!("{out:?}")
        };
        assert_eq!(
            *code, "E_DATA_WRITE",
            "the cause is the failed state.json write"
        );
        assert_eq!(failed.len(), 1, "{out:?}");
        let (index, failure) = &failed[0];
        assert_eq!(*index, font_op);
        assert_eq!(failure.code, "E_DATA_WRITE");
        assert!(
            failure.message.contains(&file.display().to_string()),
            "{failure:?}"
        );
        assert!(!registered(&m));
        let record = pending_incomplete(m.system(), &ctx).unwrap().unwrap();
        let step = &record.steps[0];
        assert_eq!(step.target, format!("font {NERD_FONT_FAMILY}"));
        assert!(
            step.fix.contains(&format!("delete {}", file.display())),
            "{}",
            step.fix
        );
        assert!(
            step.fix_th.contains(&file.display().to_string()),
            "{}",
            step.fix_th
        );
    }

    #[test]
    fn an_undo_that_is_done_reports_what_it_could_not_do_besides() {
        // The undo itself stands; what failed next to it is said, not turned into an error
        // that would hide the outcome.
        let (m, ctx) = machine();
        let before = everything(&m);
        let first = apply(&m, &ctx, Target::All);
        let file = font_file(&m);
        m.fs.fail_removes_of(&file);
        m.fs.fail_writes_to(ctx.paths.state_file());
        m.fs.fail_writes_to(point(&ctx, &first, "manifest.json"));
        let undone = undo(m.system(), &later(&ctx, 1)).unwrap();
        assert_eq!(
            undone.result,
            UndoResult::Done {
                undid: first.clone(),
                outcome: Outcome::Committed
            }
        );
        assert_eq!(everything(&m), before);
        let codes: Vec<&str> = undone.problems.iter().map(|f| f.code).collect();
        assert_eq!(codes, ["E_DATA_WRITE", "E_SNAPSHOT_WRITE"], "{undone:?}");
        assert!(
            undone.problems[0]
                .message
                .contains(&file.display().to_string()),
            "{undone:?}"
        );
        assert!(
            undone.problems[1]
                .message
                .contains("could not be marked undone"),
            "{undone:?}"
        );
    }

    #[test]
    fn a_restore_point_that_cannot_be_set_aside_is_reported() {
        let (m, ctx) = machine();
        let first = apply(&m, &ctx, Target::All);
        let journal = point(&ctx, &first, "journal.json");
        m.fs.put(&journal, b"{ damaged");
        // The rename that sets it aside fails.
        m.fs.fail_removes_of(&journal);
        let start = recover_at_start(m.system(), &later(&ctx, 1)).unwrap();
        let [UnreadablePoint {
            point,
            handled: Handled::SetAsideFailed(why),
        }] = start.unreadable.as_slice()
        else {
            panic!("{start:?}")
        };
        assert_eq!(
            (point.id.as_str(), point.kind),
            (first.as_str(), UnreadableKind::Damaged)
        );
        assert_eq!(why.code, "E_SNAPSHOT_WRITE");
        assert!(m.fs.exists(&journal));
    }

    #[test]
    fn a_shorter_pending_list_that_cannot_be_saved_is_a_problem_of_the_start() {
        let (m, ctx) = machine();
        let file = font_file(&m);
        m.fs.put(&file, b"font-bytes");
        let state = State {
            pending_deletes: vec![file.clone()],
            ..State::default()
        };
        m.fs.put(ctx.paths.state_file(), state.to_bytes().unwrap());
        m.fs.fail_writes_to(ctx.paths.state_file());
        let undone = undo(m.system(), &ctx).unwrap();
        assert!(!m.fs.exists(&file), "deleted");
        assert_eq!(
            undone
                .start
                .problems
                .iter()
                .map(|f| f.code)
                .collect::<Vec<_>>(),
            ["E_DATA_WRITE"]
        );
        // The list still names it: deleting it again is harmless.
        assert_eq!(pending(&m, &ctx), vec![file]);
    }

    #[test]
    fn a_read_failure_carries_a_read_code() {
        // Ruling Q8: reading the machine is E_MACHINE_READ, a restore point E_SNAPSHOT_READ,
        // state.json E_STATE_INVALID; none of them a write code.
        let (m, ctx) = machine();
        m.fs.fail_reads_of(WT);
        let e = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap_err();
        assert_eq!(e.code(), "E_MACHINE_READ", "{e:?}");
        assert!(e.to_string().contains("settings.json"), "{e}");
        assert_eq!(
            ActionError::from(Failure {
                code: "E_SNAPSHOT_READ",
                message: "x".into()
            })
            .code(),
            "E_SNAPSHOT_READ"
        );
    }

    #[test]
    fn a_step_whose_op_cannot_be_read_gets_the_shared_cleanup_advice() {
        // Ruling Q9: the advice is exec's, defined once, in both languages.
        let (m, ctx) = machine();
        let root = ctx.paths.snapshots();
        let store = SnapshotStore::new(&m.fs, &root);
        let id = "0000000000001-aaaaaaaa";
        m.fs.create_dir_all(&root.join(id)).unwrap();
        let outcome = Outcome::RollbackIncomplete {
            code: "E_TXN_INTERRUPTED",
            reason: "the previous apply was interrupted".into(),
            failed: vec![(
                3,
                Failure {
                    code: "E_DATA_WRITE",
                    message: "denied".into(),
                },
            )],
        };
        record_incomplete(&store, id, &outcome).unwrap();
        let record = store.latest_incomplete().unwrap().unwrap();
        assert_eq!(record.steps.len(), 1);
        let step = &record.steps[0];
        assert_eq!(
            (
                step.target.as_str(),
                step.code.as_str(),
                step.message.as_str()
            ),
            ("step 3", "E_DATA_WRITE", "denied")
        );
        assert_eq!(
            (step.fix.as_str(), step.fix_th.as_str()),
            (CLEANUP_FIX_EN, CLEANUP_FIX_TH)
        );
        // Nothing is written for any other outcome.
        let other = "0000000000002-bbbbbbbb";
        m.fs.create_dir_all(&root.join(other)).unwrap();
        record_incomplete(&store, other, &Outcome::Committed).unwrap();
        assert!(!m.fs.exists(&root.join(other).join("incomplete.json")));
    }

    #[test]
    fn a_rollback_that_cannot_finish_says_how_to_fix_it_even_after_a_restart() {
        // Spec §5.5: say which parts could not be put back and how to fix them by hand.
        let (m, ctx) = machine();
        let init = ctx.paths.generated().join("init.lua");
        m.fs.put(&init, b"old");
        // init.lua can be neither written nor written back
        m.fs.fail_writes_to(&init);
        let p = prepare_apply(m.system(), &ctx, &terracotta(), &request(Target::All)).unwrap();
        let out = run_apply(m.system(), &ctx, p);
        assert!(
            matches!(&out, Outcome::RollbackIncomplete { failed, .. } if failed.len() == 1),
            "{out:?}"
        );
        // the next start finds the record
        let later = Ctx {
            now_ms: ctx.now_ms + 1,
            ..ctx.clone()
        };
        let start = recover_at_start(m.system(), &later).unwrap();
        assert_eq!(start.recovered, None, "the journal itself is closed");
        let record = start.incomplete.unwrap();
        assert_eq!(record.code, "E_DATA_WRITE");
        let step = &record.steps[0];
        assert_eq!(step.target, format!("file {}", init.display()));
        assert_eq!(step.code, "E_DATA_WRITE");
        let payload = SnapshotStore::new(m.system().fs, &later.paths.snapshots())
            .payload_path(&record.id, step.op_index);
        assert_eq!(
            step.fix,
            format!("Copy {} over {}.", payload.display(), init.display())
        );
        // In Thai too (spec §5.9).
        assert_eq!(
            step.fix_th,
            format!("คัดลอก {} ไปวางทับ {}", payload.display(), init.display())
        );
        // and the file the fix names holds what init.lua held before
        assert_eq!(m.fs.file(&payload), Some(b"old".to_vec()));
        assert_eq!(
            pending_incomplete(m.system(), &later).unwrap(),
            Some(record)
        );
    }
}
