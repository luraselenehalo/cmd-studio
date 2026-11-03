//! System test (spec §7 integration test): apply → doctor → undo with this
//! machine's real HKCU AutoRun, real per-user fonts and the real bundled
//! Oh My Posh and Clink, against a fake Windows Terminal folder and a temp data
//! folder; then every one of those must be exactly as it was before. It changes
//! the machine, so it runs only when CMDSTUDIO_SYSTEM_TESTS is exactly `1` (CI
//! and the test VMs set it; never set it on a computer you work on). A guard
//! created before the first change puts back what the test changed even when an
//! assert fails half way, so a failing run does not leave AutoRun or fonts changed.
#![cfg(windows)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use cmdstudio_cli::{now_ms, run_with, Defaults, Io, EXIT_OK};
use cmdstudio_core::adapters::windows::{
    eventlog::WinEventLog, fonts::WinFonts, fs::WinFs, host::WinHost, process::WinProcess,
    registry::WinAutoRun, wt::WinWt,
};
use cmdstudio_core::adapters::{program_data_dir, AdapterError, Fonts, Host, RegValue, System};
use cmdstudio_core::exec::font_target;
use cmdstudio_core::plan::{Paths, CLINK_VERSION, NERD_FONT_FAMILY};
use cmdstudio_core::resources::Resources;
use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

const TEST: &str = "system_apply_doctor_undo_is_byte_identical";

/// The gate: only the exact value `1` runs the test (`0`, `true` or an empty value skip it).
fn enabled() -> bool {
    if std::env::var("CMDSTUDIO_SYSTEM_TESTS").as_deref() == Ok("1") {
        return true;
    }
    println!("SKIPPED {TEST}: set CMDSTUDIO_SYSTEM_TESTS=1");
    false
}

/// A path in this repository (the folder two levels above this crate).
fn repo(rel: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    root.join(rel)
}

/// What one `cmdstudio` command printed and returned.
struct Run {
    code: i32,
    out: String,
    err: String,
}

impl Run {
    fn log(&self) -> String {
        format!(
            "exit code {}\n--- stdout ---\n{}\n--- stderr ---\n{}",
            self.code, self.out, self.err
        )
    }
}

/// Runs one command of the CLI. The data and resources folders are on every command line as
/// well as in `Defaults`, so no command here can reach the real data folder of this computer.
fn cli(sys: System<'_>, d: &Defaults, args: &[&OsStr]) -> Run {
    let mut all: Vec<OsString> = args.iter().map(|a| a.to_os_string()).collect();
    all.extend([
        OsString::from("--data-dir"),
        d.data_dir.clone().into_os_string(),
        OsString::from("--resources"),
        d.resources.clone().into_os_string(),
    ]);
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = run_with(
        &all,
        &mut Io {
            out: &mut out,
            err: &mut err,
        },
        sys,
        d,
    );
    Run {
        code,
        out: String::from_utf8_lossy(&out).into_owned(),
        err: String::from_utf8_lossy(&err).into_owned(),
    }
}

/// A check of `doctor --json` for a person to read: not the JSON, which escapes every backslash.
fn show_check(c: &serde_json::Value) -> String {
    let text = |key: &str| c[key].as_str().unwrap_or_default().to_string();
    format!(
        "{} {}: {}\n{}",
        text("id"),
        text("status"),
        text("messageEn"),
        text("details")
    )
}

/// Where the test looks, apart from what the adapters know.
struct Places {
    /// The fake Windows Terminal's settings.json.
    wt_settings: PathBuf,
    /// The folder of the per-user fonts, and the bundled Nerd Font's file and registration
    /// value in them.
    font_dir: PathBuf,
    font_file: PathBuf,
    font_value: String,
    /// `%ProgramData%\ai.resincore.cmdstudio-<SID>`, where Clink goes when the data folder's
    /// path is not all ASCII and has no 8.3 name (G19).
    program_data: PathBuf,
    /// Clink's runtime folder inside it, `clink-<version>`.
    program_data_clink: PathBuf,
}

