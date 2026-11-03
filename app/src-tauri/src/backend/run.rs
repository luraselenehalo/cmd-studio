//! The app's own record of its run (recovery spec §5.1), the safe mode it opens in (§5.2), what
//! `--recovery` does before the window (§4.4 steps 1–2), and `app_status` (§6.3, §8). `run.json`
//! is written before the window is made and deleted on a normal exit; a record left behind says
//! how the earlier run ended: never ready (a start that failed; two in a row open safe mode) or
//! ready (a run that ended abnormally: the next start shows the crash notice). Windows gives a
//! pid again, so a record is "still running" only with the same pid and the same creation time.
//! A record that cannot be read or written never stops the app: one log line says so.
use super::recovery::PauseOutcome;
use super::{note, start_notes, start_unrestored, Backend, Machine, Note};
use crate::error::AppError;
use cmdstudio_core::actions::recover_at_start;
use cmdstudio_core::recovery::{read_pause, PauseLevel, PauseReason};
use cmdstudio_core::snapshot::Unrestored;
use cmdstudio_core::state::State;
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, PoisonError};
use ts_rs::TS;

pub const RUN_VERSION: u32 = 1;
/// Starts in a row that never reached the UI, from which the next start opens in safe mode.
pub const FAILED_STARTS_FOR_SAFE_MODE: u32 = 2;
/// How many of the UI's own errors (`log_ui_error`) are logged per run (recovery spec §6.5).
pub const UI_ERRORS_PER_RUN: u32 = 20;

/// `<data>\run.json` (recovery spec §5.1).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRecord {
    pub version: u32,
    pub pid: u32,
    /// When Windows created the process (`GetProcessTimes`); `None` when it could not be read.
    pub created_at: Option<u64>,
    pub started_at: u64,
    pub ui_ready_at: Option<u64>,
    pub failed_starts: u32,
    pub safe_mode: bool,
}

/// How the app was started (`main.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuiStart {
    Normal,
    /// `cmdstudio.exe --recovery` (recovery spec §4.4).
    Recovery,
}

/// Why the app is in safe mode (recovery spec §5.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum SafeModeReason {
    /// `count` starts in a row never reached the UI.
    FailedStarts {
        count: u32,
    },
    Recovery,
}

/// The earlier run ended abnormally (recovery spec §5.3): when, and whether a crash note is
/// there. With a note, `at_ms` is the note's time; without, the time that run started.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CrashNotice {
    #[ts(type = "number")]
    pub at_ms: u64,
    pub has_report: bool,
}

/// What the `--recovery` entry did (recovery spec §4.4 steps 1–2): the start's notes (a change
/// rolled back, a restore point that cannot be read), what a rollback left to fix by hand, the
/// pause it made, or why it made none.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RecoveryEntry {
    pub notes: Vec<Note>,
    pub unrestored: Vec<Unrestored>,
    pub pause: Option<PauseOutcome>,
    /// Why no pause was made, in the core's English: no theme, already paused on this computer,
    /// state.json unreadable.
    pub skipped: Option<String>,
}

/// `app_status` (recovery spec §8, §6.3).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AppStatus {
    /// The command that changes the machine and runs now (`command_names::CHANGES_MACHINE`).
    pub busy: Option<String>,
    pub safe_mode: bool,
    pub safe_mode_reason: Option<SafeModeReason>,
    pub crash_notice: Option<CrashNotice>,
    pub recovery_entry: Option<RecoveryEntry>,
}

/// The newest crash note, for the Report tab (recovery spec §5.3).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CrashReport {
    pub file: String,
    #[ts(type = "number")]
    pub at_ms: u64,
    pub text: String,
}

/// What `start_run` decided, for `lib.rs`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartDecision {
    pub safe_mode: Option<SafeModeReason>,
    pub crash_notice: Option<CrashNotice>,
}

/// This run's state, behind one lock (replaced whole; a poisoned lock is used as it is).
pub(crate) struct RunState {
    inner: Mutex<RunInner>,
}

#[derive(Default)]
struct RunInner {
    /// This process's record as last written; `None` before `start_run`.
    mine: Option<RunRecord>,
    safe_mode: Option<SafeModeReason>,
    crash_notice: Option<CrashNotice>,
    recovery_entry: Option<RecoveryEntry>,
    busy: Option<&'static str>,
    ui_errors: u32,
}

impl RunState {
    pub(crate) fn new() -> Self {
        RunState {
            inner: Mutex::new(RunInner::default()),
        }
    }

    fn with<T>(&self, f: impl FnOnce(&mut RunInner) -> T) -> T {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        f(&mut inner)
    }
}

/// Whether the earlier record's process is the process alive now under its pid: the same
/// creation time; with no creation time on either side, the pid alone (fallback F3).
fn same_run(old: &RunRecord, alive_created_at: Option<u64>) -> bool {
    match (old.created_at, alive_created_at) {
        (Some(a), Some(b)) => a == b,
        _ => true,
    }
}

