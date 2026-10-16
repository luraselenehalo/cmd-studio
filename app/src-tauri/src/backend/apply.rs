//! Plan, apply, undo, restore points and revert everything (spec §4.2 page 5, §4.4, §4.3
//! "ช่วยเหลือ"), over the core's transactions. The plan is shown before anything changes (spec
//! §2.1): `plan_apply` plans and lets the lock go; `apply` plans again and, when the plan is no
//! longer the one the user saw, changes nothing and shows the new one.
use super::{note, start_notes, start_unrestored, Backend, Machine, Note};
use crate::error::{AppError, INTERNAL};
use cmdstudio_core::actions::{
    pending_incomplete, prepare_apply, run_apply_with, undo, ActionError, Ctx, Prepared, Progress,
    UndoResult,
};
use cmdstudio_core::cleanup;
use cmdstudio_core::exec::{CLEANUP_FIX_EN, CLEANUP_FIX_TH};
use cmdstudio_core::model::{Target, Theme};
use cmdstudio_core::plan::{Component, Op, Paths, PlanError, Request, Warning};
use cmdstudio_core::resources::sha256_hex;
use cmdstudio_core::snapshot::{Manifest, SnapshotKind, SnapshotStore, ThemeRef, Unrestored};
use cmdstudio_core::txn::{Failure, Outcome, Stage};
use serde::{Deserialize, Serialize};
use std::panic::{catch_unwind, AssertUnwindSafe};
use ts_rs::TS;
use uuid::Uuid;

/// What the user asked for (the wizard's pages 3 and 4, the editor's "Save and apply", the
/// home page's quick switch).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ApplyRequest {
    pub theme_id: Uuid,
    pub target: Target,
    pub set_wt_default_profile: bool,
    /// The user agreed to turn off another Clink found in AutoRun (spec §5.4).
    pub replace_other_clink: bool,
}

impl ApplyRequest {
    fn request(&self) -> Request {
        Request {
            target: self.target,
            set_wt_default_profile: self.set_wt_default_profile,
            replace_foreign_clink: self.replace_other_clink,
        }
    }
}

