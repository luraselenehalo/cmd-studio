//! The transactional apply engine (spec §5.5). Side effects happen only
//! through `Executor`. The journal is saved before every stage change, so a
//! crash can always be rolled back.
//!
//! A run goes `Planned`, `Snapshotted`, `Applying { done }` once per op,
//! `Verifying`, `Committed`. Nothing on the machine changes before the journal
//! holds every op's prior state (`Snapshotted`). When an op or the check
//! fails, or the app is cut off and starts again, the run is rolled back: the
//! ops that may have changed the machine are restored, newest first, and it
//! ends in `RolledBack`, or in `RollbackIncomplete` when a restore failed.
//!
//! The journal is saved when an op has ended, so it is one op behind the
//! machine: after `Snapshotted` op 0 may be running, and after
//! `Applying { done }` op `done` may be. That op can be half written, so a
//! rollback restores it too.
use crate::plan::Op;
use serde::{Deserialize, Serialize};

/// Why a call on an [`Executor`], a [`Verifier`] or a [`JournalStore`] failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    /// Stable code from spec §5.9.
    pub code: &'static str,
    pub message: String,
}

/// Prior state of one operation's target, captured before anything changes.
/// The journal keeps it as JSON with camelCase names: `{"opIndex": 0, "prior": [..]}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// The operation this is the prior state of, by its position in [`Journal::ops`].
    pub op_index: usize,
    /// The target's content before the operation, in the form the [`Executor`] chose. `None`
    /// when the target did not exist, so that restoring removes it (spec §5.4).
    pub prior: Option<Vec<u8>>,
}

/// Where a transaction stands (spec §5.5). The journal keeps it as JSON, with the stage in
/// `state` and every name in camelCase: `{"state": "applying", "done": 2}`,
/// `{"state": "rollingBack", "from": 2}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Stage {
    /// The journal exists. Nothing is captured and nothing is applied.
    Planned,
    /// Every op's prior state is in the journal. Op 0 may be running.
    Snapshotted,
    /// `done` ops have ended. The op after them, if there is one, may be running.
    Applying {
        done: usize,
    },
    /// Every op has ended; the result is being checked.
    Verifying,
    Committed,
    /// Restoring the first `from` ops, newest first: those that may have changed the machine.
    RollingBack {
        from: usize,
    },
    RolledBack,
    /// Some ops could not be restored. `failed` are their positions in [`Journal::ops`]. Their
    /// errors are not kept here: a [`Failure`] can't be read back from JSON. The [`Outcome`]
    /// that ended the run carries them.
    RollbackIncomplete {
        failed: Vec<usize>,
    },
}

/// A transaction as the journal file holds it (spec §5.5).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Journal {
    /// Names the transaction; its files live in `snapshots\<id>\` (spec §5.3).
    pub id: String,
    pub ops: Vec<Op>,
    /// One per op, from [`Stage::Snapshotted`] on.
    pub snapshots: Vec<Snapshot>,
    pub stage: Stage,
}

impl Journal {
    /// The transaction has ended, one way or another, and there is nothing to recover.
    pub fn is_finished(&self) -> bool {
        matches!(
            self.stage,
            Stage::Committed | Stage::RolledBack | Stage::RollbackIncomplete { .. }
        )
    }
}

/// The machine side of a transaction: every change to it goes through here. Phase 1b
/// implements it for Windows; the tests use a fake.
pub trait Executor {
    /// Reads the current state of the op's target and changes nothing. `None` when the
    /// target does not exist.
    fn capture(&mut self, op: &Op) -> Result<Option<Vec<u8>>, Failure>;
    /// Applies the op. When it fails the target may be half written; the engine restores it.
    fn apply(&mut self, op: &Op) -> Result<(), Failure>;
    /// Puts back what `capture` returned (`prior`), or removes the target when `prior` is
    /// `None`.
    ///
    /// It is idempotent (spec §5.5): the engine calls it for an op that may be untouched,
    /// half written or fully applied, and again for one that an earlier rollback, cut off
    /// by a crash, already restored.
    fn restore(&mut self, op: &Op, prior: &Option<Vec<u8>>) -> Result<(), Failure>;
}

/// The check after every op has ended (spec §5.5). It changes nothing on the machine.
pub trait Verifier {
    /// Checks the machine against `ops`, the ops just applied.
    fn verify(&mut self, ops: &[Op]) -> Result<(), Failure>;
}

/// Where the journal is kept (spec §5.3).
pub trait JournalStore {
    /// Writes the journal so that it survives a crash, all or nothing: after a crash the
    /// file holds the old journal or the new one, never half of each.
    fn save(&mut self, journal: &Journal) -> Result<(), Failure>;
    /// The journal of a transaction that has not finished, if there is one.
    fn load_unfinished(&mut self) -> Result<Option<Journal>, Failure>;
}

/// How a run, or the recovery of a cut-off run, ended.
///
/// It must be looked at: dropping it could hide a `RollbackIncomplete`, and success must never
/// be reported then (spec §5.5). The compiler warns about a dropped outcome; this example
/// turns the warning into an error:
///
/// ```compile_fail
/// #![deny(unused_must_use)]
/// use cmdstudio_core::txn::{run, Executor, JournalStore, Verifier};
///
/// fn apply(exec: &mut dyn Executor, verifier: &mut dyn Verifier, store: &mut dyn JournalStore) {
///     run("t", Vec::new(), exec, verifier, store);
/// }
/// ```
#[must_use = "an outcome that is not looked at can hide a RollbackIncomplete"]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Committed,
    /// Every target is as it was before. `code` and `reason` are those of the failure that
    /// caused the rollback.
    RolledBack {
        code: &'static str,
        reason: String,
    },
    /// Some targets could not be restored. The caller must tell the user
    /// which ones and how to fix them by hand (spec §5.5); never report success.
    ///
    /// `code` and `reason` are those of the failure that caused the rollback, as in
    /// `RolledBack`. When the journal itself could not be read, that is the failure of the
    /// store.
    ///
    /// `failed` holds each op that was not restored: its position in the transaction's ops,
    /// and the error of its restore, so the user can be told what to fix by hand. They come
    /// in the order the restores were tried, the last op first. An op that the journal holds
    /// no snapshot of can't be restored at all; its error is made by the engine, with the
    /// code `E_ROLLBACK_INCOMPLETE`. `failed` is empty when the journal could not be read.
    RollbackIncomplete {
        code: &'static str,
        reason: String,
        failed: Vec<(usize, Failure)>,
    },
}

