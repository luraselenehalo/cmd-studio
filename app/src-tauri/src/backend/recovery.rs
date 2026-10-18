//! The theme's pause, from the app (recovery spec §4.1–§4.3, §8): `recovery_status`,
//! `theme_pause` and `theme_resume`, and the check of Windows' crash reports that runs once at
//! every start ([`Backend::crash_check`], which `lib.rs` starts on a thread of its own once the
//! window is up, so the window never waits for it).
//!
//! Two levels of pause (spec §4.1): init.lua pauses the theme in cmd (`by: clink`) and changes
//! nothing on the computer; the app pauses it on this computer (`by: app`) through Revert
//! everything, and notes the theme to apply again. How far a record reaches now is the core's
//! one rule, `Paused::level`: an app record older than the theme applied now is one an apply
//! could not lift, a pause in cmd only. Turning the theme back on lifts a pause in cmd at once
//! (nothing on the computer is held by it, so no transaction), and for a pause of this computer
//! gives the UI the request to apply the paused theme again through the plan; that apply's
//! commit lifts the pause (`Backend::apply`). Undo everything, and an undo or a restore that
//! leaves no theme applied, lift a pause in cmd too, since nothing of CMD Studio's is left for it
//! to pause; none of them lifts a pause of this computer, which holds the theme to turn back on.
use super::apply::{ApplyRequest, PlanOutcome, RevertOutcome};
use super::{Backend, Machine};
use crate::error::AppError;
use cmdstudio_core::cleanup;
use cmdstudio_core::eventlog::clink_crashes;
use cmdstudio_core::model::Target;
use cmdstudio_core::recovery::{
    clear_pause, read_pause, write_pause, PauseLevel, PauseReason, PauseRecord, Paused, PausedBy,
    Resume, PAUSED_VERSION, PAUSE_AFTER,
};
use cmdstudio_core::state::State;
use serde::{Deserialize, Serialize};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Condvar, Mutex, PoisonError};
use std::time::Duration;
use ts_rs::TS;
use uuid::Uuid;

/// How long `recovery_status` waits for the start's crash check (spec §4.3): the event log's
/// five seconds, a pause's Revert everything, and room to spare. Past it, the status says the
/// check has not finished, and the next read asks again.
pub const CRASH_CHECK_WAIT: Duration = Duration::from_secs(30);

/// The theme a pause of this computer applies again, and its name while it is still in the
/// library.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ResumeInfo {
    pub theme_id: Uuid,
    pub target: Target,
    /// `None` when the theme is no longer in the library: turning back on goes to the Themes
    /// page instead (spec §4.1).
    pub theme_name: Option<String>,
}

/// The pause, as the home page's banner says it (spec §4.2).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PauseInfo {
    pub by: PausedBy,
    /// How far the pause reaches now: a pause of this computer is in effect (`computer`) only
    /// while no theme is applied; with one applied no later than the record it has not gone
    /// through (`computerNotYet`); with one applied after the record it is a pause in cmd only
    /// (`cmd`), which an apply could not lift; and with a state.json that can't be read it is
    /// not known (`computerUnknown`).
    pub level: PauseLevel,
    /// `None` when paused.json can't be read as a pause record: cmd still opens without the
    /// theme, and turning it back on deletes the file.
    pub reason: Option<PauseReason>,
    #[ts(type = "number | null")]
    pub at_ms: Option<u64>,
    pub count: u32,
    pub resume: Option<ResumeInfo>,
}

/// What the start's check of Windows' crash reports found (spec §4.3).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum CrashCheck {
    /// No theme is applied: nothing to look for.
    NoTheme,
    /// `count` crashes of cmd in its add-on since the theme was applied.
    Read {
        count: u32,
        /// This check paused the theme on this computer, and the pause went through
        /// ([`PauseOutcome::paused_now`]).
        paused_now: bool,
        /// What the pause this check tried did, to say the way every change is said (spec §4.1,
        /// §5.5): a revert that was refused or rolled back, a record that could not be saved.
        /// It tries one for three crashes or more, and one an earlier pause of this computer
        /// did not get through (its record, no older than the theme applied now), with that
        /// pause's reason. One that does not go through is tried again the next time the app
        /// starts. `None` when it tried none: a record older than the theme applied now is a
        /// pause in cmd only, never a reason to revert.
        pause: Option<Box<PauseOutcome>>,
    },
    /// The reports could not be read (the log is off, this user may not read it, it did not
    /// answer in five seconds, or an earlier read is still going on), or state.json could not
    /// be, so nothing was done for them.
    Unreadable {
        why: String,
        /// A pause of this computer that had not gone through (its record no older than the
        /// theme applied now), tried again all the same: it comes from its record, not from
        /// the reports. `None` when there was none, and always when state.json could not be
        /// read (whether a theme is applied is not known).
        pause: Option<Box<PauseOutcome>>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RecoveryStatus {
    /// `None` when the theme is not paused.
    pub paused: Option<PauseInfo>,
    /// `None` while the start's crash check has not finished, or did not run.
    pub crashes: Option<CrashCheck>,
}

/// What "Pause the theme" did. `paused.json` is written first, and Revert everything runs
/// however that went (spec §4.1): while the file is there, init.lua loads no theme, and the home
/// page says the state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PauseOutcome {
    /// The pause went through: Revert everything ended well, and its record was saved. When
    /// not, the fields below say why; the theme stays applied, cmd opens without it while the
    /// record is there, and the pause is tried again the next time the app starts, unless a
    /// theme is applied after the record (then it is a pause in cmd only).
    pub paused_now: bool,
    /// What Revert everything did; `None` when it was refused before it changed anything.
    pub revert: Option<RevertOutcome>,
    /// Why Revert everything was refused (`E_BUSY`, `E_SNAPSHOT_READ`, `E_STATE_INVALID`).
    pub refused: Option<AppError>,
    /// `paused.json` could not be written (`E_DATA_WRITE`).
    pub not_saved: Option<AppError>,
}

/// What "Turn the theme back on" does (spec §4.1, §8).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum ResumeOutcome {
    /// A pause in cmd was lifted: the next cmd window loads the theme.
    Resumed,
    /// A pause of this computer: apply the paused theme again, shown as this plan first.
    Apply {
        request: ApplyRequest,
        plan: Box<PlanOutcome>,
    },
    /// A pause of this computer whose theme is no longer in the library, or that noted none:
    /// choose one on the Themes page.
    PickTheme,
    /// The theme is not paused.
    NotPaused,
}

/// Where the start's crash check leaves its answer for `recovery_status`. A lock that a
/// panicking thread left is used as it is: the slot is only ever replaced whole.
pub(crate) struct CrashSlot {
    state: Mutex<Slot>,
    done: Condvar,
}

enum Slot {
    NotStarted,
    Running,
    Done(CrashCheck),
}

impl CrashSlot {
    pub(crate) fn new() -> Self {
        CrashSlot {
            state: Mutex::new(Slot::NotStarted),
            done: Condvar::new(),
        }
    }

    fn start(&self) {
        let mut slot = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if matches!(*slot, Slot::NotStarted) {
            *slot = Slot::Running;
        }
    }

    fn finish(&self, check: CrashCheck) {
        *self.state.lock().unwrap_or_else(PoisonError::into_inner) = Slot::Done(check);
        self.done.notify_all();
    }

