//! The crash notes (recovery spec §5.3): a panic hook, installed at the start of the app and of
//! the CLI, writes `<data>\logs\crash-<unix ms>.txt` with the version, the time, the thread, the
//! panic's message and location and a backtrace, every line through the diagnostics' redaction
//! (`redact`, `redact_sids`), keeps the ten newest, never panics itself and never fails for a
//! write that fails, and then runs the hook that was there before (Rust's own, which prints to
//! stderr). A native crash (WebView2) runs no hook: only `run.json` tells of it.
use crate::backend::diagnostics::{redact, redact_sids};
use cmdstudio_core::adapters::{AdapterError, Fs};
use cmdstudio_core::utc::iso as iso_utc;
use std::path::{Path, PathBuf};

/// How many crash notes stay in `logs\` (recovery spec §5.3).
pub const CRASH_NOTES_KEPT: usize = 10;
/// A note's name: this, the time in milliseconds since 1970, `.txt`.
pub const NOTE_PREFIX: &str = "crash-";

/// What the diagnostics take out of a text (`diagnostics.rs`): the names `user_names` redacts
/// (the Windows user name, the profile folder's names) and the user's SID.
#[derive(Clone, Debug, Default)]
pub struct Redaction {
    pub names: Vec<String>,
    pub sid: Option<String>,
}

/// `redact` for each name, then `redact_sids`.
pub fn redact_all(text: &str, r: &Redaction) -> String {
    let named = r
        .names
        .iter()
        .fold(text.to_string(), |t, name| redact(&t, name));
    redact_sids(&named, r.sid.as_deref())
}

/// One panic, as the hook sees it.
pub struct Report<'a> {
    pub version: &'a str,
    pub now_ms: u64,
    pub thread: &'a str,
    pub message: &'a str,
    pub location: Option<&'a str>,
    pub backtrace: &'a str,
}

/// The note's text, every line redacted.
pub fn report_text(r: &Report<'_>, redaction: &Redaction) -> String {
    let raw = format!(
        "CMD Studio {} crashed at {}\nthread: {}\nmessage: {}\nlocation: {}\nbacktrace:\n{}",
        r.version,
        iso_utc(r.now_ms),
        r.thread,
        r.message,
        r.location.unwrap_or("unknown"),
        r.backtrace
    );
    let mut out = String::with_capacity(raw.len());
    for line in raw.lines() {
        out.push_str(&redact_all(line, redaction));
        out.push('\n');
    }
    out
}

/// The time a note's name carries; `None` for any other file.
pub fn note_time(path: &Path) -> Option<u64> {
    path.file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.strip_prefix(NOTE_PREFIX))
        .and_then(|n| n.strip_suffix(".txt"))
        .and_then(|ms| ms.parse::<u64>().ok())
}

/// The notes in `logs`, newest first.
fn notes(fs: &dyn Fs, logs: &Path) -> Vec<(u64, PathBuf)> {
    let mut found: Vec<(u64, PathBuf)> = fs
        .list_dir(logs)
        .unwrap_or_default()
        .into_iter()
        .filter(|e| !e.is_dir)
        .filter_map(|e| note_time(&e.path).map(|ms| (ms, e.path)))
        .collect();
    found.sort_by_key(|(ms, _)| std::cmp::Reverse(*ms));
    found
}

/// The newest note, by its name.
pub fn latest_note(fs: &dyn Fs, logs: &Path) -> Option<(u64, PathBuf)> {
    notes(fs, logs).into_iter().next()
}

/// Writes `crash-<now_ms>.txt` in `logs` (made when missing) and deletes the notes beyond the
/// ten newest. A note that cannot be deleted is left.
pub fn write_note(
    fs: &dyn Fs,
    logs: &Path,
    now_ms: u64,
    text: &str,
) -> Result<PathBuf, AdapterError> {
    let path = logs.join(format!("{NOTE_PREFIX}{now_ms}.txt"));
    fs.write_atomic(&path, text.as_bytes())?;
    for (_, old) in notes(fs, logs).into_iter().skip(CRASH_NOTES_KEPT) {
        let _ = fs.remove_file(&old);
    }
    Ok(path)
}