/// Everything the test changes or looks at, as it is now.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Snapshot {
    /// The user's `Command Processor` key, and its AutoRun value as stored (type and bytes).
    autorun_key: bool,
    autorun: Option<RegValue>,
    /// Windows Terminal's settings.json, byte for byte (`None` when it is not there).
    wt_settings: Option<Vec<u8>>,
    /// The font families GDI lists.
    families: Vec<String>,
    /// The Nerd Font's registration value (the file's path), whether the per-user font key and
    /// the per-user fonts folder exist, and whether the font file is in that folder.
    font_registration: Option<String>,
    font_key: bool,
    font_dir: bool,
    font_file: bool,
    /// Whether the user's own ProgramData folder exists, and Clink's runtime folder in it.
    program_data: bool,
    program_data_clink: bool,
}

/// Reads the machine. An `Err` is a read that failed, said in words.
fn snapshot(sys: System<'_>, at: &Places) -> Result<Snapshot, String> {
    let read = |what: &str, e: AdapterError| format!("could not read {what}: {e}");
    Ok(Snapshot {
        autorun_key: sys
            .autorun
            .user_key_exists()
            .map_err(|e| read("the Command Processor key", e))?,
        autorun: sys.autorun.read_user().map_err(|e| read("AutoRun", e))?,
        wt_settings: sys
            .fs
            .read(&at.wt_settings)
            .map_err(|e| read("settings.json", e))?,
        families: sys.fonts.families().map_err(|e| read("the font list", e))?,
        font_registration: sys
            .fonts
            .registration(&at.font_value)
            .map_err(|e| read("the font registration", e))?,
        font_key: sys
            .fonts
            .registration_key_exists()
            .map_err(|e| read("the font key", e))?,
        font_dir: sys.fs.is_dir(&at.font_dir),
        font_file: sys.fs.exists(&at.font_file),
        program_data: sys.fs.is_dir(&at.program_data),
        program_data_clink: sys.fs.is_dir(&at.program_data_clink),
    })
}

fn show_autorun(v: &Option<RegValue>) -> String {
    match v {
        None => "no AutoRun value".to_string(),
        Some(v) => match v.text() {
            Some(text) => format!("type {}, {text}", v.kind),
            None => format!("type {}, {} bytes that are not text", v.kind, v.bytes.len()),
        },
    }
}

fn show_font(v: &Option<String>) -> &str {
    v.as_deref().unwrap_or("no registration")
}