    /// The answer, waiting up to `timeout` for a check on its way; at once `None` when none was
    /// started.
    fn wait(&self, timeout: Duration) -> Option<CrashCheck> {
        let slot = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let (slot, _) = self
            .done
            .wait_timeout_while(slot, timeout, |s| matches!(s, Slot::Running))
            .unwrap_or_else(PoisonError::into_inner);
        match &*slot {
            Slot::Done(check) => Some(check.clone()),
            Slot::NotStarted | Slot::Running => None,
        }
    }
}

impl<M: Machine> Backend<M> {
    /// Whether the theme is paused, and what the start's crash check found (spec §8). Changes
    /// nothing; waits up to [`CRASH_CHECK_WAIT`] for that check while it runs.
    pub fn recovery_status(&self) -> Result<RecoveryStatus, AppError> {
        let crashes = self.crash.wait(CRASH_CHECK_WAIT);
        Ok(RecoveryStatus {
            paused: self.pause_now()?,
            crashes,
        })
    }

    fn pause_now(&self) -> Result<Option<PauseInfo>, AppError> {
        let paused = read_pause(self.sys().fs, self.paths())
            .map_err(|e| AppError::coded("E_MACHINE_READ", e.to_string()))?;
        Ok(paused.map(|p| self.pause_info(p)))
    }

    fn pause_info(&self, p: Paused) -> PauseInfo {
        // The app's record is written before Revert everything runs: state.json says whether
        // the theme is out.
        let level = p.level(&State::load(self.sys().fs, &self.paths().state_file()));
        let resume = p.resume.map(|r| ResumeInfo {
            theme_id: r.theme_id,
            target: r.target,
            theme_name: self
                .find_theme(r.theme_id)
                .ok()
                .map(|(theme, _, _)| theme.name),
        });
        PauseInfo {
            by: p.by,
            level,
            reason: p.reason,
            at_ms: p.at_ms,
            count: p.count,
            resume,
        }
    }

    /// Marks the start's crash check as on its way, so that `recovery_status` waits for it.
    /// `lib.rs` calls this before it starts [`Backend::crash_check`] on a thread of its own.
    pub fn crash_check_started(&self) {
        self.crash.start();
    }

    /// The start's crash check could not run (no thread could be started): `recovery_status`
    /// must not wait for it.
    pub fn crash_check_not_run(&self, why: &str) {
        self.note_line("crash_check not_run");
        self.crash.finish(CrashCheck::Unreadable {
            why: why.to_string(),
            pause: None,
        });
    }

    /// The start's check of Windows' crash reports (spec §4.3): cmd's crashes in Clink's DLL since
    /// the theme was applied, at most seven days back. [`PAUSE_AFTER`] or more, with a theme
    /// applied, pause it on this computer ([`PauseReason::ClinkCrash`]).
    ///
    /// A record of a pause of this computer counts as one only while no theme is applied,
    /// because a pause that went through takes the theme out ([`PauseLevel`]). One whose revert
    /// was refused or rolled back, its record no older than the theme applied now, is tried
    /// again at this start, with its own reason and count, keeping the theme it noted, also
    /// with fewer crashes or reports that can't be read: it comes from its record, not from
    /// the reports. One older than the theme applied now is one an apply could not lift: a
    /// pause in cmd, never retried, so a theme the user turned back on is never reverted
    /// unasked (`Paused::level` is the one rule, shared with doctor D12). A pause in cmd does
    /// not count, because Clink still injects and its DLL still loads. Reports that can't be
    /// read pause nothing of their own, and a state.json that can't be read nothing at all.
    /// One log line, never the user's data. A panic in it is an `Unreadable` answer, never a
    /// `recovery_status` that waits for nothing.
    pub fn crash_check(&self) -> CrashCheck {
        self.crash.start();
        // UNWIND SAFETY: the check only reads, and a pause it starts is a journaled
        // transaction, which the next command that changes the machine rolls back if it was cut
        // off; nothing of the backend's memory is left half changed.
        let check = match catch_unwind(AssertUnwindSafe(|| self.run_crash_check())) {
            Ok(check) => check,
            Err(payload) => {
                // Dropping the payload of a panic runs its code, which could panic too.
                std::mem::forget(payload);
                CrashCheck::Unreadable {
                    why: "the check stopped on an internal error (a panic)".to_string(),
                    pause: None,
                }
            }
        };
        self.crash.finish(check.clone());
        check
    }

    fn run_crash_check(&self) -> CrashCheck {
        let sys = self.sys();
        let ctx = self.ctx();
        let state = State::load(sys.fs, &ctx.paths.state_file());
        let applied_at_ms = match &state {
            Ok(State {
                applied: Some(applied),
                ..
            }) => applied.applied_at_ms,
            Ok(State { applied: None, .. }) => {
                self.note_line("crash_check theme=none");
                return CrashCheck::NoTheme;
            }
            Err(e) => {
                // It says whether a theme is applied, and since when to count: not knowing is
                // no answer, never "no theme".
                self.note_line("crash_check state=unreadable");
                return CrashCheck::Unreadable {
                    why: e.to_string(),
                    pause: None,
                };
            }
        };
        // A record of the app's at or after the theme applied now is a pause of this computer
        // that has not gone through (`PauseLevel::ComputerNotYet`, the one rule, in
        // `Paused::level`): it is tried again, with its reason and count, and `pause_machine`
        // keeps the theme it noted. One older than the apply is one an apply could not lift: a
        // pause in cmd, which the user turns back on, and never a reason to revert the theme
        // applied since.
        let not_yet = match read_pause(sys.fs, &ctx.paths) {
            Ok(Some(p)) if p.level(&state) == PauseLevel::ComputerNotYet => {
                p.reason.map(|reason| (reason, p.count))
            }
            _ => None,
        };
        let crashes = match clink_crashes(sys.event_log, applied_at_ms, ctx.now_ms) {
            Ok(crashes) => crashes,
            Err(e) => {
                // Never a pause for reports that can't be read (spec §4.3); one that had not
                // gone through comes from its record, and is tried again all the same.
                self.note_line("crash_check unreadable");
                let pause = not_yet.map(|(reason, count)| self.pause_machine(reason, count));
                return CrashCheck::Unreadable {
                    why: e.to_string(),
                    pause: pause.map(Box::new),
                };
            }
        };
        let count = u32::try_from(crashes.len()).unwrap_or(u32::MAX);
        let pause = if count >= PAUSE_AFTER {
            Some(self.pause_machine(PauseReason::ClinkCrash, count))
        } else {
            not_yet.map(|(reason, count)| self.pause_machine(reason, count))
        };
        let paused_now = pause.as_ref().is_some_and(|p| p.paused_now);
        self.note_line(&format!("crash_check crashes={count} paused={paused_now}"));
        CrashCheck::Read {
            count,
            paused_now,
            pause: pause.map(Box::new),
        }
    }

    /// "Pause the theme" (spec §4.2, D12's fix): on this computer, for the user (`user`) or for
    /// cmd's crashes in its add-on (`clinkCrash`, with their count). The other reasons are not
    /// the app's to give: `cmdStarts` is init.lua's, `recovery` the Recovery shortcut's (phase
    /// 3b-2), and asking for them is `E_INTERNAL`, with nothing changed.
    pub fn theme_pause(&self, reason: PauseReason) -> Result<PauseOutcome, AppError> {
        match reason {
            PauseReason::User => Ok(self.pause_machine(PauseReason::User, 0)),
            PauseReason::ClinkCrash => {
                let count = self.crashes_now();
                Ok(self.pause_machine(PauseReason::ClinkCrash, count))
            }
            PauseReason::CmdStarts | PauseReason::Recovery => Err(AppError::internal(format!(
                "the app does not pause the theme for {}",
                reason.as_str()
            ))),
        }
    }

