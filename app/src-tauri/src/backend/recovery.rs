//! The theme's pause, from the app (recovery spec §4.1–§4.3, §8): `recovery_status`,
//! `theme_pause` and `theme_resume`, and the check of Windows' crash reports that runs once at
//! every start ([`Backend::crash_check`], which `lib.rs` starts on a thread of its own once the
//! window is up, so the window never waits for it).
//!
//! Two levels of pause (spec §4.1): init.lua pauses the theme in cmd (`by: clink`) and changes
//! nothing on the computer; the app pauses it on this computer (`by: app`) through Revert
//! everything, and notes the theme to apply again. Turning the theme back on lifts a pause in cmd
//! at once (nothing on the computer changed, so no transaction), and for a pause of this computer
//! gives the UI the request to apply the paused theme again through the plan; that apply's
//! commit lifts the pause (`Backend::apply`).
use super::apply::{ApplyRequest, PlanOutcome, RevertOutcome};
use super::{Backend, Machine};
use crate::error::AppError;
use cmdstudio_core::cleanup;
use cmdstudio_core::eventlog::clink_crashes;
use cmdstudio_core::model::Target;
use cmdstudio_core::recovery::{
    clear_pause, read_pause, write_pause, PauseReason, PauseRecord, Paused, PausedBy, Resume,
    PAUSED_VERSION, PAUSE_AFTER,
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
    /// No theme is applied, or state.json can't be read: nothing to look for.
    NoTheme,
    /// `count` crashes of cmd in its add-on since the theme was applied; `paused_now` when this
    /// check paused the theme on this computer for them.
    Read { count: u32, paused_now: bool },
    /// The reports could not be read (the log is off, this user may not read it, or it did not
    /// answer in five seconds), so nothing was done.
    Unreadable { why: String },
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

/// What "Pause the theme" did. `paused.json` is written however the revert went (spec §4.1):
/// while it is there, init.lua loads no theme, and the home page says the state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PauseOutcome {
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

#[derive(Clone)]
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
        });
    }

    /// The start's check of Windows' crash reports (spec §4.3): cmd's crashes in Clink's DLL since
    /// the theme was applied, at most seven days back. [`PAUSE_AFTER`] or more, with a theme
    /// applied and no pause of this computer yet, pause it on this computer
    /// ([`PauseReason::ClinkCrash`]); a pause in cmd does not count, because Clink still injects
    /// and its DLL still loads. Reports that can't be read pause nothing. One log line, never the
    /// user's data. A panic in it is an `Unreadable` answer, never a `recovery_status` that waits
    /// for nothing.
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
                }
            }
        };
        self.crash.finish(check.clone());
        check
    }

    fn run_crash_check(&self) -> CrashCheck {
        let sys = self.sys();
        let ctx = self.ctx();
        let Ok(State {
            applied: Some(applied),
            ..
        }) = State::load(sys.fs, &ctx.paths.state_file())
        else {
            self.note_line("crash_check theme=none");
            return CrashCheck::NoTheme;
        };
        let crashes = match clink_crashes(sys.event_log, applied.applied_at_ms, ctx.now_ms) {
            Ok(crashes) => crashes,
            Err(e) => {
                // Never a pause for reports that can't be read (spec §4.3).
                self.note_line("crash_check unreadable");
                return CrashCheck::Unreadable { why: e.to_string() };
            }
        };
        let count = u32::try_from(crashes.len()).unwrap_or(u32::MAX);
        let paused_here = matches!(
            read_pause(sys.fs, &ctx.paths),
            Ok(Some(Paused {
                by: PausedBy::App,
                ..
            }))
        );
        let paused_now = count >= PAUSE_AFTER
            && !paused_here
            && self
                .pause_machine(PauseReason::ClinkCrash, count)
                .not_saved
                .is_none();
        self.note_line(&format!("crash_check crashes={count} paused={paused_now}"));
        CrashCheck::Read { count, paused_now }
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

    /// Pauses the theme on this computer (spec §4.1): Revert everything (a journaled
    /// transaction, under the lock), then `paused.json` with the theme to apply again: the one
    /// applied now, else the one an earlier pause of this computer noted. Written however the
    /// revert went. Phase 3b-2's `--recovery` pauses through here too (`recovery`).
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
        let (revert, refused) = match cleanup::revert_all(sys, &ctx) {
            Ok(report) => (Some(self.revert_outcome(&ctx, &report)), None),
            Err(e) => (None, Some(AppError::from(e))),
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
        let reverted = match (&revert, &refused) {
            (Some(r), _) if r.ok => "ok",
            (Some(_), _) => "incomplete",
            (None, Some(e)) => e.code.as_str(),
            (None, None) => "none",
        };
        self.note_line(&format!(
            "pause reason={} count={count} revert={reverted} saved={}",
            reason.as_str(),
            not_saved.is_none()
        ));
        PauseOutcome {
            revert,
            refused,
            not_saved,
        }
    }

    /// "Turn the theme back on" (spec §4.1, §8): a pause in cmd is lifted at once; for a pause
    /// of this computer the request (the paused theme, its target, the settings' choice for
    /// Windows Terminal's default profile) and its plan, which the UI shows before `apply`.
    pub fn theme_resume(&self) -> Result<ResumeOutcome, AppError> {
        let fs = self.sys().fs;
        let paused = read_pause(fs, self.paths())
            .map_err(|e| AppError::coded("E_MACHINE_READ", e.to_string()))?;
        let Some(p) = paused else {
            return Ok(ResumeOutcome::NotPaused);
        };
        match (p.by, p.resume) {
            (PausedBy::Clink, _) => {
                clear_pause(fs, self.paths())
                    .map_err(|e| AppError::coded("E_DATA_WRITE", e.to_string()))?;
                self.note_line("resume by=clink");
                Ok(ResumeOutcome::Resumed)
            }
            (PausedBy::App, Some(r)) if self.find_theme(r.theme_id).is_ok() => {
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
            (PausedBy::App, _) => Ok(ResumeOutcome::PickTheme),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::apply::{ApplyOutcome, ApplyStatus, TxnStatus};
    use crate::backend::doctor::FixKind;
    use crate::backend::testing::{
        apply_terracotta, backend, fake_events, fake_fs, plan_and_apply, terracotta_id,
    };
    use cmdstudio_core::adapters::fake::FakeMachine;
    use cmdstudio_core::adapters::Fs;
    use cmdstudio_core::doctor::Status;
    use std::sync::Arc;

    const CLINK_DLL: &str = "clink_dll_x64.dll";
    /// What init.lua writes when it pauses the theme in cmd.
    const IN_CMD: &[u8] =
        br#"{"version":1,"reason":"cmdStarts","at":1791694709000,"count":3,"by":"clink"}"#;

    fn applied_at(b: &Backend<FakeMachine>) -> u64 {
        State::load(b.sys().fs, &b.paths().state_file())
            .unwrap()
            .applied
            .unwrap()
            .applied_at_ms
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
        assert_eq!((paused.by, paused.reason), (PausedBy::Clink, None));
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
        assert_eq!(out.revert, None);
        assert_eq!(out.refused.unwrap().code, "E_BUSY");
        assert_eq!(out.not_saved, None);
        // cmd loads no theme while it is there; the theme is still applied, to apply again.
        let paused = b.recovery_status().unwrap().paused.unwrap();
        assert_eq!(paused.by, PausedBy::App);
        assert_eq!(paused.resume.unwrap().theme_id, terracotta_id());
    }

    #[test]
    fn a_pause_that_cannot_be_written_is_said() {
        let b = backend();
        assert_eq!(apply_terracotta(&b).0.status, ApplyStatus::Committed);
        fake_fs(&b).fail_writes_to(b.paths().paused_file());
        let out = b.theme_pause(PauseReason::User).unwrap();
        assert!(out.revert.unwrap().ok);
        assert_eq!(out.not_saved.unwrap().code, "E_DATA_WRITE");
    }

    #[test]
    fn three_crashes_since_the_apply_pause_the_theme_at_start() {
        let b = backend();
        assert_eq!(apply_terracotta(&b).0.status, ApplyStatus::Committed);
        let at = applied_at(&b);
        for _ in 0..3 {
            fake_events(&b).crash(at, "cmd.exe", CLINK_DLL);
        }
        assert_eq!(
            b.crash_check(),
            CrashCheck::Read {
                count: 3,
                paused_now: true
            }
        );
        let status = b.recovery_status().unwrap();
        let paused = status.paused.unwrap();
        assert_eq!(
            (paused.by, paused.reason, paused.count),
            (PausedBy::App, Some(PauseReason::ClinkCrash), 3)
        );
        assert_eq!(
            status.crashes,
            Some(CrashCheck::Read {
                count: 3,
                paused_now: true
            })
        );
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
                paused_now: false
            }
        );
        fake_events(&b).fail_reads("the event log service is off");
        let check = b.crash_check();
        assert!(
            matches!(&check, CrashCheck::Unreadable { why } if why.contains("service is off")),
            "{check:?}"
        );
        // A third crash, while an earlier read of the log is still going on (the reader's
        // single-flight guard): reports that could not be read, never "no crash" and never a
        // pause.
        fake_events(&b).crash(at, "cmd.exe", CLINK_DLL);
        fake_events(&b).fail_reads("the Application log is still being read by an earlier read");
        let check = b.crash_check();
        assert!(
            matches!(&check, CrashCheck::Unreadable { why } if why.contains("still being read")),
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
        assert_eq!(
            b.crash_check(),
            CrashCheck::Read {
                count: 3,
                paused_now: true
            }
        );
        assert_eq!(
            b.recovery_status().unwrap().paused.unwrap().by,
            PausedBy::App
        );
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
