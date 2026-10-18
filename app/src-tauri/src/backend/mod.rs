//! The app's commands behind the IPC (spec §5.7), as a [`Backend`] over a machine: the IPC layer
//! (`ipc.rs`) only moves arguments and results across, so every command is tested here against
//! the core's in-memory adapters, never against this computer. One `pub` method per command,
//! spread over the modules by subject: `system`, the theme library (`crate::library`), `preview`,
//! `apply` (plan, apply, undo, restore points, revert), `doctor`, `diagnostics`, `launch` (which
//! window "Open cmd" opens), and the settings and the Advanced page here.

use crate::backend::preview::SampleIdentity;
use crate::error::AppError;
use crate::logs::Logger;
use crate::settings::{self, AppSettings, SettingsLoad};
use cmdstudio_core::actions::{Ctx, Handled, StartReport};
use cmdstudio_core::adapters::System;
use cmdstudio_core::model::Theme;
use cmdstudio_core::plan::{Paths, CLINK_VERSION, OMP_VERSION};
use cmdstudio_core::resources::{Resources, NERD_FONTS_VERSION};
use cmdstudio_core::snapshot::Unrestored;
use cmdstudio_core::txn::Outcome;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};
use std::time::Instant;
use ts_rs::TS;

pub mod apply;
pub mod diagnostics;
pub mod doctor;
pub mod launch;
pub mod preview;
pub mod recovery;
pub mod system;

/// What the backend runs on: the real adapters (`crate::machine::RealMachine`) or the core's
/// `FakeMachine` in tests.
pub trait Machine {
    fn system(&self) -> System<'_>;
}

/// A theme read from a file the user picked, waiting for `theme_import_confirm` (spec §3.1):
/// the file is read once, at the preview, and the token stands for what was read.
pub(crate) struct PendingImport {
    pub(crate) token: String,
    pub(crate) theme: Theme,
}

/// What the backend holds of `settings.json` once it has read it: the settings, and the name of
/// the damaged file that was set aside to get them, until the Settings page has been told.
struct SettingsCache {
    settings: AppSettings,
    reset_from: Option<String>,
}

pub struct Backend<M> {
    machine: M,
    paths: Paths,
    resources: Resources,
    now: Box<dyn Fn() -> u64 + Send + Sync>,
    /// The one logger of the backend: its lock is what keeps two commands' lines from dropping
    /// each other, so a second `Logger` over the same files would not be covered by it.
    log: Logger,
    /// `settings.json`, read once by whichever settings method comes first (`None` until then).
    /// `settings::load` sets a damaged file aside the first time it sees it, and a second load
    /// would find no file and say nothing, so the first load is kept here with the notice it
    /// owes the UI. Held for the whole of a load or a save, so two commands never race on the
    /// rename.
    settings: Mutex<Option<SettingsCache>>,
    pub(crate) pending_import: Mutex<Option<PendingImport>>,
    /// Held for the whole of `theme_save`, `theme_duplicate`, `theme_delete` and
    /// `theme_import_confirm` (`crate::library`), so two commands that write the library never
    /// pick the same file name for two new themes. The IPC commands run on several threads
    /// over this one backend. It guards files, not data in memory, so a lock a panicking thread
    /// left is used as it is. Lock order: this one first, then `pending_import`, never the
    /// other way round; nothing under it takes `settings`.
    pub(crate) library_lock: Mutex<()>,
    /// The user, computer and home the preview's sample starts from, read from the environment
    /// once by `machine::real_backend` and handed to the UI by `system_detect`. The user name
    /// in it is the user's own: it goes to the UI and nowhere else, and is never logged; the
    /// diagnostics read it only to redact it.
    sample: SampleIdentity,
    /// Where the start's check of Windows' crash reports leaves its answer for
    /// `recovery_status` (recovery spec §4.3).
    crash: recovery::CrashSlot,
}