    /// cmd's crashes in its add-on since the theme was applied, as D12 counts them; 0 when they
    /// can't be read or no theme is applied.
    fn crashes_now(&self) -> u32 {
        let sys = self.sys();
        match State::load(sys.fs, &self.paths().state_file()) {
            Ok(State {
                applied: Some(a), ..
            }) => clink_crashes(sys.event_log, a.applied_at_ms, self.now_ms())
                .map_or(0, |c| u32::try_from(c.len()).unwrap_or(u32::MAX)),
            _ => 0,
        }
    }

    /// Pauses the theme on this computer (spec §4.1): first `paused.json`, by the app, with the
    /// theme to apply again (the one applied now, else the one an earlier pause of this computer
    /// noted), then Revert everything (a journaled transaction, under the lock).
    ///
    /// The record comes first because the core's Revert everything never touches it: an app
    /// that stops in between leaves a pause that can be turned back on (the next command rolls
    /// the cut-off revert back), never a reverted machine with nothing that says which theme it
    /// had. Revert everything runs however the write went, so that cmd opens either way. A
    /// revert that is refused or rolls back leaves the record as it is: init.lua loads no theme
    /// while it is there, so the theme is paused in cmd, and while the theme applied is no newer
    /// than the record the next start's crash check tries the pause again
    /// ([`PauseOutcome::paused_now`]). The record's time is `ctx.now_ms`, so a retry makes it
    /// newer than the theme again. Phase 3b-2's `--recovery` pauses through here too
    /// (`recovery`).
    pub(crate) fn pause_machine(&self, reason: PauseReason, count: u32) -> PauseOutcome {
        let sys = self.sys();
        let ctx = self.ctx();
        let applied = match State::load(sys.fs, &ctx.paths.state_file()) {
            Ok(State {
                applied: Some(a), ..
            }) => Some(Resume {
                theme_id: a.theme_id,
                target: a.target,
            }),
            _ => None,
        };
        let earlier = match read_pause(sys.fs, &ctx.paths) {
            Ok(Some(Paused {
                by: PausedBy::App,
                resume,
                ..
            })) => resume,
            _ => None,
        };
        let record = PauseRecord {
            version: PAUSED_VERSION,
            reason,
            at: ctx.now_ms,
            count,
            by: PausedBy::App,
            resume: applied.or(earlier),
        };
        let not_saved = write_pause(sys.fs, &ctx.paths, &record)
            .err()
            .map(|e| AppError::coded("E_DATA_WRITE", e.to_string()));
        let reverted = cleanup::revert_all(sys, &ctx)
            .map(|report| self.revert_outcome(&ctx, &report))
            .map_err(AppError::from);
        let how = match &reverted {
            Ok(r) if r.ok => "ok".to_string(),
            Ok(_) => "incomplete".to_string(),
            Err(e) => e.code.clone(),
        };
        self.note_line(&format!(
            "pause reason={} count={count} revert={how} saved={}",
            reason.as_str(),
            not_saved.is_none()
        ));
        let (revert, refused) = match reverted {
            Ok(r) => (Some(r), None),
            Err(e) => (None, Some(e)),
        };
        PauseOutcome {
            paused_now: revert.as_ref().is_some_and(|r| r.ok) && not_saved.is_none(),
            revert,
            refused,
            not_saved,
        }
    }

    /// "Turn the theme back on" (spec §4.1, §8): a pause in cmd ([`PauseLevel::Cmd`]: init.lua's,
    /// a record that can't be read, or an app record older than the theme applied now) is
    /// lifted at once; for a pause of this computer the request (the paused theme, its target,
    /// the settings' choice for Windows Terminal's default profile) and its plan, which the UI
    /// shows before `apply`.
    pub fn theme_resume(&self) -> Result<ResumeOutcome, AppError> {
        let fs = self.sys().fs;
        let paused = read_pause(fs, self.paths())
            .map_err(|e| AppError::coded("E_MACHINE_READ", e.to_string()))?;
        let Some(p) = paused else {
            return Ok(ResumeOutcome::NotPaused);
        };
        let level = p.level(&State::load(fs, &self.paths().state_file()));
        match (level, p.resume) {
            (PauseLevel::Cmd, _) => {
                clear_pause(fs, self.paths())
                    .map_err(|e| AppError::coded("E_DATA_WRITE", e.to_string()))?;
                let by = match p.by {
                    PausedBy::Clink => "clink",
                    PausedBy::App => "app",
                };
                self.note_line(&format!("resume by={by} level=cmd"));
                Ok(ResumeOutcome::Resumed)
            }
            (_, Some(r)) if self.find_theme(r.theme_id).is_ok() => {
                let request = ApplyRequest {
                    theme_id: r.theme_id,
                    target: r.target,
                    set_wt_default_profile: self.settings_or_default().set_wt_default_profile,
                    replace_other_clink: false,
                };
                let plan = self.plan_apply(request.clone())?;
                Ok(ResumeOutcome::Apply {
                    request,
                    plan: Box::new(plan),
                })
            }
            (_, _) => Ok(ResumeOutcome::PickTheme),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::apply::{ApplyOutcome, ApplyStatus, RestoreStatus, TxnStatus, UndoStatus};
    use crate::backend::doctor::FixKind;
    use crate::backend::testing::{
        apply_terracotta, backend, backend_on, fake_events, fake_fs, plan_and_apply, terracotta_id,
        WT,
    };
    use cmdstudio_core::adapters::fake::FakeMachine;
    use cmdstudio_core::adapters::{AdapterError, AutoRun, Fs, RegValue, System};
    use cmdstudio_core::doctor::Status;
    use cmdstudio_core::eventlog::STILL_BEING_READ;
    use std::cell::Cell;
    use std::path::Path;
    use std::sync::Arc;

    const CLINK_DLL: &str = "clink_dll_x64.dll";
    /// What init.lua writes when it pauses the theme in cmd.
    const IN_CMD: &[u8] =
        br#"{"version":1,"reason":"cmdStarts","at":1791694709000,"count":3,"by":"clink"}"#;

    fn applied_at<M: Machine>(b: &Backend<M>) -> u64 {
        State::load(b.sys().fs, &b.paths().state_file())
            .unwrap()
            .applied
            .unwrap()
            .applied_at_ms
    }

    /// Terracotta applied, and cmd crashed in its add-on three times since.
    fn crashed_three_times(b: &Backend<FakeMachine>) {
        assert_eq!(apply_terracotta(b).0.status, ApplyStatus::Committed);
        let at = applied_at(b);
        for _ in 0..3 {
            fake_events(b).crash(at, "cmd.exe", CLINK_DLL);
        }
    }

    /// The count and `paused_now` of a check that read the reports.
    fn counted(check: &CrashCheck) -> (u32, bool) {
        match check {
            CrashCheck::Read {
                count, paused_now, ..
            } => (*count, *paused_now),
            other => panic!("the reports were not read: {other:?}"),
        }
    }

    /// What the pause a check tried did.
    fn tried(check: &CrashCheck) -> PauseOutcome {
        match check {
            CrashCheck::Read {
                pause: Some(pause), ..
            } => (**pause).clone(),
            other => panic!("no pause was tried: {other:?}"),
        }
    }

    /// Pauses the theme for the user while another run holds the lock: Revert everything is
    /// refused, and the record is there with the theme still applied.
    fn pause_refused(b: &Backend<FakeMachine>) {
        let held = fake_fs(b)
            .try_lock(&b.paths().lock_file())
            .unwrap()
            .unwrap();
        let out = b.theme_pause(PauseReason::User).unwrap();
        drop(held);
        assert_eq!(out.refused.map(|e| e.code).as_deref(), Some("E_BUSY"));
    }

    fn health_is_empty<M: Machine>(b: &Backend<M>) -> bool {
        b.sys().fs.list_dir(&b.paths().health()).unwrap().is_empty()
    }

    /// A machine whose AutoRun stops the app (a panic) when the user's value is written or
    /// deleted while `cut` is set: the app ended in the middle of a change of its own.
    struct CutOff {
        m: FakeMachine,
        cut: Cell<bool>,
    }

    impl CutOff {
        fn stop_here(&self) {
            if self.cut.get() {
                panic!("the app stopped here");
            }
        }
    }

    impl AutoRun for CutOff {
        fn user_key_exists(&self) -> Result<bool, AdapterError> {
            self.m.autorun.user_key_exists()
        }
        fn read_user(&self) -> Result<Option<RegValue>, AdapterError> {
            self.m.autorun.read_user()
        }
        fn write_user(&self, value: &RegValue) -> Result<(), AdapterError> {
            self.stop_here();
            self.m.autorun.write_user(value)
        }
        fn delete_user(&self) -> Result<(), AdapterError> {
            self.stop_here();
            self.m.autorun.delete_user()
        }
        fn delete_user_key_if_empty(&self) -> Result<(), AdapterError> {
            self.stop_here();
            self.m.autorun.delete_user_key_if_empty()
        }
        fn read_machine(&self) -> Result<Option<RegValue>, AdapterError> {
            self.m.autorun.read_machine()
        }
    }

    impl Machine for CutOff {
        fn system(&self) -> System<'_> {
            System {
                autorun: self,
                ..self.m.system()
            }
        }
    }

    /// Terracotta applied, then the theme paused in cmd by init.lua, with two tags left.
    fn paused_in_cmd(b: &Backend<FakeMachine>) {
        assert_eq!(apply_terracotta(b).0.status, ApplyStatus::Committed);
        fake_fs(b).put(b.paths().paused_file(), IN_CMD);
        for pid in [11, 12] {
            fake_fs(b).put(
                b.paths().health().join(format!("start-{pid}")),
                b"v1 1791694709\n",
            );
        }
    }

    #[test]
    fn no_pause_and_no_check_yet_is_said_as_such_at_once() {
        let b = backend();
        let started = std::time::Instant::now();
        assert_eq!(
            b.recovery_status().unwrap(),
            RecoveryStatus {
                paused: None,
                crashes: None
            }
        );
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "no wait for a check never started"
        );
        assert_eq!(b.theme_resume().unwrap(), ResumeOutcome::NotPaused);
    }

