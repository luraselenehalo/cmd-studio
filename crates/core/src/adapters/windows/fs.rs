//! Files on the real disk, with atomic replacement (spec §5.4 `Fs`).
use super::io_err;
use crate::adapters::{AdapterError, DirEntry, Fs, FsLock};
use std::io::{ErrorKind, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Windows error codes worth a short retry: another process (Windows
/// Terminal, an antivirus scanner, the search indexer) has the file, or a file
/// inside the folder being renamed, open for a moment.
const ERROR_ACCESS_DENIED: i32 = 5;
const ERROR_SHARING_VIOLATION: i32 = 32;
const RETRIES: u32 = 20;
const RETRY_DELAY: Duration = Duration::from_millis(50);

#[derive(Clone, Copy, Debug, Default)]
pub struct WinFs;

/// The path to write: a symbolic link's target, so a settings.json that is a
/// link into the user's dotfiles stays a link.
fn write_target(path: &Path) -> Result<PathBuf, AdapterError> {
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => {
            std::fs::canonicalize(path).map_err(|e| io_err("resolve link", path, &e))
        }
        _ => Ok(path.to_path_buf()),
    }
}

fn busy(e: &std::io::Error) -> bool {
    matches!(
        e.raw_os_error(),
        Some(ERROR_ACCESS_DENIED | ERROR_SHARING_VIOLATION)
    )
}

/// `std::fs::rename` replaces an existing file in one step on Windows
/// (`MoveFileExW` with `MOVEFILE_REPLACE_EXISTING`, or the POSIX-semantics
/// rename on newer builds). Retries while another process holds the file.
/// `write_atomic` relies on the replacing; `Fs::rename` checks that `to` is
/// free first and uses this only for the retry.
fn replace(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut attempt = 0;
    loop {
        match std::fs::rename(from, to) {
            Err(e) if busy(&e) && attempt < RETRIES => {
                attempt += 1;
                std::thread::sleep(RETRY_DELAY);
            }
            other => return other,
        }
    }
}

impl Fs for WinFs {
    fn read(&self, path: &Path) -> Result<Option<Vec<u8>>, AdapterError> {
        match std::fs::read(path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io_err("read", path, &e)),
        }
    }

    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<(), AdapterError> {
        let target = write_target(path)?;
        let dir = target
            .parent()
            .ok_or_else(|| AdapterError::new(path.display().to_string(), "has no parent folder"))?;
        std::fs::create_dir_all(dir).map_err(|e| io_err("create folder", dir, &e))?;
        let name = target
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let tmp = dir.join(format!(
            ".{name}.{}.cmdstudio-tmp",
            uuid::Uuid::new_v4().simple()
        ));
        let written = (|| -> std::io::Result<()> {
            let mut f = std::fs::File::create_new(&tmp)?;
            f.write_all(bytes)?;
            // FlushFileBuffers: the bytes are on disk before the rename makes them visible.
            f.sync_all()
        })();
        let result = written.and_then(|()| replace(&tmp, &target));
        if let Err(e) = result {
            let _ = std::fs::remove_file(&tmp);
            return Err(io_err("write", path, &e));
        }
        Ok(())
    }

    fn remove_file(&self, path: &Path) -> Result<(), AdapterError> {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(io_err("delete", path, &e)),
        }
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), AdapterError> {
        std::fs::create_dir_all(path).map_err(|e| io_err("create folder", path, &e))
    }

    fn remove_dir(&self, path: &Path) -> Result<(), AdapterError> {
        match std::fs::remove_dir(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(io_err("delete folder", path, &e)),
        }
    }

    fn remove_dir_all(&self, path: &Path) -> Result<(), AdapterError> {
        match std::fs::remove_dir_all(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(io_err("delete folder", path, &e)),
        }
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<(), AdapterError> {
        if to.exists() {
            return Err(AdapterError::new(
                format!("rename to {}", to.display()),
                "it already exists",
            ));
        }
        replace(from, to).map_err(|e| io_err("rename", from, &e))
    }

    fn list_dir(&self, path: &Path) -> Result<Vec<DirEntry>, AdapterError> {
        let entries = match std::fs::read_dir(path) {
            Ok(entries) => entries,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(io_err("list", path, &e)),
        };
        let mut out = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| io_err("list", path, &e))?;
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            out.push(DirEntry {
                path: entry.path(),
                is_dir,
            });
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    /// The open handle is the lock: share mode 0 lets no other handle open the file, in this
    /// process or another, until it is closed, which Windows also does when the process ends.
    /// Another holder shows as `ERROR_SHARING_VIOLATION`; it is not retried, so a second run
    /// learns at once that one is already going.
    fn try_lock(&self, path: &Path) -> Result<Option<FsLock>, AdapterError> {
        let opened = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0)
            .open(path);
        match opened {
            Ok(file) => Ok(Some(FsLock::new(file))),
            Err(e) if e.raw_os_error() == Some(ERROR_SHARING_VIOLATION) => Ok(None),
            Err(e) => Err(io_err("lock", path, &e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Instant;

    /// `FILE_SHARE_READ` alone: others may read the file, but nobody may rename or delete it.
    const FILE_SHARE_READ: u32 = 1;
    /// How long a retry scenario waits for the operation under test to get going. Generous, so
    /// that a slow runner is not mistaken for a failure; a miss fails the test, loudly.
    const DEADLINE: Duration = Duration::from_secs(10);
    /// How long the file stays held once the operation is under way: several retries (50 ms
    /// apart) and well inside the 20 x 50 ms the adapter waits.
    const HOLD: Duration = Duration::from_millis(200);

    #[test]
    fn a_failure_says_windows_text_in_english() {
        // Reading a folder as a file is refused by Windows (ERROR_ACCESS_DENIED). Its text
        // reaches the user, so it is English whatever the display language (spec, 2026-10-13).
        let dir = tempfile::tempdir().unwrap();
        let e = WinFs.read(dir.path()).unwrap_err();
        assert_eq!(e.context, format!("read {}", dir.path().display()));
        assert_eq!(e.message, "Access is denied. (os error 5)");
    }

    fn names_in(dir: &Path) -> Vec<String> {
        std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect()
    }

    /// The temp file `write_atomic` leaves beside `file` while it works.
    fn is_temp_of(file: &str, name: &str) -> bool {
        name.starts_with(&format!(".{file}.")) && name.ends_with(".cmdstudio-tmp")
    }

    /// Opens `path` the way a virus scanner might: others may read it, nobody may rename it.
    fn hold_open(path: &Path) -> std::fs::File {
        std::fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(path)
            .unwrap()
    }

    /// The helper thread of a retry scenario. It keeps `held` open until `ready()` says the
    /// operation under test is under way (polled for up to `DEADLINE`), keeps holding for `HOLD`
    /// more, runs `look`, then lets go. `Err` says why it never got going: a runner too slow to
    /// start the operation in time fails the test with a message, instead of letting it pass
    /// without the retry having been tried.
    fn hold_then_release<T>(
        held: std::fs::File,
        ready: impl Fn() -> Result<bool, String>,
        look: impl FnOnce() -> T,
    ) -> Result<T, String> {
        let deadline = Instant::now() + DEADLINE;
        let outcome = loop {
            match ready() {
                Ok(true) => break Ok(()),
                Err(why) => break Err(why),
                Ok(false) if Instant::now() >= deadline => {
                    break Err(format!("nothing happened within {DEADLINE:?}"))
                }
                Ok(false) => std::thread::sleep(Duration::from_millis(5)),
            }
        };
        if outcome.is_ok() {
            std::thread::sleep(HOLD);
        }
        let seen = look();
        drop(held);
        outcome.map(|()| seen)
    }

    #[test]
    fn write_atomic_replaces_creates_parents_and_leaves_no_temp_file() {
        // Review Focus 4: a folder with Thai letters and a space.
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("ไทย dir").join("settings.json");
        WinFs.write_atomic(&f, b"one").unwrap();
        WinFs.write_atomic(&f, b"two").unwrap();
        assert_eq!(WinFs.read(&f).unwrap(), Some(b"two".to_vec()));
        let names: Vec<_> = std::fs::read_dir(f.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("settings.json")]);

        // Another program that has the file open for a moment (without FILE_SHARE_DELETE) only
        // delays the write: the rename is retried until it lets go. While it waits, the new bytes
        // sit in a temp file beside the target, named as the adapter promises. The helper lets
        // go `HOLD` after that temp file shows up (not after a fixed time), so a slow runner
        // still gives the retry its chance.
        let dir = f.parent().unwrap();
        let held = hold_open(&f);
        let finished = AtomicBool::new(false);
        let (written, watched) = std::thread::scope(|s| {
            let helper = s.spawn(|| {
                hold_then_release(
                    held,
                    || {
                        if names_in(dir).iter().any(|n| is_temp_of("settings.json", n)) {
                            Ok(true)
                        } else if finished.load(Ordering::SeqCst) {
                            Err(
                                "the write ended without leaving a temp file beside the target"
                                    .to_string(),
                            )
                        } else {
                            Ok(false)
                        }
                    },
                    || names_in(dir),
                )
            });
            let written = WinFs.write_atomic(&f, b"three");
            finished.store(true, Ordering::SeqCst);
            (written, helper.join().unwrap())
        });
        written.unwrap();
        let seen = watched.unwrap_or_else(|why| panic!("the helper thread gave up: {why}"));
        assert_eq!(WinFs.read(&f).unwrap(), Some(b"three".to_vec()));
        let waiting: Vec<_> = seen.iter().filter(|n| *n != "settings.json").collect();
        assert!(
            waiting.len() == 1 && is_temp_of("settings.json", waiting[0]),
            "while it waited there was one temp file beside the target: {seen:?}"
        );
        assert_eq!(names_in(dir).len(), 1, "temp file left behind");
    }

    #[test]
    fn a_failed_write_keeps_the_old_file() {
        let tmp = tempfile::tempdir().unwrap();
        // A folder where the file should go makes the rename fail.
        let f = tmp.path().join("x.json");
        std::fs::create_dir(&f).unwrap();
        std::fs::write(f.join("inner"), b"keep").unwrap();
        let started = Instant::now();
        assert!(WinFs.write_atomic(&f, b"new").is_err());
        // "Access denied" is retried for the whole 20 x 50 ms before the write gives up.
        // `thread::sleep` never sleeps less than asked, so this lower bound cannot flake (900,
        // not 1000, leaves room for clock rounding).
        let took = started.elapsed();
        assert!(
            took >= Duration::from_millis(900),
            "gave up after {took:?}, not after the full 20 x 50 ms of retries"
        );
        assert_eq!(std::fs::read(f.join("inner")).unwrap(), b"keep");
        assert_eq!(
            std::fs::read_dir(tmp.path()).unwrap().count(),
            1,
            "temp file left behind"
        );

        // Which errors are worth a retry: access denied (5) and a sharing violation (32), but
        // not "file not found" (2).
        assert!(busy(&std::io::Error::from_raw_os_error(5)));
        assert!(busy(&std::io::Error::from_raw_os_error(32)));
        assert!(!busy(&std::io::Error::from_raw_os_error(2)));
    }

    #[test]
    fn writing_through_a_symlink_keeps_the_link() {
        // Review Focus 5: settings.json linked into the user's dotfiles.
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("dotfiles-settings.json");
        std::fs::write(&real, b"old").unwrap();
        let link = tmp.path().join("settings.json");
        if let Err(e) = std::os::windows::fs::symlink_file(&real, &link) {
            // Creating links needs Developer Mode or admin rights; CI runners and
            // the test VMs have them, so there the test must run, not skip.
            assert!(
                std::env::var("CMDSTUDIO_SYSTEM_TESTS").as_deref() != Ok("1"),
                "cannot create a symlink: {e}"
            );
            println!("SKIPPED writing_through_a_symlink_keeps_the_link: cannot create a symlink here ({e})");
            return;
        }
        WinFs.write_atomic(&link, b"new").unwrap();
        assert!(std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(std::fs::read(&real).unwrap(), b"new");
    }

    #[test]
    fn try_lock_is_exclusive_until_dropped_and_keeps_its_file() {
        // A folder with Thai letters and a space, as a user's data folder may have.
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("ไทย dir");
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("cmdstudio.lock");

        let first = WinFs.try_lock(&path).unwrap().expect("nobody holds it yet");
        assert!(path.is_file(), "the lock file is made");
        assert!(
            WinFs.try_lock(&path).unwrap().is_none(),
            "a second lock is refused while the first is open, in this process too"
        );
        // Share mode 0: no other handle may read the file or delete it while it is held.
        let read = std::fs::read(&path).unwrap_err();
        assert_eq!(read.raw_os_error(), Some(ERROR_SHARING_VIOLATION), "{read}");
        assert!(std::fs::remove_file(&path).is_err());
        assert!(path.is_file(), "and it is still there");

        drop(first);
        assert!(path.is_file(), "the file stays after the lock is let go");
        assert_eq!(std::fs::read(&path).unwrap(), b"", "and opens again");
        let again = WinFs.try_lock(&path).unwrap();
        assert!(again.is_some(), "free again once the first is dropped");
        drop(again);

        // A folder that is not there, or a folder in the file's place, is an error, not "held".
        let missing = dir.join("missing").join("x.lock");
        assert!(WinFs.try_lock(&missing).is_err());
        assert!(!missing.parent().unwrap().exists(), "no folder is made");
        assert!(WinFs.try_lock(&dir).is_err());

        // Deleting a folder that holds a file that is locked deletes entries in the order the
        // folder lists them (by name), depth first, and stops with an error at the locked file:
        // it, and every entry after it, stays. std's remove_dir_all does that here (seen on
        // Windows 11 with Rust 1.99); FakeFs does the same, and this pins it.
        let data = dir.join("data");
        for name in ["a.txt", r"sub\b.txt", "z.txt"] {
            WinFs.write_atomic(&data.join(name), b"x").unwrap();
        }
        let lock = WinFs
            .try_lock(&data.join("cmdstudio.lock"))
            .unwrap()
            .unwrap();
        assert!(WinFs.remove_dir_all(&data).is_err());
        let mut left = names_in(&data);
        left.sort();
        assert_eq!(left, ["cmdstudio.lock", "sub", "z.txt"]);
        drop(lock);
        WinFs.remove_dir_all(&data).unwrap();
        assert!(!data.exists());
        // The same a level down, as FakeFs's test lays it out: what comes before the locked
        // file inside its own folder goes too, and so does an empty folder before it.
        for name in ["a.txt", r"b\x.txt", r"lock-dir\inner.txt", "z.txt"] {
            WinFs.write_atomic(&data.join(name), b"x").unwrap();
        }
        WinFs.create_dir_all(&data.join("a-dir")).unwrap();
        let lock = WinFs
            .try_lock(&data.join(r"lock-dir\l.lock"))
            .unwrap()
            .unwrap();
        assert!(WinFs.remove_dir_all(&data).is_err());
        let mut left = names_in(&data);
        left.sort();
        assert_eq!(left, ["lock-dir", "z.txt"]);
        assert_eq!(names_in(&data.join("lock-dir")), ["l.lock"]);
        drop(lock);
        WinFs.remove_dir_all(&data).unwrap();
        assert!(!data.exists());
    }

    #[test]
    fn missing_things_are_not_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let gone = tmp.path().join("gone");
        assert_eq!(WinFs.read(&gone).unwrap(), None);
        WinFs.remove_file(&gone).unwrap();
        WinFs.remove_dir(&gone).unwrap();
        WinFs.remove_dir_all(&gone).unwrap();
        assert!(WinFs.list_dir(&gone).unwrap().is_empty());
        assert!(!WinFs.exists(&gone) && !WinFs.is_dir(&gone));

        // Things that are there are another matter: `remove_dir` takes an empty folder only,
        // never a file or a folder with something inside (the fake disk refuses those too).
        let full = tmp.path().join("full");
        let inner = full.join("inner.txt");
        WinFs.write_atomic(&inner, b"keep").unwrap();
        assert!(WinFs.exists(&inner) && !WinFs.is_dir(&inner));
        assert!(WinFs.exists(&full) && WinFs.is_dir(&full));
        assert!(
            WinFs.remove_dir(&full).is_err(),
            "a folder with something inside is not empty"
        );
        assert!(WinFs.remove_dir(&inner).is_err(), "a file is not a folder");
        assert_eq!(
            WinFs.read(&inner).unwrap(),
            Some(b"keep".to_vec()),
            "refused calls delete nothing"
        );
        WinFs.remove_file(&inner).unwrap();
        assert_eq!(WinFs.read(&inner).unwrap(), None);
        WinFs.remove_dir(&full).unwrap();
        assert!(!WinFs.exists(&full));

        // `remove_dir_all` takes a whole populated tree with it.
        let deep = full.join("sub").join("x.txt");
        WinFs.write_atomic(&deep, b"x").unwrap();
        assert!(WinFs.exists(&deep));
        WinFs.remove_dir_all(&full).unwrap();
        assert!(!WinFs.exists(&full));
    }

    #[test]
    fn rename_refuses_to_overwrite_and_lists_sorted() {
        let tmp = tempfile::tempdir().unwrap();
        let (a, b) = (tmp.path().join("b-dir"), tmp.path().join("a-dir"));
        WinFs.create_dir_all(&a).unwrap();
        WinFs.create_dir_all(&b).unwrap();
        assert!(WinFs.rename(&a, &b).is_err());
        WinFs.write_atomic(&tmp.path().join("c.txt"), b"c").unwrap();
        let listed: Vec<_> = WinFs
            .list_dir(tmp.path())
            .unwrap()
            .into_iter()
            .map(|e| (e.path.file_name().unwrap().to_owned(), e.is_dir))
            .collect();
        assert_eq!(
            listed,
            vec![
                ("a-dir".into(), true),
                ("b-dir".into(), true),
                ("c.txt".into(), false)
            ]
        );
        WinFs.rename(&a, &tmp.path().join("z-dir")).unwrap();
        assert!(!a.exists());

        // The folder that is to hold the new name must exist, and so must the source. A
        // refused rename changes nothing and makes no folder (the fake disk refuses these too).
        let c = tmp.path().join("c.txt");
        assert!(WinFs
            .rename(&c, &tmp.path().join("nowhere").join("c.txt"))
            .is_err());
        assert_eq!(WinFs.read(&c).unwrap(), Some(b"c".to_vec()));
        assert!(!tmp.path().join("nowhere").exists());
        assert!(WinFs
            .rename(&tmp.path().join("no-such"), &tmp.path().join("other"))
            .is_err());
        assert!(!tmp.path().join("other").exists());

        // The listing is sorted by path (capital letters first), not left in the disk's order:
        // NTFS lists names alphabetically, so only a name like this one tells the two apart.
        WinFs
            .write_atomic(&tmp.path().join("Upper.txt"), b"u")
            .unwrap();
        let order: Vec<_> = WinFs
            .list_dir(tmp.path())
            .unwrap()
            .into_iter()
            .map(|e| e.path.file_name().unwrap().to_owned())
            .collect();
        assert_eq!(order, vec!["Upper.txt", "a-dir", "c.txt", "z-dir"]);

        // A file does not replace a file either: the rename is refused and both keep their bytes.
        let (one, two) = (tmp.path().join("one.txt"), tmp.path().join("two.txt"));
        WinFs.write_atomic(&one, b"1").unwrap();
        WinFs.write_atomic(&two, b"2").unwrap();
        assert!(WinFs.rename(&one, &two).is_err());
        assert_eq!(WinFs.read(&one).unwrap(), Some(b"1".to_vec()));
        assert_eq!(WinFs.read(&two).unwrap(), Some(b"2".to_vec()));

        // Installing a runtime copies its files into a ".partial" folder and renames the folder
        // into place. A scanner that holds one of the new files for a moment (without
        // FILE_SHARE_DELETE) must only delay the rename. Whether the rename has begun cannot
        // be seen from outside, so the helper lets go `HOLD` after this thread signals that it
        // is about to call it.
        let staging = tempfile::tempdir().unwrap();
        let partial = staging.path().join("runtime.partial");
        let inside = partial.join("clink_x64.exe");
        WinFs.write_atomic(&inside, b"bytes").unwrap();
        let moved = staging.path().join("runtime-1");
        let held = hold_open(&inside);
        let about_to_rename = AtomicBool::new(false);
        let (renamed, watched) = std::thread::scope(|s| {
            let helper = s.spawn(|| {
                hold_then_release(held, || Ok(about_to_rename.load(Ordering::SeqCst)), || ())
            });
            about_to_rename.store(true, Ordering::SeqCst);
            let renamed = WinFs.rename(&partial, &moved);
            (renamed, helper.join().unwrap())
        });
        renamed.unwrap();
        watched.unwrap_or_else(|why| panic!("the helper thread gave up: {why}"));
        assert!(!partial.exists(), "the folder is gone from its old name");
        assert_eq!(
            WinFs.read(&moved.join("clink_x64.exe")).unwrap(),
            Some(b"bytes".to_vec()),
            "and is at the new name with its file"
        );
    }

    /// Real-data seam: the files scripts\fetch-resources.ps1 put in resources\
    /// match their pins, read through the real adapter. Reads only.
    #[test]
    fn the_fetched_resources_match_their_pins() {
        use crate::plan::Component;
        use crate::resources::{check_bundle, Resources};
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("resources");
        if !root.join("manifest.json").is_file() {
            assert!(
                std::env::var("CMDSTUDIO_SYSTEM_TESTS").as_deref() != Ok("1"),
                "run scripts/fetch-resources.ps1 first"
            );
            println!("SKIPPED the_fetched_resources_match_their_pins: no resources folder yet");
            return;
        }
        let res = Resources { root };
        let m = res.manifest(&WinFs).unwrap();
        check_bundle(&WinFs, &res.dir(Component::Omp), &m.omp).unwrap();
        check_bundle(&WinFs, &res.dir(Component::Clink), &m.clink).unwrap();
        check_bundle(&WinFs, &res.root.join("fonts"), &m.fonts).unwrap();
    }
}