impl<M: Machine> Backend<M> {
    /// Reads the record the earlier run left, decides safe mode and the crash notice, and
    /// writes this run's record (recovery spec §5.1, §5.2). Before the window; never fails.
    pub fn start_run(&self, how: GuiStart) -> StartDecision {
        let fs = self.sys().fs;
        let file = self.paths().run_file();
        let me = self.sys().host.this_process();
        let now = self.now_ms();
        let mut failed_starts = 0;
        let mut crash_notice = None;
        match fs.read(&file) {
            Err(_) => self.note_line("run record=unreadable"),
            Ok(None) => {}
            Ok(Some(bytes)) => match serde_json::from_slice::<RunRecord>(&bytes) {
                Err(_) => self.note_line("run record=damaged"),
                Ok(old) if old.version != RUN_VERSION => {
                    self.note_line("run record=other-version");
                }
                Ok(old) => {
                    let alive = match self.sys().host.process_created_at(old.pid) {
                        Ok(Some(created)) => same_run(&old, Some(created)),
                        // Gone, or a process this user may not open (not this app's own earlier
                        // run, which runs as this user).
                        Ok(None) => false,
                        Err(_) => {
                            // Counted as gone, as before; the line says it could not be told.
                            self.note_line("run record=process-unreadable");
                            false
                        }
                    };
                    if alive && old.pid != me.pid {
                        // Spec §5.1: single-instance ends this process in a moment (its plugin
                        // runs after this). Nothing is written, so the running process's record
                        // stays its own, and `app_ready`/`end_run` here are no-ops (`mine` None).
                        self.note_line("run record=still-running");
                        return StartDecision {
                            safe_mode: None,
                            crash_notice: None,
                        };
                    } else if old.ui_ready_at.is_none() {
                        failed_starts = old.failed_starts.saturating_add(1);
                    } else {
                        crash_notice = Some(self.crash_notice_for(&old));
                    }
                }
            },
        }
        let safe_mode = match how {
            GuiStart::Recovery => Some(SafeModeReason::Recovery),
            GuiStart::Normal if failed_starts >= FAILED_STARTS_FOR_SAFE_MODE => {
                Some(SafeModeReason::FailedStarts {
                    count: failed_starts,
                })
            }
            GuiStart::Normal => None,
        };
        let mine = RunRecord {
            version: RUN_VERSION,
            pid: me.pid,
            created_at: me.created_at_ms,
            started_at: now,
            ui_ready_at: None,
            failed_starts,
            safe_mode: safe_mode.is_some(),
        };
        self.write_record(&mine);
        self.note_line(&format!(
            "run start safe={} failed_starts={failed_starts}",
            safe_mode.is_some()
        ));
        self.run.with(|r| {
            r.mine = Some(mine);
            r.safe_mode = safe_mode;
            r.crash_notice = crash_notice;
        });
        StartDecision {
            safe_mode,
            crash_notice,
        }
    }

    /// Safe mode from now on, for a `--recovery` that reached a running app (recovery spec §4.4).
    pub fn enter_safe_mode(&self, reason: SafeModeReason) {
        let mine = self.run.with(|r| {
            r.safe_mode = Some(reason);
            let m = r.mine.as_mut()?;
            m.safe_mode = true;
            Some(m.clone())
        });
        if let Some(mine) = mine {
            self.write_record(&mine);
        }
    }

    /// The notice of a run that reached its UI and ended abnormally: the newest crash note's
    /// time when there is one newer than that run's start, else the time it started.
    fn crash_notice_for(&self, old: &RunRecord) -> CrashNotice {
        match self.latest_crash_note_since(old.started_at) {
            Some(at_ms) => CrashNotice {
                at_ms,
                has_report: true,
            },
            None => CrashNotice {
                at_ms: old.started_at,
                has_report: false,
            },
        }
    }

    /// The time of the newest crash note written at or after `since_ms` (`crash::latest_note`).
    fn latest_crash_note_since(&self, since_ms: u64) -> Option<u64> {
        crate::crash::latest_note(self.sys().fs, &self.paths().logs())
            .map(|(ms, _)| ms)
            .filter(|ms| *ms >= since_ms)
    }

    /// When this run started, as its record says; `None` before `start_run`.
    pub(crate) fn run_started_at(&self) -> Option<u64> {
        self.run.with(|r| r.mine.as_ref().map(|m| m.started_at))
    }

    /// The newest crash note, for the Report tab (recovery spec §5.3): its file name, its time,
    /// and its text (written redacted; redacted once more here, in case the hook had no name to
    /// take out).
    pub fn crash_report_read(&self) -> Result<Option<CrashReport>, AppError> {
        let fs = self.sys().fs;
        let Some((at_ms, path)) = crate::crash::latest_note(fs, &self.paths().logs()) else {
            return Ok(None);
        };
        let bytes = fs
            .read(&path)
            .map_err(|e| AppError::coded("E_MACHINE_READ", e.to_string()))?
            .unwrap_or_default();
        let text = crate::crash::redact_all(&String::from_utf8_lossy(&bytes), &self.redaction());
        Ok(Some(CrashReport {
            file: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            at_ms,
            text,
        }))
    }