    #[test]
    fn a_pause_in_cmd_is_read_and_lifted_at_once() {
        let b = backend();
        paused_in_cmd(&b);
        let paused = b.recovery_status().unwrap().paused.unwrap();
        assert_eq!(
            paused,
            PauseInfo {
                by: PausedBy::Clink,
                level: PauseLevel::Cmd,
                reason: Some(PauseReason::CmdStarts),
                at_ms: Some(1_791_694_709_000),
                count: 3,
                resume: None,
            }
        );
        let autorun = b.sys().autorun.read_user().unwrap();
        assert_eq!(b.theme_resume().unwrap(), ResumeOutcome::Resumed);
        assert!(!fake_fs(&b).exists(&b.paths().paused_file()));
        assert!(b.sys().fs.list_dir(&b.paths().health()).unwrap().is_empty());
        // Nothing on the computer changed: no transaction, AutoRun as it was.
        assert_eq!(b.sys().autorun.read_user().unwrap(), autorun);
        assert_eq!(b.recovery_status().unwrap().paused, None);
    }

    #[test]
    fn pausing_takes_the_theme_out_and_turning_it_back_on_applies_it_through_the_plan() {
        let b = backend();
        assert_eq!(apply_terracotta(&b).0.status, ApplyStatus::Committed);
        let out = b.theme_pause(PauseReason::User).unwrap();
        assert!(out.paused_now, "{out:?}");
        let revert = out.revert.unwrap();
        assert!(revert.ok, "{revert:?}");
        assert_eq!(revert.status, Some(TxnStatus::Committed));
        assert_eq!((out.refused, out.not_saved), (None, None));
        assert_eq!(
            b.sys().autorun.read_user().unwrap(),
            None,
            "CMD Studio's part is out"
        );
        let paused = b.recovery_status().unwrap().paused.unwrap();
        assert_eq!(
            (paused.by, paused.reason),
            (PausedBy::App, Some(PauseReason::User))
        );
        assert_eq!(
            paused.resume,
            Some(ResumeInfo {
                theme_id: terracotta_id(),
                target: Target::All,
                theme_name: Some("Terracotta".to_string()),
            })
        );
        let ResumeOutcome::Apply { request, plan } = b.theme_resume().unwrap() else {
            panic!("a pause of this computer applies the theme again");
        };
        assert_eq!(
            (
                request.theme_id,
                request.target,
                request.replace_other_clink
            ),
            (terracotta_id(), Target::All, false)
        );
        assert!(matches!(*plan, PlanOutcome::Ready { .. }));
        // The pause stays until that apply commits.
        assert!(fake_fs(&b).exists(&b.paths().paused_file()));
        assert_eq!(plan_and_apply(&b, request).0.status, ApplyStatus::Committed);
        assert_eq!(b.recovery_status().unwrap().paused, None);
        let log = b.log_tail(20);
        assert!(
            log.contains(" pause reason=user count=0 revert=ok saved=true"),
            "{log}"
        );
    }

    #[test]
    fn a_paused_theme_no_longer_in_the_library_sends_the_user_to_the_themes_page() {
        let b = backend();
        let gone = PauseRecord {
            version: PAUSED_VERSION,
            reason: PauseReason::ClinkCrash,
            at: 1,
            count: 3,
            by: PausedBy::App,
            resume: Some(Resume {
                theme_id: Uuid::nil(),
                target: Target::Wt,
            }),
        };
        write_pause(b.sys().fs, b.paths(), &gone).unwrap();
        let resume = b.recovery_status().unwrap().paused.unwrap().resume.unwrap();
        assert_eq!(resume.theme_name, None);
        assert_eq!(b.theme_resume().unwrap(), ResumeOutcome::PickTheme);
        // And one that noted no theme at all.
        let none = PauseRecord {
            resume: None,
            ..gone
        };
        write_pause(b.sys().fs, b.paths(), &none).unwrap();
        assert_eq!(b.theme_resume().unwrap(), ResumeOutcome::PickTheme);
    }

    #[test]
    fn a_pause_record_that_cannot_be_read_is_a_pause_in_cmd_and_turning_back_on_lifts_it() {
        // Review Focus 3: init.lua loads no theme while the file is there, whatever it holds.
        let b = backend();
        fake_fs(&b).put(b.paths().paused_file(), b"{ not json");
        let paused = b.recovery_status().unwrap().paused.unwrap();
        assert_eq!(
            (paused.by, paused.level, paused.reason),
            (PausedBy::Clink, PauseLevel::Cmd, None)
        );
        assert_eq!(b.theme_resume().unwrap(), ResumeOutcome::Resumed);
        assert!(!fake_fs(&b).exists(&b.paths().paused_file()));
        // A file that can't be read at all is an error, never "not paused".
        fake_fs(&b).put(b.paths().paused_file(), IN_CMD);
        fake_fs(&b).fail_reads_of(b.paths().paused_file());
        assert_eq!(b.recovery_status().unwrap_err().code, "E_MACHINE_READ");
        assert_eq!(b.theme_resume().unwrap_err().code, "E_MACHINE_READ");
    }

