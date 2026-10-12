//! Runs CMD Studio's own bundled programs (spec §5.10): only executables
//! under the given folders, fixed arguments, no shell, no console window, a
//! time limit. The caller never waits longer than that limit plus a short
//! grace for the output, even when the program started another program that
//! keeps its output open.
use crate::adapters::{AdapterError, Output, Process};
use std::ffi::OsStr;
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

/// `CREATE_NO_WINDOW`: no console flashes on the user's screen (spec §2.2 lesson 7).
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A program that ends close to its time limit still gets this long to hand over what it
/// printed: its output is already in the pipe, and the readers only need to be scheduled.
const OUTPUT_GRACE: Duration = Duration::from_millis(500);

/// How often the program is looked at while it runs.
const POLL: Duration = Duration::from_millis(20);

#[derive(Clone, Debug)]
pub struct WinProcess {
    allowed: Vec<PathBuf>,
    timeout: Duration,
}

/// Whether the path names a `.exe` file: no batch file, which would start `cmd` (and with it
/// the user's AutoRun), and no extension-less name that the system would complete itself.
fn has_exe_extension(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
}

impl WinProcess {
    /// `allowed`: folders whose programs may run (the runtime folders). `timeout` is how long a
    /// program may run before it is stopped.
    pub fn new(allowed: Vec<PathBuf>, timeout: Duration) -> Self {
        WinProcess { allowed, timeout }
    }