/// Applies `ops` as one transaction (spec §5.5).
///
/// Every op's prior state goes into the journal before the first op is applied. When the
/// journal can't be saved, an op fails, or the check fails, the ops that may have changed
/// the machine are restored, and the outcome says whether that worked: `RolledBack` or
/// `RollbackIncomplete`.
pub fn run(
    id: &str,
    ops: Vec<Op>,
    exec: &mut dyn Executor,
    verifier: &mut dyn Verifier,
    store: &mut dyn JournalStore,
) -> Outcome {
    let mut j = Journal {
        id: id.to_string(),
        ops,
        snapshots: Vec::new(),
        stage: Stage::Planned,
    };
    if let Err(f) = store.save(&j) {
        return Outcome::RolledBack {
            code: f.code,
            reason: f.message,
        };
    }
    for (index, op) in j.ops.iter().enumerate() {
        match exec.capture(op) {
            Ok(prior) => j.snapshots.push(Snapshot {
                op_index: index,
                prior,
            }),
            Err(f) => return finish(&mut j, store, Vec::new(), f),
        }
    }
    j.stage = Stage::Snapshotted;
    if let Err(f) = store.save(&j) {
        return finish(&mut j, store, Vec::new(), f);
    }
    for (index, op) in j.ops.iter().enumerate() {
        if let Err(f) = exec.apply(op) {
            // The failed op may be half-applied, so it is restored too.
            return rollback(&mut j, index + 1, exec, store, f);
        }
        j.stage = Stage::Applying { done: index + 1 };
        if let Err(f) = store.save(&j) {
            return rollback(&mut j, index + 1, exec, store, f);
        }
    }
    let all = j.ops.len();
    j.stage = Stage::Verifying;
    if let Err(f) = store.save(&j) {
        return rollback(&mut j, all, exec, store, f);
    }
    if let Err(f) = verifier.verify(&j.ops) {
        return rollback(&mut j, all, exec, store, f);
    }
    j.stage = Stage::Committed;
    match store.save(&j) {
        Ok(()) => Outcome::Committed,
        Err(f) => rollback(&mut j, all, exec, store, f),
    }
}

/// Finishes a transaction that was cut off (the app crashed or was killed) by rolling it
/// back, with the cause `E_TXN_INTERRUPTED`. `None` when there is nothing to recover. The
/// caller tells the user (spec §5.5).
///
/// The journal is one op behind the machine, so the ops restored, from the first, are those
/// up to the one that may have been running when it was last saved:
///
/// | stage | restored |
/// |---|---|
/// | `Planned` | none: nothing was applied |
/// | `Snapshotted` | op 0 |
/// | `Applying { done }` | the first `done + 1` ops |
/// | `Verifying` | all |
/// | `RollingBack { from }` | the first `from`: an earlier rollback was cut off |
///
/// A journal that can't be read gives `RollbackIncomplete`, with the store's own code: the
/// machine may be half applied and nothing says how.
///
/// The result must be looked at, like an [`Outcome`]: the compiler warns about a dropped one,
/// and this example turns the warning into an error:
///
/// ```compile_fail
/// #![deny(unused_must_use)]
/// use cmdstudio_core::txn::{recover, Executor, JournalStore};
///
/// fn start_up(exec: &mut dyn Executor, store: &mut dyn JournalStore) {
///     recover(exec, store);
/// }
/// ```
#[must_use = "a recovery that ended in RollbackIncomplete must reach the user"]
pub fn recover(exec: &mut dyn Executor, store: &mut dyn JournalStore) -> Option<Outcome> {
    let mut j = match store.load_unfinished() {
        Ok(Some(j)) => j,
        Ok(None) => return None,
        Err(f) => {
            return Some(Outcome::RollbackIncomplete {
                code: f.code,
                reason: format!("cannot read the journal: {}", f.message),
                failed: Vec::new(),
            })
        }
    };
    let touched = match j.stage {
        Stage::Planned => 0,
        Stage::Snapshotted => 1,
        // A damaged journal may hold any number; it must not wrap around to 0.
        Stage::Applying { done } => done.saturating_add(1),
        Stage::Verifying => j.ops.len(),
        Stage::RollingBack { from } => from,
        // Whatever the store says, a journal that has ended is not rolled back again.
        Stage::Committed | Stage::RolledBack | Stage::RollbackIncomplete { .. } => return None,
    };
    let cause = Failure {
        code: "E_TXN_INTERRUPTED",
        message: "the previous apply was interrupted".to_string(),
    };
    Some(rollback(&mut j, touched, exec, store, cause))
}

/// Restores the first `touched` ops (at most all of them), newest first, and finishes the
/// journal. An op whose restore fails is listed with the error. An op with no snapshot can't
/// be restored, so it is listed too: taking it for restored would report a success that did
/// not happen.
fn rollback(
    j: &mut Journal,
    touched: usize,
    exec: &mut dyn Executor,
    store: &mut dyn JournalStore,
    cause: Failure,
) -> Outcome {
    let touched = touched.min(j.ops.len());
    j.stage = Stage::RollingBack { from: touched };
    // Best effort: we are already failing. The outcome reports the real state either way.
    let _ = store.save(j);
    let mut failed = Vec::new();
    for (index, op) in j.ops.iter().enumerate().take(touched).rev() {
        let restored = match j.snapshots.iter().find(|s| s.op_index == index) {
            Some(snapshot) => exec.restore(op, &snapshot.prior),
            None => Err(Failure {
                code: "E_ROLLBACK_INCOMPLETE",
                message: "the journal has no snapshot of this operation".to_string(),
            }),
        };
        if let Err(f) = restored {
            failed.push((index, f));
        }
    }
    finish(j, store, failed, cause)
}