    #[test]
    fn the_app_pauses_only_for_the_user_and_for_cmds_crashes() {
        let b = backend();
        assert_eq!(apply_terracotta(&b).0.status, ApplyStatus::Committed);
        let before = fake_fs(&b).snapshot();
        for reason in [PauseReason::CmdStarts, PauseReason::Recovery] {
            assert_eq!(b.theme_pause(reason).unwrap_err().code, "E_INTERNAL");
        }
        assert_eq!(fake_fs(&b).snapshot(), before);
    }

    #[test]
    fn a_pause_whose_revert_is_refused_is_written_all_the_same() {
        let b = backend();
        assert_eq!(apply_terracotta(&b).0.status, ApplyStatus::Committed);
        let held = fake_fs(&b)
            .try_lock(&b.paths().lock_file())
            .unwrap()
            .unwrap();
        let out = b.theme_pause(PauseReason::User).unwrap();
        drop(held);
        // Said at once, in the press's own result: the pause did not go through.
        assert!(!out.paused_now, "{out:?}");
        assert_eq!(out.revert, None);
        assert_eq!(out.refused.unwrap().code, "E_BUSY");
        assert_eq!(out.not_saved, None);
        // cmd loads no theme while it is there; the theme is still applied, to apply again,
        // and the status says the pause of this computer is not in effect yet.
        let paused = b.recovery_status().unwrap().paused.unwrap();
        assert_eq!(
            (paused.by, paused.level),
            (PausedBy::App, PauseLevel::ComputerNotYet)
        );
        assert_eq!(paused.resume.unwrap().theme_id, terracotta_id());
    }

    #[test]
    fn a_pause_press_whose_revert_rolls_back_says_so_at_once() {
        let b = backend();
        assert_eq!(apply_terracotta(&b).0.status, ApplyStatus::Committed);
        // Windows Terminal's settings.json can't be written: the revert rolls back, and its
        // rollback can't finish either.
        fake_fs(&b).fail_writes_to(WT);
        let out = b.theme_pause(PauseReason::User).unwrap();
        assert!(!out.paused_now, "{out:?}");
        let revert = out.revert.unwrap();
        assert!(!revert.ok);
        assert_eq!(revert.status, Some(TxnStatus::RollbackIncomplete));
        assert_eq!(
            b.recovery_status().unwrap().paused.unwrap().level,
            PauseLevel::ComputerNotYet
        );
    }

    #[test]
    fn the_status_says_a_pause_of_this_computer_is_in_effect_only_when_the_theme_is_out() {
        let b = backend();
        assert_eq!(apply_terracotta(&b).0.status, ApplyStatus::Committed);
        pause_refused(&b);
        let level = |b: &Backend<FakeMachine>| b.recovery_status().unwrap().paused.unwrap().level;
        assert_eq!(level(&b), PauseLevel::ComputerNotYet);
        // state.json can't be read: whether the changes are out is not known, never "out".
        let state = b.paths().state_file();
        fake_fs(&b).fail_reads_of(&state);
        assert_eq!(level(&b), PauseLevel::ComputerUnknown);
        fake_fs(&b).stop_failing(&state);
        // Pressed again with nothing in the way: it goes through, and the theme is out.
        assert!(b.theme_pause(PauseReason::User).unwrap().paused_now);
        assert_eq!(level(&b), PauseLevel::Computer);
        assert_eq!(b.sys().autorun.read_user().unwrap(), None);
    }

    #[test]
    fn a_pause_that_cannot_be_written_is_said() {
        let b = backend();
        assert_eq!(apply_terracotta(&b).0.status, ApplyStatus::Committed);
        fake_fs(&b).fail_writes_to(b.paths().paused_file());
        let out = b.theme_pause(PauseReason::User).unwrap();
        assert!(!out.paused_now, "no record: no pause");
        assert!(out.revert.unwrap().ok);
        assert_eq!(out.not_saved.unwrap().code, "E_DATA_WRITE");
    }

    #[test]
    fn a_pause_that_did_not_go_through_is_tried_again_at_the_next_start_whatever_its_reason() {
        // The user's press was refused, and cmd never crashed: the start still finishes it.
        let b = backend();
        assert_eq!(apply_terracotta(&b).0.status, ApplyStatus::Committed);
        pause_refused(&b);
        let check = b.crash_check();
        assert_eq!(counted(&check), (0, true), "{check:?}");
        assert!(tried(&check).paused_now);
        assert_eq!(
            b.sys().autorun.read_user().unwrap(),
            None,
            "the theme is out"
        );
        let paused = b.recovery_status().unwrap().paused.unwrap();
        assert_eq!(
            (paused.level, paused.reason, paused.count),
            (PauseLevel::Computer, Some(PauseReason::User), 0),
            "its own reason, kept"
        );
        assert_eq!(
            paused.resume.map(|r| r.theme_id),
            Some(terracotta_id()),
            "and its theme"
        );
        // Crash reports that can't be read stop nothing of it: the record is no crash report.
        let b = backend();
        assert_eq!(apply_terracotta(&b).0.status, ApplyStatus::Committed);
        pause_refused(&b);
        fake_events(&b).fail_reads("the event log service is off");
        let check = b.crash_check();
        let CrashCheck::Unreadable {
            why,
            pause: Some(pause),
        } = &check
        else {
            panic!("{check:?}");
        };
        assert!(
            why.contains("service is off") && pause.paused_now,
            "{check:?}"
        );
        assert_eq!(
            b.recovery_status().unwrap().paused.unwrap().level,
            PauseLevel::Computer
        );
        // With state.json unreadable nothing is tried: whether a theme is applied is not known.
        let b = backend();
        assert_eq!(apply_terracotta(&b).0.status, ApplyStatus::Committed);
        pause_refused(&b);
        fake_fs(&b).fail_reads_of(b.paths().state_file());
        assert!(matches!(
            b.crash_check(),
            CrashCheck::Unreadable { pause: None, .. }
        ));
        assert!(b.sys().autorun.read_user().unwrap().is_some());
    }