/// What one step of the plan does, for the UI's words (spec §4.5: no technical terms).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum StepKind {
    InstallOhMyPosh,
    InstallClink,
    WriteGenerated,
    WindowsTerminal,
    AutoRun,
    InstallFont,
    SaveState,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlanStep {
    pub kind: StepKind,
    /// The folder, file or font the step touches, as written.
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum PlanWarning {
    NoWindowsTerminal,
    MachineWideClink,
    /// The theme's font is not installed; Windows Terminal gets the bundled Nerd Font.
    FontFallback {
        wanted: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ApplyPlan {
    /// Stands for what the user saw; `apply` takes it back and compares.
    pub digest: String,
    pub theme_name: String,
    pub summary_en: Vec<String>,
    pub summary_th: Vec<String>,
    pub warnings: Vec<PlanWarning>,
    pub steps: Vec<PlanStep>,
    /// What the start found (spec §5.5), to show with the plan.
    pub notes: Vec<Note>,
    /// The steps of a rollback that did not finish, which still need a hand.
    pub unrestored: Vec<Unrestored>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum PlanOutcome {
    Ready {
        plan: ApplyPlan,
    },
    /// Another Clink starts from AutoRun: the user chooses between turning it off (these are
    /// the commands that go) and cancelling (spec §5.4).
    OtherClink {
        commands: Vec<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ApplyStatus {
    Committed,
    RolledBack,
    RollbackIncomplete,
    /// The machine changed since the plan was shown: nothing was done; `plan` is the new plan.
    PlanChanged,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ApplyOutcome {
    pub status: ApplyStatus,
    pub theme_name: String,
    /// The restore point of this apply, which "Undo" in the after-apply bar goes back to.
    pub snapshot_id: Option<String>,
    /// Why it rolled back, when it did.
    pub error: Option<AppError>,
    pub unrestored: Vec<Unrestored>,
    pub notes: Vec<Note>,
    pub plan: Option<ApplyPlan>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Phase {
    BackingUp,
    Applying,
    Verifying,
    RollingBack,
}

/// The `apply-progress` event (spec §4.2 page 5).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ApplyProgress {
    pub phase: Phase,
    pub step: u32,
    pub total: u32,
    /// The step being applied; absent for the other phases.
    pub kind: Option<StepKind>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum UndoStatus {
    NothingToUndo,
    Committed,
    RolledBack,
    RollbackIncomplete,
}

/// A font an undo left installed because the user installed one of that name since.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FontLeft {
    pub family: String,
    pub message_en: String,
    pub message_th: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UndoOutcome {
    pub status: UndoStatus,
    /// The restore point of the apply that was undone.
    pub undid: Option<String>,
    pub error: Option<AppError>,
    pub unrestored: Vec<Unrestored>,
    pub notes: Vec<Note>,
    pub fonts_left: Vec<FontLeft>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RestorePoint {
    pub id: String,
    pub kind: SnapshotKind,
    #[ts(type = "number")]
    pub created_ms: u64,
    pub theme: Option<ThemeRef>,
    pub target: Option<Target>,
    pub summary: Vec<String>,
    pub undoes: Option<String>,
    pub undone_by: Option<String>,
    /// An apply that committed, is not undone and was not set aside: "Restore" may go back to
    /// it (see `restorable`).
    pub restorable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RestorePoints {
    /// Newest first.
    pub points: Vec<RestorePoint>,
    /// Restore points that could not be read: the manifest, or the journal that says whether an
    /// apply committed.
    pub problems: Vec<Note>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum RestoreStatus {
    /// Every apply from the newest down to that one is undone.
    Done,
    /// Not an apply, already undone, set aside, or it did not commit: nothing was done.
    NotRestorable,
    /// An undo on the way did not commit, was refused, or did not undo the apply it was to
    /// undo; `undone` says how far it got, `last` why it stopped.
    Stopped,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RestoreOutcome {
    pub status: RestoreStatus,
    /// The applies undone, in the order they were undone.
    pub undone: Vec<String>,
    /// The last undo that ran. When the restore stopped for a reason other than that undo's own
    /// outcome (another CMD Studio, or the command line, changed the restore points between two
    /// undos, the next undo was refused, or a point it undid could not be marked undone), the
    /// last of its notes says why.
    pub last: Option<UndoOutcome>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum TxnStatus {
    Committed,
    RolledBack,
    RollbackIncomplete,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StepReport {
    /// What the step is about, in English.
    pub what: String,
    pub ok: bool,
    /// What happened, in English, as the core says it.
    pub detail: String,
    /// A failed step's code, a row of spec §5.9 (the code of `error`); `None` for one that went
    /// well.
    pub code: Option<String>,
    pub fix_en: Option<String>,
    pub fix_th: Option<String>,
    /// Why a failed step failed, like every problem the UI shows (spec §5.9): its code, the Thai
    /// and English of that row, and the core's words as the details. `None` for one that went
    /// well.
    pub error: Option<AppError>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RevertOutcome {
    /// Every step went well and the transaction, if it ran, committed.
    pub ok: bool,
    pub steps: Vec<StepReport>,
    pub notes: Vec<Note>,
    pub status: Option<TxnStatus>,
    pub error: Option<AppError>,
    pub unrestored: Vec<Unrestored>,
}

/// How a transaction ended, for the UI.
struct TxnResult {
    status: TxnStatus,
    error: Option<AppError>,
    unrestored: Vec<Unrestored>,
    notes: Vec<Note>,
}

fn step_of(op: &Op, paths: &Paths) -> PlanStep {
    let (kind, detail) = match op {
        Op::InstallRuntime {
            component: Component::Omp,
            dir,
        } => (StepKind::InstallOhMyPosh, dir.display().to_string()),
        Op::InstallRuntime {
            component: Component::Clink,
            dir,
        } => (StepKind::InstallClink, dir.display().to_string()),
        Op::WriteFile { path, .. } if *path == paths.state_file() => {
            (StepKind::SaveState, path.display().to_string())
        }
        Op::WriteFile { path, .. } => (
            StepKind::WriteGenerated,
            path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        ),
        Op::WtApply { settings, .. } => (StepKind::WindowsTerminal, settings.display().to_string()),
        Op::SetAutoRun { .. } => (StepKind::AutoRun, String::new()),
        Op::InstallFont { family } => (StepKind::InstallFont, family.clone()),
    };
    PlanStep { kind, detail }
}

fn warning_of(w: &Warning) -> PlanWarning {
    match w {
        Warning::NoWindowsTerminal => PlanWarning::NoWindowsTerminal,
        Warning::MachineWideClink => PlanWarning::MachineWideClink,
        Warning::FontFallback { wanted } => PlanWarning::FontFallback {
            wanted: wanted.clone(),
        },
    }
}

/// What the user is shown, reduced to a hash: the same plan gives the same digest.
fn digest(summary_en: &[String], warnings: &[PlanWarning], steps: &[PlanStep]) -> String {
    #[derive(Serialize)]
    struct Fingerprint<'a> {
        summary_en: &'a [String],
        warnings: &'a [PlanWarning],
        steps: &'a [PlanStep],
    }
    let json = serde_json::to_vec(&Fingerprint {
        summary_en,
        warnings,
        steps,
    })
    .unwrap_or_default();
    sha256_hex(&json)
}

fn plan_of(theme: &Theme, prepared: &Prepared, paths: &Paths) -> ApplyPlan {
    let steps: Vec<PlanStep> = prepared.ops.iter().map(|op| step_of(op, paths)).collect();
    let warnings: Vec<PlanWarning> = prepared.plan.warnings.iter().map(warning_of).collect();
    let summary_en: Vec<String> = prepared
        .plan
        .summary_en
        .iter()
        .map(|s| s.to_string())
        .collect();
    let summary_th: Vec<String> = prepared
        .plan
        .summary_th
        .iter()
        .map(|s| s.to_string())
        .collect();
    ApplyPlan {
        digest: digest(&summary_en, &warnings, &steps),
        theme_name: theme.name.clone(),
        summary_en,
        summary_th,
        warnings,
        steps,
        notes: start_notes(&prepared.start),
        unrestored: start_unrestored(&prepared.start),
    }
}

fn progress_of(p: Progress, paths: &Paths) -> ApplyProgress {
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    match p {
        Progress::BackingUp { step, total } => ApplyProgress {
            phase: Phase::BackingUp,
            step: count(step),
            total: count(total),
            kind: None,
        },
        Progress::Applying { step, total, op } => ApplyProgress {
            phase: Phase::Applying,
            step: count(step),
            total: count(total),
            kind: Some(step_of(&op, paths).kind),
        },
        Progress::Verifying => ApplyProgress {
            phase: Phase::Verifying,
            step: 0,
            total: 0,
            kind: None,
        },
        Progress::RollingBack { step, total } => ApplyProgress {
            phase: Phase::RollingBack,
            step: count(step),
            total: count(total),
            kind: None,
        },
    }
}

/// Whether "Restore" may go back to restore point `id`, whose manifest is `m`: an apply that
/// committed, is not undone and was not set aside. These are the applies `undo` goes back
/// through (`SnapshotStore::latest_undoable`), and both the list and the restore ask this, so
/// the list offers exactly what a restore goes back to: never an apply that rolled back. `Err`
/// when the journal, which says whether it committed, cannot be read.
fn restorable(store: &SnapshotStore<'_>, id: &str, m: &Manifest) -> Result<bool, Failure> {
    if m.kind != SnapshotKind::Apply || m.undone_by.is_some() || store.is_set_aside(id) {
        return Ok(false);
    }
    Ok(store.journal(id)?.stage == Stage::Committed)
}

/// A restore that stopped after `last`.
fn stopped(undone: Vec<String>, last: UndoOutcome) -> RestoreOutcome {
    RestoreOutcome {
        status: RestoreStatus::Stopped,
        undone,
        last: Some(last),
    }
}

/// A step of "Undo everything" for the UI. A failed one carries its reason as an [`AppError`]
/// with the step's own code, so a Thai UI shows the Thai of that code's row of spec §5.9, never
/// the core's English alone. The core gives every failed step a code (`cleanup::Step`); one
/// that came without would be CMD Studio's own fault, [`INTERNAL`].
fn step_report(s: &cleanup::Step) -> StepReport {
    let error = (!s.ok).then(|| {
        // The core writes a failed step's detail as `E_…: why`; the why is the details.
        let why = s
            .code
            .and_then(|code| s.detail.strip_prefix(code))
            .and_then(|rest| rest.strip_prefix(": "))
            .unwrap_or(&s.detail);
        AppError::coded(s.code.unwrap_or(INTERNAL), why)
    });
    StepReport {
        what: s.what.clone(),
        ok: s.ok,
        detail: s.detail.clone(),
        code: error.as_ref().map(|e| e.code.clone()),
        fix_en: s.fix.as_ref().map(|f| f.en.clone()),
        fix_th: s.fix.as_ref().map(|f| f.th.clone()),
        error,
    }
}

impl<M: Machine> Backend<M> {
    /// The plan for applying `req`, shown before anything changes (spec §2.1, §4.2 page 5).
    /// Takes the lock while it plans and lets it go: nothing waits on the user's reading.
    /// Another Clink in AutoRun is an answer, not an error: the UI asks, and plans again with
    /// `replace_other_clink`.
    pub fn plan_apply(&self, req: ApplyRequest) -> Result<PlanOutcome, AppError> {
        let (theme, _, _) = self.find_theme(req.theme_id)?;
        let ctx = self.ctx();
        match prepare_apply(self.sys(), &ctx, &theme, &req.request()) {
            Ok(prepared) => Ok(PlanOutcome::Ready {
                plan: plan_of(&theme, &prepared, &ctx.paths),
            }),
            Err(ActionError::Plan(PlanError::ForeignClink { existing })) => {
                Ok(PlanOutcome::OtherClink { commands: existing })
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Applies `req` as one transaction, telling `on_progress` each step (the `apply-progress`
    /// event). `digest` is the plan the user saw: when the plan now differs, nothing is done
    /// and the new plan comes back as `PlanChanged`.
    ///
    /// `on_progress` runs between two steps of the transaction, which must run to its end
    /// whatever the listener does: the core does not catch a panic, and one would leave the
    /// machine half changed until the next start's recovery (`run_apply_with`). So the listener
    /// returns nothing: an IPC layer that emits the event from it ignores an emit that fails
    /// (the window may be gone; the outcome is still the answer). And a listener that panics is
    /// caught here, is not told again, and the apply goes on.
    pub fn apply(
        &self,
        req: ApplyRequest,
        digest: &str,
        on_progress: &mut dyn FnMut(ApplyProgress),
    ) -> Result<ApplyOutcome, AppError> {
        let (theme, _, _) = self.find_theme(req.theme_id)?;
        let ctx = self.ctx();
        let prepared = prepare_apply(self.sys(), &ctx, &theme, &req.request())?;
        let plan = plan_of(&theme, &prepared, &ctx.paths);
        if plan.digest != digest {
            return Ok(ApplyOutcome {
                status: ApplyStatus::PlanChanged,
                theme_name: theme.name.clone(),
                snapshot_id: None,
                error: None,
                unrestored: Vec::new(),
                notes: plan.notes.clone(),
                plan: Some(plan),
            });
        }
        let snapshot_id = prepared.manifest.id.clone();
        let mut notes = plan.notes.clone();
        let mut unrestored = plan.unrestored.clone();
        let paths = ctx.paths.clone();
        let mut deaf = false;
        let mut tell = |p: Progress| {
            if deaf {
                return;
            }
            // Nothing of the listener unwinds into the transaction.
            let told = catch_unwind(AssertUnwindSafe(|| on_progress(progress_of(p, &paths))));
            if let Err(payload) = told {
                // Dropping the payload of a panic runs its code, which could panic too.
                std::mem::forget(payload);
                deaf = true;
            }
        };
        let outcome = run_apply_with(self.sys(), &ctx, prepared, &mut tell);
        let result = self.transaction_result(&outcome, &ctx);
        notes.extend(result.notes);
        unrestored.extend(result.unrestored);
        Ok(ApplyOutcome {
            status: match result.status {
                TxnStatus::Committed => ApplyStatus::Committed,
                TxnStatus::RolledBack => ApplyStatus::RolledBack,
                TxnStatus::RollbackIncomplete => ApplyStatus::RollbackIncomplete,
            },
            theme_name: theme.name.clone(),
            snapshot_id: Some(snapshot_id),
            error: result.error,
            unrestored,
            notes,
            plan: None,
        })
    }

    /// How a transaction ended: its error, and for a rollback that did not finish the steps
    /// to fix by hand, from the record the core wrote (or, when it could not be written, from
    /// the failed ops with the clean-up advice).
    fn transaction_result(&self, outcome: &Outcome, ctx: &Ctx) -> TxnResult {
        match outcome {
            Outcome::Committed => TxnResult {
                status: TxnStatus::Committed,
                error: None,
                unrestored: Vec::new(),
                notes: Vec::new(),
            },
            Outcome::RolledBack { code, reason } => TxnResult {
                status: TxnStatus::RolledBack,
                error: Some(AppError::coded(code, reason.clone())),
                unrestored: Vec::new(),
                notes: Vec::new(),
            },
            Outcome::RollbackIncomplete {
                code,
                reason,
                failed,
            } => {
                let mut notes = Vec::new();
                let unrestored = match pending_incomplete(self.sys(), ctx) {
                    Ok(Some(record)) => record.steps,
                    Ok(None) => failed
                        .iter()
                        .map(|(i, f)| Unrestored {
                            op_index: *i,
                            target: format!("step {}", i + 1),
                            code: f.code.to_string(),
                            message: f.message.clone(),
                            fix: CLEANUP_FIX_EN.to_string(),
                            fix_th: CLEANUP_FIX_TH.to_string(),
                            restore_point: None,
                        })
                        .collect(),
                    Err(e) => {
                        notes.push(note(e.code(), e.to_string()));
                        Vec::new()
                    }
                };
                TxnResult {
                    status: TxnStatus::RollbackIncomplete,
                    error: Some(AppError::coded(code, reason.clone())),
                    unrestored,
                    notes,
                }
            }
        }
    }

    /// Undoes the newest apply that is not undone (spec §4.4 "ย้อนกลับ").
    pub fn undo_last(&self) -> Result<UndoOutcome, AppError> {
        let ctx = self.ctx();
        let report = undo(self.sys(), &ctx)?;
        let mut notes = start_notes(&report.start);
        let mut unrestored = start_unrestored(&report.start);
        for p in &report.problems {
            notes.push(note(p.code, p.message.clone()));
        }
        let fonts_left = report
            .fonts_left
            .iter()
            .map(|f| FontLeft {
                family: f.family.clone(),
                message_en: f.en(),
                message_th: f.th(),
            })
            .collect();
        match report.result {
            UndoResult::NothingToUndo => Ok(UndoOutcome {
                status: UndoStatus::NothingToUndo,
                undid: None,
                error: None,
                unrestored,
                notes,
                fonts_left,
            }),
            UndoResult::Done { undid, outcome } => {
                let result = self.transaction_result(&outcome, &ctx);
                notes.extend(result.notes);
                unrestored.extend(result.unrestored);
                Ok(UndoOutcome {
                    status: match result.status {
                        TxnStatus::Committed => UndoStatus::Committed,
                        TxnStatus::RolledBack => UndoStatus::RolledBack,
                        TxnStatus::RollbackIncomplete => UndoStatus::RollbackIncomplete,
                    },
                    undid: Some(undid),
                    error: result.error,
                    unrestored,
                    notes,
                    fonts_left,
                })
            }
        }
    }

    /// Every restore point, newest first (spec §4.3 "จุดย้อนกลับทั้งหมด"). Read only. One whose
    /// manifest or journal cannot be read is a problem, and is not offered for a restore.
    pub fn snapshots_list(&self) -> Result<RestorePoints, AppError> {
        let root = self.paths().snapshots();
        let store = SnapshotStore::new(self.sys().fs, &root);
        let ids = store.ids()?;
        let mut points = Vec::new();
        let mut problems = Vec::new();
        for id in ids.iter().rev() {
            let unreadable =
                |f: Failure| note(f.code, format!("restore point {id}: {}", f.message));
            let m = match store.manifest(id) {
                Ok(m) => m,
                Err(f) => {
                    problems.push(unreadable(f));
                    continue;
                }
            };
            let restorable = restorable(&store, id, &m).unwrap_or_else(|f| {
                problems.push(unreadable(f));
                false
            });
            points.push(RestorePoint {
                // The folder's name, which `snapshot_restore` looks for.
                id: id.clone(),
                kind: m.kind,
                created_ms: m.created_ms,
                theme: m.theme,
                target: m.target,
                summary: m.summary,
                undoes: m.undoes,
                undone_by: m.undone_by,
                restorable,
            });
        }
        Ok(RestorePoints { points, problems })
    }

    /// Goes back to restore point `id` (an apply): undoes every apply from the newest down to
    /// it, each as its own transaction, and stops at the first that does not commit. The
    /// restore points are layered, so an older one cannot be put back alone.
    ///
    /// Each undo takes the core's lock (spec §5.5) and lets it go when it ends; the restore
    /// does not hold it in between. So another CMD Studio, or the command line, can change the
    /// machine between two undos, and the next undo would undo that change instead. The applies
    /// to undo are listed first, and the restore stops, saying why in the last undo's notes, as
    /// soon as an undo did not undo the next of them, an undo is refused after others were
    /// done, or a point it undid could not be marked undone (the next undo would undo it again).
    pub fn snapshot_restore(&self, id: &str) -> Result<RestoreOutcome, AppError> {
        self.restore_through(id, &mut |_| {})
    }

    /// [`Backend::snapshot_restore`], running `before_each_undo` before each undo, while this
    /// backend holds no lock: the tests' way into the gaps where another program can change the
    /// machine.
    fn restore_through(
        &self,
        id: &str,
        before_each_undo: &mut dyn FnMut(&Self),
    ) -> Result<RestoreOutcome, AppError> {
        let not_restorable = RestoreOutcome {
            status: RestoreStatus::NotRestorable,
            undone: Vec::new(),
            last: None,
        };
        let root = self.paths().snapshots();
        let store = SnapshotStore::new(self.sys().fs, &root);
        let ids = store.ids()?;
        let Some(at) = ids.iter().position(|known| known == id) else {
            return Err(AppError::coded(
                "E_SNAPSHOT_READ",
                format!("there is no restore point {id}"),
            ));
        };
        if !restorable(&store, id, &store.manifest(id)?)? {
            return Ok(not_restorable);
        }
        // The applies the undos go back through, newest first: those `undo` takes one after the
        // other, down to `id`. One that cannot be read would stop an undo at it, so it stops
        // the restore before anything is undone.
        let mut expected = Vec::new();
        for newer in ids.iter().skip(at).rev() {
            if store.is_set_aside(newer) {
                continue;
            }
            let unreadable = |f: Failure| {
                AppError::coded(
                    f.code,
                    format!(
                        "restore point {newer} cannot be read, so nothing was undone: {}",
                        f.message
                    ),
                )
            };
            let m = store.manifest(newer).map_err(unreadable)?;
            if restorable(&store, newer, &m).map_err(unreadable)? {
                expected.push(newer.clone());
            }
        }
        if expected.last().map(String::as_str) != Some(id) {
            return Ok(not_restorable);
        }
        let mut undone = Vec::new();
        let mut last: Option<UndoOutcome> = None;
        for want in &expected {
            before_each_undo(self);
            let mut this = match self.undo_last() {
                Ok(this) => this,
                Err(e) => {
                    // Nothing is undone yet: the refusal is the answer, as for one undo.
                    let Some(mut before) = last else {
                        return Err(e);
                    };
                    before.notes.push(Note {
                        code: e.code.clone(),
                        message_th: e.message_th.clone(),
                        message_en: format!(
                            "the undo of restore point {want} was refused, so the restore \
                             stopped there: {}",
                            e.details
                        ),
                    });
                    return Ok(stopped(undone, before));
                }
            };
            if let Some(u) = &this.undid {
                undone.push(u.clone());
            }
            match this.status {
                UndoStatus::Committed if this.undid.as_ref() == Some(want) => {}
                // It undid an apply that is not the next one listed, or found none left:
                // another CMD Studio, or the command line, changed the machine in between.
                UndoStatus::Committed | UndoStatus::NothingToUndo => {
                    let what = match &this.undid {
                        Some(got) => format!("restore point {got} was undone, not {want}"),
                        None => format!("nothing was left to undo before restore point {want}"),
                    };
                    this.notes.push(note(
                        "E_BUSY",
                        format!(
                            "{what}: another CMD Studio, or the command line, changed this \
                             computer while the restore went back through the restore points, \
                             so the restore stopped there. The restore points show where it is \
                             now."
                        ),
                    ));
                    return Ok(stopped(undone, this));
                }
                // It did not commit: its own outcome says why.
                UndoStatus::RolledBack | UndoStatus::RollbackIncomplete => {
                    return Ok(stopped(undone, this));
                }
            }
            let marked = store.manifest(want).is_ok_and(|m| m.undone_by.is_some());
            if expected.last() != Some(want) && !marked {
                this.notes.push(note(
                    "E_SNAPSHOT_WRITE",
                    format!(
                        "restore point {want} is undone but could not be marked undone, so the \
                         next undo would undo it again: the restore stopped there. Restore again \
                         once the restore point can be written."
                    ),
                ));
                return Ok(stopped(undone, this));
            }
            last = Some(this);
        }
        Ok(RestoreOutcome {
            status: RestoreStatus::Done,
            undone,
            last,
        })
    }

    /// "Undo everything" (spec §4.3, §6.3 steps 1–3): every change of CMD Studio's taken out,
    /// key by key, without uninstalling.
    pub fn revert_all(&self) -> Result<RevertOutcome, AppError> {
        let ctx = self.ctx();
        let report = cleanup::revert_all(self.sys(), &ctx)?;
        let mut notes = start_notes(&report.start);
        let mut unrestored = start_unrestored(&report.start);
        let steps = report.steps.iter().map(step_report).collect();
        let (status, error) = match &report.outcome {
            None => (None, None),
            Some(outcome) => {
                let result = self.transaction_result(outcome, &ctx);
                notes.extend(result.notes);
                unrestored.extend(result.unrestored);
                (Some(result.status), result.error)
            }
        };
        Ok(RevertOutcome {
            ok: report.ok(),
            steps,
            notes,
            status,
            error,
            unrestored,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testing::{
        apply_terracotta, backend, fake_fs, plan_and_apply, terracotta_id, terracotta_request, WT,
    };
    use crate::error::meaning;
    use cmdstudio_core::adapters::fake::FakeMachine;
    use cmdstudio_core::adapters::{AutoRun, RegValue, REG_SZ};
    use cmdstudio_core::state::State;
    use std::path::Path;

    fn request(id: Uuid) -> ApplyRequest {
        ApplyRequest {
            theme_id: id,
            ..terracotta_request()
        }
    }

    fn ready(outcome: PlanOutcome) -> ApplyPlan {
        match outcome {
            PlanOutcome::Ready { plan } => plan,
            other => panic!("{other:?}"),
        }
    }

    fn paper_id() -> Uuid {
        cmdstudio_core::presets::builtin()
            .unwrap()
            .into_iter()
            .find(|(k, _)| *k == "paper")
            .unwrap()
            .1
            .id
    }

    /// Terracotta, then Paper: two restore points, newest last.
    fn two_applies(b: &Backend<FakeMachine>) -> (String, String) {
        let (first, _) = apply_terracotta(b);
        let (second, _) = plan_and_apply(b, request(paper_id()));
        assert_eq!(
            (first.status, second.status),
            (ApplyStatus::Committed, ApplyStatus::Committed)
        );
        (first.snapshot_id.unwrap(), second.snapshot_id.unwrap())
    }

    fn point(list: &RestorePoints, id: &str) -> RestorePoint {
        list.points.iter().find(|p| p.id == id).unwrap().clone()
    }

    #[test]
    fn the_plan_names_each_step_and_lets_the_lock_go() {
        let b = backend();
        let plan = ready(b.plan_apply(request(terracotta_id())).unwrap());
        assert_eq!(plan.theme_name, "Terracotta");
        let kinds: Vec<StepKind> = plan.steps.iter().map(|s| s.kind).collect();
        assert_eq!(
            kinds,
            [
                StepKind::InstallOhMyPosh,
                StepKind::InstallClink,
                StepKind::WriteGenerated,
                StepKind::WriteGenerated,
                StepKind::WriteGenerated,
                StepKind::WriteGenerated,
                StepKind::WindowsTerminal,
                StepKind::AutoRun,
                StepKind::SaveState,
            ]
        );
        assert_eq!(plan.summary_en[0], "Update the prompt and the welcome box");
        assert_eq!(plan.summary_th[0], "ปรับ prompt และกล่องต้อนรับตามธีม");
        assert!(plan.warnings.is_empty() && plan.notes.is_empty() && plan.unrestored.is_empty());
        assert_eq!(plan.digest.len(), 64);
        // The same plan twice: the same digest, and the lock is free in between (a second
        // plan under a held lock would be E_BUSY).
        let again = ready(b.plan_apply(request(terracotta_id())).unwrap());
        assert_eq!(again.digest, plan.digest);
        assert!(b
            .sys()
            .fs
            .try_lock(&b.paths().lock_file())
            .unwrap()
            .is_some());
    }

    #[test]
    fn apply_commits_and_tells_each_step() {
        let b = backend();
        let (done, seen) = apply_terracotta(&b);
        assert_eq!(done.status, ApplyStatus::Committed);
        assert_eq!(done.theme_name, "Terracotta");
        assert!(done.snapshot_id.is_some() && done.error.is_none() && done.plan.is_none());
        let state = State::load(b.sys().fs, &b.paths().state_file()).unwrap();
        assert_eq!(
            state.applied.as_ref().map(|a| &a.snapshot_id),
            done.snapshot_id.as_ref()
        );
        // Back up 9, apply 9 with the step kinds in order, then verify.
        let backing: Vec<u32> = seen
            .iter()
            .filter(|p| p.phase == Phase::BackingUp)
            .map(|p| p.step)
            .collect();
        assert_eq!(backing, (1..=9).collect::<Vec<_>>());
        let applying: Vec<(u32, Option<StepKind>)> = seen
            .iter()
            .filter(|p| p.phase == Phase::Applying)
            .map(|p| (p.step, p.kind))
            .collect();
        assert_eq!(applying[0], (1, Some(StepKind::InstallOhMyPosh)));
        assert_eq!(applying[8], (9, Some(StepKind::SaveState)));
        assert!(seen
            .iter()
            .all(|p| p.total == 9 || p.phase == Phase::Verifying));
        assert_eq!(seen.last().unwrap().phase, Phase::Verifying);
        assert!(!seen.iter().any(|p| p.phase == Phase::RollingBack));
    }

    #[test]
    fn a_plan_that_changed_since_it_was_shown_is_not_applied() {
        let mut b = backend();
        let req = request(terracotta_id());
        let plan = ready(b.plan_apply(req.clone()).unwrap());
        // Windows Terminal is uninstalled in between: the plan has fewer steps now.
        b.machine.wt.locations.clone_from(&Vec::new());
        let done = b.apply(req.clone(), &plan.digest, &mut |_| {}).unwrap();
        assert_eq!(done.status, ApplyStatus::PlanChanged);
        let new_plan = done.plan.unwrap();
        assert_ne!(new_plan.digest, plan.digest);
        assert_eq!(new_plan.warnings, [PlanWarning::NoWindowsTerminal]);
        assert!(
            !b.sys().fs.exists(&b.paths().state_file()),
            "nothing was applied"
        );
        // A wrong digest never applies.
        let done = b.apply(req, "not-a-digest", &mut |_| {}).unwrap();
        assert_eq!(done.status, ApplyStatus::PlanChanged);
    }

    #[test]
    fn another_clink_is_asked_about_then_turned_off_on_agreement() {
        let b = backend();
        let other = r#""C:\clink\clink_x64.exe" inject --autorun"#;
        b.machine
            .autorun
            .write_user(&RegValue::from_text(REG_SZ, other))
            .unwrap();
        let req = request(terracotta_id());
        match b.plan_apply(req.clone()).unwrap() {
            PlanOutcome::OtherClink { commands } => assert_eq!(commands, [other]),
            other => panic!("{other:?}"),
        }
        let agreed = ApplyRequest {
            replace_other_clink: true,
            ..req
        };
        let plan = ready(b.plan_apply(agreed.clone()).unwrap());
        assert!(
            plan.summary_en
                .iter()
                .any(|s| s.contains("Turn off the cmd add-on")),
            "{:?}",
            plan.summary_en
        );
        let done = b.apply(agreed, &plan.digest, &mut |_| {}).unwrap();
        assert_eq!(done.status, ApplyStatus::Committed);
        let value = b
            .machine
            .autorun
            .read_user()
            .unwrap()
            .unwrap()
            .text()
            .unwrap();
        assert!(
            !value.contains(other) && value.contains("clink-profile"),
            "{value}"
        );
        // Undo brings the other Clink back.
        let undone = b.undo_last().unwrap();
        assert_eq!(undone.status, UndoStatus::Committed);
        assert_eq!(
            b.machine
                .autorun
                .read_user()
                .unwrap()
                .unwrap()
                .text()
                .unwrap(),
            other
        );
    }

    #[test]
    fn a_failed_check_rolls_back_and_says_why() {
        let b = backend();
        b.machine.process.answer("oh-my-posh.exe", "print", 1, "");
        let (done, seen) = apply_terracotta(&b);
        assert_eq!(done.status, ApplyStatus::RolledBack);
        let error = done.error.unwrap();
        assert_eq!(error.code, "E_VERIFY_OMP");
        assert_eq!(error.message_th, "ขั้นตรวจผล: OMP เรนเดอร์ config ไม่ได้");
        assert!(seen.iter().any(|p| p.phase == Phase::RollingBack));
        assert!(!b.sys().fs.exists(&b.paths().state_file()));
        assert_eq!(b.machine.autorun.read_user().unwrap(), None);
        // The apply that rolled back left its restore point, which is no point to go back to:
        // the list does not offer it, and a restore of it does nothing.
        let id = done.snapshot_id.unwrap();
        let rolled_back = point(&b.snapshots_list().unwrap(), &id);
        assert_eq!(rolled_back.kind, SnapshotKind::Apply);
        assert!(!rolled_back.restorable, "{rolled_back:?}");
        assert_eq!(
            b.snapshot_restore(&id).unwrap().status,
            RestoreStatus::NotRestorable
        );
    }

    #[test]
    fn undo_undoes_then_has_nothing_to_undo() {
        let b = backend();
        let wt_before = b.sys().fs.read(Path::new(WT)).unwrap();
        let (done, _) = apply_terracotta(&b);
        let undone = b.undo_last().unwrap();
        assert_eq!(undone.status, UndoStatus::Committed);
        assert_eq!(undone.undid, done.snapshot_id);
        assert!(undone.error.is_none() && undone.fonts_left.is_empty());
        assert_eq!(b.sys().fs.read(Path::new(WT)).unwrap(), wt_before);
        let nothing = b.undo_last().unwrap();
        assert_eq!(
            (nothing.status, nothing.undid),
            (UndoStatus::NothingToUndo, None)
        );
    }

    #[test]
    fn restore_points_list_newest_first_and_restore_goes_back_through_them() {
        let b = backend();
        let (first, second) = two_applies(&b);
        let list = b.snapshots_list().unwrap();
        assert!(list.problems.is_empty());
        let ids: Vec<&str> = list.points.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, [second.as_str(), first.as_str()]);
        assert!(list
            .points
            .iter()
            .all(|p| p.kind == SnapshotKind::Apply && p.restorable));
        assert_eq!(list.points[0].theme.as_ref().unwrap().name, "Paper");
        // Restoring the first point undoes Paper, then Terracotta.
        let restored = b.snapshot_restore(&first).unwrap();
        assert_eq!(restored.status, RestoreStatus::Done);
        assert_eq!(restored.undone, [second.clone(), first.clone()]);
        assert_eq!(restored.last.unwrap().status, UndoStatus::Committed);
        let list = b.snapshots_list().unwrap();
        assert_eq!(list.points.len(), 4);
        assert!(list.points.iter().all(|p| !p.restorable));
        assert_eq!(list.points[0].kind, SnapshotKind::Undo);
        // An undone point, or an undo, is not restorable; a missing id is an error.
        assert_eq!(
            b.snapshot_restore(&first).unwrap().status,
            RestoreStatus::NotRestorable
        );
        assert_eq!(
            b.snapshot_restore(&list.points[0].id).unwrap().status,
            RestoreStatus::NotRestorable
        );
        assert_eq!(
            b.snapshot_restore("0000000000000-nowhere")
                .unwrap_err()
                .code,
            "E_SNAPSHOT_READ"
        );
    }

    /// Spec §5.5: a restore is one undo after another, each under the lock, which it lets go in
    /// between. Another CMD Studio, or the CLI, can apply a theme in that gap; the next undo
    /// then undoes that apply, the newest, and the restore must stop there and say so. The fake
    /// machine has no hook between two commands, so the test goes in through the restore's own
    /// seam (`restore_through`) and runs a real apply in the gap.
    #[test]
    fn a_restore_stops_when_another_apply_lands_between_its_undos() {
        let b = backend();
        let (first, second) = two_applies(&b);
        let mut undos = 0;
        let mut foreign = None;
        let restored = b
            .restore_through(&first, &mut |b: &Backend<FakeMachine>| {
                undos += 1;
                if undos == 2 {
                    foreign = Some(apply_terracotta(b).0);
                }
            })
            .unwrap();
        // The apply in the gap committed: the restore did not hold the lock between its undos.
        let foreign = foreign.unwrap();
        assert_eq!(foreign.status, ApplyStatus::Committed);
        let foreign = foreign.snapshot_id.unwrap();
        // Paper was undone, then the newest apply, which was not one the restore meant to undo,
        // and the restore stopped there: Terracotta's own apply is not undone.
        assert_eq!(restored.status, RestoreStatus::Stopped);
        assert_eq!(restored.undone, [second, foreign.clone()]);
        let last = restored.last.unwrap();
        assert_eq!(last.status, UndoStatus::Committed);
        let why = last.notes.last().unwrap();
        assert_eq!(why.code, "E_BUSY");
        assert_eq!(why.message_th, meaning("E_BUSY").unwrap().0);
        assert!(
            why.message_en.contains(&foreign) && why.message_en.contains(&first),
            "{}",
            why.message_en
        );
        let list = b.snapshots_list().unwrap();
        assert!(point(&list, &first).restorable);
        assert_eq!(point(&list, &first).undone_by, None);
        // Restoring it again goes the rest of the way.
        let again = b.snapshot_restore(&first).unwrap();
        assert_eq!(
            (again.status, again.undone),
            (RestoreStatus::Done, vec![first])
        );
    }

    #[test]
    fn a_restore_that_finds_the_lock_taken_between_its_undos_says_how_far_it_got() {
        let b = backend();
        let (first, second) = two_applies(&b);
        let mut undos = 0;
        let mut held = None;
        let restored = b
            .restore_through(&first, &mut |b: &Backend<FakeMachine>| {
                undos += 1;
                if undos == 2 {
                    // Another CMD Studio takes the lock, and is still changing the machine.
                    held = b.sys().fs.try_lock(&b.paths().lock_file()).unwrap();
                }
            })
            .unwrap();
        assert!(held.is_some());
        drop(held);
        // Not an error that hides the undo done already: how far it got, and why it stopped.
        assert_eq!(restored.status, RestoreStatus::Stopped);
        assert_eq!(restored.undone, [second]);
        let last = restored.last.unwrap();
        assert_eq!(last.status, UndoStatus::Committed);
        let why = last.notes.last().unwrap();
        assert_eq!(why.code, "E_BUSY");
        assert_eq!(why.message_th, cmdstudio_core::actions::BUSY_TH);
        assert!(why.message_en.contains(&first), "{}", why.message_en);
        assert!(point(&b.snapshots_list().unwrap(), &first).restorable);
    }

    #[test]
    fn a_restore_stops_where_an_undone_point_could_not_be_marked_undone() {
        let b = backend();
        let (first, second) = two_applies(&b);
        let manifest = b.paths().snapshots().join(&second).join("manifest.json");
        fake_fs(&b).fail_writes_to(&manifest);
        let restored = b.snapshot_restore(&first).unwrap();
        // Paper is undone but not marked so: the next undo would go over it again, so the
        // restore stops there and says why.
        assert_eq!(restored.status, RestoreStatus::Stopped);
        assert_eq!(restored.undone, std::slice::from_ref(&second));
        let last = restored.last.unwrap();
        assert_eq!(last.status, UndoStatus::Committed);
        let codes: Vec<&str> = last.notes.iter().map(|n| n.code.as_str()).collect();
        assert_eq!(
            codes,
            ["E_SNAPSHOT_WRITE", "E_SNAPSHOT_WRITE"],
            "{:?}",
            last.notes
        );
        assert!(
            last.notes[1].message_en.contains(&second),
            "{:?}",
            last.notes
        );
        // Once the restore point can be written, a restore goes the whole way.
        fake_fs(&b).stop_failing(&manifest);
        let again = b.snapshot_restore(&first).unwrap();
        assert_eq!(again.status, RestoreStatus::Done);
        assert_eq!(again.undone, [second, first]);
    }

    #[test]
    fn revert_all_takes_every_change_out() {
        let b = backend();
        let wt_before = b.sys().fs.read(Path::new(WT)).unwrap();
        apply_terracotta(&b);
        assert!(b.machine.autorun.read_user().unwrap().is_some());
        let reverted = b.revert_all().unwrap();
        assert!(reverted.ok, "{:?}", reverted.steps);
        assert_eq!(reverted.status, Some(TxnStatus::Committed));
        assert!(reverted.steps.iter().all(|s| s.ok && s.error.is_none()));
        assert_eq!(b.machine.autorun.read_user().unwrap(), None);
        assert_eq!(b.sys().fs.read(Path::new(WT)).unwrap(), wt_before);
        // Nothing applied any more.
        let state = State::load(b.sys().fs, &b.paths().state_file()).unwrap();
        assert!(state.applied.is_none());
    }

    /// A Thai UI never shows an English-only reason (spec §5.9): a failed step carries its
    /// reason as an error like every other, the texts of its code's row in both languages.
    #[test]
    fn a_revert_step_that_failed_says_why_in_thai_and_english() {
        let b = backend();
        apply_terracotta(&b);
        // Windows Terminal's settings.json cannot be read now: that step fails, the rest goes.
        fake_fs(&b).fail_reads_of(Path::new(WT));
        let reverted = b.revert_all().unwrap();
        assert!(!reverted.ok);
        assert_eq!(reverted.status, Some(TxnStatus::Committed));
        let failed: Vec<&StepReport> = reverted.steps.iter().filter(|s| !s.ok).collect();
        assert_eq!(failed.len(), 1, "{:?}", reverted.steps);
        let step = failed[0];
        let error = step.error.as_ref().unwrap();
        let (th, en) = meaning("E_MACHINE_READ").unwrap();
        assert_eq!(
            (
                error.code.as_str(),
                error.message_th.as_str(),
                error.message_en.as_str()
            ),
            ("E_MACHINE_READ", th, en)
        );
        assert!(
            error.details.starts_with("it could not be read"),
            "{}",
            error.details
        );
        assert_eq!(step.code.as_deref(), Some("E_MACHINE_READ"));
        assert!(step.fix_en.is_some() && step.fix_th.is_some());
        assert!(reverted
            .steps
            .iter()
            .filter(|s| s.ok)
            .all(|s| s.error.is_none() && s.code.is_none()));
        // A failed step that came without a code would be CMD Studio's own fault.
        let bare = step_report(&cleanup::Step {
            what: "Something".to_string(),
            ok: false,
            detail: "went wrong".to_string(),
            code: None,
            fix: None,
        });
        let error = bare.error.unwrap();
        assert_eq!(
            (error.code.as_str(), error.details.as_str()),
            ("E_INTERNAL", "went wrong")
        );
        assert_eq!(error.message_th, meaning("E_INTERNAL").unwrap().0);
        assert_eq!(bare.code.as_deref(), Some("E_INTERNAL"));
    }

    /// `run_apply_with`: a listener that panics would unwind out of the apply between two steps,
    /// and only the next start's recovery would put the machine back. The window's listener
    /// emits an event, which can fail; the apply must end either way.
    #[test]
    fn a_progress_listener_that_panics_does_not_stop_the_apply() {
        let b = backend();
        let req = terracotta_request();
        let plan = ready(b.plan_apply(req.clone()).unwrap());
        let mut told = 0;
        let done = b
            .apply(req, &plan.digest, &mut |_| {
                told += 1;
                panic!("the window is gone");
            })
            .unwrap();
        assert_eq!(done.status, ApplyStatus::Committed);
        // A listener that panicked is not told again.
        assert_eq!(told, 1);
        let state = State::load(b.sys().fs, &b.paths().state_file()).unwrap();
        assert_eq!(state.applied.map(|a| a.snapshot_id), done.snapshot_id);
    }

    #[test]
    fn a_missing_theme_is_refused_before_anything_is_planned() {
        let b = backend();
        let e = b.plan_apply(request(Uuid::new_v4())).unwrap_err();
        assert_eq!(e.code, "E_THEME_INVALID");
        assert!(b.sys().fs.read(&b.paths().lock_file()).unwrap().is_none());
    }

    // `advanced_read` is a method of backend/mod.rs; it needs an apply first, so its test is here.
    #[test]
    fn the_advanced_page_reads_the_generated_files_after_an_apply() {
        let b = backend();
        let before = b.advanced_read().unwrap();
        assert!(before.generated.is_empty());
        assert_eq!(before.versions.oh_my_posh, "31.5.0");
        assert_eq!(before.versions.clink, "1.9.34");
        assert_eq!(before.data_dir, crate::backend::testing::DATA);
        // No command has run through `logged` yet.
        assert!(before.log_tail.is_empty(), "{}", before.log_tail);
        // The apply as the IPC runs it: one log line for the command.
        let (done, _) = b.logged("apply", |b| Ok(apply_terracotta(b))).unwrap();
        assert_eq!(done.status, ApplyStatus::Committed);
        let after = b.advanced_read().unwrap();
        let names: Vec<&str> = after.generated.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(
            names,
            ["theme.omp.json", "init.lua", "banner.lua", "clink_settings"]
        );
        let lines: Vec<&str> = after.log_tail.lines().collect();
        assert_eq!(lines.len(), 1, "{}", after.log_tail);
        assert!(
            lines[0].contains(" cmd=apply ms=") && lines[0].ends_with(" ok"),
            "{}",
            after.log_tail
        );
    }

    // The library's "in use" badge comes from the applied state, so its test lives with apply.
    #[test]
    fn the_applied_theme_is_marked_in_use() {
        let b = backend();
        apply_terracotta(&b);
        let lib = b.themes_list().unwrap();
        assert!(lib.themes[0].in_use && !lib.themes[1].in_use);
        let applied = lib.applied.unwrap();
        assert_eq!(
            (
                applied.theme_id,
                applied.theme_name.as_str(),
                applied.target
            ),
            (terracotta_id(), "Terracotta", Target::All)
        );
        // A damaged state.json is a problem the library says, not a crash.
        b.sys()
            .fs
            .write_atomic(&b.paths().state_file(), b"{ damaged")
            .unwrap();
        let lib = b.themes_list().unwrap();
        assert!(lib.applied.is_none());
        assert_eq!(lib.state_problem.unwrap().code, "E_STATE_INVALID");
        assert!(lib.themes.iter().all(|t| !t.in_use));
    }
}
