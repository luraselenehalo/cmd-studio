//! The updater's state in the app (spec §6.2, §2.1, §5.5, phase 4b): the check after the window
//! is up, the download in the background, the install at close (the own spawn, when the lock is
//! free) or now (the own spawn with a relaunch, on the user's click), the record
//! `<data>\update.json` written before either, read at the next start for "Updated to vX" and
//! deleted once the UI has shown it. Every step writes one log line of names, versions and
//! codes, never a URL or a path. The machine is never touched here: the installer does that, and
//! only after the lock was taken and the run record ended. Only the process that wrote the run
//! record does any of it (`run_is_ours`): an instance that single-instance is about to end reads,
//! checks and installs nothing.
use super::{Backend, Machine};
use crate::error::AppError;
use crate::settings::UpdateMode;
use crate::update::{Available, Updater};
use cmdstudio_core::adapters::FsLock;
use cmdstudio_core::resources::sha256_hex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use ts_rs::TS;

/// The version of `update.json` this build writes and reads.
pub const UPDATE_RECORD_VERSION: u32 = 1;
/// The name `app_status.busy` carries while the re-apply after an update runs (Task 5; recovery
/// spec §6.3). A name of its own, like `recovery_entry`: it is no IPC command.
pub const UPDATE_REAPPLY_BUSY: &str = "update_reapply";

/// `<data>\update.json` (spec §5.3, §6.2): written before the installer starts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRecord {
    pub version: u32,
    pub from: String,
    pub to: String,
    pub notes_url: String,
}

/// Where the updater is, for `update_status`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum UpdateState {
    /// `UpdateMode::Off`: nothing was asked.
    Off,
    /// No update, or a check that failed (one log line), or the check has not run yet.
    #[default]
    Idle,
    Checking,
    /// Notify mode: a newer version waits for "Update now".
    Available {
        version: String,
    },
    Downloading {
        version: String,
    },
    /// Automatic mode: downloaded and verified; installed at close.
    Ready {
        version: String,
    },
    /// The installer was started (at close, or now).
    Installing {
        version: String,
    },
    /// The install at close did not run: `busy` (the lock), `lock` (the lock file), `record`
    /// (update.json could not be written), `restart` (the exit is `app_restart`'s). The next
    /// launch, or the next normal close, tries again.
    Postponed {
        version: String,
        reason: String,
    },
}

/// `update_status` (spec §6.2).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateStatus {
    pub mode: UpdateMode,
    pub state: UpdateState,
    /// `None` until the check of this run has run (the UI polls until then).
    #[ts(type = "number | null")]
    pub checked_at_ms: Option<u64>,
}

/// The notice of the next start (spec §2.1): `app_status.updated`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Updated {
    pub from: String,
    pub to: String,
    pub notes_url: String,
}

/// A verified download: the file and the hash of the bytes the plugin verified, which the file
/// must still match before it is started (Review Focus 1).
struct Ready {
    available: Available,
    file: PathBuf,
    sha256: String,
}

#[derive(Default)]
struct UpdateInner {
    /// The check ran (or was refused by the mode) in this run: once per run.
    checked: bool,
    state: UpdateState,
    checked_at_ms: Option<u64>,
    /// This run's version, as `update_after_start` was told it.
    current: Option<String>,
    available: Option<Available>,
    ready: Option<Ready>,
    updated: Option<Updated>,
    /// `app_restart` spawned its child: the exit that follows installs nothing (Q9).
    restarting: bool,
    /// A download or an install runs now (the check's download, "Update now", the close's
    /// install): claimed and let go under this lock, so two never run at once (review I1).
    busy: bool,
    /// An installer was started in this run: nothing installs again, and the exit installs
    /// nothing (review I1: one install per run).
    spawned: bool,
}

/// The updater's state of this run, behind one lock (a poisoned lock is used as it is).
pub(crate) struct UpdateSlot {
    inner: Mutex<UpdateInner>,
}

impl UpdateSlot {
    pub(crate) fn new() -> Self {
        UpdateSlot {
            inner: Mutex::new(UpdateInner::default()),
        }
    }
    fn with<T>(&self, f: impl FnOnce(&mut UpdateInner) -> T) -> T {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        f(&mut inner)
    }
}

impl<M: Machine> Backend<M> {
    fn set_update_state(&self, state: UpdateState) -> UpdateState {
        self.update.with(|u| u.state = state.clone());
        state
    }

    /// `updates\cmdstudio-<version>-setup.exe`: the version is SemVer (checked when it was
    /// offered), so the name holds nothing a path would read.
    fn update_file_for(&self, version: &str) -> PathBuf {
        self.paths()
            .updates_dir()
            .join(format!("cmdstudio-{version}-setup.exe"))
    }