    #[test]
    fn a_pause_record_older_than_the_theme_applied_now_is_never_retried_and_is_lifted_at_once() {
        // The user's pause was refused, then the user turned the theme back on (an apply),
        // and that apply's lift failed at paused.json: the apply stands, with its note, and
        // the record is older than the theme in use. The next start must not run Revert
        // everything on a theme the user asked for.
        let b = backend();
        assert_eq!(apply_terracotta(&b).0.status, ApplyStatus::Committed);
        pause_refused(&b);
        fake_fs(&b).fail_removes_of(b.paths().paused_file());
        let (done, _) = apply_terracotta(&b);
        assert_eq!(done.status, ApplyStatus::Committed);
        assert!(
            done.notes.iter().any(|n| n.code == "E_DATA_WRITE"),
            "{:?}",
            done.notes
        );
        fake_fs(&b).stop_failing(b.paths().paused_file());
        let record = read_pause(b.sys().fs, b.paths()).unwrap().unwrap();
        assert!(
            record.at_ms.unwrap() < applied_at(&b),
            "the record is older than the apply"
        );
        let on = b.sys().autorun.read_user().unwrap();
        assert!(on.is_some(), "the theme is on");
        // The next start: nothing is reverted, and nothing was tried.
        assert_eq!(
            b.crash_check(),
            CrashCheck::Read {
                count: 0,
                paused_now: false,
                pause: None,
            }
        );
        assert_eq!(
            b.sys().autorun.read_user().unwrap(),
            on,
            "the theme the user turned back on stays"
        );
        assert!(
            fake_fs(&b).exists(&b.paths().paused_file()),
            "the record is left for the lift"
        );
        // The status says a pause in cmd, and so does D12, with the cmd-level words.
        let paused = b.recovery_status().unwrap().paused.unwrap();
        assert_eq!(
            (paused.by, paused.level, paused.reason),
            (PausedBy::App, PauseLevel::Cmd, Some(PauseReason::User))
        );
        let d12 = b
            .doctor_run()
            .unwrap()
            .checks
            .into_iter()
            .find(|c| c.id == "D12")
            .unwrap();
        assert_eq!(
            (d12.status, d12.code.as_deref(), d12.fix),
            (Status::Warn, Some("E_THEME_PAUSED"), Some(FixKind::Resume))
        );
        assert!(
            d12.message_en
                .starts_with("The theme is paused in cmd only:"),
            "{d12:?}"
        );
        assert!(d12.details.starts_with("paused: cmd only"), "{d12:?}");
        // "Turn the theme back on" lifts it at once: the theme is on already.
        assert_eq!(b.theme_resume().unwrap(), ResumeOutcome::Resumed);
        assert!(!fake_fs(&b).exists(&b.paths().paused_file()));
        assert!(health_is_empty(&b));
        assert_eq!(b.sys().autorun.read_user().unwrap(), on);
        assert_eq!(b.recovery_status().unwrap().paused, None);
    }

    #[test]
    fn a_pause_record_at_the_applied_themes_own_time_is_still_retried() {
        // The boundary of the one rule (`Paused::level`): a record at the apply's own
        // `applied_at_ms` is "not yet" and is retried; one millisecond before is a pause in
        // cmd, and nothing is tried.
        let at_the_apply = |b: &Backend<FakeMachine>, before_ms: u64| {
            let record = PauseRecord {
                version: PAUSED_VERSION,
                reason: PauseReason::User,
                at: applied_at(b) - before_ms,
                count: 0,
                by: PausedBy::App,
                resume: Some(Resume {
                    theme_id: terracotta_id(),
                    target: Target::All,
                }),
            };
            write_pause(b.sys().fs, b.paths(), &record).unwrap();
        };
        let b = backend();
        assert_eq!(apply_terracotta(&b).0.status, ApplyStatus::Committed);
        at_the_apply(&b, 0);
        assert_eq!(
            b.recovery_status().unwrap().paused.unwrap().level,
            PauseLevel::ComputerNotYet
        );
        let check = b.crash_check();
        assert_eq!(counted(&check), (0, true), "{check:?}");
        assert_eq!(
            b.sys().autorun.read_user().unwrap(),
            None,
            "the theme is out"
        );
        let b = backend();
        assert_eq!(apply_terracotta(&b).0.status, ApplyStatus::Committed);
        at_the_apply(&b, 1);
        assert_eq!(
            b.recovery_status().unwrap().paused.unwrap().level,
            PauseLevel::Cmd
        );
        assert_eq!(
            b.crash_check(),
            CrashCheck::Read {
                count: 0,
                paused_now: false,
                pause: None,
            }
        );
        assert!(
            b.sys().autorun.read_user().unwrap().is_some(),
            "the theme is still on"
        );
    }

    #[test]
    fn three_crashes_since_the_apply_pause_the_theme_at_start() {
        let b = backend();
        assert_eq!(apply_terracotta(&b).0.status, ApplyStatus::Committed);
        let at = applied_at(&b);
        for _ in 0..3 {
            fake_events(&b).crash(at, "cmd.exe", CLINK_DLL);
        }
        let check = b.crash_check();
        assert_eq!(counted(&check), (3, true));
        let pause = tried(&check);
        assert!(pause.paused_now, "{pause:?}");
        let status = b.recovery_status().unwrap();
        let paused = status.paused.unwrap();
        assert_eq!(
            (paused.by, paused.reason, paused.count),
            (PausedBy::App, Some(PauseReason::ClinkCrash), 3)
        );
        assert_eq!(status.crashes, Some(check));
        assert_eq!(b.sys().autorun.read_user().unwrap(), None);
        let log = b.log_tail(20);
        assert!(log.contains(" crash_check crashes=3 paused=true"), "{log}");
        // The next start finds the theme out: nothing to count from, nothing paused again.
        assert_eq!(b.crash_check(), CrashCheck::NoTheme);
    }

    #[test]
    fn two_crashes_or_reports_that_cannot_be_read_pause_nothing() {
        // Review Focus 4.
        let b = backend();
        assert_eq!(apply_terracotta(&b).0.status, ApplyStatus::Committed);
        let at = applied_at(&b);
        for _ in 0..2 {
            fake_events(&b).crash(at, "cmd.exe", CLINK_DLL);
        }
        assert_eq!(
            b.crash_check(),
            CrashCheck::Read {
                count: 2,
                paused_now: false,
                pause: None,
            }
        );
        fake_events(&b).fail_reads("the event log service is off");
        let check = b.crash_check();
        assert!(
            matches!(&check, CrashCheck::Unreadable { why, .. } if why.contains("service is off")),
            "{check:?}"
        );
        // A third crash, while an earlier read of the log is still going on (the reader's
        // single-flight guard): reports that could not be read, never "no crash" and never a
        // pause.
        fake_events(&b).crash(at, "cmd.exe", CLINK_DLL);
        fake_events(&b).fail_reads(STILL_BEING_READ);
        let check = b.crash_check();
        assert!(
            matches!(&check, CrashCheck::Unreadable { why, .. } if why.contains(STILL_BEING_READ)),
            "{check:?}"
        );
        assert_eq!(b.recovery_status().unwrap().paused, None);
        assert!(
            b.sys().autorun.read_user().unwrap().is_some(),
            "the theme is still on"
        );
        assert!(b.log_tail(20).contains(" crash_check unreadable"));
    }

    #[test]
    fn no_theme_is_nothing_to_check() {
        let b = backend();
        assert_eq!(b.crash_check(), CrashCheck::NoTheme);
        assert!(fake_events(&b).asked().is_empty());
    }

    #[test]
    fn a_pause_in_cmd_is_written_over_by_a_pause_of_this_computer() {
        // Spec §4.3: Clink still injects while the theme is paused in cmd, and its DLL crashes
        // before any script runs.
        let b = backend();
        paused_in_cmd(&b);
        let at = applied_at(&b);
        for _ in 0..3 {
            fake_events(&b).crash(at, "cmd.exe", CLINK_DLL);
        }
        assert_eq!(counted(&b.crash_check()), (3, true));
        assert_eq!(
            b.recovery_status().unwrap().paused.unwrap().by,
            PausedBy::App
        );
    }