    /// The one rule of what is taken out of a text: the names `diagnostics_copy` redacts
    /// (`user_names`: the Windows user name and the profile folder's names, the 8.3 alias
    /// included) and this user's SID; empty when they cannot be read, so nothing is taken out.
    /// The panic hook takes its copy from here (`main.rs`).
    pub fn redaction(&self) -> crate::crash::Redaction {
        crate::crash::Redaction {
            names: self.user_names().unwrap_or_default(),
            sid: self.sys().host.user_sid().ok(),
        }
    }

    /// The `--recovery` entry's thread could not be started (carry 3): the reason is the
    /// entry's outcome, for the Recovery page, and one line in the log.
    pub(crate) fn recovery_entry_not_run(&self, why: &str) {
        self.note_line("recovery_entry not_run");
        let entry = RecoveryEntry {
            notes: Vec::new(),
            unrestored: Vec::new(),
            pause: None,
            skipped: Some(format!(
                "CMD Studio could not start its recovery step: {why}"
            )),
        };
        self.run.with(|r| r.recovery_entry = Some(entry));
    }

    fn write_record(&self, record: &RunRecord) {
        let bytes = match serde_json::to_vec_pretty(record) {
            Ok(mut b) => {
                b.push(b'\n');
                b
            }
            Err(_) => return self.note_line("run record=unwritable"),
        };
        if self
            .sys()
            .fs
            .write_atomic(&self.paths().run_file(), &bytes)
            .is_err()
        {
            self.note_line("run record=unwritable");
        }
    }

    /// `--recovery`'s steps 1–2 (recovery spec §4.4): the cut-off change rolled back under the
    /// lock, as every command that changes the machine does first, then the theme paused on
    /// this computer (`reason: recovery`) when one is applied and no pause of this computer is
    /// in place. Run off the window's thread (`lib.rs`); the outcome goes to `app_status`.
    pub fn recovery_entry(&self) -> RecoveryEntry {
        self.recovery_entry_with(|_| {})
    }

    /// [`recovery_entry`] with `during` run once the entry is marked busy, for a test that looks
    /// at `app_status` meanwhile. The entry is `busy` under [`RECOVERY_ENTRY_BUSY`] for its whole
    /// duration (K12): the UI's start waits for it and asks again when it ends.
    pub(crate) fn recovery_entry_with(&self, during: impl FnOnce(&Self)) -> RecoveryEntry {
        let _busy = self.busy(RECOVERY_ENTRY_BUSY);
        during(self);
        let sys = self.sys();
        let ctx = self.ctx();
        let (mut notes, unrestored) = match recover_at_start(sys, &ctx) {
            Ok(report) => (start_notes(&report), start_unrestored(&report)),
            Err(e) => (vec![note(e.code(), e.to_string())], Vec::new()),
        };
        let state = State::load(sys.fs, &ctx.paths.state_file());
        let level = match read_pause(sys.fs, &ctx.paths) {
            Ok(Some(p)) => Some(p.level(&state)),
            Ok(None) => None,
            Err(e) => {
                notes.push(note(
                    "E_MACHINE_READ",
                    format!("paused.json could not be read: {e}"),
                ));
                None
            }
        };
        let (pause, skipped) = match (&state, level) {
            (Err(e), _) => (None, Some(format!("state.json could not be read: {e}"))),
            (_, Some(PauseLevel::Computer)) => (
                None,
                Some("the theme is already paused on this computer".to_string()),
            ),
            (Ok(State { applied: None, .. }), _) => (None, Some("no theme is applied".to_string())),
            (Ok(_), _) => (Some(self.pause_machine(PauseReason::Recovery, 0)), None),
        };
        let entry = RecoveryEntry {
            notes,
            unrestored,
            pause,
            skipped,
        };
        self.run.with(|r| r.recovery_entry = Some(entry.clone()));
        entry
    }

    /// The UI drew its first page (recovery spec §5.1): `uiReadyAt` is noted and the count of
    /// failed starts goes back to 0. A record that cannot be written is logged, never an error
    /// to the UI.
    pub fn app_ready(&self) -> Result<(), AppError> {
        let now = self.now_ms();
        let mine = self.run.with(|r| {
            let m = r.mine.as_mut()?;
            m.ui_ready_at = Some(now);
            m.failed_starts = 0;
            Some(m.clone())
        });
        if let Some(mine) = mine {
            self.write_record(&mine);
        }
        Ok(())
    }

    /// A normal exit (the window closed, Tauri's exit event): this process's record goes. One
    /// another process wrote since (`app_restart`: the new process started before this one
    /// ended) stays as it is.
    pub fn end_run(&self) {
        let fs = self.sys().fs;
        let file = self.paths().run_file();
        let Some(mine) = self.run.with(|r| r.mine.clone()) else {
            return;
        };
        // Deleted when there is no file, when the file is nobody's record (it does not parse), or
        // when it is this process's own. A read that fails says nothing about whose it is
        // (another process may be renaming its record into place): it stays.
        let delete = match fs.read(&file) {
            Err(_) => {
                self.note_line("run record=unreadable-at-end");
                false
            }
            Ok(None) => false,
            Ok(Some(bytes)) => serde_json::from_slice::<RunRecord>(&bytes).map_or(true, |t| {
                t.pid == mine.pid && t.created_at == mine.created_at
            }),
        };
        if delete {
            let _ = fs.remove_file(&file);
        }
        self.note_line("run end");
    }