impl<M: Machine> Backend<M> {
    /// `data` is the data folder (spec §5.3), `resources` the bundled files, `now` the clock in
    /// milliseconds since 1970 (restore point ids and the log read it), `sample` who and where
    /// the preview's sample is (`SampleIdentity::from_env`).
    pub fn new(
        machine: M,
        data: PathBuf,
        resources: PathBuf,
        now: Box<dyn Fn() -> u64 + Send + Sync>,
        sample: SampleIdentity,
    ) -> Self {
        let paths = Paths { data };
        let log = Logger::new(paths.logs());
        Backend {
            machine,
            paths,
            resources: Resources { root: resources },
            now,
            log,
            settings: Mutex::new(None),
            pending_import: Mutex::new(None),
            library_lock: Mutex::new(()),
            sample,
            crash: recovery::CrashSlot::new(),
        }
    }

    pub fn paths(&self) -> &Paths {
        &self.paths
    }

    pub(crate) fn sys(&self) -> System<'_> {
        self.machine.system()
    }

    pub(crate) fn now_ms(&self) -> u64 {
        (self.now)()
    }

    /// The core's context for one command: the paths, the bundled files and the clock now.
    pub(crate) fn ctx(&self) -> Ctx {
        Ctx {
            paths: self.paths.clone(),
            resources: self.resources.clone(),
            now_ms: self.now_ms(),
        }
    }

    /// Runs one command and writes one log line: its name, how long it took, and `ok` or the
    /// code it failed with. Nothing else of the command goes in: not its arguments, not an
    /// error's message or `details` (they can hold the user's name in a path), which is why the
    /// name is a `&'static str` of the command table and not text built at run time. A log that
    /// cannot be written never stops the command.
    pub fn logged<T>(
        &self,
        name: &'static str,
        f: impl FnOnce(&Self) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let started = Instant::now();
        let result = f(self);
        let ms = started.elapsed().as_millis();
        let ended = match &result {
            Ok(_) => "ok".to_string(),
            Err(e) => format!("err={}", e.code),
        };
        let _ = self.log.write(
            self.sys().fs,
            self.now_ms(),
            &format!("cmd={name} ms={ms} {ended}"),
        );
        result
    }

    /// Writes one line of the app's own, not a command's, to the log: the start's crash check, a
    /// pause (recovery spec §4.3). As with `logged`, names and codes only, never the user's data;
    /// a line that cannot be written is dropped.
    pub(crate) fn note_line(&self, line: &str) {
        let _ = self.log.write(self.sys().fs, self.now_ms(), line);
    }

    /// The last lines of the log, for the Advanced page and the diagnostics.
    pub(crate) fn log_tail(&self, lines: usize) -> String {
        self.log.tail(self.sys().fs, lines)
    }

    /// Runs `f` on the settings the backend holds, reading `settings.json` first if no command
    /// has yet. A read that fails is not kept, so the next command tries again: the file was
    /// not read, or could not be set aside and is still where it was, so nothing is lost by
    /// trying. A lock a panicking thread left is used as it is: the settings and the notice are
    /// only ever replaced whole.
    fn with_settings<T>(
        &self,
        f: impl FnOnce(&mut SettingsCache) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let mut held = self.settings.lock().unwrap_or_else(PoisonError::into_inner);
        let cache = match &mut *held {
            Some(cache) => cache,
            empty => {
                let loaded =
                    settings::load(self.sys().fs, &self.paths.settings_file(), self.now_ms())?;
                empty.insert(SettingsCache {
                    settings: loaded.settings,
                    reset_from: loaded.reset_from,
                })
            }
        };
        f(cache)
    }

    /// The settings, and, once, the name of the damaged file that was set aside to get them
    /// (spec §2.1: no silent change). Whichever command reached the file first read it; this
    /// is the one that hands the notice to the UI: the notice is taken out of the cache inside
    /// the closure, under the lock, and is in the value this call returns. A second call finds
    /// none, so the UI is told once.
    pub fn settings_get(&self) -> Result<SettingsLoad, AppError> {
        self.with_settings(|cache| {
            Ok(SettingsLoad {
                settings: cache.settings.clone(),
                reset_from: cache.reset_from.take(),
            })
        })
    }

    /// Saves the settings and returns them as saved. A damaged file is set aside and told
    /// (`settings_get`) before it is written over, never replaced unseen; when it cannot be
    /// read or set aside, nothing is written and that is the error.
    pub fn settings_set(&self, settings: AppSettings) -> Result<AppSettings, AppError> {
        self.with_settings(|cache| {
            settings::save(self.sys().fs, &self.paths.settings_file(), &settings)?;
            cache.settings = settings.clone();
            Ok(settings)
        })
    }

    /// The settings a command works from when it is not told otherwise; the defaults when the
    /// file cannot be used (the Settings page shows that problem itself). Never uses up the
    /// notice of a reset that `settings_get` has yet to hand to the UI.
    pub(crate) fn settings_or_default(&self) -> AppSettings {
        self.with_settings(|cache| Ok(cache.settings.clone()))
            .unwrap_or_default()
    }

    /// The Advanced page (spec §4.3): the components' versions, the data folder, the log's
    /// last lines and the generated files, read only.
    pub fn advanced_read(&self) -> Result<AdvancedInfo, AppError> {
        let fs = self.sys().fs;
        let mut generated = Vec::new();
        let files = [
            self.paths.omp_config(),
            self.paths.generated().join("init.lua"),
            self.paths.generated().join("banner.lua"),
            self.paths.clink_profile().join("clink_settings"),
        ];
        for path in files {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let text = match fs.read(&path) {
                Ok(Some(bytes)) => String::from_utf8_lossy(&bytes).into_owned(),
                Ok(None) => continue,
                Err(e) => format!("(could not be read: {e})"),
            };
            generated.push(GeneratedFile { name, text });
        }
        Ok(AdvancedInfo {
            data_dir: self.paths.data.display().to_string(),
            log_tail: self.log_tail(200),
            generated,
            versions: versions(),
        })
    }
}