    #[test]
    fn an_automatic_pause_that_did_not_go_through_says_why_and_is_tried_again_at_the_next_start() {
        let b = backend();
        crashed_three_times(&b);
        // Another run holds the lock: Revert everything is refused.
        let held = fake_fs(&b)
            .try_lock(&b.paths().lock_file())
            .unwrap()
            .unwrap();
        let check = b.crash_check();
        drop(held);
        assert_eq!(counted(&check), (3, false), "{check:?}");
        let pause = tried(&check);
        assert_eq!(pause.refused.map(|e| e.code).as_deref(), Some("E_BUSY"));
        assert_eq!((pause.revert, pause.not_saved), (None, None));
        assert!(
            b.sys().autorun.read_user().unwrap().is_some(),
            "the theme is still on"
        );
        let status = b.recovery_status().unwrap();
        assert_eq!(status.crashes, Some(check));
        // Its record is there, with the theme to turn back on (cmd loads no theme meanwhile).
        // While a theme is applied it is no pause of this computer, so the next start tries
        // again, and this time it goes through.
        let paused = status.paused.unwrap();
        assert_eq!(
            (paused.by, paused.resume.map(|r| r.theme_id)),
            (PausedBy::App, Some(terracotta_id()))
        );
        let again = b.crash_check();
        assert_eq!(counted(&again), (3, true), "{again:?}");
        assert!(tried(&again).paused_now);
        assert_eq!(
            b.sys().autorun.read_user().unwrap(),
            None,
            "the theme is out"
        );
        let paused = b.recovery_status().unwrap().paused.unwrap();
        assert_eq!(
            paused.resume.map(|r| r.theme_id),
            Some(terracotta_id()),
            "the theme it noted is kept"
        );
        let log = b.log_tail(20);
        assert!(
            log.contains(" pause reason=clinkCrash count=3 revert=E_BUSY saved=true"),
            "{log}"
        );
        assert!(log.contains(" crash_check crashes=3 paused=false"), "{log}");
    }

    #[test]
    fn a_pause_whose_revert_cannot_finish_its_rollback_says_so_and_keeps_its_record() {
        let b = backend();
        crashed_three_times(&b);
        // Windows Terminal's settings.json can't be written: the revert fails there, and so
        // does putting it back (as in the core's a_revert_whose_rollback_cannot_finish_...).
        fake_fs(&b).fail_writes_to(WT);
        let check = b.crash_check();
        assert_eq!(counted(&check), (3, false), "{check:?}");
        let pause = tried(&check);
        assert_eq!((pause.refused, pause.not_saved), (None, None));
        let revert = pause.revert.unwrap();
        assert!(!revert.ok);
        assert_eq!(revert.status, Some(TxnStatus::RollbackIncomplete));
        assert!(!revert.unrestored.is_empty(), "{revert:?}");
        let paused = b.recovery_status().unwrap().paused.unwrap();
        assert_eq!(
            (paused.by, paused.resume.map(|r| r.theme_id)),
            (PausedBy::App, Some(terracotta_id()))
        );
        let log = b.log_tail(20);
        assert!(log.contains(" revert=incomplete saved=true"), "{log}");
        // The theme is still applied: the next start tries again, and once the file can be
        // written the pause goes through.
        fake_fs(&b).stop_failing(WT);
        let again = b.crash_check();
        assert_eq!(counted(&again), (3, true), "{again:?}");
        assert_eq!(
            b.sys().autorun.read_user().unwrap(),
            None,
            "the theme is out"
        );
    }

    #[test]
    fn the_pause_record_comes_first_so_an_app_stopped_before_the_revert_leaves_one_to_turn_back_on()
    {
        let b = backend_on(|m| CutOff {
            m,
            cut: Cell::new(false),
        });
        assert_eq!(apply_terracotta(&b).0.status, ApplyStatus::Committed);
        let at = applied_at(&b);
        for _ in 0..3 {
            b.machine.m.event_log.crash(at, "cmd.exe", CLINK_DLL);
        }
        // The app stops as Revert everything takes CMD Studio's AutoRun entry out.
        b.machine.cut.set(true);
        let check = b.crash_check();
        b.machine.cut.set(false);
        assert!(
            matches!(&check, CrashCheck::Unreadable { why, .. } if why.contains("panic")),
            "{check:?}"
        );
        // The record was written before: a pause that can be turned back on.
        let paused = read_pause(b.sys().fs, b.paths()).unwrap().unwrap();
        assert_eq!(
            (paused.by, paused.resume.map(|r| r.theme_id)),
            (PausedBy::App, Some(terracotta_id()))
        );
        // The next start rolls the cut-off revert back, finds the theme applied, and pauses.
        let again = b.crash_check();
        assert_eq!(counted(&again), (3, true), "{again:?}");
        assert_eq!(b.sys().autorun.read_user().unwrap(), None);
    }

    #[test]
    fn a_state_file_that_cannot_be_read_is_no_answer_never_no_theme() {
        let b = backend();
        crashed_three_times(&b);
        let state = b.paths().state_file();
        fake_fs(&b).fail_reads_of(&state);
        let check = b.crash_check();
        assert!(
            matches!(&check, CrashCheck::Unreadable { why, .. } if why.contains("state.json")),
            "{check:?}"
        );
        fake_fs(&b).stop_failing(&state);
        fake_fs(&b).put(&state, b"{ damaged");
        let check = b.crash_check();
        assert!(
            matches!(&check, CrashCheck::Unreadable { why, .. } if why.contains("state.json")),
            "{check:?}"
        );
        assert!(fake_events(&b).asked().is_empty(), "nothing to count from");
        assert!(b.sys().autorun.read_user().unwrap().is_some());
        assert!(b.log_tail(20).contains(" crash_check state=unreadable"));
    }

    #[test]
    fn an_undo_that_leaves_no_theme_lifts_a_pause_in_cmd_but_never_one_of_this_computer() {
        let b = backend();
        // Two applies, then a pause in cmd.
        assert_eq!(apply_terracotta(&b).0.status, ApplyStatus::Committed);
        paused_in_cmd(&b);
        // Undoing the newer apply leaves the older theme on, and its pause with it.
        assert_eq!(b.undo_last().unwrap().status, UndoStatus::Committed);
        assert_eq!(
            b.recovery_status().unwrap().paused.unwrap().by,
            PausedBy::Clink
        );
        // Undoing the last one leaves none: nothing is left for the pause to pause.
        let undone = b.undo_last().unwrap();
        assert_eq!(undone.status, UndoStatus::Committed);
        assert!(undone.notes.is_empty(), "{:?}", undone.notes);
        assert_eq!(b.recovery_status().unwrap().paused, None);
        assert!(health_is_empty(&b));
        // A pause of this computer keeps its record: it holds the theme to turn back on.
        assert_eq!(apply_terracotta(&b).0.status, ApplyStatus::Committed);
        pause_refused(&b);
        assert_eq!(b.undo_last().unwrap().status, UndoStatus::Committed);
        assert_eq!(
            b.recovery_status().unwrap().paused.unwrap().by,
            PausedBy::App
        );
    }