/// Font family names for a person to read: each as written, not `{:?}`, which escapes the
/// combining marks of Thai names.
fn show_names(names: &[&String]) -> String {
    if names.is_empty() {
        return "(none)".to_string();
    }
    names
        .iter()
        .map(|n| n.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

impl Snapshot {
    /// Panics, naming what differs, unless what `undo` itself puts back is as `before` found
    /// it. The font file and the fonts folder are left out: Windows may still hold the file
    /// when `undo` ends, and `assert_font_file_same_as` checks them after the retries.
    fn assert_same_as(&self, before: &Snapshot) {
        assert!(
            self.autorun_key == before.autorun_key && self.autorun == before.autorun,
            "AutoRun is not back: before it was (key {}) {}, now it is (key {}) {}",
            before.autorun_key,
            show_autorun(&before.autorun),
            self.autorun_key,
            show_autorun(&self.autorun)
        );
        assert!(
            self.wt_settings == before.wt_settings,
            "settings.json is not byte-identical"
        );
        let added: Vec<_> = self
            .families
            .iter()
            .filter(|f| !before.families.contains(f))
            .collect();
        let removed: Vec<_> = before
            .families
            .iter()
            .filter(|f| !self.families.contains(f))
            .collect();
        assert!(
            added.is_empty() && removed.is_empty(),
            "the font list changed: added: {}; removed: {}",
            show_names(&added),
            show_names(&removed)
        );
        assert!(
            self.font_registration == before.font_registration,
            "the font registration is not back: before: {}, now: {}",
            show_font(&before.font_registration),
            show_font(&self.font_registration)
        );
        for (what, now, was) in [
            ("the per-user font key", self.font_key, before.font_key),
            (
                "the user's ProgramData folder",
                self.program_data,
                before.program_data,
            ),
            (
                "Clink's runtime folder in the user's ProgramData folder",
                self.program_data_clink,
                before.program_data_clink,
            ),
        ] {
            assert!(
                now == was,
                "{what} is not back: before it {}, now it {}",
                if was { "existed" } else { "did not exist" },
                if now { "exists" } else { "does not exist" }
            );
        }
    }

    /// Panics unless the font file is as `before` found it. `retries` is how many times `undo`
    /// was run again to delete a file Windows held.
    fn assert_font_file_same_as(&self, before: &Snapshot, retries: u32) {
        assert!(
            !(self.font_file && !before.font_file),
            "the font file is still there after undo, and still held after {retries} retries of \
             the pending delete (undo run again each time)"
        );
        assert!(
            self.font_file == before.font_file,
            "the font file was there before the test and is gone after undo"
        );
    }

    /// Panics unless the per-user fonts folder exists, or not, as it did before.
    fn assert_font_dir_same_as(&self, before: &Snapshot) {
        assert!(
            self.font_dir == before.font_dir,
            "the per-user fonts folder is not back: before it {}, now it {}",
            if before.font_dir {
                "existed"
            } else {
                "did not exist"
            },
            if self.font_dir {
                "exists"
            } else {
                "does not exist"
            }
        );
    }
}

/// Puts back, when it is dropped, what the test changed on the real machine: AutoRun, the
/// bundled Nerd Font and the user's ProgramData folder. It is created before the first change
/// and compares with `before`, so after a run that came back to `before` it does nothing. Windows
/// Terminal's settings and the data folder are in the test's temp folder, which goes away on
/// its own. It never panics: it may run while another panic unwinds, so a step that fails
/// is written to stderr and the rest still run.
struct Restore<'a> {
    sys: System<'a>,
    at: &'a Places,
    before: &'a Snapshot,
}

impl Restore<'_> {
    /// What could not be put back, in words.
    fn put_back(&self) -> Vec<String> {
        let (sys, at, before) = (self.sys, self.at, self.before);
        if snapshot(sys, at).is_ok_and(|now| now == *before) {
            return Vec::new();
        }
        let mut problems = Vec::new();
        let mut step = |what: &str, result: Result<(), AdapterError>| {
            if let Err(e) = result {
                problems.push(format!("could not {what}: {e}"));
            }
        };
        // AutoRun: the value as it was (or none), and the key only when the test made it.
        if !sys
            .autorun
            .read_user()
            .is_ok_and(|now| now == before.autorun)
        {
            match &before.autorun {
                Some(value) => step(
                    "write the old AutoRun value back",
                    sys.autorun.write_user(value),
                ),
                None => step("delete the AutoRun value", sys.autorun.delete_user()),
            }
        }
        if !before.autorun_key {
            step(
                "delete the Command Processor key",
                sys.autorun.delete_user_key_if_empty(),
            );
        }
        // The font, in the order G12 proved: out of this session, then the registration, then
        // the file. Only a font that was not there before is unloaded and deleted.
        let file_now = sys.fs.exists(&at.font_file);
        if file_now && (!before.font_file || before.font_registration.is_none()) {
            step("unload the font", sys.fonts.unload(&at.font_file));
        }
        if !sys
            .fonts
            .registration(&at.font_value)
            .is_ok_and(|now| now == before.font_registration)
        {
            match &before.font_registration {
                Some(file) => step(
                    "register the font as before",
                    sys.fonts.register(&at.font_value, file),
                ),
                None => step("unregister the font", sys.fonts.unregister(&at.font_value)),
            }
        }
        if file_now && !before.font_file {
            step("unload the font again", sys.fonts.unload(&at.font_file));
            step("delete the font file", sys.fs.remove_file(&at.font_file));
        }
        if !before.font_dir {
            // Only when empty: another per-user font may be in it, and the error says so.
            let _ = sys.fs.remove_dir(&at.font_dir);
        }
        if !before.font_key {
            step(
                "delete the per-user font key",
                sys.fonts.delete_registration_key_if_empty(),
            );
        }
        // Clink's runtime folder in the user's own folder under ProgramData, and that folder
        // itself, each only if the test made it.
        if !before.program_data_clink && sys.fs.exists(&at.program_data_clink) {
            step(
                "delete Clink's runtime folder in ProgramData",
                sys.fs.remove_dir_all(&at.program_data_clink),
            );
        }
        if !before.program_data && sys.fs.exists(&at.program_data) {
            step(
                "delete the ProgramData folder",
                sys.fs.remove_dir_all(&at.program_data),
            );
        }
        problems
    }
}