    /// The check and, in automatic mode, the download (spec §6.2), once per run, after the window
    /// is up (`update::start_check`), for a run that is this process's own. Off is one log line.
    /// A check that fails is one log line and `Idle`: nothing is shown. Notify stops at
    /// `Available`; Auto downloads, verifies (the updater), writes the file in one step and is
    /// `Ready` for the close.
    pub fn update_check_and_download(&self, updater: &dyn Updater) -> UpdateState {
        let first = self.update.with(|u| {
            let first = !u.checked;
            u.checked = true;
            first
        });
        if !first || !self.run_is_ours() {
            return self.update.with(|u| u.state.clone());
        }
        let mode = self.settings_or_default().updates;
        if mode == UpdateMode::Off {
            self.note_line("update mode=off");
            return self.set_update_state(UpdateState::Off);
        }
        self.set_update_state(UpdateState::Checking);
        let checked = updater.check();
        let now = self.now_ms();
        self.update.with(|u| u.checked_at_ms = Some(now));
        let available = match checked {
            Err(e) => {
                self.note_line(&format!("update check={}", e.log_text()));
                return self.set_update_state(UpdateState::Idle);
            }
            Ok(None) => {
                self.note_line("update check=none");
                return self.set_update_state(UpdateState::Idle);
            }
            Ok(Some(available)) => available,
        };
        self.note_line(&format!("update available={}", available.version));
        // What was offered and what comes next, in one step, so "Update now" (which needs
        // `available`) never runs between the two; the download is claimed (review I1).
        let download = self.update.with(|u| {
            u.available = Some(available.clone());
            if u.busy || u.spawned {
                return false;
            }
            let version = available.version.clone();
            if mode == UpdateMode::Notify {
                u.state = UpdateState::Available { version };
                return false;
            }
            u.busy = true;
            u.state = UpdateState::Downloading { version };
            true
        });
        if !download {
            return self.update.with(|u| u.state.clone());
        }
        let downloaded = self.update_download(updater, &available);
        match &downloaded {
            Ok(()) => self.note_line(&format!("update downloaded={}", available.version)),
            Err(why) => self.note_line(&format!("update download={why}")),
        }
        // The claim's end and the outcome in one step; a state another step set meanwhile
        // (Installing above all) is never written over.
        self.update.with(|u| {
            u.busy = false;
            if matches!(u.state, UpdateState::Downloading { .. }) {
                u.state = match downloaded {
                    Ok(()) => UpdateState::Ready {
                        version: available.version,
                    },
                    Err(_) => UpdateState::Idle,
                };
            }
            u.state.clone()
        })
    }

    /// Downloads and verifies through the updater, writes the bytes to `updates\` in one step and
    /// keeps their hash, so that the file is checked again before it is ever started. The error
    /// is the log's word.
    fn update_download(&self, updater: &dyn Updater, available: &Available) -> Result<(), String> {
        let bytes = updater.download(available).map_err(|e| e.log_text())?;
        let file = self.update_file_for(&available.version);
        self.sys()
            .fs
            .write_atomic(&file, &bytes)
            .map_err(|_| "write".to_string())?;
        let sha256 = sha256_hex(&bytes);
        self.update.with(|u| {
            u.ready = Some(Ready {
                available: available.clone(),
                file,
                sha256,
            })
        });
        Ok(())
    }

    pub fn update_status(&self) -> Result<UpdateStatus, AppError> {
        let mode = self.settings_or_default().updates;
        Ok(self.update.with(|u| UpdateStatus {
            mode,
            state: u.state.clone(),
            checked_at_ms: u.checked_at_ms,
        }))
    }

    /// The notice of this start, when the last run's update went through (`update_after_start`).
    pub fn updated(&self) -> Option<Updated> {
        self.update.with(|u| u.updated.clone())
    }

    /// `app_restart` spawned its child (`ipc.rs`): the exit it asks for installs nothing (Q9).
    pub fn mark_restarting(&self) {
        self.update.with(|u| u.restarting = true);
    }

    /// The lock of spec §5.5, tried once: `E_BUSY` while another change runs.
    fn update_lock(&self) -> Result<FsLock, AppError> {
        match self.sys().fs.try_lock(&self.paths().lock_file()) {
            Ok(Some(lock)) => Ok(lock),
            Ok(None) => Err(AppError::coded(
                "E_BUSY",
                "cmdstudio.lock is held: a change is running, so the update waits",
            )),
            Err(e) => Err(AppError::coded(
                "E_DATA_WRITE",
                format!("CMD Studio's lock file could not be opened: {e}"),
            )),
        }
    }

    /// The downloaded file, when it still is the file that was verified (Review Focus 1); one that
    /// changed or went is deleted, said, and forgotten, so the caller downloads again.
    fn update_ready_file(&self) -> Option<PathBuf> {
        let (file, sha256) = self
            .update
            .with(|u| u.ready.as_ref().map(|r| (r.file.clone(), r.sha256.clone())))?;
        let bytes = self.sys().fs.read(&file).ok().flatten();
        match bytes {
            Some(bytes) if sha256_hex(&bytes) == sha256 => Some(file),
            _ => {
                let _ = self.sys().fs.remove_file(&file);
                self.update.with(|u| u.ready = None);
                self.note_line("update install=file-changed");
                None
            }
        }
    }

    fn update_record_for(&self, available: &Available) -> UpdateRecord {
        let from = self
            .update
            .with(|u| u.current.clone())
            .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
        UpdateRecord {
            version: UPDATE_RECORD_VERSION,
            from,
            to: available.version.clone(),
            notes_url: available.notes_url.clone(),
        }
    }

    /// `update.json`, in one step; whether it was written.
    fn write_update_record(&self, record: &UpdateRecord) -> bool {
        let Ok(mut bytes) = serde_json::to_vec_pretty(record) else {
            return false;
        };
        bytes.push(b'\n');
        self.sys()
            .fs
            .write_atomic(&self.paths().update_file(), &bytes)
            .is_ok()
    }