/// A line the UI shows about what a command found when it started (spec §5.5): a crash rolled
/// back, a restore point that cannot be read, a record of a rollback that did not finish, a
/// problem the start met. Made as an [`AppError`] is: the Thai and English of the code's row of
/// spec §5.9, and the specific reason (which restore point, what failed) in the core's English
/// as `details`, so a Thai UI shows as much as an English one. Its own type, not an error: what
/// it tells did not stop the command.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Note {
    pub code: String,
    pub message_th: String,
    pub message_en: String,
    /// The specific reason, in the core's English words.
    pub details: String,
}

impl From<AppError> for Note {
    fn from(e: AppError) -> Self {
        Note {
            code: e.code,
            message_th: e.message_th,
            message_en: e.message_en,
            details: e.details,
        }
    }
}

/// A note for `code` with the core's English words as its details, as [`AppError::coded`] makes
/// an error: a code that is not a row of spec §5.9 becomes `E_INTERNAL`, with the code it was
/// given in front of the details.
pub fn note(code: &str, details: impl Into<String>) -> Note {
    AppError::coded(code, details).into()
}

/// What a start found and did, as notes whose details say it in the words of the CLI's
/// `Say::start` and `Say::unreadable`.
pub fn start_notes(start: &StartReport) -> Vec<Note> {
    let mut notes = Vec::new();
    match &start.recovered {
        Some(Outcome::RolledBack { code, reason }) => {
            notes.push(note(
                code,
                format!("{reason}; the machine is as it was before it."),
            ));
        }
        Some(Outcome::RollbackIncomplete { code, reason, .. }) => {
            notes.push(note(code, format!("{reason}.")));
        }
        Some(Outcome::Committed) | None => {}
    }
    for u in &start.unreadable {
        let (id, code, why) = (&u.point.id, u.point.failure.code, &u.point.failure.message);
        let en = match &u.handled {
            Handled::SetAside => format!(
                "restore point {id} could not be used ({why}). It was set aside, its files kept \
                 for a look, and an undo no longer goes back to it."
            ),
            Handled::LeftForLater => format!(
                "restore point {id} can't be read right now ({why}). It was left as it is, and \
                 an undo does not go past it until it can be read."
            ),
            Handled::SetAsideFailed(f) => format!(
                "restore point {id} could not be used ({why}), and setting it aside failed ({}: \
                 {}). An undo does not go past it.",
                f.code, f.message
            ),
        };
        notes.push(note(code, en));
    }
    for problem in &start.problems {
        notes.push(note(problem.code, problem.message.clone()));
    }
    if let Some(record) = &start.incomplete {
        notes.push(note(
            "E_ROLLBACK_INCOMPLETE",
            format!(
                "the rollback of restore point {} did not finish: {} ({})",
                record.id, record.reason, record.code
            ),
        ));
    }
    notes
}