impl Drop for Restore<'_> {
    fn drop(&mut self) {
        for problem in self.put_back() {
            // `writeln!` and not `eprintln!`: a closed stderr must not panic here.
            let _ = writeln!(std::io::stderr(), "system test restore: {problem}");
        }
    }
}

#[test]
fn system_apply_doctor_undo_is_byte_identical() {
    if !enabled() {
        return;
    }
    let resources = repo("resources");
    assert!(
        resources.join("manifest.json").is_file(),
        "run scripts/fetch-resources.ps1 first"
    );
    let tmp = tempfile::tempdir().unwrap();
    // A fake %LOCALAPPDATA% for Windows Terminal only; a CI runner has no WT.
    let fake_local = tmp.path().join("Local");
    let wt = fake_local
        .join(r"Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json");
    std::fs::create_dir_all(wt.parent().unwrap()).unwrap();
    std::fs::write(
        &wt,
        std::fs::read(repo(r"crates\core\tests\fixtures\wt\default.json")).unwrap(),
    )
    .unwrap();
    // A font no machine has, so the bundled CaskaydiaCove NF really gets installed.
    let preset = std::fs::read_to_string(repo(r"presets\terracotta.cmdtheme")).unwrap();
    let swapped = preset.replace("\"Cascadia Mono\"", "\"CMD Studio Missing Font\"");
    assert!(
        swapped != preset,
        "the preset no longer names Cascadia Mono, so no font would be installed"
    );
    let theme = tmp.path().join("ธีม ทดสอบ.cmdtheme");
    std::fs::write(&theme, swapped).unwrap();
    // The data folder is a temp folder. On VM-C the temp folder is under the Thai user name, and
    // Clink 1.9.34 cannot inject from a path with Thai letters (G19), so `<clink-launch>` is the
    // data folder's 8.3 path or the user's own folder under ProgramData; on an ASCII path (CI) it
    // is the data folder itself.
    let data = tmp.path().join("ai.resincore.cmdstudio");

    let host = WinHost;
    let real_local = host.local_app_data().unwrap();
    let program_data = program_data_dir(&host.program_data().unwrap(), &host.user_sid().unwrap());
    assert!(
        program_data
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|n| n.starts_with("ai.resincore.cmdstudio-")),
        "{} is not a folder of CMD Studio's",
        program_data.display()
    );
    let (fs, autorun, fonts, wt_loc) = (
        WinFs,
        WinAutoRun::real(),
        WinFonts::real(&real_local),
        WinWt::new(fake_local),
    );
    // Only programs from these two folders may run, the folders `cmdstudio` itself allows.
    let process = WinProcess::new(
        vec![data.join("runtime"), program_data.clone()],
        Duration::from_secs(60),
    );
    let event_log = WinEventLog::real();
    let sys = System {
        fs: &fs,
        autorun: &autorun,
        fonts: &fonts,
        wt: &wt_loc,
        process: &process,
        host: &host,
        event_log: &event_log,
    };
    let (_, font_file, font_value) = font_target(
        sys,
        &Resources {
            root: resources.clone(),
        },
        NERD_FONT_FAMILY,
    )
    .unwrap();
    let at = Places {
        wt_settings: wt,
        font_dir: fonts.user_fonts_dir(),
        font_file,
        font_value,
        program_data_clink: program_data.join(format!("clink-{CLINK_VERSION}")),
        program_data,
    };

    let d = Defaults {
        data_dir: data,
        resources,
        now_ms: now_ms(),
    };
    apply_doctor_undo(sys, &at, &d, &theme, &mut || {
        std::thread::sleep(RETRY_EVERY)
    });
}

/// How many times `undo` is run again for a font file Windows still holds, and how long
/// `apply_doctor_undo` waits before each.
const FONT_RETRIES: u32 = 10;
const RETRY_EVERY: Duration = Duration::from_secs(1);