    /// "Update now" (the notify card, the Recovery page; spec §6.2): the offered version is
    /// downloaded and verified if it is not yet (before the lock: a download may take minutes),
    /// the lock is taken (E_BUSY while a change runs), the file is checked against its hash, the
    /// record is written, the run ended, and the installer started by `spawn` with a relaunch
    /// (`update::start_installer`); the caller then ends the process (`app.exit(0)`, whose exit
    /// hook installs nothing more: an installer was started). Answers only on a failure: the
    /// record is taken back, the run record written again (Q16), and the state is as before.
    /// One install per run (review I1): while a download or an install runs, or once an
    /// installer was started, it is refused with E_BUSY and one log line, and does nothing.
    pub fn update_install(
        &self,
        updater: &dyn Updater,
        spawn: &dyn Fn(&Path, bool) -> Result<(), String>,
    ) -> Result<(), AppError> {
        let claim = self.update.with(|u| {
            if u.busy || u.spawned {
                return Err(true);
            }
            let available = u.available.clone().ok_or(false)?;
            u.busy = true;
            Ok(available)
        });
        let available = match claim {
            Ok(available) => available,
            Err(true) => {
                self.note_line("update install=already");
                return Err(AppError::coded(
                    "E_BUSY",
                    "the update is already being downloaded or installed",
                ));
            }
            Err(false) => return Err(AppError::internal("no update is available to install")),
        };
        let result = self.update_install_claimed(updater, spawn, available);
        self.update.with(|u| {
            u.busy = false;
            u.spawned |= result.is_ok();
        });
        result
    }

    /// `update_install`'s steps, with the claim held by the caller.
    fn update_install_claimed(
        &self,
        updater: &dyn Updater,
        spawn: &dyn Fn(&Path, bool) -> Result<(), String>,
        available: Available,
    ) -> Result<(), AppError> {
        let ready = UpdateState::Ready {
            version: available.version.clone(),
        };
        if self.update_ready_file().is_none() {
            self.set_update_state(UpdateState::Downloading {
                version: available.version.clone(),
            });
            if let Err(why) = self.update_download(updater, &available) {
                self.note_line(&format!("update download={why}"));
                self.set_update_state(UpdateState::Available {
                    version: available.version.clone(),
                });
                return Err(AppError::internal(format!(
                    "the update could not be downloaded ({why})"
                )));
            }
            self.note_line(&format!("update downloaded={}", available.version));
        }
        self.set_update_state(ready.clone());
        let _lock = self.update_lock()?;
        let Some(file) = self.update_ready_file() else {
            self.set_update_state(UpdateState::Available {
                version: available.version.clone(),
            });
            return Err(AppError::internal(
                "the downloaded installer changed on disk; try again",
            ));
        };
        if !self.write_update_record(&self.update_record_for(&available)) {
            return Err(AppError::coded(
                "E_DATA_WRITE",
                "update.json could not be written, so the update was not started",
            ));
        }
        self.note_line(&format!("update installing={} how=now", available.version));
        self.set_update_state(UpdateState::Installing {
            version: available.version.clone(),
        });
        self.end_run();
        if let Err(why) = spawn(&file, true) {
            let _ = self.sys().fs.remove_file(&self.paths().update_file());
            self.rewrite_run_record();
            self.note_line("update install=spawn");
            self.set_update_state(ready);
            return Err(AppError::internal(format!(
                "the update could not be installed: {why}"
            )));
        }
        Ok(())
    }

    /// At close (spec §6.2's primary; `lib.rs` on `RunEvent::Exit`, after `end_run`): when a
    /// verified download is ready, this run is its own, the exit is not `app_restart`'s, no
    /// installer was started in this run yet (review I1), and the lock is free, the record is
    /// written and the file started with `spawn` (no relaunch); the lock is let go when this
    /// returns, which is after the start. Busy (the lock, or an "Update now" that still runs), a
    /// lock file that cannot be opened, a record that cannot be written, or a restart postpone
    /// the install to the next launch or close, each with one log line; a file that changed is
    /// never started.
    pub fn update_at_exit(&self, spawn: &dyn Fn(&Path, bool) -> Result<(), String>) {
        if !self.run_is_ours() {
            return;
        }
        // Claimed in one step (review I1): nothing after an installer was started in this run;
        // an "Update now" that still runs postpones this one.
        let claim = self.update.with(|u| {
            if u.spawned {
                return None;
            }
            let available = u.ready.as_ref().map(|r| r.available.clone())?;
            if u.busy {
                return Some(Err(available));
            }
            u.busy = true;
            Some(Ok((available, u.restarting)))
        });
        let (available, restarting) = match claim {
            None => return,
            Some(Err(available)) => return self.update_postponed(&available, "busy"),
            Some(Ok(claimed)) => claimed,
        };
        let started = self.update_at_exit_claimed(spawn, &available, restarting);
        self.update.with(|u| {
            u.busy = false;
            u.spawned |= started;
        });
    }

    fn update_postponed(&self, available: &Available, reason: &str) {
        self.note_line(&format!("update postponed={reason}"));
        self.set_update_state(UpdateState::Postponed {
            version: available.version.clone(),
            reason: reason.to_string(),
        });
    }

    /// `update_at_exit`'s steps, with the claim held by the caller: whether the installer started.
    fn update_at_exit_claimed(
        &self,
        spawn: &dyn Fn(&Path, bool) -> Result<(), String>,
        available: &Available,
        restarting: bool,
    ) -> bool {
        let postponed = |reason: &str| {
            self.update_postponed(available, reason);
            false
        };
        if restarting {
            return postponed("restart");
        }
        let _lock = match self.sys().fs.try_lock(&self.paths().lock_file()) {
            Ok(Some(lock)) => lock,
            Ok(None) => return postponed("busy"),
            Err(_) => return postponed("lock"),
        };
        let Some(file) = self.update_ready_file() else {
            return false;
        };
        if !self.write_update_record(&self.update_record_for(available)) {
            return postponed("record");
        }
        match spawn(&file, false) {
            Ok(()) => {
                self.note_line(&format!("update installing={} how=exit", available.version));
                self.set_update_state(UpdateState::Installing {
                    version: available.version.clone(),
                });
                true
            }
            Err(_) => {
                let _ = self.sys().fs.remove_file(&self.paths().update_file());
                self.note_line("update install=spawn");
                false
            }
        }
    }