    pub fn app_status(&self) -> Result<AppStatus, AppError> {
        Ok(self.run.with(|r| AppStatus {
            busy: r.busy.map(str::to_string),
            safe_mode: r.safe_mode.is_some(),
            safe_mode_reason: r.safe_mode,
            crash_notice: r.crash_notice,
            recovery_entry: r.recovery_entry.clone(),
        }))
    }

    /// "Close" on the crash notice (recovery spec §5.3): noted for this run.
    pub fn crash_dismiss(&self) {
        self.run.with(|r| r.crash_notice = None);
    }

    /// The UI's own error (recovery spec §6.5): one log line, at most [`UI_ERRORS_PER_RUN`] per
    /// run; `place` is letters and digits only, 40 at most. The text goes through the
    /// diagnostics' redaction (`redact_for_log`), with line breaks folded.
    pub fn log_ui_error(&self, message: &str, place: &str) -> Result<(), AppError> {
        let allowed = self.run.with(|r| {
            if r.ui_errors >= UI_ERRORS_PER_RUN {
                return false;
            }
            r.ui_errors += 1;
            true
        });
        if !allowed {
            return Ok(());
        }
        let text = self.redact_for_log(message);
        let place: String = place
            .chars()
            .filter(|c| c.is_alphanumeric())
            .take(40)
            .collect();
        self.note_line(&format!("ui_error place={place} {text}"));
        Ok(())
    }

    /// The text as the diagnostics would show it: the user's names and SID taken out, line
    /// breaks folded.
    fn redact_for_log(&self, text: &str) -> String {
        crate::crash::redact_all(&text.replace(['\r', '\n'], " "), &self.redaction())
    }

    /// Marks `name` as what runs now, until the guard drops: also when the work panics, so a
    /// panic inside a change never leaves `app_status` saying busy for the rest of the run.
    pub(crate) fn busy(&self, name: &'static str) -> Busy<'_, M> {
        let replaced = self.run.with(|r| r.busy.replace(name));
        Busy {
            backend: self,
            replaced,
        }
    }

    /// Marks `name` as what runs now, with no guard: for work that is started on another thread
    /// (`lib.rs`'s `--recovery` entry is marked before its thread exists, so the UI's first
    /// `app_status` cannot find it idle). [`Backend::busy_clear`] ends it.
    pub(crate) fn busy_mark(&self, name: &'static str) {
        self.run.with(|r| r.busy = Some(name));
    }

    /// Ends what [`Backend::busy_mark`] began.
    pub(crate) fn busy_clear(&self) {
        self.run.with(|r| r.busy = None);
    }
}

/// `UI_ERRORS_PER_RUN`'s sibling: the name `app_status.busy` carries while the `--recovery`
/// entry runs (recovery spec §4.4, §6.3). Not in `CHANGES_MACHINE` (it is no IPC command); the
/// UI's start shows its own text for it.
pub const RECOVERY_ENTRY_BUSY: &str = "recovery_entry";

/// The guard of [`Backend::busy`].
/// On drop it gives back what it replaced, so a command that runs inside the `--recovery` entry
/// does not end the entry's busy early.
pub(crate) struct Busy<'a, M: Machine> {
    backend: &'a Backend<M>,
    replaced: Option<&'static str>,
}