    #[test]
    fn a_restore_that_leaves_no_theme_lifts_a_pause_in_cmd_but_never_one_of_this_computer() {
        let b = backend();
        let first = apply_terracotta(&b).0.snapshot_id.unwrap();
        paused_in_cmd(&b);
        let second = b.snapshots_list().unwrap().points[0].id.clone();
        // Back to before the second apply: the first theme is still on, and its pause too.
        assert_eq!(
            b.snapshot_restore(&second).unwrap().status,
            RestoreStatus::Done
        );
        assert_eq!(
            b.recovery_status().unwrap().paused.unwrap().by,
            PausedBy::Clink
        );
        // Back to before the first: no theme is left, and neither is the pause.
        let restored = b.snapshot_restore(&first).unwrap();
        assert_eq!(restored.status, RestoreStatus::Done);
        assert!(restored.notes.is_empty(), "{:?}", restored.notes);
        assert_eq!(b.recovery_status().unwrap().paused, None);
        assert!(health_is_empty(&b));
        // A pause of this computer keeps its record.
        let third = apply_terracotta(&b).0.snapshot_id.unwrap();
        pause_refused(&b);
        assert_eq!(
            b.snapshot_restore(&third).unwrap().status,
            RestoreStatus::Done
        );
        assert_eq!(
            b.recovery_status().unwrap().paused.unwrap().by,
            PausedBy::App
        );
    }

    #[test]
    fn undo_everything_keeps_a_pause_in_cmd_unless_it_went_fully_well_and_says_one_it_could_not_lift(
    ) {
        let b = backend();
        paused_in_cmd(&b);
        // Windows Terminal's settings.json can't be read: that step fails, so CMD Studio's
        // changes are not all out, and the pause in cmd stays.
        fake_fs(&b).fail_reads_of(Path::new(WT));
        let out = b.revert_all().unwrap();
        assert!(!out.ok, "{out:?}");
        assert_eq!(
            b.recovery_status().unwrap().paused.unwrap().by,
            PausedBy::Clink
        );
        fake_fs(&b).stop_failing(Path::new(WT));
        // Fully well this time, but paused.json is held: said, and the pause is as it was.
        fake_fs(&b).fail_removes_of(b.paths().paused_file());
        let out = b.revert_all().unwrap();
        assert!(out.ok, "{out:?}");
        assert!(
            out.notes.iter().any(|n| n.code == "E_DATA_WRITE"
                && n.details.starts_with(
                    "everything was undone, but the pause in cmd could not be lifted"
                )),
            "{:?}",
            out.notes
        );
        assert!(fake_fs(&b).exists(&b.paths().paused_file()));
    }

    #[test]
    fn an_apply_on_a_fresh_machine_makes_the_folder_init_lua_writes_its_tags_in() {
        // init.lua can't make folders: the first window's tag needs health\ to be there.
        let b = backend();
        assert!(!fake_fs(&b).is_dir(&b.paths().health()));
        assert_eq!(apply_terracotta(&b).0.status, ApplyStatus::Committed);
        assert!(fake_fs(&b).is_dir(&b.paths().health()));
    }

    #[test]
    fn an_apply_through_the_plan_lifts_any_pause() {
        // Review Focus 5.
        let b = backend();
        paused_in_cmd(&b);
        let (done, _) = apply_terracotta(&b);
        assert_eq!(done.status, ApplyStatus::Committed);
        assert!(!fake_fs(&b).exists(&b.paths().paused_file()));
        assert!(b.sys().fs.list_dir(&b.paths().health()).unwrap().is_empty());
        // One that can't be lifted is said, and the apply stands.
        let not_cleared = |done: &ApplyOutcome| {
            done.notes
                .iter()
                .find(|n| {
                    n.code == "E_DATA_WRITE"
                        && n.details
                            .starts_with("the theme was applied, but CMD Studio's start records in")
                })
                .cloned()
        };
        fake_fs(&b).put(b.paths().paused_file(), IN_CMD);
        fake_fs(&b).fail_removes_of(b.paths().paused_file());
        let (done, _) = apply_terracotta(&b);
        assert_eq!(done.status, ApplyStatus::Committed);
        assert!(not_cleared(&done).is_some(), "{:?}", done.notes);
        // A start tag held with no pause at all (a virus scanner has it): said too, and never
        // as a pause, which there is none of (as the CLI says it, Task 6).
        fake_fs(&b).stop_failing(b.paths().paused_file());
        fake_fs(&b).remove_file(&b.paths().paused_file()).unwrap();
        let tag = b.paths().health().join("start-13");
        fake_fs(&b).put(&tag, b"v1 1791694709\n");
        fake_fs(&b).fail_removes_of(&tag);
        let (done, _) = apply_terracotta(&b);
        assert_eq!(done.status, ApplyStatus::Committed);
        let note = not_cleared(&done).unwrap_or_else(|| panic!("{:?}", done.notes));
        assert!(!note.details.contains("pause"), "{}", note.details);
        assert!(note.details.contains("start-13"), "{}", note.details);
    }

    #[test]
    fn undo_everything_lifts_a_pause_in_cmd_but_not_one_of_this_computer() {
        let b = backend();
        paused_in_cmd(&b);
        assert!(b.revert_all().unwrap().ok);
        assert_eq!(b.recovery_status().unwrap().paused, None);
        assert!(health_is_empty(&b), "its start tags went too");
        assert_eq!(apply_terracotta(&b).0.status, ApplyStatus::Committed);
        b.theme_pause(PauseReason::User).unwrap();
        assert!(b.revert_all().unwrap().ok);
        assert_eq!(
            b.recovery_status().unwrap().paused.unwrap().by,
            PausedBy::App,
            "it holds the theme to turn back on"
        );
    }

    #[test]
    fn the_health_check_offers_to_turn_back_on_or_to_pause() {
        let b = backend();
        paused_in_cmd(&b);
        let d12 = |b: &Backend<FakeMachine>| {
            b.doctor_run()
                .unwrap()
                .checks
                .into_iter()
                .find(|c| c.id == "D12")
                .unwrap()
        };
        let check = d12(&b);
        assert_eq!(
            (check.status, check.code.as_deref(), check.fix),
            (Status::Warn, Some("E_THEME_PAUSED"), Some(FixKind::Resume))
        );
        assert_eq!(b.doctor_fix("D12").unwrap().kind, Some(FixKind::Resume));
        b.theme_resume().unwrap();
        let at = applied_at(&b);
        fake_events(&b).crash(at, "cmd.exe", CLINK_DLL);
        let check = d12(&b);
        assert_eq!(
            (check.status, check.code.as_deref(), check.fix),
            (Status::Warn, Some("E_CMD_CRASHED"), Some(FixKind::Pause))
        );
        // The D12 fix pauses for cmd's crashes, with their count.
        b.theme_pause(PauseReason::ClinkCrash).unwrap();
        let paused = b.recovery_status().unwrap().paused.unwrap();
        assert_eq!(
            (paused.reason, paused.count),
            (Some(PauseReason::ClinkCrash), 1)
        );
    }

    #[test]
    fn the_slot_waits_for_a_check_on_its_way_and_never_for_one_not_started() {
        let slot = Arc::new(CrashSlot::new());
        assert_eq!(slot.wait(Duration::from_secs(5)), None);
        slot.start();
        assert_eq!(slot.wait(Duration::from_millis(20)), None, "still running");
        let finisher = slot.clone();
        let thread = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            finisher.finish(CrashCheck::NoTheme);
        });
        assert_eq!(slot.wait(Duration::from_secs(5)), Some(CrashCheck::NoTheme));
        thread.join().unwrap();
        // Done stays done.
        slot.start();
        assert_eq!(slot.wait(Duration::ZERO), Some(CrashCheck::NoTheme));
    }

    #[test]
    fn a_check_that_could_not_start_is_not_waited_for() {
        let b = backend();
        b.crash_check_started();
        b.crash_check_not_run("the check could not start: out of threads");
        let started = std::time::Instant::now();
        let status = b.recovery_status().unwrap();
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(matches!(
            status.crashes,
            Some(CrashCheck::Unreadable { .. })
        ));
    }
}