    /// After the run record and before the window (`lib.rs`), for a run that is this process's
    /// own: the last run's download is thrown away (never trusted across runs), and
    /// `update.json`, when there, makes the notice if its `to` is this run's version
    /// (`update installed to=…`); else the installer did not finish (`update failed to=…`), and
    /// the record goes. A record that cannot be read stays and is said; one that is not this
    /// version's, not JSON, or names a version that is not SemVer is damaged: said and deleted.
    /// The prune of old runtime folders is not here (Task 5: one place, under the lock, never in
    /// safe mode).
    pub fn update_after_start(&self, current_version: &str) {
        if !self.run_is_ours() {
            return;
        }
        self.update
            .with(|u| u.current = Some(current_version.to_string()));
        let fs = self.sys().fs;
        let updates = self.paths().updates_dir();
        if fs.exists(&updates) && fs.remove_dir_all(&updates).is_err() {
            self.note_line("update updates=kept");
        }
        let file = self.paths().update_file();
        let record = match fs.read(&file) {
            Err(_) => return self.note_line("update record=unreadable"),
            Ok(None) => return,
            Ok(Some(bytes)) => serde_json::from_slice::<UpdateRecord>(&bytes)
                .ok()
                .filter(|r| r.version == UPDATE_RECORD_VERSION)
                .and_then(|r| crate::open::release_url(&r.to).map(|url| (r, url))),
        };
        let Some((record, notes_url)) = record else {
            let _ = fs.remove_file(&file);
            return self.note_line("update record=damaged");
        };
        if record.to != current_version {
            self.note_line(&format!(
                "update failed to={} running={current_version}",
                record.to
            ));
            let _ = fs.remove_file(&file);
            return;
        }
        self.note_line(&format!("update installed to={}", record.to));
        self.update.with(|u| {
            u.updated = Some(Updated {
                from: record.from,
                to: record.to,
                notes_url,
            })
        });
    }

