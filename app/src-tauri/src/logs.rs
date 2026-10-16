//! `<data>\logs\cmdstudio.log` (spec §5.3): one line per IPC command, with the time, what was
//! asked and how it ended; five files of at most 1 MB, `cmdstudio.log`, `.1` to `.4`. Written
//! through `Fs` like every file of CMD Studio's, so the tests use the in-memory disk. No theme
//! contents and no user text go into it: the diagnostics (spec §5.10) quote it after redaction.
use cmdstudio_core::adapters::{AdapterError, Fs};
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

pub const LOG_FILE: &str = "cmdstudio.log";
/// The log and its rotated copies, together.
pub const KEEP: usize = 5;
pub const MAX_BYTES: usize = 1024 * 1024;

pub struct Logger {
    dir: PathBuf,
    /// Held for the whole of a `write` (the read, the rotation and the replacing write) and of a
    /// `tail`. The IPC commands run on several threads and each logs a line: with no lock two
    /// of them read the same file and the later write drops the other's line, and two that both
    /// see a full file rotate twice, the second rename onto a file that is there. The lock
    /// guards the files, not data in memory, so a thread that panicked while holding it left
    /// nothing broken and a poisoned lock is used as it is.
    files: Mutex<()>,
}

impl Logger {
    pub fn new(dir: PathBuf) -> Self {
        Logger {
            dir,
            files: Mutex::new(()),
        }
    }

    pub fn file(&self) -> PathBuf {
        self.dir.join(LOG_FILE)
    }

    fn rotated(&self, n: usize) -> PathBuf {
        self.dir.join(format!("{LOG_FILE}.{n}"))
    }

    /// Appends `<time> <line>`. When the file would pass 1 MB, the copies move up one (`.4`
    /// goes) and a new file starts. A line that cannot be written is the caller's to drop: a
    /// log never stops a command.
    pub fn write(&self, fs: &dyn Fs, now_ms: u64, line: &str) -> Result<(), AdapterError> {
        let entry = format!("{} {}\n", iso_utc(now_ms), line.replace(['\r', '\n'], " "));
        let _one_at_a_time = self.files.lock().unwrap_or_else(PoisonError::into_inner);
        let file = self.file();
        let mut current = fs.read(&file)?.unwrap_or_default();
        if !current.is_empty() && current.len() + entry.len() > MAX_BYTES {
            self.rotate(fs)?;
            current.clear();
        }
        current.extend_from_slice(entry.as_bytes());
        fs.write_atomic(&file, &current)
    }

    fn rotate(&self, fs: &dyn Fs) -> Result<(), AdapterError> {
        let last = self.rotated(KEEP - 1);
        if fs.exists(&last) {
            fs.remove_file(&last)?;
        }
        for n in (1..KEEP - 1).rev() {
            let from = self.rotated(n);
            if fs.exists(&from) {
                fs.rename(&from, &self.rotated(n + 1))?;
            }
        }
        let file = self.file();
        if fs.exists(&file) {
            fs.rename(&file, &self.rotated(1))?;
        }
        Ok(())
    }