/// Closes the journal as `RolledBack`, or as `RollbackIncomplete` when `failed` holds ops
/// that were not restored. The journal keeps their positions; the outcome also carries
/// their errors.
fn finish(
    j: &mut Journal,
    store: &mut dyn JournalStore,
    failed: Vec<(usize, Failure)>,
    cause: Failure,
) -> Outcome {
    if failed.is_empty() {
        j.stage = Stage::RolledBack;
        let _ = store.save(j);
        Outcome::RolledBack {
            code: cause.code,
            reason: cause.message,
        }
    } else {
        j.stage = Stage::RollbackIncomplete {
            failed: failed.iter().map(|(index, _)| *index).collect(),
        };
        let _ = store.save(j);
        Outcome::RollbackIncomplete {
            code: cause.code,
            reason: cause.message,
            failed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::Op;
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::rc::Rc;

    /// A fake machine: a key → bytes map, with optional injected failures.
    #[derive(Default)]
    struct FakeMachine {
        state: BTreeMap<String, Vec<u8>>,
        fail_apply_key: Option<String>,
        /// Restoring one of these keys fails with the error stored for it.
        fail_restore: BTreeMap<String, Failure>,
    }

    fn failure(code: &'static str, message: &str) -> Failure {
        Failure {
            code,
            message: message.to_string(),
        }
    }

    fn key(op: &Op) -> String {
        match op {
            Op::WriteFile { path, .. } => path.display().to_string(),
            Op::SetAutoRun { .. } => "autorun".to_string(),
            Op::WtApply { settings, .. } => settings.display().to_string(),
            Op::InstallFont { family } => format!("font:{family}"),
            Op::InstallRuntime { dir, .. } => dir.display().to_string(),
        }
    }

    fn new_value(op: &Op) -> Option<Vec<u8>> {
        match op {
            Op::WriteFile { contents, .. } => Some(contents.as_bytes().to_vec()),
            Op::SetAutoRun { value } => value.as_ref().map(|v| v.as_bytes().to_vec()),
            Op::WtApply { profile, .. } => Some(serde_json::to_vec(profile).unwrap()),
            Op::InstallFont { .. } => Some(b"installed".to_vec()),
            Op::InstallRuntime { .. } => Some(b"runtime".to_vec()),
        }
    }

    impl Executor for FakeMachine {
        fn capture(&mut self, op: &Op) -> Result<Option<Vec<u8>>, Failure> {
            Ok(self.state.get(&key(op)).cloned())
        }
        fn apply(&mut self, op: &Op) -> Result<(), Failure> {
            let k = key(op);
            if self.fail_apply_key.as_deref() == Some(k.as_str()) {
                self.state.insert(k, b"HALF-WRITTEN".to_vec());
                return Err(Failure {
                    code: "E_WT_WRITE",
                    message: "disk said no".into(),
                });
            }
            match new_value(op) {
                Some(v) => self.state.insert(k, v),
                None => self.state.remove(&k),
            };
            Ok(())
        }
        fn restore(&mut self, op: &Op, prior: &Option<Vec<u8>>) -> Result<(), Failure> {
            let k = key(op);
            if let Some(failure) = self.fail_restore.get(&k) {
                return Err(failure.clone());
            }
            match prior {
                Some(v) => self.state.insert(k, v.clone()),
                None => self.state.remove(&k),
            };
            Ok(())
        }
    }

    struct Verify(Result<(), Failure>);
    impl Verifier for Verify {
        fn verify(&mut self, _: &[Op]) -> Result<(), Failure> {
            self.0.clone()
        }
    }

    #[derive(Default)]
    struct MemStore {
        saved: Vec<Journal>,
    }
    impl JournalStore for MemStore {
        fn save(&mut self, j: &Journal) -> Result<(), Failure> {
            self.saved.push(j.clone());
            Ok(())
        }
        fn load_unfinished(&mut self) -> Result<Option<Journal>, Failure> {
            Ok(self.saved.last().filter(|j| !j.is_finished()).cloned())
        }
    }

    fn ops() -> Vec<Op> {
        vec![
            Op::WriteFile {
                path: PathBuf::from("a.lua"),
                contents: "new a".into(),
            },
            Op::WtApply {
                settings: PathBuf::from("wt.json"),
                scheme: serde_json::json!({}),
                profile: serde_json::Map::new(),
                set_default: false,
            },
            Op::SetAutoRun {
                value: Some("ours".into()),
            },
        ]
    }

    fn machine_with_old_state() -> FakeMachine {
        let mut m = FakeMachine::default();
        m.state.insert("a.lua".into(), b"old a".to_vec());
        m.state.insert("wt.json".into(), b"old wt".to_vec());
        m
    }

    #[test]
    fn commits_and_journals_every_stage_in_order() {
        let (mut m, mut store) = (machine_with_old_state(), MemStore::default());
        let out = run("t1", ops(), &mut m, &mut Verify(Ok(())), &mut store);
        assert_eq!(out, Outcome::Committed);
        assert_eq!(m.state["a.lua"], b"new a");
        assert_eq!(m.state["autorun"], b"ours");
        let stages: Vec<Stage> = store.saved.iter().map(|j| j.stage.clone()).collect();
        assert_eq!(
            stages,
            vec![
                Stage::Planned,
                Stage::Snapshotted,
                Stage::Applying { done: 1 },
                Stage::Applying { done: 2 },
                Stage::Applying { done: 3 },
                Stage::Verifying,
                Stage::Committed
            ]
        );
    }

    #[test]
    fn apply_failure_restores_everything_byte_for_byte() {
        let mut m = machine_with_old_state();
        let before = m.state.clone();
        m.fail_apply_key = Some("wt.json".into());
        let out = run(
            "t2",
            ops(),
            &mut m,
            &mut Verify(Ok(())),
            &mut MemStore::default(),
        );
        assert_eq!(
            out,
            Outcome::RolledBack {
                code: "E_WT_WRITE",
                reason: "disk said no".into()
            }
        );
        assert_eq!(m.state, before);
    }

    #[test]
    fn verify_failure_rolls_back_with_the_verifier_code() {
        let mut m = machine_with_old_state();
        let before = m.state.clone();
        let mut verify = Verify(Err(Failure {
            code: "E_VERIFY_OMP",
            message: "omp exited 1".into(),
        }));
        let out = run("t3", ops(), &mut m, &mut verify, &mut MemStore::default());
        assert_eq!(
            out,
            Outcome::RolledBack {
                code: "E_VERIFY_OMP",
                reason: "omp exited 1".into()
            }
        );
        assert_eq!(m.state, before);
    }

    #[test]
    fn a_failed_restore_is_reported_never_hidden() {
        let mut m = machine_with_old_state();
        m.fail_apply_key = Some("autorun".into());
        m.fail_restore
            .insert("a.lua".into(), failure("E_ROLLBACK_INCOMPLETE", "locked"));
        let mut store = MemStore::default();
        let out = run("t4", ops(), &mut m, &mut Verify(Ok(())), &mut store);
        assert_eq!(
            out,
            Outcome::RollbackIncomplete {
                code: "E_WT_WRITE",
                reason: "disk said no".into(),
                failed: vec![(0, failure("E_ROLLBACK_INCOMPLETE", "locked"))]
            }
        );
        assert_eq!(
            store.saved.last().unwrap().stage,
            Stage::RollbackIncomplete { failed: vec![0] }
        );
    }

    #[test]
    fn recovers_a_transaction_cut_off_mid_apply() {
        let mut m = machine_with_old_state();
        let before = m.state.clone();
        let ops = ops();
        let snapshots: Vec<Snapshot> = ops
            .iter()
            .enumerate()
            .map(|(i, op)| Snapshot {
                op_index: i,
                prior: m.capture(op).unwrap(),
            })
            .collect();
        m.apply(&ops[0]).unwrap();
        m.state.insert("wt.json".into(), b"HALF-WRITTEN".to_vec()); // crashed while writing op 1
        let mut store = MemStore {
            saved: vec![Journal {
                id: "t5".into(),
                ops,
                snapshots,
                stage: Stage::Applying { done: 1 },
            }],
        };
        let out = recover(&mut m, &mut store).unwrap();
        assert_eq!(
            out,
            Outcome::RolledBack {
                code: "E_TXN_INTERRUPTED",
                reason: "the previous apply was interrupted".into()
            }
        );
        assert_eq!(m.state, before);
        assert!(store.saved.last().unwrap().is_finished());
    }

    #[test]
    fn finished_journals_are_left_alone() {
        let mut store = MemStore {
            saved: vec![Journal {
                id: "t6".into(),
                ops: ops(),
                snapshots: vec![],
                stage: Stage::Committed,
            }],
        };
        assert_eq!(recover(&mut FakeMachine::default(), &mut store), None);
    }

    // ---- Helpers for the tests below ----

    /// What `recover` reports when it rolls back a cut-off transaction.
    fn interrupted() -> Outcome {
        Outcome::RolledBack {
            code: "E_TXN_INTERRUPTED",
            reason: "the previous apply was interrupted".into(),
        }
    }

    /// What the engine captures of `ops` on `m` before it applies anything.
    fn snapshots_of(m: &mut FakeMachine, ops: &[Op]) -> Vec<Snapshot> {
        ops.iter()
            .enumerate()
            .map(|(i, op)| Snapshot {
                op_index: i,
                prior: m.capture(op).unwrap(),
            })
            .collect()
    }

    /// A journal in `stage` for the three `ops()`, with snapshots of the first two.
    fn journal_in(stage: Stage) -> Journal {
        Journal {
            id: "j".into(),
            ops: ops(),
            snapshots: vec![
                Snapshot {
                    op_index: 0,
                    prior: Some(b"old a".to_vec()),
                },
                Snapshot {
                    op_index: 1,
                    prior: None,
                },
            ],
            stage,
        }
    }

    /// Every stage, with the name the journal gives its `state`.
    fn all_stages() -> Vec<(Stage, &'static str)> {
        vec![
            (Stage::Planned, "planned"),
            (Stage::Snapshotted, "snapshotted"),
            (Stage::Applying { done: 2 }, "applying"),
            (Stage::Verifying, "verifying"),
            (Stage::Committed, "committed"),
            (Stage::RollingBack { from: 2 }, "rollingBack"),
            (Stage::RolledBack, "rolledBack"),
            (
                Stage::RollbackIncomplete { failed: vec![1, 0] },
                "rollbackIncomplete",
            ),
        ]
    }

    fn all_keys(v: &serde_json::Value, out: &mut Vec<String>) {
        match v {
            serde_json::Value::Object(map) => {
                for (k, v) in map {
                    out.push(k.clone());
                    all_keys(v, out);
                }
            }
            serde_json::Value::Array(items) => items.iter().for_each(|i| all_keys(i, out)),
            _ => {}
        }
    }

    /// What the engine did, in order: calls on the machine, journal saves and verification.
    type Trace = Rc<RefCell<Vec<String>>>;

    /// A `FakeMachine` that writes down each call, and that can fail to capture.
    struct Traced {
        machine: FakeMachine,
        fail_capture_key: Option<String>,
        trace: Trace,
    }

    impl Executor for Traced {
        fn capture(&mut self, op: &Op) -> Result<Option<Vec<u8>>, Failure> {
            self.trace.borrow_mut().push(format!("capture {}", key(op)));
            if self.fail_capture_key.as_deref() == Some(key(op).as_str()) {
                return Err(Failure {
                    code: "E_SNAPSHOT_WRITE",
                    message: format!("cannot read {}", key(op)),
                });
            }
            self.machine.capture(op)
        }
        fn apply(&mut self, op: &Op) -> Result<(), Failure> {
            self.trace.borrow_mut().push(format!("apply {}", key(op)));
            self.machine.apply(op)
        }
        fn restore(&mut self, op: &Op, prior: &Option<Vec<u8>>) -> Result<(), Failure> {
            self.trace.borrow_mut().push(format!("restore {}", key(op)));
            self.machine.restore(op, prior)
        }
    }

    /// Whether a save fails, given its number (from 1) and the stage it holds.
    type FailsSave = Box<dyn Fn(usize, &Stage) -> bool>;

    /// A `MemStore` that writes down each save, and that fails the saves `fail_save` picks.
    #[derive(Default)]
    struct TracedStore {
        saved: Vec<Journal>,
        fail_save: Option<FailsSave>,
        calls: usize,
        trace: Trace,
    }

    impl JournalStore for TracedStore {
        fn save(&mut self, j: &Journal) -> Result<(), Failure> {
            self.calls += 1;
            self.trace.borrow_mut().push(format!("save {:?}", j.stage));
            if self
                .fail_save
                .as_ref()
                .is_some_and(|fails| fails(self.calls, &j.stage))
            {
                return Err(Failure {
                    code: "E_SNAPSHOT_WRITE",
                    message: "journal said no".into(),
                });
            }
            self.saved.push(j.clone());
            Ok(())
        }
        fn load_unfinished(&mut self) -> Result<Option<Journal>, Failure> {
            Ok(self.saved.last().filter(|j| !j.is_finished()).cloned())
        }
    }

    /// A verifier that passes and writes down the ops it was asked to check.
    struct TracedVerify(Trace);

    impl Verifier for TracedVerify {
        fn verify(&mut self, ops: &[Op]) -> Result<(), Failure> {
            let keys: Vec<String> = ops.iter().map(key).collect();
            self.0
                .borrow_mut()
                .push(format!("verify {}", keys.join(",")));
            Ok(())
        }
    }

    /// `machine` and an empty store, both writing to one new trace.
    fn traced(machine: FakeMachine) -> (Traced, TracedStore, Trace) {
        let trace = Trace::default();
        let exec = Traced {
            machine,
            fail_capture_key: None,
            trace: trace.clone(),
        };
        let store = TracedStore {
            trace: trace.clone(),
            ..TracedStore::default()
        };
        (exec, store, trace)
    }

    /// The targets restored, in order.
    fn restores(trace: &Trace) -> Vec<String> {
        trace
            .borrow()
            .iter()
            .filter_map(|e| e.strip_prefix("restore "))
            .map(str::to_string)
            .collect()
    }

    fn applied_anything(trace: &Trace) -> bool {
        trace.borrow().iter().any(|e| e.starts_with("apply "))
    }

    // ---- The journal file ----

    #[test]
    fn the_journal_is_written_in_camel_case_in_every_stage() {
        for (stage, state) in all_stages() {
            let journal = journal_in(stage);
            let json = serde_json::to_value(&journal).unwrap();
            assert_eq!(json["stage"]["state"], state);
            let mut names = Vec::new();
            all_keys(&json, &mut names);
            assert!(names.contains(&"opIndex".to_string()), "{json}");
            for name in &names {
                assert!(!name.contains('_'), "{name} in {json}");
            }
            let back: Journal = serde_json::from_value(json).unwrap();
            assert_eq!(back, journal);
        }
    }

    #[test]
    fn the_journal_on_disk_has_this_exact_shape() {
        let journal = Journal {
            id: "j1".into(),
            ops: vec![Op::SetAutoRun {
                value: Some("ours".into()),
            }],
            snapshots: vec![Snapshot {
                op_index: 0,
                prior: Some(b"old".to_vec()),
            }],
            stage: Stage::Applying { done: 1 },
        };
        assert_eq!(
            serde_json::to_value(&journal).unwrap(),
            serde_json::json!({
                "id": "j1",
                "ops": [{ "kind": "setAutoRun", "value": "ours" }],
                "snapshots": [{ "opIndex": 0, "prior": [111, 108, 100] }],
                "stage": { "state": "applying", "done": 1 }
            })
        );
    }

    #[test]
    fn committed_rolled_back_and_incomplete_journals_are_finished() {
        let finished: Vec<bool> = all_stages()
            .into_iter()
            .map(|(stage, _)| journal_in(stage).is_finished())
            .collect();
        // In the order of `all_stages`: planned, snapshotted, applying, verifying, committed,
        // rollingBack, rolledBack, rollbackIncomplete.
        assert_eq!(
            finished,
            [false, false, false, false, true, false, true, true]
        );
    }

    // ---- The write-ahead order of `run` ----

    #[test]
    fn the_journal_is_always_ahead_of_the_machine() {
        let (mut exec, mut store, trace) = traced(machine_with_old_state());
        let out = run(
            "t8",
            ops(),
            &mut exec,
            &mut TracedVerify(trace.clone()),
            &mut store,
        );
        assert_eq!(out, Outcome::Committed);
        assert_eq!(
            *trace.borrow(),
            [
                "save Planned",
                "capture a.lua",
                "capture wt.json",
                "capture autorun",
                "save Snapshotted",
                "apply a.lua",
                "save Applying { done: 1 }",
                "apply wt.json",
                "save Applying { done: 2 }",
                "apply autorun",
                "save Applying { done: 3 }",
                "save Verifying",
                "verify a.lua,wt.json,autorun",
                "save Committed",
            ]
        );
        // The journal saved before the first apply already holds every op's prior state.
        assert_eq!(
            store.saved.get(1).unwrap().snapshots,
            vec![
                Snapshot {
                    op_index: 0,
                    prior: Some(b"old a".to_vec())
                },
                Snapshot {
                    op_index: 1,
                    prior: Some(b"old wt".to_vec())
                },
                Snapshot {
                    op_index: 2,
                    prior: None
                },
            ]
        );
    }

    #[test]
    fn a_failed_apply_journals_the_rollback_then_restores_newest_first() {
        let (mut exec, mut store, trace) = traced(machine_with_old_state());
        exec.machine.fail_apply_key = Some("wt.json".into());
        let out = run("t9", ops(), &mut exec, &mut Verify(Ok(())), &mut store);
        assert_eq!(
            out,
            Outcome::RolledBack {
                code: "E_WT_WRITE",
                reason: "disk said no".into()
            }
        );
        // Op 1 failed half written, so the first 2 ops are restored, op 1 first.
        assert_eq!(
            *trace.borrow(),
            [
                "save Planned",
                "capture a.lua",
                "capture wt.json",
                "capture autorun",
                "save Snapshotted",
                "apply a.lua",
                "save Applying { done: 1 }",
                "apply wt.json",
                "save RollingBack { from: 2 }",
                "restore wt.json",
                "restore a.lua",
                "save RolledBack",
            ]
        );
    }

    #[test]
    fn a_journal_that_cannot_be_written_never_leaves_the_machine_changed() {
        // Fail each save of a clean run in turn: planned, snapshotted, applying x3, verifying,
        // committed.
        let (mut clean_exec, mut clean_store, _) = traced(machine_with_old_state());
        let out = run(
            "t10",
            ops(),
            &mut clean_exec,
            &mut Verify(Ok(())),
            &mut clean_store,
        );
        assert_eq!(out, Outcome::Committed);
        for fail_at in 1..=clean_store.calls {
            let (mut exec, mut store, trace) = traced(machine_with_old_state());
            let before = exec.machine.state.clone();
            store.fail_save = Some(Box::new(move |n, _| n == fail_at));
            let out = run("t10", ops(), &mut exec, &mut Verify(Ok(())), &mut store);
            assert_eq!(
                out,
                Outcome::RolledBack {
                    code: "E_SNAPSHOT_WRITE",
                    reason: "journal said no".into()
                },
                "save {fail_at} fails"
            );
            assert_eq!(exec.machine.state, before, "save {fail_at} fails");
            // Until the snapshots are in the journal (saves 1 and 2), nothing may be applied.
            if fail_at <= 2 {
                assert!(!applied_anything(&trace), "save {fail_at} fails");
            }
        }
    }

    #[test]
    fn a_snapshot_that_cannot_be_taken_applies_nothing() {
        let (mut exec, mut store, trace) = traced(machine_with_old_state());
        let before = exec.machine.state.clone();
        exec.fail_capture_key = Some("wt.json".into());
        let out = run("t11", ops(), &mut exec, &mut Verify(Ok(())), &mut store);
        assert_eq!(
            out,
            Outcome::RolledBack {
                code: "E_SNAPSHOT_WRITE",
                reason: "cannot read wt.json".into()
            }
        );
        assert_eq!(exec.machine.state, before);
        assert!(!applied_anything(&trace));
        // The journal is closed, so the next start does not report an interrupted apply.
        assert_eq!(store.saved.last().unwrap().stage, Stage::RolledBack);
    }

    // ---- A rollback whose own journal saves fail ----

    /// A case: its name, which saves fail, the stage the journal on disk ends in, and what the
    /// next start reports.
    type SaveCase = (&'static str, fn(&Stage) -> bool, Stage, Option<Outcome>);

    #[test]
    fn a_rollback_goes_on_when_its_own_journal_saves_fail() {
        // A failed apply of wt.json saves planned, snapshotted, applying 1, rolling back 2 and
        // rolled back. What the journal on disk says in the end decides what the next start does.
        let cases: [SaveCase; 3] = [
            (
                "the rolling-back save fails",
                |s| matches!(s, Stage::RollingBack { .. }),
                Stage::RolledBack,
                None,
            ),
            (
                "the final save fails",
                |s| matches!(s, Stage::RolledBack),
                Stage::RollingBack { from: 2 },
                Some(interrupted()),
            ),
            (
                "both fail",
                |s| matches!(s, Stage::RollingBack { .. } | Stage::RolledBack),
                Stage::Applying { done: 1 },
                Some(interrupted()),
            ),
        ];
        for (name, fails, on_disk, next_start) in cases {
            let (mut exec, mut store, _) = traced(machine_with_old_state());
            let before = exec.machine.state.clone();
            exec.machine.fail_apply_key = Some("wt.json".into());
            store.fail_save = Some(Box::new(move |_, stage| fails(stage)));
            let out = run("t19", ops(), &mut exec, &mut Verify(Ok(())), &mut store);
            // The outcome and the machine don't depend on the journal being written.
            assert_eq!(
                out,
                Outcome::RolledBack {
                    code: "E_WT_WRITE",
                    reason: "disk said no".into()
                },
                "{name}"
            );
            assert_eq!(exec.machine.state, before, "{name}");
            assert_eq!(store.saved.last().unwrap().stage, on_disk, "{name}");
            // The next start finishes what the journal left open, and changes nothing more.
            store.fail_save = None;
            assert_eq!(recover(&mut exec, &mut store), next_start, "{name}");
            assert_eq!(exec.machine.state, before, "{name}");
        }
    }

    #[test]
    fn an_incomplete_rollback_that_cannot_be_journaled_is_tried_again_at_the_next_start() {
        let (mut exec, mut store, _) = traced(machine_with_old_state());
        let before = exec.machine.state.clone();
        exec.machine.fail_apply_key = Some("wt.json".into());
        exec.machine
            .fail_restore
            .insert("a.lua".into(), failure("E_ROLLBACK_INCOMPLETE", "locked"));
        store.fail_save = Some(Box::new(|_, stage| {
            matches!(stage, Stage::RollbackIncomplete { .. })
        }));
        let out = run("t20", ops(), &mut exec, &mut Verify(Ok(())), &mut store);
        // The outcome is still the truth: a.lua was not restored.
        assert_eq!(
            out,
            Outcome::RollbackIncomplete {
                code: "E_WT_WRITE",
                reason: "disk said no".into(),
                failed: vec![(0, failure("E_ROLLBACK_INCOMPLETE", "locked"))]
            }
        );
        // The journal could not be closed, so it still says RollingBack and the next start
        // tries again. By then the lock is gone.
        assert_eq!(
            store.saved.last().unwrap().stage,
            Stage::RollingBack { from: 2 }
        );
        store.fail_save = None;
        exec.machine.fail_restore.clear();
        assert_eq!(recover(&mut exec, &mut store), Some(interrupted()));
        assert_eq!(exec.machine.state, before);
    }

    // ---- What `recover` restores ----

    #[test]
    fn a_crash_during_the_first_op_restores_it() {
        // The journal is saved as Snapshotted just before op 0 starts, so a crash inside op 0
        // leaves the journal at Snapshotted and op 0 half written.
        let mut m = machine_with_old_state();
        let before = m.state.clone();
        let ops = ops();
        let snapshots = snapshots_of(&mut m, &ops);
        m.state.insert("a.lua".into(), b"HALF-WRITTEN".to_vec());
        let mut store = MemStore {
            saved: vec![Journal {
                id: "t7".into(),
                ops,
                snapshots,
                stage: Stage::Snapshotted,
            }],
        };
        let out = recover(&mut m, &mut store).unwrap();
        assert_eq!(out, interrupted());
        assert_eq!(m.state, before);
        assert!(store.saved.last().unwrap().is_finished());
    }

    #[test]
    fn recovery_restores_the_ops_a_crash_may_have_touched_newest_first() {
        // The journal is saved before op 0 starts (Snapshotted) and after each op ends
        // (Applying { done }), so the op that was running, `done` (counting from 0), may be
        // half written. Nothing was applied yet in Planned.
        let cases = [
            (Stage::Planned, vec![]),
            (Stage::Snapshotted, vec!["a.lua"]),
            (Stage::Applying { done: 1 }, vec!["wt.json", "a.lua"]),
            (
                Stage::Applying { done: 2 },
                vec!["autorun", "wt.json", "a.lua"],
            ),
            (
                Stage::Applying { done: 3 },
                vec!["autorun", "wt.json", "a.lua"],
            ),
            (Stage::Verifying, vec!["autorun", "wt.json", "a.lua"]),
            (Stage::RollingBack { from: 2 }, vec!["wt.json", "a.lua"]),
        ];
        for (stage, restored) in cases {
            let (mut exec, mut store, trace) = traced(machine_with_old_state());
            let snapshots = snapshots_of(&mut exec.machine, &ops());
            store.saved.push(Journal {
                id: "t12".into(),
                ops: ops(),
                snapshots,
                stage: stage.clone(),
            });
            let out = recover(&mut exec, &mut store);
            assert_eq!(out, Some(interrupted()), "{stage:?}");
            assert_eq!(restores(&trace), restored, "{stage:?}");
            assert!(store.saved.last().unwrap().is_finished(), "{stage:?}");
        }
    }

    /// A store that hands back its journal whatever its stage.
    struct HandsBack(Journal);

    impl JournalStore for HandsBack {
        fn save(&mut self, j: &Journal) -> Result<(), Failure> {
            self.0 = j.clone();
            Ok(())
        }
        fn load_unfinished(&mut self) -> Result<Option<Journal>, Failure> {
            Ok(Some(self.0.clone()))
        }
    }

    #[test]
    fn recovery_leaves_a_finished_journal_alone_even_if_the_store_hands_it_back() {
        // Rolling back a committed apply would undo the user's theme.
        for stage in [
            Stage::Committed,
            Stage::RolledBack,
            Stage::RollbackIncomplete { failed: vec![0] },
        ] {
            let (mut exec, _, trace) = traced(machine_with_old_state());
            let snapshots = snapshots_of(&mut exec.machine, &ops());
            let journal = Journal {
                id: "t13".into(),
                ops: ops(),
                snapshots,
                stage: stage.clone(),
            };
            let mut store = HandsBack(journal.clone());
            assert_eq!(recover(&mut exec, &mut store), None, "{stage:?}");
            assert!(trace.borrow().is_empty(), "{stage:?}");
            assert_eq!(store.0, journal, "{stage:?}");
        }
    }

    #[test]
    fn a_journal_with_absurd_counts_still_restores_every_op() {
        // A damaged or hand-edited journal must neither panic nor skip ops.
        for stage in [
            Stage::Applying { done: usize::MAX },
            Stage::RollingBack { from: usize::MAX },
        ] {
            let (mut exec, mut store, trace) = traced(machine_with_old_state());
            let snapshots = snapshots_of(&mut exec.machine, &ops());
            store.saved.push(Journal {
                id: "t14".into(),
                ops: ops(),
                snapshots,
                stage: stage.clone(),
            });
            assert_eq!(
                recover(&mut exec, &mut store),
                Some(interrupted()),
                "{stage:?}"
            );
            assert_eq!(
                restores(&trace),
                ["autorun", "wt.json", "a.lua"],
                "{stage:?}"
            );
        }
    }

    #[test]
    fn an_unreadable_journal_is_reported_never_ignored() {
        struct Unreadable;
        impl JournalStore for Unreadable {
            fn save(&mut self, _: &Journal) -> Result<(), Failure> {
                Ok(())
            }
            fn load_unfinished(&mut self) -> Result<Option<Journal>, Failure> {
                Err(Failure {
                    code: "E_SNAPSHOT_WRITE",
                    message: "journal.json is not valid JSON".into(),
                })
            }
        }
        // If the journal can't be read, the machine may be half applied and nothing says how.
        // The store's own code is kept, not only its message.
        assert_eq!(
            recover(&mut machine_with_old_state(), &mut Unreadable),
            Some(Outcome::RollbackIncomplete {
                code: "E_SNAPSHOT_WRITE",
                reason: "cannot read the journal: journal.json is not valid JSON".into(),
                failed: vec![]
            })
        );
    }

    #[test]
    fn an_op_with_no_snapshot_is_reported_as_not_restored() {
        // A journal cut short or damaged: op 1 has no snapshot to restore it from.
        let mut m = machine_with_old_state();
        let ops = ops();
        let mut snapshots = snapshots_of(&mut m, &ops);
        snapshots.retain(|s| s.op_index != 1);
        m.apply(&ops[0]).unwrap();
        m.state.insert("wt.json".into(), b"HALF-WRITTEN".to_vec());
        let mut store = MemStore {
            saved: vec![Journal {
                id: "t15".into(),
                ops,
                snapshots,
                stage: Stage::Applying { done: 1 },
            }],
        };
        let out = recover(&mut m, &mut store).unwrap();
        assert_eq!(
            out,
            Outcome::RollbackIncomplete {
                code: "E_TXN_INTERRUPTED",
                reason: "the previous apply was interrupted".into(),
                failed: vec![(
                    1,
                    failure(
                        "E_ROLLBACK_INCOMPLETE",
                        "the journal has no snapshot of this operation"
                    )
                )]
            }
        );
        assert_eq!(
            store.saved.last().unwrap().stage,
            Stage::RollbackIncomplete { failed: vec![1] }
        );
        // What could be restored was restored: op 0 is back, op 1 stays as the crash left it.
        assert_eq!(m.state["a.lua"], b"old a");
        assert_eq!(m.state["wt.json"], b"HALF-WRITTEN");
    }

    #[test]
    fn a_snapshot_belongs_to_the_op_its_op_index_names() {
        let mut m = machine_with_old_state();
        let before = m.state.clone();
        let ops = ops();
        let mut snapshots = snapshots_of(&mut m, &ops);
        snapshots.reverse();
        for op in &ops {
            m.apply(op).unwrap();
        }
        let mut store = MemStore {
            saved: vec![Journal {
                id: "t16".into(),
                ops,
                snapshots,
                stage: Stage::Verifying,
            }],
        };
        assert_eq!(recover(&mut m, &mut store), Some(interrupted()));
        assert_eq!(m.state, before);
    }

    // ---- When a restore fails ----

    #[test]
    fn failed_restores_are_listed_newest_first_each_with_its_own_error() {
        // The check fails, so all 3 ops are restored, the last first. The restores of the last
        // and the first op fail, each with its own error; the one between them works.
        let mut m = machine_with_old_state();
        m.fail_restore.insert(
            "autorun".into(),
            failure("E_AUTORUN_WRITE", "the registry key is locked"),
        );
        m.fail_restore.insert(
            "a.lua".into(),
            failure("E_ROLLBACK_INCOMPLETE", "the file is in use"),
        );
        let mut verify = Verify(Err(failure("E_VERIFY_OMP", "omp exited 1")));
        let mut store = MemStore::default();
        let out = run("t17", ops(), &mut m, &mut verify, &mut store);
        assert_eq!(
            out,
            Outcome::RollbackIncomplete {
                code: "E_VERIFY_OMP",
                reason: "omp exited 1".into(),
                failed: vec![
                    (2, failure("E_AUTORUN_WRITE", "the registry key is locked")),
                    (0, failure("E_ROLLBACK_INCOMPLETE", "the file is in use")),
                ]
            }
        );
        // A failed restore does not stop the others: wt.json is back.
        assert_eq!(m.state["wt.json"], b"old wt");
        assert_eq!(
            store.saved.last().unwrap().stage,
            Stage::RollbackIncomplete { failed: vec![2, 0] }
        );
    }

    #[test]
    fn a_restore_error_during_recovery_is_reported_with_its_error() {
        // Every op had ended (Verifying), and wt.json can't be restored now.
        let mut m = machine_with_old_state();
        let ops = ops();
        let snapshots = snapshots_of(&mut m, &ops);
        for op in &ops {
            m.apply(op).unwrap();
        }
        m.fail_restore.insert(
            "wt.json".into(),
            failure("E_WT_WRITE", "settings.json is open in another program"),
        );
        let mut store = MemStore {
            saved: vec![Journal {
                id: "t18".into(),
                ops,
                snapshots,
                stage: Stage::Verifying,
            }],
        };
        let out = recover(&mut m, &mut store).unwrap();
        assert_eq!(
            out,
            Outcome::RollbackIncomplete {
                code: "E_TXN_INTERRUPTED",
                reason: "the previous apply was interrupted".into(),
                failed: vec![(
                    1,
                    failure("E_WT_WRITE", "settings.json is open in another program")
                )]
            }
        );
        // The other two are back; wt.json stays as the interrupted apply left it.
        assert_eq!(m.state["a.lua"], b"old a");
        assert!(!m.state.contains_key("autorun"));
        assert_eq!(m.state["wt.json"], b"{}");
        assert_eq!(
            store.saved.last().unwrap().stage,
            Stage::RollbackIncomplete { failed: vec![1] }
        );
    }
}