    /// Whether `exe` is a `.exe` file inside one of the allowed folders, judged on canonical
    /// paths: `..`, junctions, symbolic links, 8.3 names and letter case cannot lead out of a
    /// folder. A relative path is refused, because it would depend on the current folder. The
    /// program then starts from the path it was given (a program may need its 8.3 name, G19),
    /// so the check and the start are two steps; the folders are the user's own (spec §5.4).
    fn permitted(&self, exe: &Path) -> bool {
        if !exe.is_absolute() || !has_exe_extension(exe) {
            return false;
        }
        let Ok(exe) = std::fs::canonicalize(exe) else {
            return false;
        };
        // The file the link leads to must be a program too.
        if !has_exe_extension(&exe) || !exe.is_file() {
            return false;
        }
        self.allowed
            .iter()
            .filter_map(|root| std::fs::canonicalize(root).ok())
            .any(|root| exe.starts_with(root))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stream {
    Stdout,
    Stderr,
}

/// What one reader found: the bytes up to the end of the pipe, or why they could not be read.
type Drained = (Stream, std::io::Result<Vec<u8>>);

/// Reads `pipe` to its end on a thread of its own and sends the result to `done`. The thread
/// is not joined: when a program started by the program keeps the pipe open, the thread waits
/// for that program, and the caller has long gone.
fn drain(
    stream: Stream,
    mut pipe: impl Read + Send + 'static,
    done: Sender<Drained>,
) -> std::io::Result<()> {
    std::thread::Builder::new().spawn(move || {
        let mut bytes = Vec::new();
        let read = pipe.read_to_end(&mut bytes).map(|_| bytes);
        // The receiver is gone when the caller stopped waiting.
        let _ = done.send((stream, read));
    })?;
    Ok(())
}

/// Takes the output of `expected` readers, waiting at most `within` for all of them. `Err` is
/// the reason the output is not complete: a partial output is never passed on as a whole one.
fn collect(
    results: &Receiver<Drained>,
    expected: usize,
    within: Duration,
) -> Result<(Vec<u8>, Vec<u8>), String> {
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let since = Instant::now();
    for _ in 0..expected {
        match results.recv_timeout(within.saturating_sub(since.elapsed())) {
            Ok((Stream::Stdout, Ok(bytes))) => stdout = bytes,
            Ok((Stream::Stderr, Ok(bytes))) => stderr = bytes,
            Ok((_, Err(e))) => return Err(format!("its output could not be read: {e}")),
            Err(RecvTimeoutError::Timeout) => {
                return Err(
                    "its output did not end (a program it started may still hold its output open)"
                        .to_string(),
                )
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err("its output could not be read: the reader stopped".to_string())
            }
        }
    }
    Ok((stdout, stderr))
}

fn stop(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

impl Process for WinProcess {
    fn run(&self, exe: &Path, args: &[&OsStr]) -> Result<Output, AdapterError> {
        let fail = |message: String| AdapterError::new(format!("run {}", exe.display()), message);
        if !self.permitted(exe) {
            return Err(fail("not one of CMD Studio's own programs".to_string()));
        }
        let mut child = Command::new(exe)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| fail(e.to_string()))?;
        let started = Instant::now();
        let (done, results) = mpsc::channel();
        let readers = (|| -> std::io::Result<usize> {
            let mut count = 0;
            if let Some(pipe) = child.stdout.take() {
                drain(Stream::Stdout, pipe, done.clone())?;
                count += 1;
            }
            if let Some(pipe) = child.stderr.take() {
                drain(Stream::Stderr, pipe, done.clone())?;
                count += 1;
            }
            Ok(count)
        })();
        // Only the readers hold a sender now, so a reader that dies is noticed.
        drop(done);
        let expected = match readers {
            Ok(count) => count,
            Err(e) => {
                stop(&mut child);
                return Err(fail(format!("could not read its output: {e}")));
            }
        };
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if started.elapsed() >= self.timeout => {
                    stop(&mut child);
                    return Err(fail(format!("no answer after {:?}", self.timeout)));
                }
                Ok(None) => std::thread::sleep(POLL),
                Err(e) => {
                    stop(&mut child);
                    return Err(fail(e.to_string()));
                }
            }
        };
        // The same time limit covers the output. A program the program started may hold the
        // pipes open for much longer than the program itself ran.
        let within = self
            .timeout
            .saturating_sub(started.elapsed())
            .max(OUTPUT_GRACE);
        let (stdout, stderr) = collect(&results, expected, within).map_err(fail)?;
        Ok(Output {
            status: status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn system32() -> PathBuf {
        PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32")
    }

    #[test]
    fn runs_an_allowed_program_and_captures_output() {
        let p = WinProcess::new(vec![system32()], Duration::from_secs(30));
        let out = p
            .run(
                &system32().join("cmd.exe"),
                &[
                    OsStr::new("/d"),
                    OsStr::new("/c"),
                    OsStr::new("echo cmdstudio"),
                ],
            )
            .unwrap();
        assert_eq!(out.status, 0);
        assert_eq!(out.stdout.trim(), "cmdstudio");
    }

    #[test]
    fn refuses_programs_outside_the_allowed_folders() {
        let tmp = tempfile::tempdir().unwrap();
        let p = WinProcess::new(vec![tmp.path().to_path_buf()], Duration::from_secs(30));
        let args = [OsStr::new("/d"), OsStr::new("/c"), OsStr::new("exit")];
        assert!(p.run(&system32().join("cmd.exe"), &args).is_err());
    }

    #[test]
    fn a_hung_program_is_stopped() {
        let p = WinProcess::new(vec![system32()], Duration::from_secs(1));
        let started = Instant::now();
        let err = p
            .run(
                &system32().join("PING.EXE"),
                &[OsStr::new("-n"), OsStr::new("30"), OsStr::new("127.0.0.1")],
            )
            .unwrap_err();
        assert!(err.message.contains("no answer"), "{err}");
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    // ---- Focused extra tests (Q20, Q31) ----
    // Every cmd.exe below runs with /d (the user's AutoRun injects Clink into a plain one);
    // PING.EXE only pings the loopback address.

    fn os<'a>(items: &[&'a str]) -> Vec<&'a OsStr> {
        items.iter().map(|s| OsStr::new(*s)).collect()
    }

    #[test]
    fn a_program_that_leaves_a_child_holding_its_output_cannot_hang_the_caller() {
        // cmd exits at once; the ping it started keeps cmd's output pipes open for 7 s.
        let p = WinProcess::new(vec![system32()], Duration::from_secs(2));
        let started = Instant::now();
        let err = p
            .run(
                &system32().join("cmd.exe"),
                &os(&[
                    "/d",
                    "/c",
                    "start",
                    "/b",
                    "PING.EXE",
                    "-n",
                    "8",
                    "127.0.0.1",
                ]),
            )
            .unwrap_err();
        let took = started.elapsed();
        // Honest about it: the program is named, and the output is not passed off as whole.
        assert!(err.context.contains("cmd.exe"), "{err}");
        assert!(err.message.contains("its output did not end"), "{err}");
        // Back by the time limit, plus a margin; far from the ping's 7 s.
        assert!(took < Duration::from_secs(5), "{took:?}");
    }

    #[test]
    fn exit_status_and_error_output_come_back() {
        let p = WinProcess::new(vec![system32()], Duration::from_secs(30));
        let out = p
            .run(
                &system32().join("cmd.exe"),
                &os(&["/d", "/c", "echo oops 1>&2 & exit 3"]),
            )
            .unwrap();
        assert_eq!(out.status, 3);
        assert_eq!(out.stderr.trim(), "oops");
        assert_eq!(out.stdout, "");
    }

    #[test]
    fn standard_input_is_closed() {
        // findstr reads standard input to its end. With input closed it ends at once and finds
        // nothing (exit 1); with a console or an open pipe it would wait.
        let p = WinProcess::new(vec![system32()], Duration::from_secs(10));
        let started = Instant::now();
        let out = p
            .run(&system32().join("findstr.exe"), &os(&["/n", "."]))
            .unwrap();
        assert_eq!(out.status, 1);
        assert_eq!(out.stdout, "");
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn collect_hands_over_whole_outputs_and_only_whole_ones() {
        // Both readers done: the bytes go to the right stream, even with no time left.
        let (tx, rx) = mpsc::channel::<Drained>();
        tx.send((Stream::Stderr, Ok(b"err".to_vec()))).unwrap();
        tx.send((Stream::Stdout, Ok(b"out".to_vec()))).unwrap();
        let (stdout, stderr) = collect(&rx, 2, Duration::ZERO).unwrap();
        assert_eq!((stdout, stderr), (b"out".to_vec(), b"err".to_vec()));
        assert_eq!(collect(&rx, 0, Duration::ZERO).unwrap(), (vec![], vec![]));

        // One reader never ends (a program it started holds the pipe): back by the limit.
        let (tx, rx) = mpsc::channel::<Drained>();
        tx.send((Stream::Stdout, Ok(b"partial".to_vec()))).unwrap();
        let started = Instant::now();
        let err = collect(&rx, 2, Duration::from_millis(200)).unwrap_err();
        assert!(err.contains("did not end"), "{err}");
        assert!(started.elapsed() < Duration::from_secs(3));
        drop(tx);

        // A reader that failed, and one that vanished.
        let (tx, rx) = mpsc::channel::<Drained>();
        tx.send((Stream::Stdout, Err(std::io::Error::other("the pipe broke"))))
            .unwrap();
        let err = collect(&rx, 1, Duration::from_secs(1)).unwrap_err();
        assert!(
            err.contains("could not be read") && err.contains("the pipe broke"),
            "{err}"
        );
        let (tx, rx) = mpsc::channel::<Drained>();
        drop(tx);
        let err = collect(&rx, 1, Duration::from_secs(1)).unwrap_err();
        assert!(err.contains("could not be read"), "{err}");
    }

    #[test]
    fn a_reader_reports_what_it_read_and_why_it_failed() {
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("no more"))
            }
        }
        let (tx, rx) = mpsc::channel();
        drain(Stream::Stderr, &b"text"[..], tx.clone()).unwrap();
        drain(Stream::Stdout, Broken, tx).unwrap();
        let mut seen = Vec::new();
        for _ in 0..2 {
            seen.push(rx.recv_timeout(Duration::from_secs(5)).unwrap());
        }
        seen.sort_by_key(|(s, _)| *s == Stream::Stdout);
        assert!(matches!(&seen[0], (Stream::Stderr, Ok(b)) if b == b"text"));
        assert!(matches!(&seen[1], (Stream::Stdout, Err(e)) if e.to_string() == "no more"));
    }

    #[test]
    fn only_a_dot_exe_name_is_a_program_name() {
        for yes in ["a.exe", "A.EXE", r"C:\x\clink_x64.Exe", "two.dots.exe"] {
            assert!(has_exe_extension(Path::new(yes)), "{yes}");
        }
        for no in [
            "a.bat",
            "a.cmd",
            "a.exe.bat",
            "exe",
            "a",
            "a.exe ",
            r"C:\x\",
            "",
        ] {
            assert!(!has_exe_extension(Path::new(no)), "{no:?}");
        }
    }

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"").unwrap();
    }

    #[test]
    fn only_exe_files_inside_an_allowed_folder_are_permitted() {
        let tmp = tempfile::tempdir().unwrap();
        let (allowed, evil) = (tmp.path().join("rt"), tmp.path().join("rt-evil"));
        for file in [
            allowed.join("a.exe"),
            allowed.join("sub").join("b.exe"),
            allowed.join("d.bat"),
            allowed.join("noext"),
            evil.join("c.exe"),
        ] {
            touch(&file);
        }
        let p = WinProcess::new(vec![allowed.clone()], Duration::from_secs(30));
        assert!(p.permitted(&allowed.join("a.exe")));
        assert!(p.permitted(&allowed.join("sub").join("b.exe")));
        // Windows ignores letter case, in the folder and in the extension.
        let shouted = PathBuf::from(allowed.join("a.exe").to_string_lossy().to_uppercase());
        assert!(p.permitted(&shouted), "{}", shouted.display());
        // A `..` that stays inside is fine; one that leaves is not, nor is a sibling folder
        // that only starts with the same letters.
        assert!(p.permitted(&allowed.join("sub").join("..").join("a.exe")));
        let up = allowed
            .join("sub")
            .join("..")
            .join("..")
            .join("rt-evil")
            .join("c.exe");
        assert!(!p.permitted(&up), "{}", up.display());
        assert!(!p.permitted(&evil.join("c.exe")));
        // Not a program, not a file, not there, not an absolute path.
        assert!(!p.permitted(&allowed.join("d.bat")));
        assert!(!p.permitted(&allowed.join("noext")));
        // A folder with a program's name is no program.
        std::fs::create_dir_all(allowed.join("dir.exe")).unwrap();
        assert!(!p.permitted(&allowed.join("dir.exe")));
        assert!(!p.permitted(&allowed));
        assert!(!p.permitted(&allowed.join("missing.exe")));
        assert!(!p.permitted(Path::new("a.exe")));
        // The same through `run`: nothing starts, the program is named.
        let err = p.run(&evil.join("c.exe"), &[]).unwrap_err();
        assert!(err.context.contains("c.exe"), "{err}");
        assert!(
            err.message.contains("not one of CMD Studio's own programs"),
            "{err}"
        );
        // A program may sit under more than one allowed folder list entry.
        let both = WinProcess::new(vec![evil.clone(), allowed.clone()], Duration::from_secs(30));
        assert!(both.permitted(&evil.join("c.exe")) && both.permitted(&allowed.join("a.exe")));
        assert!(!WinProcess::new(vec![], Duration::from_secs(30)).permitted(&allowed.join("a.exe")));
    }

    #[test]
    fn a_junction_cannot_lead_out_of_an_allowed_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let (allowed, evil) = (tmp.path().join("rt"), tmp.path().join("rt-evil"));
        touch(&allowed.join("a.exe"));
        touch(&evil.join("c.exe"));
        let link = allowed.join("link");
        let made = std::process::Command::new(system32().join("cmd.exe"))
            .args(["/d", "/c", "mklink", "/J"])
            .arg(&link)
            .arg(&evil)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .unwrap();
        assert!(made.status.success(), "mklink: {made:?}");
        let p = WinProcess::new(vec![allowed.clone()], Duration::from_secs(30));
        // `link` sits inside the allowed folder, but what it holds is outside.
        assert!(!p.permitted(&link.join("c.exe")));
        assert!(p.permitted(&allowed.join("a.exe")));
        // Only the junction goes, never what it points to.
        std::fs::remove_dir(&link).unwrap();
        assert!(evil.join("c.exe").is_file());
    }

    #[test]
    fn a_link_must_name_and_lead_to_a_program() {
        use std::os::windows::fs::{symlink_dir, symlink_file};
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("rt");
        touch(&allowed.join("tool.exe"));
        touch(&allowed.join("tool.bat"));
        std::fs::create_dir_all(allowed.join("folder")).unwrap();
        let p = WinProcess::new(vec![allowed.clone()], Duration::from_secs(30));
        let links = [
            // Named like a program, leads to a batch file (which would start `cmd`).
            (allowed.join("to-bat.exe"), allowed.join("tool.bat"), false),
            // Named like a batch file, leads to a program.
            (allowed.join("to-exe.bat"), allowed.join("tool.exe"), false),
            // Named like a program, leads to a folder.
            (allowed.join("to-dir.exe"), allowed.join("folder"), true),
        ];
        for (link, target, is_dir) in links {
            let made = if is_dir {
                symlink_dir(&target, &link)
            } else {
                symlink_file(&target, &link)
            };
            if let Err(e) = made {
                // Creating a symbolic link needs Developer Mode or a privilege.
                eprintln!("SKIPPED the symbolic link check: {e}");
                return;
            }
            assert!(!p.permitted(&link), "{} leads out", link.display());
            if is_dir {
                std::fs::remove_dir(&link).unwrap();
            } else {
                std::fs::remove_file(&link).unwrap();
            }
        }
        // A link to a program inside the folder is the program.
        let link = allowed.join("alias.exe");
        if symlink_file(allowed.join("tool.exe"), &link).is_ok() {
            assert!(p.permitted(&link));
            std::fs::remove_file(&link).unwrap();
        }
    }

    #[test]
    fn a_short_8_3_name_leads_to_the_same_place() {
        use crate::adapters::Host;
        let tmp = tempfile::tempdir().unwrap();
        let long = tmp.path().join("a long folder name");
        touch(&long.join("x.exe"));
        let short_dir = crate::adapters::windows::host::WinHost
            .short_path(&long)
            .unwrap();
        if short_dir == long {
            eprintln!("SKIPPED the 8.3 name check: this volume makes no 8.3 names");
            return;
        }
        let (long_exe, short_exe) = (long.join("x.exe"), short_dir.join("x.exe"));
        // Allowed by its long name, run by its short one, and the other way round.
        assert!(WinProcess::new(vec![long.clone()], Duration::from_secs(30)).permitted(&short_exe));
        assert!(WinProcess::new(vec![short_dir], Duration::from_secs(30)).permitted(&long_exe));
    }
}