/// `apply`, `doctor --json` and `undo` of `theme` on `sys`, and the proof that the machine is
/// exactly as it was before. A guard made before the first change repairs a run that fails
/// half way. `pause` is called before each retry of the pending font delete (see below).
fn apply_doctor_undo(
    sys: System<'_>,
    at: &Places,
    d: &Defaults,
    theme: &Path,
    pause: &mut dyn FnMut(),
) {
    let before = snapshot(sys, at).unwrap();
    // Created before the first change: from here on, whatever happens, the machine comes back.
    let _restore = Restore {
        sys,
        at,
        before: &before,
    };

    let apply = cli(sys, d, &[OsStr::new("apply"), theme.as_os_str()]);
    assert_eq!(apply.code, EXIT_OK, "{}", apply.log());
    assert!(
        sys.fonts
            .families()
            .unwrap()
            .iter()
            .any(|f| f == NERD_FONT_FAMILY),
        "font not installed"
    );
    // Our entry comes first, in parentheses (spec §5.4).
    let value = sys.autorun.read_user().unwrap().unwrap().text().unwrap();
    assert!(value.starts_with("(if exist \""), "{value}");
    assert!(
        value.contains("inject --autorun --quiet --profile"),
        "{value}"
    );

    let d2 = Defaults {
        now_ms: d.now_ms + 1,
        ..d.clone()
    };
    let doctor = cli(sys, &d2, &[OsStr::new("doctor"), OsStr::new("--json")]);
    let report: serde_json::Value = serde_json::from_str(&doctor.out)
        .unwrap_or_else(|e| panic!("doctor --json printed no JSON ({e}): {}", doctor.log()));
    let checks = report["checks"]
        .as_array()
        .unwrap_or_else(|| panic!("no checks in the report: {}", doctor.log()));
    assert!(!checks.is_empty(), "doctor made no checks");
    for c in checks {
        assert_ne!(c["status"], "fail", "{}", show_check(c));
    }
    assert_eq!(doctor.code, EXIT_OK, "{}", doctor.log());

    let d3 = Defaults {
        now_ms: d.now_ms + 2,
        ..d.clone()
    };
    let undo = cli(sys, &d3, &[OsStr::new("undo")]);
    assert_eq!(undo.code, EXIT_OK, "{}", undo.log());
    // What undo itself puts back, checked at once: AutoRun, settings.json, the font list, the
    // font registration and key, the ProgramData folders.
    snapshot(sys, at).unwrap().assert_same_as(&before);
    // The font file is another matter. `undo` deletes it once (exec `remove_font`); when Windows,
    // a virus scanner or the font cache still holds the fresh file, that fails, the file is
    // noted in state.json, and the delete is tried again at the start of the next command that
    // changes the machine (actions `delete_pending`), while `undo` still ends with EXIT_OK. So
    // the product's own retry is run: `undo` again, which finds nothing to undo ("Nothing to
    // undo.", `UndoResult::NothingToUndo`, EXIT_OK) after it has tried the pending delete.
    // The file is checked only after that, so a file left for ever is still a failure.
    let mut retries = 0;
    while !before.font_file && sys.fs.exists(&at.font_file) && retries < FONT_RETRIES {
        pause();
        retries += 1;
        let again = Defaults {
            now_ms: d.now_ms + 2 + u64::from(retries),
            ..d.clone()
        };
        let run = cli(sys, &again, &[OsStr::new("undo")]);
        assert_eq!(run.code, EXIT_OK, "{}", run.log());
    }
    let now = snapshot(sys, at).unwrap();
    now.assert_font_file_same_as(&before, retries);
    if retries > 0 && now.font_dir && !before.font_dir {
        // The file was held, so undo could not remove the fonts folder apply made (exec
        // `remove_font` tries once, and the retry above deletes only the file). The empty folder
        // is not a leaked font, so it may stay; anything in it may not.
        let left = sys.fs.list_dir(&at.font_dir).unwrap();
        assert!(
            left.is_empty(),
            "the per-user fonts folder, which did not exist before, still holds {} entries after undo",
            left.len()
        );
        println!(
            "note: undo could not delete the font file at first; it was deleted after {retries} retries, and the empty per-user fonts folder stays"
        );
    } else {
        now.assert_font_dir_same_as(&before);
    }
    // The runtimes under the data folder are gone (Clink's folder in ProgramData, on VM-C, is
    // part of the snapshot above: gone unless it was there before the test).
    let paths = Paths {
        data: d.data_dir.clone(),
    };
    for dir in [paths.omp_dir(), paths.clink_dir()] {
        assert!(
            !sys.fs.exists(&dir),
            "{} is still there after undo",
            dir.display()
        );
    }
}