impl<M: Machine> Drop for Busy<'_, M> {
    fn drop(&mut self) {
        let replaced = self.replaced;
        self.backend.run.with(|r| r.busy = replaced);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testing::{apply_terracotta, backend, fake_fs, fake_host};
    use cmdstudio_core::adapters::fake::FakeMachine;
    use cmdstudio_core::recovery::{read_pause, PauseLevel, PauseReason, PausedBy};
    use cmdstudio_core::state::State;

    fn record(b: &Backend<FakeMachine>) -> Option<RunRecord> {
        let bytes = b.sys().fs.read(&b.paths().run_file()).unwrap()?;
        Some(serde_json::from_slice(&bytes).unwrap())
    }

    fn old(ready: Option<u64>, failed: u32) -> RunRecord {
        RunRecord {
            version: 1,
            pid: 9999,
            created_at: Some(5),
            started_at: 1_791_694_000_000,
            ui_ready_at: ready,
            failed_starts: failed,
            safe_mode: false,
        }
    }

    fn put(b: &Backend<FakeMachine>, r: &RunRecord) {
        b.sys()
            .fs
            .write_atomic(&b.paths().run_file(), &serde_json::to_vec(r).unwrap())
            .unwrap();
    }

    /// The first start writes its record before the window, as spec §5.1 has it, and a
    /// normal run marks itself ready and ends with the file gone.
    #[test]
    fn a_start_writes_its_record_and_a_normal_run_ends_without_one() {
        let b = backend();
        let decided = b.start_run(GuiStart::Normal);
        assert_eq!((decided.safe_mode, decided.crash_notice), (None, None));
        let r = record(&b).unwrap();
        assert_eq!(
            (r.version, r.pid, r.created_at),
            (1, 4242, Some(1_791_694_000_000))
        );
        assert_eq!(
            (r.ui_ready_at, r.failed_starts, r.safe_mode),
            (None, 0, false)
        );
        assert!(r.started_at >= 1_791_694_709_000);
        b.app_ready().unwrap();
        assert!(record(&b).unwrap().ui_ready_at.is_some());
        assert!(!b.app_status().unwrap().safe_mode);
        b.end_run();
        assert_eq!(record(&b), None);
        let log = b.log_tail(5);
        assert!(
            log.contains("run start safe=false failed_starts=0"),
            "{log}"
        );
        assert!(log.contains("run end"), "{log}");
    }

    /// Spec §5.1: a record whose process is gone and that never reached its UI is a start that
    /// failed; two of them open the third start in safe mode. One that reached its UI is a run
    /// that ended abnormally: the next start says so, and counts no failed start.
    #[test]
    fn two_failed_starts_open_safe_mode_and_an_ended_run_is_a_crash_notice() {
        let b = backend();
        put(&b, &old(None, 0));
        let d = b.start_run(GuiStart::Normal);
        assert_eq!(d.safe_mode, None);
        assert_eq!(record(&b).unwrap().failed_starts, 1);
        put(&b, &old(None, 1));
        let d = b.start_run(GuiStart::Normal);
        assert_eq!(d.safe_mode, Some(SafeModeReason::FailedStarts { count: 2 }));
        let r = record(&b).unwrap();
        assert_eq!((r.failed_starts, r.safe_mode), (2, true));
        let s = b.app_status().unwrap();
        assert!(s.safe_mode);
        assert_eq!(
            s.safe_mode_reason,
            Some(SafeModeReason::FailedStarts { count: 2 })
        );
        // Ready in safe mode: the count goes back to 0, so a restart is a normal start.
        b.app_ready().unwrap();
        assert_eq!(record(&b).unwrap().failed_starts, 0);
        // A run that reached its UI and then died.
        put(&b, &old(Some(1_791_694_100_000), 0));
        let d = b.start_run(GuiStart::Normal);
        assert_eq!(d.safe_mode, None);
        assert_eq!(
            d.crash_notice,
            Some(CrashNotice {
                at_ms: 1_791_694_000_000,
                has_report: false
            })
        );
        assert_eq!(record(&b).unwrap().failed_starts, 0);
        assert_eq!(b.app_status().unwrap().crash_notice, d.crash_notice);
        b.crash_dismiss();
        assert_eq!(b.app_status().unwrap().crash_notice, None);
    }

    /// Review Focus 1: a pid Windows gave again is a run that ended, not one still running.
    /// Only the same pid with the same creation time is "still running" (spec §5.1: then
    /// single-instance handled it, and the record is skipped, not counted).
    #[test]
    fn a_live_pid_with_another_creation_time_is_a_run_that_ended() {
        let b = backend();
        let alive = RunRecord {
            pid: 777,
            created_at: Some(50),
            failed_starts: 1,
            ..old(None, 1)
        };
        // Alive with the same creation time: skipped (no count), and its record is left as it
        // is: single-instance ends this process, and nothing of this process's touches the file
        // (K11: `app_ready` and `end_run` are no-ops for it).
        fake_host(&b).processes.borrow_mut().insert(777, 50);
        put(&b, &alive);
        let d = b.start_run(GuiStart::Normal);
        assert_eq!((d.safe_mode, d.crash_notice), (None, None));
        assert_eq!(record(&b), Some(alive.clone()));
        b.app_ready().unwrap();
        b.end_run();
        assert_eq!(
            record(&b),
            Some(alive.clone()),
            "the running process's record is untouched"
        );
        assert!(
            b.log_tail(5).contains("run record=still-running"),
            "{}",
            b.log_tail(5)
        );
        // The same pid, another creation time: that process is another program; the run ended.
        fake_host(&b).processes.borrow_mut().insert(777, 51);
        put(&b, &alive);
        let d = b.start_run(GuiStart::Normal);
        assert_eq!(d.safe_mode, Some(SafeModeReason::FailedStarts { count: 2 }));
        // A record with no creation time is compared on the pid alone (F3).
        let no_time = RunRecord {
            created_at: None,
            ..alive.clone()
        };
        put(&b, &no_time);
        let d = b.start_run(GuiStart::Normal);
        assert_eq!(d.safe_mode, None, "pid 777 is alive: skipped");
        assert_eq!(record(&b), Some(no_time));
    }

    /// Review Focus 2: after `app_restart` the old process exits while the new one's record is
    /// there; the exit deletes only a record of this process.
    #[test]
    fn a_normal_exit_deletes_only_this_processs_own_record() {
        let b = backend();
        b.start_run(GuiStart::Normal);
        let theirs = RunRecord {
            pid: 4243,
            created_at: Some(1_791_694_000_500),
            started_at: 1_791_694_709_500,
            ..old(None, 0)
        };
        put(&b, &theirs);
        b.end_run();
        assert_eq!(record(&b), Some(theirs));
    }

    /// Spec §5.1's last line: a record that cannot be read or written never stops the app, and
    /// says so in one log line.
    #[test]
    fn a_record_that_cannot_be_read_or_written_never_stops_the_start() {
        let b = backend();
        let file = b.paths().run_file();
        fake_fs(&b).put(&file, b"{ not json");
        let d = b.start_run(GuiStart::Normal);
        assert_eq!((d.safe_mode, d.crash_notice), (None, None));
        assert_eq!(
            record(&b).unwrap().failed_starts,
            0,
            "a damaged record counts nothing"
        );
        assert!(
            b.log_tail(5).contains("run record=damaged"),
            "{}",
            b.log_tail(5)
        );
        fake_fs(&b).fail_writes_to(&file);
        let d = b.start_run(GuiStart::Normal);
        assert_eq!(d.safe_mode, None);
        assert!(
            b.app_ready().is_ok(),
            "ready never fails the UI for the record"
        );
        assert!(
            b.log_tail(10).contains("run record=unwritable"),
            "{}",
            b.log_tail(10)
        );
        fake_fs(&b).stop_failing(&file);
        fake_fs(&b).fail_reads_of(&file);
        let d = b.start_run(GuiStart::Normal);
        assert_eq!(d.safe_mode, None);
        assert!(
            b.log_tail(10).contains("run record=unreadable"),
            "{}",
            b.log_tail(10)
        );
    }

    /// `--recovery` opens safe mode whatever the record says, and a running app told to recover
    /// is in safe mode from then on.
    #[test]
    fn the_recovery_start_is_safe_mode_for_its_own_reason() {
        let b = backend();
        let d = b.start_run(GuiStart::Recovery);
        assert_eq!(d.safe_mode, Some(SafeModeReason::Recovery));
        assert!(record(&b).unwrap().safe_mode);
        assert_eq!(
            b.app_status().unwrap().safe_mode_reason,
            Some(SafeModeReason::Recovery)
        );
        let b = backend();
        b.start_run(GuiStart::Normal);
        b.enter_safe_mode(SafeModeReason::Recovery);
        assert_eq!(
            b.app_status().unwrap().safe_mode_reason,
            Some(SafeModeReason::Recovery)
        );
        assert!(record(&b).unwrap().safe_mode);
    }

    /// Spec §4.4 steps 1–2: with a theme applied and no pause of this computer, `--recovery`
    /// pauses it (`reason: recovery`) through the usual pause; the status carries the outcome.
    #[test]
    fn recovery_entry_pauses_an_applied_theme_on_this_computer() {
        let b = backend();
        apply_terracotta(&b);
        b.start_run(GuiStart::Recovery);
        let entry = b.recovery_entry();
        assert_eq!(entry.skipped, None);
        let pause = entry.pause.unwrap();
        assert!(pause.paused_now, "{pause:?}");
        let p = read_pause(b.sys().fs, b.paths()).unwrap().unwrap();
        assert_eq!(
            (p.by, p.reason),
            (PausedBy::App, Some(PauseReason::Recovery))
        );
        assert_eq!(
            p.level(&State::load(b.sys().fs, &b.paths().state_file())),
            PauseLevel::Computer
        );
        let status = b.app_status().unwrap();
        assert!(status.recovery_entry.unwrap().pause.unwrap().paused_now);
        let log = b.log_tail(10);
        assert!(
            log.contains("pause reason=recovery count=0 revert=ok saved=true"),
            "{log}"
        );
    }

    /// Review Focus 5: a computer already paused is not reverted again, and a state.json that
    /// cannot be read is not reverted blind; both are said, and no theme is nothing to pause.
    #[test]
    fn recovery_entry_pauses_nothing_twice_and_says_when_it_cannot_tell() {
        let b = backend();
        b.start_run(GuiStart::Recovery);
        let entry = b.recovery_entry();
        assert_eq!(entry.pause, None);
        assert_eq!(entry.skipped.as_deref(), Some("no theme is applied"));
        // Paused on this computer already (by the user): nothing to do again.
        apply_terracotta(&b);
        b.theme_pause(PauseReason::User).unwrap();
        let points_before = b.snapshots_list().unwrap().points.len();
        let entry = b.recovery_entry();
        assert_eq!(entry.pause, None);
        assert_eq!(
            entry.skipped.as_deref(),
            Some("the theme is already paused on this computer")
        );
        assert_eq!(
            b.snapshots_list().unwrap().points.len(),
            points_before,
            "no second revert"
        );
        // state.json cannot be read: whether a theme is applied is not known; nothing is reverted.
        fake_fs(&b).fail_reads_of(b.paths().state_file());
        let entry = b.recovery_entry();
        assert_eq!(entry.pause, None);
        assert!(
            entry
                .skipped
                .as_deref()
                .unwrap()
                .starts_with("state.json could not be read"),
            "{entry:?}"
        );
    }

    /// Spec §4.4 step 1: the entry runs the start every change runs first (`recover_at_start`:
    /// a cut-off change rolled back, restore points that cannot be read set aside), and its
    /// notes say so the §5.5 way. A restore point whose manifest is damaged is the case the fake
    /// can make: the start sets it aside and the note carries `E_SNAPSHOT_READ`.
    #[test]
    fn recovery_entry_runs_the_start_first_and_carries_its_notes() {
        let b = backend();
        apply_terracotta(&b);
        let id = b
            .snapshots_list()
            .unwrap()
            .points
            .into_iter()
            .next()
            .unwrap()
            .id;
        let manifest = b.paths().snapshots().join(&id).join("manifest.json");
        assert!(b.sys().fs.exists(&manifest), "{}", manifest.display());
        fake_fs(&b).put(&manifest, b"{ not json");
        let entry = b.recovery_entry();
        assert!(
            entry
                .notes
                .iter()
                .any(|n| n.code == "E_SNAPSHOT_READ" && n.details.contains(&id)),
            "{:?}",
            entry.notes
        );
        // The pause came after the start, and went through.
        assert!(entry.pause.unwrap().paused_now);
    }

    /// Spec §6.3: while a command that changes the machine runs, `app_status` names it; after
    /// it, nothing. The names are the one list `CHANGES_MACHINE`.
    #[test]
    fn the_status_names_the_change_that_runs() {
        use crate::command_names::{APPLY, CHANGES_MACHINE, PLAN_APPLY, THEMES_LIST};
        let b = backend();
        assert_eq!(b.app_status().unwrap().busy, None);
        let seen = std::cell::Cell::new(None);
        let _ = b.logged(APPLY, |b| {
            seen.set(b.app_status().unwrap().busy);
            Ok::<(), AppError>(())
        });
        assert_eq!(seen.into_inner().as_deref(), Some("apply"));
        assert_eq!(b.app_status().unwrap().busy, None);
        let _ = b.logged(THEMES_LIST, |b| {
            assert_eq!(b.app_status().unwrap().busy, None, "a read is not busy");
            Ok::<(), AppError>(())
        });
        assert!(
            !CHANGES_MACHINE.contains(&PLAN_APPLY),
            "planning changes nothing"
        );
        assert_eq!(CHANGES_MACHINE.len(), 5);
        // A change that panics leaves nothing busy (the guard, K20 f).
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _: Result<(), AppError> = b.logged(APPLY, |_| panic!("a bug"));
        }));
        assert_eq!(b.app_status().unwrap().busy, None);
    }

    /// K12: the `--recovery` entry is busy under its own name for its whole duration, so the
    /// UI's start waits for it (recovery spec §6.3) and reads the page once it has ended.
    #[test]
    fn the_recovery_entry_is_busy_under_its_own_name_while_it_runs() {
        let b = backend();
        apply_terracotta(&b);
        b.start_run(GuiStart::Recovery);
        let seen = std::cell::Cell::new(None);
        let entry = b.recovery_entry_with(|b| seen.set(b.app_status().unwrap().busy));
        assert_eq!(seen.into_inner().as_deref(), Some(RECOVERY_ENTRY_BUSY));
        assert_eq!(RECOVERY_ENTRY_BUSY, "recovery_entry");
        assert!(!crate::command_names::CHANGES_MACHINE.contains(&RECOVERY_ENTRY_BUSY));
        assert!(entry.pause.unwrap().paused_now);
        assert_eq!(b.app_status().unwrap().busy, None);
    }

    /// Spec §6.5: the UI's errors are logged, at most twenty per run (Task 5 pins the
    /// redaction; here the cap and the place).
    #[test]
    fn ui_errors_are_logged_twenty_times_per_run() {
        let b = backend();
        for n in 0..25 {
            b.log_ui_error(&format!("TypeError: x is undefined ({n})"), "Home")
                .unwrap();
        }
        let log = b.log_tail(40);
        assert_eq!(log.matches("ui_error place=Home ").count(), 20, "{log}");
        assert!(!log.contains("(24)"), "{log}");
    }

    /// Spec §5.3: a run that ended abnormally with a crash note newer than its start has a
    /// report, which `crash_report_read` gives redacted, and the diagnostics attach.
    #[test]
    fn a_crash_note_newer_than_the_run_makes_the_notice_a_report() {
        let b = backend();
        let logs = b.paths().logs();
        crate::crash::write_note(b.sys().fs, &logs, 1_791_693_000_000, "older\n").unwrap();
        put(&b, &old(Some(1_791_694_100_000), 0));
        let d = b.start_run(GuiStart::Normal);
        assert!(
            !d.crash_notice.unwrap().has_report,
            "an older note is not this run's"
        );
        crate::crash::write_note(
            b.sys().fs,
            &logs,
            1_791_694_200_000,
            "message: at C:\\Users\\tester\\x\n",
        )
        .unwrap();
        put(&b, &old(Some(1_791_694_100_000), 0));
        let d = b.start_run(GuiStart::Normal);
        assert_eq!(
            d.crash_notice,
            Some(CrashNotice {
                at_ms: 1_791_694_200_000,
                has_report: true
            })
        );
        let report = b.crash_report_read().unwrap().unwrap();
        assert_eq!(report.file, "crash-1791694200000.txt");
        assert_eq!(report.at_ms, 1_791_694_200_000);
        assert!(
            report.text.contains("%USERNAME%") && !report.text.contains("tester"),
            "{}",
            report.text
        );
        let diagnostics = b.diagnostics_copy().unwrap();
        assert!(
            diagnostics.contains("== crash report crash-1791694200000.txt =="),
            "{diagnostics}"
        );
        assert!(diagnostics.contains("%USERNAME%\\x"), "{diagnostics}");
        // A note older than this run, and no notice: the diagnostics leave it out.
        let b2 = backend();
        crate::crash::write_note(b2.sys().fs, &b2.paths().logs(), 1_791_694_000_000, "old\n")
            .unwrap();
        let d2 = b2.start_run(GuiStart::Normal);
        assert_eq!(d2.crash_notice, None);
        assert!(b2.run_started_at().unwrap() > 1_791_694_000_000);
        assert!(!b2.diagnostics_copy().unwrap().contains("== crash report"));
        // A note written during this run (a caught panic, say): attached.
        crate::crash::write_note(b2.sys().fs, &b2.paths().logs(), b2.now_ms() + 1, "now\n")
            .unwrap();
        assert!(b2.diagnostics_copy().unwrap().contains("== crash report"));
        // No note at all: none to read.
        let b = backend();
        assert_eq!(b.crash_report_read().unwrap(), None);
    }

    /// Spec §6.5: the UI's error text is redacted like the diagnostics.
    #[test]
    fn a_ui_error_is_logged_redacted() {
        let b = backend();
        b.log_ui_error(
            "Error: file:///C:/Users/tester/x.js S-1-5-21-1-2-3-1001",
            "Themes",
        )
        .unwrap();
        let log = b.log_tail(3);
        assert!(
            log.contains("ui_error place=Themes Error: file:///C:/Users/%USERNAME%/x.js <SID>"),
            "{log}"
        );
    }

    /// Carry 3: an entry whose thread could not be started leaves a reason the Recovery page
    /// can show, and nothing stays busy.
    #[test]
    fn an_entry_that_could_not_start_leaves_its_reason_and_nothing_busy() {
        let b = backend();
        b.busy_mark(RECOVERY_ENTRY_BUSY);
        assert_eq!(
            b.app_status().unwrap().busy.as_deref(),
            Some(RECOVERY_ENTRY_BUSY)
        );
        b.busy_clear();
        b.recovery_entry_not_run("out of threads");
        let status = b.app_status().unwrap();
        assert_eq!(status.busy, None);
        let entry = status.recovery_entry.unwrap();
        assert_eq!(
            entry.skipped.as_deref(),
            Some("CMD Studio could not start its recovery step: out of threads")
        );
        assert_eq!((entry.pause, entry.notes.len()), (None, 0));
        assert!(
            b.log_tail(3).contains("recovery_entry not_run"),
            "{}",
            b.log_tail(3)
        );
    }

    /// Carry 1 (Task 4's review): a record that cannot be read at the end is left alone, since
    /// another process may be renaming its own into place under a sharing violation; one line
    /// says so. No file, a file that is nobody's record, and our own record are deleted.
    #[test]
    fn the_end_deletes_only_what_is_known_to_be_ours() {
        let b = backend();
        let file = b.paths().run_file();
        b.start_run(GuiStart::Normal);
        fake_fs(&b).fail_reads_of(&file);
        b.end_run();
        fake_fs(&b).stop_failing(&file);
        assert!(
            b.sys().fs.exists(&file),
            "a record that could not be read stays"
        );
        assert!(
            b.log_tail(5).contains("run record=unreadable-at-end"),
            "{}",
            b.log_tail(5)
        );
        // Not a record at all: nobody's, deleted.
        fake_fs(&b).put(&file, b"{ not json");
        b.end_run();
        assert!(!b.sys().fs.exists(&file));
        // No file: nothing to delete, and no failure.
        b.end_run();
        assert!(!b.sys().fs.exists(&file));
        // Ours: deleted.
        b.start_run(GuiStart::Normal);
        assert!(b.sys().fs.exists(&file));
        b.end_run();
        assert!(!b.sys().fs.exists(&file));
    }

    /// Carry 2: a guard gives back what it replaced. An entry that is busy under its own name
    /// stays busy when a command that ran inside it ends.
    #[test]
    fn a_busy_guard_restores_what_it_replaced() {
        let b = backend();
        let outer = b.busy(RECOVERY_ENTRY_BUSY);
        {
            let inner = b.busy("apply");
            assert_eq!(b.app_status().unwrap().busy.as_deref(), Some("apply"));
            drop(inner);
        }
        assert_eq!(
            b.app_status().unwrap().busy.as_deref(),
            Some(RECOVERY_ENTRY_BUSY)
        );
        drop(outer);
        assert_eq!(b.app_status().unwrap().busy, None);
    }

    /// Carry 4: a pid that is there but cannot be opened is a run that is gone for the count,
    /// and the log says that it could not be told.
    #[test]
    fn a_process_that_cannot_be_opened_is_counted_as_gone_and_said() {
        let b = backend();
        fake_host(&b).unreadable_processes.borrow_mut().insert(9999);
        put(&b, &old(None, 0));
        let d = b.start_run(GuiStart::Normal);
        assert_eq!(d.safe_mode, None);
        assert_eq!(
            record(&b).unwrap().failed_starts,
            1,
            "the count is as it was"
        );
        assert!(
            b.log_tail(5).contains("run record=process-unreadable"),
            "{}",
            b.log_tail(5)
        );
    }
}