    /// `app_ready`: the UI drew the notice, so the record goes (spec §2.1: once); the notice
    /// stays for the run.
    pub(crate) fn update_record_consumed(&self) {
        if self.updated().is_some() {
            let _ = self.sys().fs.remove_file(&self.paths().update_file());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::run::GuiStart;
    use crate::backend::testing::{backend, fake_fs, fake_host};
    use crate::settings::AppSettings;
    use crate::update::{FakeUpdater, UpdateError};
    use cmdstudio_core::adapters::fake::FakeMachine;
    use std::cell::RefCell;

    /// A backend whose run is its own: `start_run` wrote the record, as `run_gui` does first.
    fn started() -> Backend<FakeMachine> {
        let b = backend();
        b.start_run(GuiStart::Normal);
        b
    }

    fn set_mode<M: Machine>(b: &Backend<M>, updates: UpdateMode) {
        b.settings_set(AppSettings {
            updates,
            ..AppSettings::default()
        })
        .unwrap();
    }

    fn update_lines<M: Machine>(b: &Backend<M>) -> Vec<String> {
        b.log_tail(50)
            .lines()
            .filter_map(|l| {
                l.split_once(" update ")
                    .map(|(_, rest)| format!("update {rest}"))
            })
            .collect()
    }

    fn record(b: &Backend<FakeMachine>) -> Option<UpdateRecord> {
        let bytes = b.sys().fs.read(&b.paths().update_file()).unwrap()?;
        Some(serde_json::from_slice(&bytes).unwrap())
    }

    const RECORD_0_2_0: &[u8] = br#"{"version":1,"from":"0.1.0","to":"0.2.0","notesUrl":"https://github.com/luraselenehalo/cmd-studio/releases/tag/v0.2.0"}"#;

    /// Spec §6.2: Off checks nothing and says so once per run.
    #[test]
    fn off_checks_nothing_and_logs_once() {
        let b = started();
        set_mode(&b, UpdateMode::Off);
        let fake = FakeUpdater::offering("0.2.0");
        assert_eq!(b.update_check_and_download(&fake), UpdateState::Off);
        assert_eq!(b.update_check_and_download(&fake), UpdateState::Off);
        assert_eq!(fake.calls(), Vec::<String>::new());
        assert_eq!(update_lines(&b), ["update mode=off"]);
        let status = b.update_status().unwrap();
        assert_eq!(
            (status.mode, status.state, status.checked_at_ms),
            (UpdateMode::Off, UpdateState::Off, None)
        );
    }

    /// Spec §6.2: a check that fails (no network, GitHub down) is one log line and `idle`; nothing
    /// is downloaded, nothing is shown, and the time of the check is noted.
    #[test]
    fn offline_is_silent_and_idle() {
        let b = started();
        let fake = FakeUpdater::failing(UpdateError::Offline("dns".into()));
        assert_eq!(b.update_check_and_download(&fake), UpdateState::Idle);
        assert_eq!(fake.calls(), ["check"]);
        assert_eq!(update_lines(&b), ["update check=offline"]);
        assert!(!b.sys().fs.exists(&b.paths().updates_dir()));
        assert_eq!(b.updated(), None);
        assert!(b.update_status().unwrap().checked_at_ms.is_some());
        // An error no arm names: silent as offline, its text on the line (Q18).
        let b = started();
        let fake = FakeUpdater::failing(UpdateError::Other("a variant 2.14 added".into()));
        assert_eq!(b.update_check_and_download(&fake), UpdateState::Idle);
        assert_eq!(
            update_lines(&b),
            ["update check=offline other=a variant 2.14 added"]
        );
    }

    /// U5: a manifest that is not JSON, or offers no Windows target, is a refused check, said as
    /// such; an up-to-date one is `none`.
    #[test]
    fn a_malformed_manifest_is_offline() {
        let b = started();
        let fake = FakeUpdater::failing(UpdateError::Manifest("expected value at line 1".into()));
        assert_eq!(b.update_check_and_download(&fake), UpdateState::Idle);
        assert_eq!(update_lines(&b), ["update check=manifest"]);
        let b = started();
        assert_eq!(
            b.update_check_and_download(&FakeUpdater::none()),
            UpdateState::Idle
        );
        assert_eq!(update_lines(&b), ["update check=none"]);
    }

    /// Spec §6.2 (automatic, the primary): the installer is downloaded after the check, written to
    /// updates\ in one step, and started at close with the record written (the run ended before,
    /// by lib.rs) and no relaunch.
    #[test]
    fn auto_downloads_and_is_ready_at_exit() {
        let b = started();
        b.update_after_start("0.1.0");
        let fake = FakeUpdater::offering("0.2.0");
        assert_eq!(
            b.update_check_and_download(&fake),
            UpdateState::Ready {
                version: "0.2.0".into()
            }
        );
        assert_eq!(fake.calls(), ["check", "download 0.2.0"]);
        let file = b.paths().updates_dir().join("cmdstudio-0.2.0-setup.exe");
        assert_eq!(
            b.sys().fs.read(&file).unwrap().as_deref(),
            Some(&b"MZ-the-installer"[..])
        );
        assert_eq!(
            update_lines(&b),
            ["update available=0.2.0", "update downloaded=0.2.0"]
        );
        assert_eq!(
            b.update_status().unwrap().state,
            UpdateState::Ready {
                version: "0.2.0".into()
            }
        );

        let spawned = RefCell::new(Vec::new());
        b.end_run();
        b.update_at_exit(&|f, relaunch| {
            spawned.borrow_mut().push((f.to_path_buf(), relaunch));
            Ok(())
        });
        assert_eq!(spawned.into_inner(), [(file, false)]);
        assert_eq!(
            record(&b),
            Some(UpdateRecord {
                version: 1,
                from: "0.1.0".into(),
                to: "0.2.0".into(),
                notes_url: "https://github.com/luraselenehalo/cmd-studio/releases/tag/v0.2.0"
                    .into(),
            })
        );
        assert_eq!(
            update_lines(&b).last().map(String::as_str),
            Some("update installing=0.2.0 how=exit")
        );
        assert_eq!(
            b.update_status().unwrap().state,
            UpdateState::Installing {
                version: "0.2.0".into()
            }
        );
        // The record, on the wire, in camelCase like every file CMD Studio writes.
        let text =
            String::from_utf8(b.sys().fs.read(&b.paths().update_file()).unwrap().unwrap()).unwrap();
        assert!(
            text.contains("\"notesUrl\"") && text.contains("\"from\": \"0.1.0\""),
            "{text}"
        );
    }

    /// Spec §6.2 (notify): nothing is downloaded until the button; "Update now" downloads, writes
    /// the record, ends the run and starts the installer with a relaunch; the window's own exit
    /// then installs nothing more (the state says so).
    #[test]
    fn notify_waits_for_the_button() {
        let b = started();
        b.update_after_start("0.1.0");
        set_mode(&b, UpdateMode::Notify);
        let fake = FakeUpdater::offering("0.2.0");
        assert_eq!(
            b.update_check_and_download(&fake),
            UpdateState::Available {
                version: "0.2.0".into()
            }
        );
        assert_eq!(fake.calls(), ["check"]);
        assert!(!b.sys().fs.exists(&b.paths().updates_dir()));
        let spawned = RefCell::new(Vec::new());
        let spawn = |f: &Path, relaunch: bool| {
            spawned.borrow_mut().push((f.to_path_buf(), relaunch));
            Ok(())
        };
        b.update_at_exit(&spawn);
        assert!(
            spawned.borrow().is_empty(),
            "nothing is ready: nothing starts at close"
        );

        b.update_install(&fake, &spawn).unwrap();
        let file = b.paths().updates_dir().join("cmdstudio-0.2.0-setup.exe");
        assert_eq!(*spawned.borrow(), [(file, true)]);
        assert_eq!(fake.calls(), ["check", "download 0.2.0"]);
        assert_eq!(record(&b).map(|r| r.to), Some("0.2.0".into()));
        assert!(
            !b.sys().fs.exists(&b.paths().run_file()),
            "the run ended before the installer"
        );
        let lines = update_lines(&b);
        assert_eq!(
            lines.get(lines.len() - 2).map(String::as_str),
            Some("update downloaded=0.2.0")
        );
        assert_eq!(
            lines.last().map(String::as_str),
            Some("update installing=0.2.0 how=now")
        );
        assert_eq!(
            b.update_status().unwrap().state,
            UpdateState::Installing {
                version: "0.2.0".into()
            }
        );
        // The exit that follows (app.exit): nothing starts twice.
        b.update_at_exit(&spawn);
        assert_eq!(spawned.borrow().len(), 1);
        // Nothing to install: refused with an internal error, nothing written.
        let b = started();
        let e = b.update_install(&FakeUpdater::none(), &spawn).unwrap_err();
        assert_eq!(e.code, "E_INTERNAL");
        assert!(!b.sys().fs.exists(&b.paths().update_file()));
    }

    /// Spec §5.5, §6.2 (Review Focus 2): a change that still runs holds the lock; the install at
    /// close is postponed and said, "Update now" answers E_BUSY, and the next launch empties the
    /// download and checks again.
    #[test]
    fn a_busy_lock_postpones_the_install_and_tries_next_launch() {
        let b = started();
        b.update_after_start("0.1.0");
        let fake = FakeUpdater::offering("0.2.0");
        assert_eq!(
            b.update_check_and_download(&fake),
            UpdateState::Ready {
                version: "0.2.0".into()
            }
        );
        let held = b
            .sys()
            .fs
            .try_lock(&b.paths().lock_file())
            .unwrap()
            .unwrap();
        let spawned = RefCell::new(0);
        let spawn = |_: &Path, _: bool| {
            *spawned.borrow_mut() += 1;
            Ok(())
        };
        b.update_at_exit(&spawn);
        assert_eq!(*spawned.borrow(), 0);
        assert_eq!(
            update_lines(&b).last().map(String::as_str),
            Some("update postponed=busy")
        );
        assert_eq!(
            b.update_status().unwrap().state,
            UpdateState::Postponed {
                version: "0.2.0".into(),
                reason: "busy".into()
            }
        );
        assert_eq!(record(&b), None);
        assert_eq!(b.update_install(&fake, &spawn).unwrap_err().code, "E_BUSY");
        assert!(b.sys().fs.exists(&b.paths().run_file()), "the run goes on");
        drop(held);
        // The next launch: the last run's download is not trusted, the folder is emptied, the
        // check runs anew.
        let next = started();
        fake_fs(&next).put(
            next.paths().updates_dir().join("cmdstudio-0.2.0-setup.exe"),
            b"old",
        );
        next.update_after_start("0.1.0");
        assert!(!next.sys().fs.exists(&next.paths().updates_dir()));
        let fake = FakeUpdater::offering("0.2.0");
        assert_eq!(
            next.update_check_and_download(&fake),
            UpdateState::Ready {
                version: "0.2.0".into()
            }
        );
        assert_eq!(fake.calls(), ["check", "download 0.2.0"]);
    }

    /// Spec §2.1: the record makes the notice, once; the file goes with app_ready, the notice stays
    /// for the run; a start without a record shows nothing.
    #[test]
    fn update_json_makes_the_notice_once_and_goes_with_app_ready() {
        let b = started();
        fake_fs(&b).put(b.paths().update_file(), RECORD_0_2_0);
        b.update_after_start("0.2.0");
        assert_eq!(
            b.updated(),
            Some(Updated {
                from: "0.1.0".into(),
                to: "0.2.0".into(),
                notes_url: "https://github.com/luraselenehalo/cmd-studio/releases/tag/v0.2.0"
                    .into(),
            })
        );
        assert_eq!(update_lines(&b), ["update installed to=0.2.0"]);
        assert!(
            b.sys().fs.exists(&b.paths().update_file()),
            "kept until the UI has shown it"
        );
        b.app_ready().unwrap();
        assert!(!b.sys().fs.exists(&b.paths().update_file()));
        assert!(b.updated().is_some(), "the card stays for the run");
        let next = started();
        next.update_after_start("0.2.0");
        assert_eq!(next.updated(), None);
        assert_eq!(update_lines(&next), Vec::<String>::new());
    }

    /// Review Focus 3: the installer did not finish and the old version starts with the record
    /// there: no notice, one log line, the record gone.
    #[test]
    fn a_failed_install_is_logged_and_the_file_goes() {
        let b = started();
        fake_fs(&b).put(b.paths().update_file(), RECORD_0_2_0);
        b.update_after_start("0.1.0");
        assert_eq!(b.updated(), None);
        assert_eq!(update_lines(&b), ["update failed to=0.2.0 running=0.1.0"]);
        assert!(!b.sys().fs.exists(&b.paths().update_file()));
    }

    /// Review Focus 4: a record that is not JSON, of another version, or whose `to` is not SemVer
    /// makes no notice and goes; one that cannot be read stays and is said.
    #[test]
    fn a_damaged_update_record_is_logged_and_removed() {
        for text in [
            "{ not json",
            r#"{"version":2,"from":"0.1.0","to":"0.2.0","notesUrl":""}"#,
            r#"{"version":1,"from":"0.1.0","to":"../x","notesUrl":""}"#,
        ] {
            let b = started();
            fake_fs(&b).put(b.paths().update_file(), text);
            b.update_after_start("0.2.0");
            assert_eq!(b.updated(), None, "{text}");
            assert_eq!(update_lines(&b), ["update record=damaged"], "{text}");
            assert!(!b.sys().fs.exists(&b.paths().update_file()), "{text}");
        }
        let b = started();
        fake_fs(&b).put(b.paths().update_file(), "{}");
        fake_fs(&b).fail_reads_of(b.paths().update_file());
        b.update_after_start("0.2.0");
        assert_eq!(update_lines(&b), ["update record=unreadable"]);
        assert!(b.sys().fs.exists(&b.paths().update_file()));
    }

    /// Review Focus 1: the file in updates\ is compared with the hash of the verified bytes before
    /// it is started; one that changed is deleted and never started, and "Update now" downloads again.
    #[test]
    fn a_download_that_changed_on_disk_is_not_installed() {
        let b = started();
        b.update_after_start("0.1.0");
        let fake = FakeUpdater::offering("0.2.0");
        assert_eq!(
            b.update_check_and_download(&fake),
            UpdateState::Ready {
                version: "0.2.0".into()
            }
        );
        let file = b.paths().updates_dir().join("cmdstudio-0.2.0-setup.exe");
        fake_fs(&b).put(&file, b"MZ-something-else");
        let spawned = RefCell::new(Vec::new());
        let spawn = |f: &Path, relaunch: bool| {
            spawned.borrow_mut().push((f.to_path_buf(), relaunch));
            Ok(())
        };
        b.update_at_exit(&spawn);
        assert!(spawned.borrow().is_empty());
        assert_eq!(
            update_lines(&b).last().map(String::as_str),
            Some("update install=file-changed")
        );
        assert!(!b.sys().fs.exists(&file));
        assert_eq!(record(&b), None);
        b.update_install(&fake, &spawn).unwrap();
        assert_eq!(fake.calls(), ["check", "download 0.2.0", "download 0.2.0"]);
        assert_eq!(*spawned.borrow(), [(file, true)]);
    }

    /// A download that cannot be written is said and leaves the state idle; a spawn that fails
    /// at close takes the record back; a "now" spawn that fails is an error the window sees, with
    /// the record taken back, the run record written again (Q16) and the state as before.
    #[test]
    fn a_download_or_a_start_that_fails_is_said_and_leaves_no_record() {
        let b = started();
        b.update_after_start("0.1.0");
        let file = b.paths().updates_dir().join("cmdstudio-0.2.0-setup.exe");
        fake_fs(&b).fail_writes_to(&file);
        assert_eq!(
            b.update_check_and_download(&FakeUpdater::offering("0.2.0")),
            UpdateState::Idle
        );
        assert_eq!(
            update_lines(&b),
            ["update available=0.2.0", "update download=write"]
        );

        let b = started();
        b.update_after_start("0.1.0");
        let fake = FakeUpdater::offering("0.2.0");
        b.update_check_and_download(&fake);
        b.update_at_exit(&|_, _| Err("access denied".into()));
        assert_eq!(
            update_lines(&b).last().map(String::as_str),
            Some("update install=spawn")
        );
        assert_eq!(record(&b), None);

        let b = started();
        b.update_after_start("0.1.0");
        let fake = FakeUpdater::offering("0.2.0");
        b.update_check_and_download(&fake);
        let e = b
            .update_install(&fake, &|_, _| Err("ShellExecuteW answered 5".into()))
            .unwrap_err();
        assert_eq!(e.code, "E_INTERNAL");
        assert_eq!(
            update_lines(&b).last().map(String::as_str),
            Some("update install=spawn")
        );
        assert_eq!(record(&b), None);
        assert!(
            b.sys().fs.exists(&b.paths().run_file()),
            "the run goes on, its record back"
        );
        assert_eq!(
            b.update_status().unwrap().state,
            UpdateState::Ready {
                version: "0.2.0".into()
            }
        );
    }

    /// Q8: an instance whose start found an earlier run still running (single-instance ends it a
    /// moment later) reads no record, empties nothing, checks nothing and installs nothing.
    #[test]
    fn a_second_instance_reads_checks_and_installs_nothing() {
        let b = backend();
        fake_host(&b).processes.borrow_mut().insert(9999, 5);
        fake_fs(&b).put(
            b.paths().run_file(),
            br#"{"version":1,"pid":9999,"createdAt":5,"startedAt":1791694000000,"uiReadyAt":1791694100000,"failedStarts":0,"safeMode":false}"#,
        );
        b.start_run(GuiStart::Normal);
        assert!(!b.run_is_ours());
        let leftover = b.paths().updates_dir().join("cmdstudio-0.2.0-setup.exe");
        fake_fs(&b).put(&leftover, b"the first instance's download");
        fake_fs(&b).put(b.paths().update_file(), RECORD_0_2_0);
        b.update_after_start("0.2.0");
        assert_eq!(b.updated(), None);
        assert!(
            b.sys().fs.exists(&leftover),
            "the first instance's download stays"
        );
        assert!(b.sys().fs.exists(&b.paths().update_file()));
        let fake = FakeUpdater::offering("0.3.0");
        assert_eq!(b.update_check_and_download(&fake), UpdateState::Idle);
        assert_eq!(fake.calls(), Vec::<String>::new());
        let spawned = RefCell::new(0);
        b.update_at_exit(&|_, _| {
            *spawned.borrow_mut() += 1;
            Ok(())
        });
        assert_eq!(spawned.into_inner(), 0);
        assert_eq!(update_lines(&b), Vec::<String>::new());
    }

    /// Q9: the exit `app_restart` asks for installs nothing (its child is starting); the update
    /// waits for the next normal close.
    #[test]
    fn a_restart_exit_installs_nothing() {
        let b = started();
        b.update_after_start("0.1.0");
        let fake = FakeUpdater::offering("0.2.0");
        assert_eq!(
            b.update_check_and_download(&fake),
            UpdateState::Ready {
                version: "0.2.0".into()
            }
        );
        b.mark_restarting();
        let spawned = RefCell::new(0);
        b.update_at_exit(&|_, _| {
            *spawned.borrow_mut() += 1;
            Ok(())
        });
        assert_eq!(spawned.into_inner(), 0);
        assert_eq!(
            update_lines(&b).last().map(String::as_str),
            Some("update postponed=restart")
        );
        assert_eq!(
            b.update_status().unwrap().state,
            UpdateState::Postponed {
                version: "0.2.0".into(),
                reason: "restart".into()
            }
        );
        assert_eq!(record(&b), None);
    }

    /// The check runs once per run: a second app_ready (a reload) asks the updater nothing more.
    #[test]
    fn the_check_runs_once_per_run() {
        let b = started();
        let fake = FakeUpdater::offering("0.2.0");
        assert_eq!(
            b.update_check_and_download(&fake),
            UpdateState::Ready {
                version: "0.2.0".into()
            }
        );
        assert_eq!(
            b.update_check_and_download(&fake),
            UpdateState::Ready {
                version: "0.2.0".into()
            }
        );
        assert_eq!(fake.calls(), ["check", "download 0.2.0"]);
    }

    /// Review I1 (one install per run): a second "Update now" while the first one's download
    /// runs is refused (E_BUSY, one log line, nothing downloaded or started); after the first one
    /// started the installer, another click is refused too, and the exit installs nothing.
    #[test]
    fn a_double_update_now_spawns_once() {
        let b = started();
        b.update_after_start("0.1.0");
        set_mode(&b, UpdateMode::Notify);
        assert_eq!(
            b.update_check_and_download(&FakeUpdater::offering("0.2.0")),
            UpdateState::Available {
                version: "0.2.0".into()
            }
        );
        let spawned = RefCell::new(Vec::new());
        let spawn = |f: &Path, relaunch: bool| {
            spawned.borrow_mut().push((f.to_path_buf(), relaunch));
            Ok(())
        };
        let second = RefCell::new(None);
        let plain = FakeUpdater::offering("0.2.0");
        let first = FakeUpdater::offering("0.2.0").during_download(|| {
            *second.borrow_mut() = Some(b.update_install(&plain, &spawn).map_err(|e| e.code));
        });
        b.update_install(&first, &spawn).unwrap();
        assert_eq!(*second.borrow(), Some(Err("E_BUSY".to_string())));
        assert_eq!(
            plain.calls(),
            Vec::<String>::new(),
            "the second click downloads nothing"
        );
        let file = b.paths().updates_dir().join("cmdstudio-0.2.0-setup.exe");
        assert_eq!(*spawned.borrow(), [(file.clone(), true)]);
        assert_eq!(
            b.update_install(&plain, &spawn).unwrap_err().code,
            "E_BUSY",
            "an installer was started: no second one"
        );
        b.update_at_exit(&spawn);
        assert_eq!(
            *spawned.borrow(),
            [(file, true)],
            "the exit installs nothing"
        );
        let lines = update_lines(&b);
        assert_eq!(
            lines
                .iter()
                .filter(|l| *l == "update install=already")
                .count(),
            2
        );
        assert_eq!(
            lines
                .iter()
                .filter(|l| l.starts_with("update installing="))
                .count(),
            1
        );
        assert_eq!(
            b.update_status().unwrap().state,
            UpdateState::Installing {
                version: "0.2.0".into()
            }
        );
    }

    /// Review I1, automatic mode: "Update now" (the Recovery page) while the background download
    /// runs is refused and starts nothing; once the download is ready, "Update now" uses it and
    /// starts the installer once, and the exit that follows installs nothing more.
    #[test]
    fn update_now_during_the_background_download_spawns_once() {
        let b = started();
        b.update_after_start("0.1.0");
        let spawned = RefCell::new(Vec::new());
        let spawn = |f: &Path, relaunch: bool| {
            spawned.borrow_mut().push((f.to_path_buf(), relaunch));
            Ok(())
        };
        let during = RefCell::new(None);
        let now = FakeUpdater::offering("0.2.0");
        let background = FakeUpdater::offering("0.2.0").during_download(|| {
            *during.borrow_mut() = Some(b.update_install(&now, &spawn).map_err(|e| e.code));
        });
        assert_eq!(
            b.update_check_and_download(&background),
            UpdateState::Ready {
                version: "0.2.0".into()
            }
        );
        assert_eq!(*during.borrow(), Some(Err("E_BUSY".to_string())));
        assert!(
            spawned.borrow().is_empty(),
            "nothing started during the download"
        );
        b.update_install(&now, &spawn).unwrap();
        assert_eq!(
            now.calls(),
            Vec::<String>::new(),
            "the background download is used"
        );
        let file = b.paths().updates_dir().join("cmdstudio-0.2.0-setup.exe");
        assert_eq!(*spawned.borrow(), [(file.clone(), true)]);
        b.update_at_exit(&spawn);
        assert_eq!(
            *spawned.borrow(),
            [(file, true)],
            "the exit installs nothing"
        );
        assert_eq!(
            b.update_status().unwrap().state,
            UpdateState::Installing {
                version: "0.2.0".into()
            }
        );
    }

    /// Review Minor 3: a download that hangs ends at the updater's timeout as an offline error:
    /// one log line, `Idle`, nothing written.
    #[test]
    fn a_download_that_times_out_is_one_line_and_idle() {
        let b = started();
        b.update_after_start("0.1.0");
        let fake = FakeUpdater::offering("0.2.0");
        *fake.bytes.borrow_mut() = Err(UpdateError::Offline("operation timed out".into()));
        assert_eq!(b.update_check_and_download(&fake), UpdateState::Idle);
        assert_eq!(
            update_lines(&b),
            ["update available=0.2.0", "update download=offline"]
        );
        assert!(!b.sys().fs.exists(&b.paths().updates_dir()));
    }
}