/// What the hook does, with the pieces it has: writes the note and drops any failure. Split out
/// so a test runs it under `catch_unwind` with a disk that refuses the write.
pub fn hook_body(fs: &dyn Fs, logs: &Path, now_ms: u64, redaction: &Redaction, r: &Report<'_>) {
    let text = report_text(r, redaction);
    let _ = write_note(fs, logs, now_ms, &text);
}

/// Installs the hook (recovery spec §5.3). The earlier hook runs after the note is written.
///
/// The hook's safety does not come from `catch_unwind`: Rust aborts the process at once when a
/// panic hook itself panics, and the `catch_unwind` inside the hook does not prevent that (it is
/// kept as harmless). It rests on what the hook calls never panicking: `redact`, `redact_sids`,
/// `iso_utc`, `WinFs`'s `write_atomic`, `list_dir` and `remove_file`, `now` (`cmdstudio_cli::now_ms`)
/// and the `downcast_ref` chains here. Any change to them must keep that.
#[cfg(windows)]
pub fn install_hook(logs: PathBuf, redaction: Redaction, now: fn() -> u64) {
    use cmdstudio_core::adapters::windows::fs::WinFs;
    let earlier = std::panic::take_hook();
    let version = crate::backend::versions().app;
    std::panic::set_hook(Box::new(move |info| {
        // UNWIND SAFETY: the closure reads `info` and its own captures and writes one file;
        // nothing of the program's state is left half changed when it unwinds.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let message = match info.payload().downcast_ref::<&str>() {
                Some(s) => (*s).to_string(),
                None => match info.payload().downcast_ref::<String>() {
                    Some(s) => s.clone(),
                    None => "(no message)".to_string(),
                },
            };
            let location = info
                .location()
                .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
            let thread = std::thread::current();
            let backtrace = std::backtrace::Backtrace::force_capture().to_string();
            let at = now();
            hook_body(
                &WinFs,
                &logs,
                at,
                &redaction,
                &Report {
                    version: &version,
                    now_ms: at,
                    thread: thread.name().unwrap_or("unnamed"),
                    message: &message,
                    location: location.as_deref(),
                    backtrace: &backtrace,
                },
            );
        }));
        earlier(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmdstudio_core::adapters::fake::FakeFs;
    use cmdstudio_core::adapters::Fs;
    use std::path::PathBuf;

    fn logs() -> PathBuf {
        PathBuf::from(r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio\logs")
    }

    fn redaction() -> Redaction {
        Redaction {
            names: vec!["tester".to_string()],
            sid: Some("S-1-5-21-1-2-3-1001".to_string()),
        }
    }

    fn report<'a>(message: &'a str, backtrace: &'a str) -> Report<'a> {
        Report {
            version: "0.1.0",
            now_ms: 1_791_694_709_000,
            thread: "main",
            message,
            location: Some(r"app\src-tauri\src\backend\apply.rs:120:9"),
            backtrace,
        }
    }

    /// Review Focus 4: the user's name and SID never reach the note, wherever they are: the
    /// message, the location, the backtrace.
    #[test]
    fn the_note_is_redacted_before_it_is_written() {
        let text = report_text(
            &report(
                r"called `Option::unwrap()` on a `None` value at C:\Users\tester\x.json",
                "   0: cmdstudio_app::f\n             at C:\\Users\\tester\\cmd-studio\\app\\src-tauri\\src\\lib.rs:40\n   1: S-1-5-21-1-2-3-1001 owned\n",
            ),
            &redaction(),
        );
        assert!(
            text.starts_with("CMD Studio 0.1.0 crashed at 2026-10-11T04:58:29Z\n"),
            "{text}"
        );
        assert!(text.contains("thread: main\n"), "{text}");
        assert!(
            text.contains("message: called `Option::unwrap()` on a `None` value at C:\\Users\\%USERNAME%\\x.json\n"),
            "{text}"
        );
        assert!(
            text.contains("location: app\\src-tauri\\src\\backend\\apply.rs:120:9\n"),
            "{text}"
        );
        assert!(
            text.contains("C:\\Users\\%USERNAME%\\cmd-studio\\app"),
            "{text}"
        );
        assert!(text.contains("<SID> owned"), "{text}");
        assert!(
            !text.contains("tester") && !text.contains("S-1-5-21"),
            "{text}"
        );
        assert!(text.ends_with('\n'));
    }

    /// Ten notes are kept, the newest; the file is named by its time, which `note_time` reads
    /// back, and `latest_note` finds the newest by that name.
    #[test]
    fn ten_notes_are_kept_and_the_newest_is_found_by_its_name() {
        let fs = FakeFs::new();
        for n in 0..12_u64 {
            let ms = 1_791_694_709_000 + n * 1000;
            let path = write_note(&fs, &logs(), ms, "x\n").unwrap();
            assert_eq!(path, logs().join(format!("crash-{ms}.txt")));
            assert_eq!(note_time(&path), Some(ms));
        }
        let left: Vec<String> = fs
            .list_dir(&logs())
            .unwrap()
            .into_iter()
            .map(|e| e.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left.len(), CRASH_NOTES_KEPT, "{left:?}");
        assert!(
            !left.contains(&"crash-1791694709000.txt".to_string()),
            "{left:?}"
        );
        assert!(
            !left.contains(&"crash-1791694710000.txt".to_string()),
            "{left:?}"
        );
        assert_eq!(
            latest_note(&fs, &logs()),
            Some((1_791_694_720_000, logs().join("crash-1791694720000.txt")))
        );
        // Another file of the log folder is neither counted nor touched.
        fs.put(logs().join("cmdstudio.log"), b"log");
        assert_eq!(note_time(&logs().join("cmdstudio.log")), None);
        assert_eq!(note_time(&logs().join("crash-x.txt")), None);
        write_note(&fs, &logs(), 1_791_694_730_000, "y\n").unwrap();
        assert!(fs.exists(&logs().join("cmdstudio.log")));
    }

    /// Review Focus 4: a note that cannot be written is dropped, and the hook's body, run as
    /// the hook runs it, never panics for it (a second panic inside a hook aborts the process).
    #[test]
    fn a_note_that_cannot_be_written_is_dropped_without_a_second_panic() {
        let fs = FakeFs::new();
        fs.fail_writes_to(logs().join("crash-1791694709000.txt"));
        // UNWIND SAFETY (test): `FakeFs` holds a `RefCell`, so the closure is wrapped; nothing is
        // read from it after a panic but through the assertions below.
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            hook_body(
                &fs,
                &logs(),
                1_791_694_709_000,
                &redaction(),
                &report("boom", ""),
            )
        }));
        assert!(outcome.is_ok(), "the hook's body panicked");
        assert_eq!(latest_note(&fs, &logs()), None);
        // The disk is whole: once the folder can be written again, the next note lands.
        fs.stop_failing(logs().join("crash-1791694709000.txt"));
        hook_body(
            &fs,
            &logs(),
            1_791_694_709_000,
            &redaction(),
            &report("boom", ""),
        );
        assert_eq!(
            latest_note(&fs, &logs()),
            Some((1_791_694_709_000, logs().join("crash-1791694709000.txt")))
        );
    }

    /// On the real disk, as the hook writes it: a temp folder of its own, removed afterwards
    /// (as `logs.rs`'s on-disk tests do).
    #[cfg(windows)]
    #[test]
    fn on_disk_a_note_is_written_in_utf8_and_read_back() {
        use cmdstudio_core::adapters::windows::fs::WinFs;
        let dir = std::env::temp_dir().join(format!(
            "cmdstudio-crash-test-{}",
            uuid::Uuid::new_v4().simple()
        ));
        struct Scratch(PathBuf);
        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let scratch = Scratch(dir.clone());
        let text = report_text(&report("ไทย and English", "   0: f\n"), &redaction());
        let path = write_note(&WinFs, &scratch.0.join("logs"), 1, &text).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        assert_eq!(
            latest_note(&WinFs, &scratch.0.join("logs")).map(|(ms, _)| ms),
            Some(1)
        );
    }
}