    /// The last `lines` lines of the current file; empty when there is none or it cannot be read.
    pub fn tail(&self, fs: &dyn Fs, lines: usize) -> String {
        // Not between the two renames of a rotation, where the current file is not there yet.
        let _one_at_a_time = self.files.lock().unwrap_or_else(PoisonError::into_inner);
        let Ok(Some(bytes)) = fs.read(&self.file()) else {
            return String::new();
        };
        let text = String::from_utf8_lossy(&bytes);
        let all: Vec<&str> = text.lines().collect();
        let skip = all.len().saturating_sub(lines);
        all.iter()
            .skip(skip)
            .copied()
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// `YYYY-MM-DDTHH:MM:SSZ` from milliseconds since 1970 (the civil-from-days algorithm).
pub fn iso_utc(ms: u64) -> String {
    let secs = ms / 1000;
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // Howard Hinnant's days-to-civil, for days since 1970-01-01.
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    format!("{year:04}-{month:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmdstudio_core::adapters::fake::FakeFs;
    use std::path::Path;

    fn logger() -> Logger {
        Logger::new(PathBuf::from(
            r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio\logs",
        ))
    }

    #[test]
    fn the_time_is_utc_iso_8601() {
        assert_eq!(iso_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_utc(1_791_694_709_000), "2026-10-11T04:58:29Z");
        assert_eq!(iso_utc(951_782_400_000), "2000-02-29T00:00:00Z");
    }

    #[test]
    fn lines_are_appended_with_their_time_and_without_line_breaks() {
        let (fs, log) = (FakeFs::new(), logger());
        log.write(&fs, 0, "cmd=themes_list ok").unwrap();
        log.write(&fs, 1000, "cmd=apply\nerr=E_BUSY").unwrap();
        let text = String::from_utf8(fs.file(log.file()).unwrap()).unwrap();
        assert_eq!(
            text,
            "1970-01-01T00:00:00Z cmd=themes_list ok\n1970-01-01T00:00:01Z cmd=apply err=E_BUSY\n"
        );
        assert_eq!(
            log.tail(&fs, 1),
            "1970-01-01T00:00:01Z cmd=apply err=E_BUSY"
        );
        assert_eq!(log.tail(&fs, 9).lines().count(), 2);
        assert_eq!(Logger::new(PathBuf::from(r"C:\none")).tail(&fs, 3), "");
    }

    #[test]
    fn the_log_rotates_at_one_megabyte_and_keeps_five_files() {
        let (fs, log) = (FakeFs::new(), logger());
        let big = "x".repeat(MAX_BYTES - 100);
        for n in 0..6_u64 {
            log.write(&fs, n * 1000, &big).unwrap();
            log.write(&fs, n * 1000 + 1, &format!("round {n}")).unwrap();
        }
        // Round 5 is current; rounds 4, 3, 2, 1 are .1 to .4; round 0 is gone.
        let text = |p: PathBuf| String::from_utf8(fs.file(p).unwrap()).unwrap();
        assert!(text(log.file()).ends_with("round 5\n"));
        for (n, round) in [(1, 4), (2, 3), (3, 2), (4, 1)] {
            assert!(
                text(log.rotated(n)).ends_with(&format!("round {round}\n")),
                "{n}"
            );
        }
        assert!(!fs.exists(&log.rotated(5)));
        let entries = fs
            .list_dir(Path::new(
                r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio\logs",
            ))
            .unwrap();
        assert_eq!(entries.len(), KEEP);
        for e in &entries {
            assert!(
                fs.file(&e.path).unwrap().len() <= MAX_BYTES,
                "{}",
                e.path.display()
            );
        }
    }

    #[test]
    fn the_logger_can_be_shared_between_threads() {
        fn is_send_and_sync<T: Send + Sync>() {}
        is_send_and_sync::<Logger>();
    }

    /// The same `Logger` on the real disk from several threads, as the IPC commands are
    /// (`spawn_blocking` on one shared backend). `FakeFs` cannot be shared between threads.
    #[cfg(windows)]
    mod on_disk {
        use super::*;
        use cmdstudio_core::adapters::windows::fs::WinFs;

        const THREADS: u32 = 8;

        /// A folder of its own under the system temp folder, removed when the test ends, also
        /// when it fails. The tests touch nothing else on this computer.
        struct Scratch(PathBuf);

        impl Scratch {
            fn new() -> Self {
                let name = format!("cmdstudio-logs-test-{}", uuid::Uuid::new_v4().simple());
                Scratch(std::env::temp_dir().join(name))
            }
        }

        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        /// `THREADS` threads, each writing `lines` lines of `cmd=t<thread>_<n> <filler>` through
        /// the one `log`. Every write must succeed.
        fn write_from_threads(log: &Logger, lines: u32, filler: &str) {
            std::thread::scope(|scope| {
                for t in 0..THREADS {
                    scope.spawn(move || {
                        for n in 0..lines {
                            let line = format!("cmd=t{t}_{n} {filler}");
                            log.write(&WinFs, u64::from(t * lines + n), &line).unwrap();
                        }
                    });
                }
            });
        }

        /// The command of each line of the log and its copies, sorted; and the files found.
        fn commands_and_files(log: &Logger) -> (Vec<String>, usize) {
            let mut paths = vec![log.file()];
            paths.extend((1..KEEP).map(|n| log.rotated(n)));
            let (mut commands, mut files) = (Vec::new(), 0);
            for path in paths {
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                files += 1;
                assert!(text.len() <= MAX_BYTES, "{}", path.display());
                commands.extend(
                    text.lines()
                        .map(|l| l.split(' ').nth(1).unwrap().to_string()),
                );
            }
            commands.sort();
            (commands, files)
        }

        fn expected(lines: u32) -> Vec<String> {
            let mut all: Vec<String> = (0..THREADS)
                .flat_map(|t| (0..lines).map(move |n| format!("cmd=t{t}_{n}")))
                .collect();
            all.sort();
            all
        }

        #[test]
        fn lines_written_at_the_same_time_are_all_kept_once() {
            let scratch = Scratch::new();
            let log = Logger::new(scratch.0.join("logs"));
            write_from_threads(&log, 25, "ok");
            let (found, files) = commands_and_files(&log);
            assert_eq!(files, 1);
            assert_eq!(found.len(), expected(25).len(), "lost or doubled lines");
            assert_eq!(found, expected(25));
        }

        #[test]
        fn rotation_at_the_same_time_loses_no_line_and_fails_no_write() {
            let scratch = Scratch::new();
            let log = Logger::new(scratch.0.join("logs"));
            // 24 lines of 150 KB: about six to a file, four files, below the five that are kept.
            write_from_threads(&log, 3, &"x".repeat(150_000));
            let (found, files) = commands_and_files(&log);
            assert!((2..=KEEP).contains(&files), "{files} files");
            assert_eq!(found.len(), expected(3).len(), "lost or doubled lines");
            assert_eq!(found, expected(3));
        }
    }
}