/// The steps a rollback could not put back, from the record a start found (spec §5.5), each
/// with how to fix it by hand in both languages.
pub fn start_unrestored(start: &StartReport) -> Vec<Unrestored> {
    start
        .incomplete
        .as_ref()
        .map(|record| record.steps.clone())
        .unwrap_or_default()
}

/// The components' versions, for the Advanced page and the diagnostics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Versions {
    pub app: String,
    pub oh_my_posh: String,
    pub clink: String,
    pub nerd_fonts: String,
}

pub fn versions() -> Versions {
    Versions {
        app: env!("CARGO_PKG_VERSION").to_string(),
        oh_my_posh: OMP_VERSION.to_string(),
        clink: CLINK_VERSION.to_string(),
        nerd_fonts: NERD_FONTS_VERSION.to_string(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GeneratedFile {
    pub name: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AdvancedInfo {
    pub data_dir: String,
    pub log_tail: String,
    pub generated: Vec<GeneratedFile>,
    pub versions: Versions,
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use crate::backend::apply::{ApplyOutcome, ApplyProgress, ApplyRequest, PlanOutcome};
    use cmdstudio_core::adapters::fake::{FakeEventLog, FakeFonts, FakeFs, FakeMachine};
    use cmdstudio_core::adapters::{Fs, WtLocation, WtVariant};
    use cmdstudio_core::model::Target;
    use cmdstudio_core::plan::NERD_FONT_FAMILY;
    use cmdstudio_core::resources::{fake_resources, NERD_FONT_FILE};
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    impl Machine for FakeMachine {
        fn system(&self) -> System<'_> {
            FakeMachine::system(self)
        }
    }

    pub(crate) const WT: &str = r"C:\Users\tester\AppData\Local\Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json";
    pub(crate) const DATA: &str = r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio";

    /// A clock the test moves: every read is one millisecond later than the one before, so
    /// restore point ids never collide.
    pub(crate) fn clock() -> (Arc<AtomicU64>, Box<dyn Fn() -> u64 + Send + Sync>) {
        let now = Arc::new(AtomicU64::new(1_791_694_709_000));
        let shared = now.clone();
        (now, Box::new(move || shared.fetch_add(1, Ordering::SeqCst)))
    }

    /// The identity every test backend is built with. It is not the core's sample, so a test
    /// can tell the backend's identity from a default.
    pub(crate) fn sample_identity() -> SampleIdentity {
        SampleIdentity {
            user: "tester".to_string(),
            host: "TESTPC".to_string(),
            home: r"C:\Users\tester".to_string(),
        }
    }

    /// A machine with Windows Terminal, Oh My Posh and Clink answering like the real ones, and
    /// the bundled files; the backend over it.
    pub(crate) fn backend() -> Backend<FakeMachine> {
        backend_on(|m| m)
    }

    /// [`backend`] over a machine of the test's own, made from that fake machine (one adapter
    /// replaced, say).
    pub(crate) fn backend_on<M: Machine>(machine: impl FnOnce(FakeMachine) -> M) -> Backend<M> {
        let mut m = FakeMachine::new();
        m.fs.create_dir_all(Path::new(
            r"C:\Users\tester\AppData\Local\Microsoft\Windows",
        ))
        .unwrap();
        m.fs.put(
            WT,
            include_str!("../../../../crates/core/tests/fixtures/wt/default.json"),
        );
        m.wt.locations = vec![WtLocation {
            variant: WtVariant::Store,
            installed: true,
            settings: PathBuf::from(WT),
        }];
        m.fonts = FakeFonts::new(
            r"C:\Users\tester\AppData\Local\Microsoft\Windows\Fonts",
            &["Arial", "Cascadia Mono", "Consolas"],
        );
        m.fonts.knows_file(NERD_FONT_FILE, NERD_FONT_FAMILY);
        m.process.answer("oh-my-posh.exe", "print", 0, "> ");
        m.process.answer("oh-my-posh.exe", "version", 0, "31.5.0");
        let resources = fake_resources(&m.fs, Path::new(r"C:\Program Files\CMD Studio\resources"));
        let (_, now) = clock();
        Backend::new(
            machine(m),
            PathBuf::from(DATA),
            resources.root,
            now,
            sample_identity(),
        )
    }

    /// The fake file system under the backend, for a test to make a file fail to read, write
    /// or be removed. The machine is private to this module, so a test of another module goes
    /// through here.
    pub(crate) fn fake_fs(b: &Backend<FakeMachine>) -> &FakeFs {
        &b.machine.fs
    }

    /// The fake crash reports under the backend (see [`fake_fs`]).
    pub(crate) fn fake_events(b: &Backend<FakeMachine>) -> &FakeEventLog {
        &b.machine.event_log
    }

    /// The id of the Terracotta preset.
    pub(crate) fn terracotta_id() -> uuid::Uuid {
        cmdstudio_core::presets::builtin()
            .unwrap()
            .into_iter()
            .find(|(key, _)| *key == "terracotta")
            .map(|(_, t)| t.id)
            .unwrap()
    }

    /// What the wizard asks for when the user changes nothing: Terracotta in every cmd window,
    /// Windows Terminal's default profile set, another Clink left alone.
    pub(crate) fn terracotta_request() -> ApplyRequest {
        ApplyRequest {
            theme_id: terracotta_id(),
            target: Target::All,
            set_wt_default_profile: true,
            replace_other_clink: false,
        }
    }

    /// Plans `req`, then applies it with the digest of the plan it was shown, as the UI does:
    /// the outcome, and every progress event in the order it was told. The plan must be ready.
    pub(crate) fn plan_and_apply<M: Machine>(
        b: &Backend<M>,
        req: ApplyRequest,
    ) -> (ApplyOutcome, Vec<ApplyProgress>) {
        let plan = match b.plan_apply(req.clone()).unwrap() {
            PlanOutcome::Ready { plan } => plan,
            other => panic!("{other:?}"),
        };
        let mut seen = Vec::new();
        let done = b.apply(req, &plan.digest, &mut |p| seen.push(p)).unwrap();
        (done, seen)
    }

    /// Applies Terracotta as [`terracotta_request`] asks (see [`plan_and_apply`]).
    pub(crate) fn apply_terracotta<M: Machine>(
        b: &Backend<M>,
    ) -> (ApplyOutcome, Vec<ApplyProgress>) {
        plan_and_apply(b, terracotta_request())
    }
}

#[cfg(test)]
mod tests {
    use super::testing::backend;
    use super::*;

    #[test]
    fn a_logged_command_writes_one_line_with_its_name_and_outcome() {
        let b = backend();
        let ok: Result<u8, AppError> = b.logged("themes_list", |_| Ok(7));
        assert_eq!(ok, Ok(7));
        let err: Result<u8, AppError> = b.logged("apply", |_| {
            Err(AppError::coded(
                "E_BUSY",
                r"C:\Users\tester\AppData\Local\x.json is locked",
            ))
        });
        assert_eq!(err.unwrap_err().code, "E_BUSY");
        let log = b.log_tail(10);
        let lines: Vec<&str> = log.lines().collect();
        assert_eq!(lines.len(), 2, "{log}");
        assert!(
            lines[0].contains(" cmd=themes_list ms=") && lines[0].ends_with(" ok"),
            "{log}"
        );
        assert!(
            lines[1].contains(" cmd=apply ms=") && lines[1].ends_with(" err=E_BUSY"),
            "{log}"
        );
        assert!(b.sys().fs.exists(&b.paths().logs().join("cmdstudio.log")));
        // The code only: an error's details can hold a path with the user's name in it.
        assert!(!log.contains("tester") && !log.contains("locked"), "{log}");
    }

    #[test]
    fn settings_default_then_round_trip_through_the_data_folder() {
        let b = backend();
        let loaded = b.settings_get().unwrap();
        assert_eq!(loaded.settings, AppSettings::default());
        assert_eq!(loaded.reset_from, None);
        let mut mine = loaded.settings;
        mine.language = crate::settings::Language::Th;
        mine.wizard_done = true;
        assert_eq!(b.settings_set(mine.clone()).unwrap(), mine);
        assert_eq!(b.settings_get().unwrap().settings, mine);
        assert!(b.sys().fs.exists(&b.paths().settings_file()));
        // What `settings_get` returns is held in memory; the file is what was saved.
        let on_disk = crate::settings::load(b.sys().fs, &b.paths().settings_file(), 0).unwrap();
        assert_eq!(on_disk.settings, mine);
    }

    #[test]
    fn a_reset_is_still_told_when_another_command_read_the_settings_first() {
        let b = backend();
        let file = b.paths().settings_file();
        b.sys().fs.write_atomic(&file, b"{ not json").unwrap();
        // A command that only needs the settings to work from gets the defaults...
        assert_eq!(b.settings_or_default(), AppSettings::default());
        // ...and must not use up the notice: the Settings page is told what was set aside, once.
        let told = b.settings_get().unwrap();
        assert_eq!(told.settings, AppSettings::default());
        let aside = told.reset_from.expect("the reset is told");
        assert!(aside.starts_with("settings.json.damaged-"), "{aside}");
        assert!(b.sys().fs.exists(&file.with_file_name(&aside)));
        assert!(!b.sys().fs.exists(&file));
        let again = b.settings_get().unwrap();
        assert_eq!(again.settings, AppSettings::default());
        assert_eq!(again.reset_from, None);
    }

    #[test]
    fn saving_first_over_a_damaged_file_sets_it_aside_and_still_tells_the_page() {
        let b = backend();
        let file = b.paths().settings_file();
        b.sys().fs.write_atomic(&file, b"{ not json").unwrap();
        let mine = AppSettings {
            language: crate::settings::Language::Th,
            wizard_done: true,
            ..AppSettings::default()
        };
        // The first access is a save: the damaged file must not be written over unseen.
        assert_eq!(b.settings_set(mine.clone()).unwrap(), mine);
        let dir = file.parent().unwrap().to_path_buf();
        let asides: Vec<_> = b
            .sys()
            .fs
            .list_dir(&dir)
            .unwrap()
            .into_iter()
            .filter(|e| {
                e.path
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("settings.json.damaged-"))
            })
            .collect();
        assert_eq!(asides.len(), 1, "{asides:?}");
        let aside = asides[0].path.clone();
        assert_eq!(
            b.sys().fs.read(&aside).unwrap().as_deref(),
            Some(&b"{ not json"[..])
        );
        // The file holds what was saved, and the page is told of the reset, once.
        let told = b.settings_get().unwrap();
        assert_eq!(told.settings, mine);
        assert_eq!(
            told.reset_from.as_deref(),
            aside.file_name().and_then(|n| n.to_str())
        );
        let again = b.settings_get().unwrap();
        assert_eq!(again.settings, mine);
        assert_eq!(again.reset_from, None);
        let on_disk = crate::settings::load(b.sys().fs, &file, 0).unwrap();
        assert_eq!(on_disk.settings, mine);
    }

    #[test]
    fn a_first_read_that_failed_is_tried_again_not_kept() {
        let b = backend();
        let file = b.paths().settings_file();
        let saved = AppSettings {
            language: crate::settings::Language::Th,
            ..AppSettings::default()
        };
        crate::settings::save(b.sys().fs, &file, &saved).unwrap();
        let before = b.machine.fs.file(&file).unwrap();
        b.machine.fs.fail_reads_of(&file);
        // Nothing could be read: the page is told, a save writes nothing, a command that only
        // needs settings to work from gets the defaults, and none of it is remembered.
        assert_eq!(b.settings_get().unwrap_err().code, "E_MACHINE_READ");
        let other = AppSettings {
            wizard_done: true,
            ..AppSettings::default()
        };
        assert_eq!(b.settings_set(other).unwrap_err().code, "E_MACHINE_READ");
        assert_eq!(b.settings_or_default(), AppSettings::default());
        assert_eq!(b.machine.fs.file(&file).unwrap(), before);
        // The file can be read again: the next command reads it, as it really is.
        b.machine.fs.stop_failing(&file);
        let loaded = b.settings_get().unwrap();
        assert_eq!(loaded.settings, saved);
        assert_eq!(loaded.reset_from, None);
    }

    /// A note says what happened in the words of its code's row of spec §5.9, in Thai and in
    /// English alike, and the specific reason (which restore point, what went wrong) in its
    /// details, as an error does: a Thai UI shows as much as an English one.
    #[test]
    fn a_note_has_both_row_texts_and_the_specific_details() {
        use cmdstudio_core::actions::UnreadablePoint;
        use cmdstudio_core::snapshot::{Unreadable, UnreadableKind};
        use cmdstudio_core::txn::Failure;

        let id = "1791694709000-0123abcd";
        let start = StartReport {
            recovered: Some(Outcome::RolledBack {
                code: "E_TXN_INTERRUPTED",
                reason: "an earlier change was cut off at step 3".to_string(),
            }),
            unreadable: vec![UnreadablePoint {
                point: Unreadable {
                    id: id.to_string(),
                    kind: UnreadableKind::Damaged,
                    failure: Failure {
                        code: "E_SNAPSHOT_READ",
                        message: "manifest.json is not JSON".to_string(),
                    },
                },
                handled: Handled::SetAside,
            }],
            ..StartReport::default()
        };
        let notes = start_notes(&start);
        let [crash, point] = &notes[..] else {
            panic!("two notes expected: {notes:?}");
        };
        let row = |code: &str| crate::error::meaning(code).unwrap();
        // The crash recovery: the row's texts, and the reason with what was done about it.
        let (th, en) = row("E_TXN_INTERRUPTED");
        assert_eq!(
            (
                crash.code.as_str(),
                crash.message_th.as_str(),
                crash.message_en.as_str()
            ),
            ("E_TXN_INTERRUPTED", th, en)
        );
        assert_eq!(
            crash.details,
            "an earlier change was cut off at step 3; the machine is as it was before it."
        );
        // The restore point that could not be used: which one, why, and what was done.
        let (th, en) = row("E_SNAPSHOT_READ");
        assert_eq!(
            (
                point.code.as_str(),
                point.message_th.as_str(),
                point.message_en.as_str()
            ),
            ("E_SNAPSHOT_READ", th, en)
        );
        assert!(
            point.details.starts_with(&format!(
                "restore point {id} could not be used (manifest.json is not JSON). It was set aside"
            )),
            "{}",
            point.details
        );
        // The codes are the rows of spec §5.9 and no other, as for an error.
        assert_eq!(
            note("E_NOT_A_ROW", "what happened"),
            Note::from(AppError::coded("E_NOT_A_ROW", "what happened"))
        );
        assert_eq!(note("E_NOT_A_ROW", "x").code, crate::error::INTERNAL);
        // Over the IPC, a note has the fields of an error.
        let v = serde_json::to_value(crash).unwrap();
        let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["code", "details", "messageEn", "messageTh"]);
    }

    /// The backend is shared by the IPC commands' threads (Tauri state): the real machine must
    /// allow it. Nothing is built; the compiler checks the types.
    #[cfg(windows)]
    #[test]
    fn the_real_backend_can_be_shared_between_threads() {
        fn is_send_and_sync<T: Send + Sync>() {}
        is_send_and_sync::<Backend<crate::machine::RealMachine>>();
    }
}
