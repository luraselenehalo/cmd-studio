//! In-memory adapters for tests (this crate's and, through the `fake`
//! feature, crates/cli's). They never touch the machine.
use super::{
    AdapterError, Arch, AutoRun, Created, DefaultTerminal, DirEntry, DirState, Fonts, Fs, FsLock,
    Host, Output, Process, RegValue, System, SystemInfo, WtLocation, WtSettings,
};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

fn err(context: &Path, message: &str) -> AdapterError {
    AdapterError::new(context.display().to_string(), message)
}

/// The paths that a live `FakeFs::try_lock` guard holds, shared with the guards.
type Locks = Arc<Mutex<BTreeSet<PathBuf>>>;

/// The set itself. A guard that panicked while holding the mutex left a set that is still
/// whole, so a poisoned mutex is used as it is.
fn held(locks: &Locks) -> MutexGuard<'_, BTreeSet<PathBuf>> {
    locks.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Keeps one path of a `FakeFs` locked until it is dropped.
struct FakeLock {
    locks: Locks,
    path: PathBuf,
}

impl Drop for FakeLock {
    fn drop(&mut self) {
        held(&self.locks).remove(&self.path);
    }
}

#[derive(Default)]
struct FsState {
    files: BTreeMap<PathBuf, Vec<u8>>,
    dirs: BTreeSet<PathBuf>,
    fail_writes: BTreeSet<PathBuf>,
    fail_removes: BTreeSet<PathBuf>,
    fail_reads: BTreeSet<PathBuf>,
}

/// Files and folders in memory. Paths compare exactly (case-sensitive).
#[derive(Default)]
pub struct FakeFs {
    state: RefCell<FsState>,
    /// Files locked by `try_lock`. While a file is locked, reading, writing, deleting or moving
    /// it (or a folder above it) fails, as share mode 0 makes it on the disk.
    locks: Locks,
}

impl FakeFs {
    pub fn new() -> Self {
        Self::default()
    }

    fn is_locked(&self, path: &Path) -> bool {
        held(&self.locks).contains(path)
    }

    /// Whether `path`, or anything inside it, is locked.
    fn holds_a_lock(&self, path: &Path) -> bool {
        held(&self.locks).iter().any(|p| p.starts_with(path))
    }

    /// Puts a file in place (and its parent folders), bypassing failure injection.
    pub fn put(&self, path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) {
        let mut s = self.state.borrow_mut();
        add_parents(&mut s.dirs, path.as_ref());
        s.files
            .insert(path.as_ref().to_path_buf(), bytes.as_ref().to_vec());
    }

    pub fn file(&self, path: impl AsRef<Path>) -> Option<Vec<u8>> {
        self.state.borrow().files.get(path.as_ref()).cloned()
    }

    /// Every file and folder, for byte-for-byte comparisons of the whole fake disk.
    pub fn snapshot(&self) -> (BTreeMap<PathBuf, Vec<u8>>, BTreeSet<PathBuf>) {
        let s = self.state.borrow();
        (s.files.clone(), s.dirs.clone())
    }

    /// Makes every later write to `path` fail.
    pub fn fail_writes_to(&self, path: impl AsRef<Path>) {
        self.state
            .borrow_mut()
            .fail_writes
            .insert(path.as_ref().to_path_buf());
    }

    /// Makes every later read of exactly `path` fail, as when the file is locked or the user may
    /// not read it. The file, if there, stays: `exists`, `file` and `put` are not affected, and a
    /// write does not clear it.
    pub fn fail_reads_of(&self, path: impl AsRef<Path>) {
        self.state
            .borrow_mut()
            .fail_reads
            .insert(path.as_ref().to_path_buf());
    }

    /// Makes every later removal of `path` fail, as when a file is in use: `remove_file` or
    /// `remove_dir` of that path, and `remove_dir_all` or `rename` of that path or of a folder
    /// above it.
    pub fn fail_removes_of(&self, path: impl AsRef<Path>) {
        self.state
            .borrow_mut()
            .fail_removes
            .insert(path.as_ref().to_path_buf());
    }
}

fn add_parents(dirs: &mut BTreeSet<PathBuf>, path: &Path) {
    for a in path.ancestors().skip(1) {
        if !a.as_os_str().is_empty() {
            dirs.insert(a.to_path_buf());
        }
    }
}

/// Whether the folder that would hold `path` exists. A folder with no name of its own always
/// does: a volume root, the current folder (`.`, or what a bare file name leaves) and `..`.
fn parent_exists(dirs: &BTreeSet<PathBuf>, path: &Path) -> bool {
    path.parent()
        .is_none_or(|p| p.file_name().is_none() || dirs.contains(p))
}

impl Fs for FakeFs {
    fn read(&self, path: &Path) -> Result<Option<Vec<u8>>, AdapterError> {
        if self.is_locked(path) {
            return Err(err(path, "the file is locked"));
        }
        let s = self.state.borrow();
        if s.fail_reads.contains(path) {
            return Err(err(path, "read failed (injected)"));
        }
        Ok(s.files.get(path).cloned())
    }

    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<(), AdapterError> {
        if self.is_locked(path) {
            return Err(err(path, "the file is locked"));
        }
        let mut s = self.state.borrow_mut();
        if s.fail_writes.contains(path) {
            return Err(err(path, "write failed (injected)"));
        }
        if s.dirs.contains(path) {
            return Err(err(path, "is a folder"));
        }
        add_parents(&mut s.dirs, path);
        s.files.insert(path.to_path_buf(), bytes.to_vec());
        Ok(())
    }

    fn remove_file(&self, path: &Path) -> Result<(), AdapterError> {
        if self.is_locked(path) {
            return Err(err(path, "the file is locked"));
        }
        let mut s = self.state.borrow_mut();
        if s.fail_removes.contains(path) {
            return Err(err(path, "file is in use (injected)"));
        }
        s.files.remove(path);
        Ok(())
    }

    fn exists(&self, path: &Path) -> bool {
        let s = self.state.borrow();
        s.files.contains_key(path) || s.dirs.contains(path)
    }

    fn is_dir(&self, path: &Path) -> bool {
        self.state.borrow().dirs.contains(path)
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), AdapterError> {
        let mut s = self.state.borrow_mut();
        if s.files.contains_key(path) {
            return Err(err(path, "is a file"));
        }
        add_parents(&mut s.dirs, path);
        s.dirs.insert(path.to_path_buf());
        Ok(())
    }

    fn remove_dir(&self, path: &Path) -> Result<(), AdapterError> {
        let mut s = self.state.borrow_mut();
        if s.fail_removes.contains(path) {
            return Err(err(path, "folder is in use (injected)"));
        }
        if s.files.contains_key(path) {
            return Err(err(path, "is a file, not a folder"));
        }
        let busy = s.files.keys().any(|f| f.parent() == Some(path))
            || s.dirs.iter().any(|d| d.parent() == Some(path));
        if busy {
            return Err(err(path, "folder is not empty"));
        }
        s.dirs.remove(path);
        Ok(())
    }

    fn remove_dir_all(&self, path: &Path) -> Result<(), AdapterError> {
        if self.holds_a_lock(path) {
            return Err(err(path, "a file inside is locked"));
        }
        let mut s = self.state.borrow_mut();
        if s.fail_removes.iter().any(|f| f.starts_with(path)) {
            return Err(err(path, "a file inside is in use (injected)"));
        }
        s.files.retain(|f, _| !f.starts_with(path));
        s.dirs.retain(|d| !d.starts_with(path));
        Ok(())
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<(), AdapterError> {
        if self.holds_a_lock(from) {
            return Err(err(from, "it, or a file inside, is locked"));
        }
        let mut s = self.state.borrow_mut();
        if s.files.contains_key(to) || s.dirs.contains(to) {
            return Err(err(to, "already exists"));
        }
        if !parent_exists(&s.dirs, to) {
            return Err(err(to, "parent folder not found"));
        }
        if s.fail_removes.iter().any(|f| f.starts_with(from)) {
            return Err(err(from, "in use (injected)"));
        }
        // The renamed root becomes `to` itself; `to.join("")` would end in a separator.
        let moved = |p: &Path| {
            p.strip_prefix(from).ok().map(|rest| {
                if rest.as_os_str().is_empty() {
                    to.to_path_buf()
                } else {
                    to.join(rest)
                }
            })
        };
        let files: Vec<(PathBuf, Vec<u8>)> = s
            .files
            .iter()
            .filter_map(|(p, b)| moved(p).map(|n| (n, b.clone())))
            .collect();
        let dirs: Vec<PathBuf> = s.dirs.iter().filter_map(|p| moved(p)).collect();
        if files.is_empty() && dirs.is_empty() {
            return Err(err(from, "not found"));
        }
        s.files.retain(|p, _| !p.starts_with(from));
        s.dirs.retain(|p| !p.starts_with(from));
        // The folder of `to` exists, so this only records a volume root.
        add_parents(&mut s.dirs, to);
        s.files.extend(files);
        s.dirs.extend(dirs);
        Ok(())
    }

    fn list_dir(&self, path: &Path) -> Result<Vec<DirEntry>, AdapterError> {
        let s = self.state.borrow();
        let mut out: Vec<DirEntry> = s
            .files
            .keys()
            .filter(|f| f.parent() == Some(path))
            .map(|f| DirEntry {
                path: f.clone(),
                is_dir: false,
            })
            .chain(
                s.dirs
                    .iter()
                    .filter(|d| d.parent() == Some(path))
                    .map(|d| DirEntry {
                        path: d.clone(),
                        is_dir: true,
                    }),
            )
            .collect();
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    /// Like the disk: the folder must exist, a folder is not a file, a file that cannot be
    /// written cannot be opened to lock it, and the file is made (empty) when missing and stays.
    fn try_lock(&self, path: &Path) -> Result<Option<FsLock>, AdapterError> {
        {
            let s = self.state.borrow();
            if s.dirs.contains(path) {
                return Err(err(path, "is a folder"));
            }
            if !parent_exists(&s.dirs, path) {
                return Err(err(path, "parent folder not found"));
            }
            if s.fail_writes.contains(path) {
                return Err(err(path, "open for writing failed (injected)"));
            }
        }
        if !held(&self.locks).insert(path.to_path_buf()) {
            return Ok(None);
        }
        self.state
            .borrow_mut()
            .files
            .entry(path.to_path_buf())
            .or_default();
        Ok(Some(FsLock::new(FakeLock {
            locks: Arc::clone(&self.locks),
            path: path.to_path_buf(),
        })))
    }
}

#[derive(Default)]
struct AutoRunState {
    key: bool,
    user: Option<RegValue>,
    machine: Option<RegValue>,
    fail_writes: bool,
}

/// The `Command Processor` keys in memory. The user key exists by default.
pub struct FakeAutoRun {
    state: RefCell<AutoRunState>,
}

impl Default for FakeAutoRun {
    fn default() -> Self {
        FakeAutoRun {
            state: RefCell::new(AutoRunState {
                key: true,
                ..AutoRunState::default()
            }),
        }
    }
}

impl FakeAutoRun {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn set_machine(&self, value: Option<RegValue>) {
        self.state.borrow_mut().machine = value;
    }
    pub fn set_key_exists(&self, exists: bool) {
        self.state.borrow_mut().key = exists;
    }
    pub fn fail_writes(&self) {
        self.state.borrow_mut().fail_writes = true;
    }
    /// `(key exists, user value)`, for byte-for-byte comparisons.
    pub fn snapshot(&self) -> (bool, Option<RegValue>) {
        let s = self.state.borrow();
        (s.key, s.user.clone())
    }
}

impl AutoRun for FakeAutoRun {
    fn user_key_exists(&self) -> Result<bool, AdapterError> {
        Ok(self.state.borrow().key)
    }
    fn read_user(&self) -> Result<Option<RegValue>, AdapterError> {
        Ok(self.state.borrow().user.clone())
    }
    fn write_user(&self, value: &RegValue) -> Result<(), AdapterError> {
        let mut s = self.state.borrow_mut();
        if s.fail_writes {
            return Err(AdapterError::new("AutoRun", "access denied (injected)"));
        }
        s.key = true;
        s.user = Some(value.clone());
        Ok(())
    }
    fn delete_user(&self) -> Result<(), AdapterError> {
        let mut s = self.state.borrow_mut();
        if s.fail_writes {
            return Err(AdapterError::new("AutoRun", "access denied (injected)"));
        }
        s.user = None;
        Ok(())
    }
    fn delete_user_key_if_empty(&self) -> Result<(), AdapterError> {
        let mut s = self.state.borrow_mut();
        if s.user.is_none() {
            s.key = false;
        }
        Ok(())
    }
    fn read_machine(&self) -> Result<Option<RegValue>, AdapterError> {
        Ok(self.state.borrow().machine.clone())
    }
}

#[derive(Default)]
struct FontsState {
    families: BTreeSet<String>,
    registry: BTreeMap<String, String>,
    loaded: BTreeSet<PathBuf>,
    /// File name → the family GDI would report once the file is loaded.
    file_families: BTreeMap<String, String>,
    /// The registration key is absent, as on an account that never had a per-user font.
    key_missing: bool,
    /// `families` fails.
    fail_families: bool,
    /// Value names whose `registration` fails.
    fail_registration: BTreeSet<String>,
}

/// Per-user fonts in memory.
pub struct FakeFonts {
    dir: PathBuf,
    state: RefCell<FontsState>,
}

impl FakeFonts {
    pub fn new(dir: impl Into<PathBuf>, families: &[&str]) -> Self {
        let state = FontsState {
            families: families.iter().map(|f| (*f).to_string()).collect(),
            ..FontsState::default()
        };
        FakeFonts {
            dir: dir.into(),
            state: RefCell::new(state),
        }
    }
    /// Declares which family a font file holds.
    pub fn knows_file(&self, file_name: &str, family: &str) {
        self.state
            .borrow_mut()
            .file_families
            .insert(file_name.to_string(), family.to_string());
    }
    pub fn set_key_exists(&self, exists: bool) {
        self.state.borrow_mut().key_missing = !exists;
    }
    /// Makes every later `families` fail, as the real one does when GDI lists nothing.
    pub fn fail_families(&self) {
        self.state.borrow_mut().fail_families = true;
    }
    /// Makes every later `registration(value_name)` fail, as the real one does for a value that
    /// is not plain `REG_SZ` text (`REG_EXPAND_SZ`, `REG_DWORD`, bad UTF-16). Other value names,
    /// `register` and `unregister` are not affected, and the stored data stays as it was.
    pub fn fail_registration_of(&self, value_name: &str) {
        self.state
            .borrow_mut()
            .fail_registration
            .insert(value_name.to_string());
    }
    /// `(registry values, loaded files, whether the registration key exists)`, for comparisons.
    pub fn snapshot(&self) -> (BTreeMap<String, String>, BTreeSet<PathBuf>, bool) {
        let s = self.state.borrow();
        (s.registry.clone(), s.loaded.clone(), !s.key_missing)
    }
}

impl Fonts for FakeFonts {
    fn families(&self) -> Result<Vec<String>, AdapterError> {
        let s = self.state.borrow();
        if s.fail_families {
            return Err(AdapterError::new(
                "EnumFontFamiliesExW",
                "GDI listed no font families (injected)",
            ));
        }
        let mut all = s.families.clone();
        for file in &s.loaded {
            let name = file.file_name().and_then(OsStr::to_str).unwrap_or_default();
            if let Some(family) = s.file_families.get(name) {
                all.insert(family.clone());
            }
        }
        Ok(all.into_iter().collect())
    }
    fn user_fonts_dir(&self) -> PathBuf {
        self.dir.clone()
    }
    fn registration(&self, value_name: &str) -> Result<Option<String>, AdapterError> {
        let s = self.state.borrow();
        if s.fail_registration.contains(value_name) {
            return Err(AdapterError::new(
                format!("read {value_name}"),
                "the value is not plain REG_SZ text (injected)",
            ));
        }
        Ok(s.registry.get(value_name).cloned())
    }
    fn register(&self, value_name: &str, file: &str) -> Result<(), AdapterError> {
        let mut s = self.state.borrow_mut();
        s.key_missing = false;
        s.registry.insert(value_name.to_string(), file.to_string());
        Ok(())
    }
    fn unregister(&self, value_name: &str) -> Result<(), AdapterError> {
        self.state.borrow_mut().registry.remove(value_name);
        Ok(())
    }
    fn registration_key_exists(&self) -> Result<bool, AdapterError> {
        Ok(!self.state.borrow().key_missing)
    }
    fn delete_registration_key_if_empty(&self) -> Result<(), AdapterError> {
        let mut s = self.state.borrow_mut();
        if s.registry.is_empty() {
            s.key_missing = true;
        }
        Ok(())
    }
    fn load(&self, file: &Path) -> Result<(), AdapterError> {
        self.state.borrow_mut().loaded.insert(file.to_path_buf());
        Ok(())
    }
    fn unload(&self, file: &Path) -> Result<(), AdapterError> {
        self.state.borrow_mut().loaded.remove(file);
        Ok(())
    }
}

/// Windows Terminal locations given by the test.
#[derive(Default)]
pub struct FakeWt {
    pub locations: Vec<WtLocation>,
}

impl WtSettings for FakeWt {
    fn locations(&self) -> Vec<WtLocation> {
        self.locations.clone()
    }
}

/// One recorded call to `FakeProcess::run`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Call {
    pub exe: PathBuf,
    pub args: Vec<String>,
}

#[derive(Default)]
struct ProcessState {
    calls: Vec<Call>,
    /// (exe file name, first argument) → output.
    answers: BTreeMap<(String, String), Output>,
}

/// Answers each call from a table keyed by the exe's file name and the first
/// argument. Unknown calls succeed with empty output.
#[derive(Default)]
pub struct FakeProcess {
    state: RefCell<ProcessState>,
}

impl FakeProcess {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn answer(&self, exe_name: &str, first_arg: &str, status: i32, stdout: &str) {
        let out = Output {
            status,
            stdout: stdout.to_string(),
            stderr: String::new(),
        };
        self.state
            .borrow_mut()
            .answers
            .insert((exe_name.to_string(), first_arg.to_string()), out);
    }
    pub fn calls(&self) -> Vec<Call> {
        self.state.borrow().calls.clone()
    }
}

impl Process for FakeProcess {
    fn run(&self, exe: &Path, args: &[&OsStr]) -> Result<Output, AdapterError> {
        let args: Vec<String> = args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let name = exe
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or_default()
            .to_string();
        let first = args.first().cloned().unwrap_or_default();
        let mut s = self.state.borrow_mut();
        s.calls.push(Call {
            exe: exe.to_path_buf(),
            args,
        });
        Ok(s.answers.get(&(name, first)).cloned().unwrap_or(Output {
            status: 0,
            stdout: String::new(),
            stderr: String::new(),
        }))
    }
}

/// Identity and system facts given by the test.
pub struct FakeHost {
    pub local_app_data: PathBuf,
    pub program_data: PathBuf,
    pub sid: String,
    /// Long path → what `GetShortPathNameW` returns. Paths not listed come back unchanged.
    pub short: BTreeMap<PathBuf, PathBuf>,
    pub info: SystemInfo,
    private: RefCell<BTreeMap<PathBuf, DirState>>,
}

impl Default for FakeHost {
    fn default() -> Self {
        FakeHost {
            local_app_data: PathBuf::from(r"C:\Users\tester\AppData\Local"),
            program_data: PathBuf::from(r"C:\ProgramData"),
            sid: "S-1-5-21-1-2-3-1001".to_string(),
            short: BTreeMap::new(),
            info: SystemInfo {
                build: Some(26100),
                arch: Arch::X64,
                default_terminal: DefaultTerminal::LetWindowsDecide,
                webview2: None,
                packaged: false,
                ui_language: 0x0409,
                unreadable: Vec::new(),
            },
            private: RefCell::new(BTreeMap::new()),
        }
    }
}

impl FakeHost {
    pub fn set_dir_state(&self, dir: impl Into<PathBuf>, state: DirState) {
        self.private.borrow_mut().insert(dir.into(), state);
    }
    pub fn dir_state(&self, dir: &Path) -> DirState {
        self.private
            .borrow()
            .get(dir)
            .cloned()
            .unwrap_or(DirState::Absent)
    }
}

impl Host for FakeHost {
    fn local_app_data(&self) -> Result<PathBuf, AdapterError> {
        Ok(self.local_app_data.clone())
    }
    fn program_data(&self) -> Result<PathBuf, AdapterError> {
        Ok(self.program_data.clone())
    }
    fn user_sid(&self) -> Result<String, AdapterError> {
        Ok(self.sid.clone())
    }
    fn short_path(&self, path: &Path) -> Result<PathBuf, AdapterError> {
        Ok(self
            .short
            .get(path)
            .cloned()
            .unwrap_or_else(|| path.to_path_buf()))
    }
    fn private_dir_state(&self, dir: &Path) -> Result<DirState, AdapterError> {
        Ok(self.dir_state(dir))
    }
    fn create_private_dir(&self, dir: &Path) -> Result<Created, AdapterError> {
        let mut map = self.private.borrow_mut();
        if map.get(dir).is_some_and(|s| *s != DirState::Absent) {
            return Ok(Created::AlreadyExisted);
        }
        map.insert(dir.to_path_buf(), DirState::Private);
        Ok(Created::New)
    }
    fn system_info(&self) -> SystemInfo {
        self.info.clone()
    }
}

/// One of each fake, for tests that need a whole `System`.
pub struct FakeMachine {
    pub fs: FakeFs,
    pub autorun: FakeAutoRun,
    pub fonts: FakeFonts,
    pub wt: FakeWt,
    pub process: FakeProcess,
    pub host: FakeHost,
}

impl FakeMachine {
    pub fn new() -> Self {
        FakeMachine {
            fs: FakeFs::new(),
            autorun: FakeAutoRun::new(),
            fonts: FakeFonts::new(
                r"C:\Users\tester\AppData\Local\Microsoft\Windows\Fonts",
                &["Arial", "Cascadia Mono"],
            ),
            wt: FakeWt::default(),
            process: FakeProcess::new(),
            host: FakeHost::default(),
        }
    }

    pub fn system(&self) -> System<'_> {
        System {
            fs: &self.fs,
            autorun: &self.autorun,
            fonts: &self.fonts,
            wt: &self.wt,
            process: &self.process,
            host: &self.host,
        }
    }
}

impl Default for FakeMachine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::REG_SZ;

    #[test]
    fn fake_fs_behaves_like_a_disk() {
        let fs = FakeFs::new();
        let f = Path::new(r"C:\d\sub\a.txt");
        fs.write_atomic(f, b"one").unwrap();
        assert!(fs.is_dir(Path::new(r"C:\d\sub")));
        assert_eq!(fs.read(f).unwrap(), Some(b"one".to_vec()));
        assert!(fs.remove_dir(Path::new(r"C:\d\sub")).is_err());
        fs.rename(Path::new(r"C:\d\sub"), Path::new(r"C:\d\moved"))
            .unwrap();
        assert_eq!(
            fs.read(Path::new(r"C:\d\moved\a.txt")).unwrap(),
            Some(b"one".to_vec())
        );
        assert_eq!(fs.read(f).unwrap(), None);
        assert_eq!(
            fs.list_dir(Path::new(r"C:\d")).unwrap(),
            vec![DirEntry {
                path: PathBuf::from(r"C:\d\moved"),
                is_dir: true
            }]
        );
        fs.remove_dir_all(Path::new(r"C:\d")).unwrap();
        assert!(!fs.exists(Path::new(r"C:\d\moved\a.txt")));
        fs.remove_file(f).unwrap();
        fs.fail_writes_to(f);
        assert!(fs.write_atomic(f, b"x").is_err());

        // The fake is as strict as the disk. A renamed folder or file is listed under the name
        // it was given, with no trailing separator.
        fs.put(r"C:\g\old\x.txt", b"x");
        fs.rename(Path::new(r"C:\g\old"), Path::new(r"C:\g\new"))
            .unwrap();
        fs.rename(Path::new(r"C:\g\new\x.txt"), Path::new(r"C:\g\new\y.txt"))
            .unwrap();
        let listed = |dir: &str| -> Vec<String> {
            fs.list_dir(Path::new(dir))
                .unwrap()
                .iter()
                .map(|e| e.path.display().to_string())
                .collect()
        };
        assert_eq!(
            listed(r"C:\g"),
            [r"C:\g\new"],
            "a renamed folder keeps the name it was given"
        );
        assert_eq!(
            listed(r"C:\g\new"),
            [r"C:\g\new\y.txt"],
            "a renamed file keeps the name it was given"
        );

        // A folder that is in use is not removed, and stays.
        let busy = Path::new(r"C:\busy");
        fs.create_dir_all(busy).unwrap();
        fs.fail_removes_of(busy);
        assert!(
            fs.remove_dir(busy).is_err(),
            "a folder that is in use is not removed"
        );
        assert!(fs.is_dir(busy), "and it is still there");

        // A file is not an empty folder.
        assert!(
            fs.remove_dir(Path::new(r"C:\g\new\y.txt")).is_err(),
            "a file is not a folder"
        );
        assert_eq!(
            fs.file(r"C:\g\new\y.txt"),
            Some(b"x".to_vec()),
            "and it is still there"
        );

        // A rename needs the folder of the new name to exist; a refused one changes nothing.
        assert!(
            fs.rename(Path::new(r"C:\g\new\y.txt"), Path::new(r"C:\nowhere\y.txt"))
                .is_err(),
            "the folder of the new name must exist"
        );
        assert_eq!(
            fs.file(r"C:\g\new\y.txt"),
            Some(b"x".to_vec()),
            "a refused rename leaves the source alone"
        );
        assert!(!fs.exists(Path::new(r"C:\nowhere")), "and makes no folder");

        // A name with no folder part is in the current folder, which is always there.
        fs.put("loose.txt", b"l");
        fs.rename(Path::new("loose.txt"), Path::new("kept.txt"))
            .unwrap();
        assert_eq!(fs.file("kept.txt"), Some(b"l".to_vec()));
    }

    #[test]
    fn fake_fs_can_fail_the_read_of_one_path() {
        let fs = FakeFs::new();
        let f = Path::new(r"C:\d\a.txt");
        let other = Path::new(r"C:\d\b.txt");
        fs.put(f, b"a");
        fs.put(other, b"b");
        assert_eq!(fs.read(f).unwrap(), Some(b"a".to_vec()));

        fs.fail_reads_of(f);
        let e = fs.read(f).unwrap_err();
        assert!(e.message.contains("(injected)"), "{e}");
        assert!(
            fs.read(f).is_err(),
            "it keeps failing, as the other hooks do"
        );
        // Only that exact path, and only reads: the file is still there.
        assert_eq!(fs.read(other).unwrap(), Some(b"b".to_vec()));
        assert!(fs.exists(f));
        assert_eq!(fs.file(f), Some(b"a".to_vec()));
        fs.write_atomic(f, b"new").unwrap();
        assert!(fs.read(f).is_err(), "a write does not clear it");
        assert_eq!(fs.file(f), Some(b"new".to_vec()));

        // A path that has no file fails too, and a path inside a failing folder name does not.
        fs.fail_reads_of(r"C:\d\none.txt");
        assert!(fs.read(Path::new(r"C:\d\none.txt")).is_err());
        fs.fail_reads_of(r"C:\d\dir");
        fs.put(r"C:\d\dir\x.txt", b"x");
        assert_eq!(
            fs.read(Path::new(r"C:\d\dir\x.txt")).unwrap(),
            Some(b"x".to_vec())
        );
    }

    #[test]
    fn fake_fs_lock_is_exclusive_until_dropped_and_keeps_its_file() {
        let fs = FakeFs::new();
        let dir = Path::new(r"C:\d");
        let path = dir.join("app.lock");
        assert!(
            fs.try_lock(&path).is_err(),
            "the folder must exist, as on the disk"
        );
        fs.create_dir_all(dir).unwrap();

        let first = fs.try_lock(&path).unwrap().expect("nobody holds it yet");
        assert_eq!(fs.file(&path), Some(Vec::new()), "the lock file is made");
        assert!(
            fs.try_lock(&path).unwrap().is_none(),
            "a second lock is refused while the first lives"
        );
        // Nobody may open the file while it is held: not to read it, write it or delete it, nor
        // to move or delete the folder around it.
        assert!(fs.read(&path).is_err());
        assert!(fs.write_atomic(&path, b"x").is_err());
        assert!(fs.remove_file(&path).is_err());
        assert!(fs.remove_dir_all(dir).is_err());
        assert!(fs.rename(dir, Path::new(r"C:\e")).is_err());
        assert_eq!(fs.file(&path), Some(Vec::new()), "and it is still there");
        // Another path is another lock.
        let other = fs.try_lock(&dir.join("other.lock")).unwrap();
        assert!(other.is_some());

        drop(first);
        assert_eq!(fs.file(&path), Some(Vec::new()), "the file stays");
        assert_eq!(fs.read(&path).unwrap(), Some(Vec::new()), "and opens again");
        let again = fs.try_lock(&path).unwrap();
        assert!(again.is_some(), "free again once the first is dropped");
        drop((again, other));
        fs.remove_dir_all(dir).unwrap();

        // A file that cannot be opened for writing cannot be locked either; a folder is no file.
        fs.create_dir_all(dir).unwrap();
        fs.fail_writes_to(&path);
        assert!(fs.try_lock(&path).is_err());
        assert!(fs.try_lock(dir).is_err());
    }

    #[test]
    fn fake_autorun_keeps_kind_and_bytes() {
        let a = FakeAutoRun::new();
        let v = RegValue {
            kind: 2,
            bytes: vec![0x41, 0, 0, 0, 0x42, 0],
        };
        a.write_user(&v).unwrap();
        assert_eq!(a.read_user().unwrap(), Some(v));
        a.delete_user().unwrap();
        a.delete_user_key_if_empty().unwrap();
        assert_eq!(a.snapshot(), (false, None));
        a.write_user(&RegValue::from_text(REG_SZ, "x")).unwrap();
        assert!(a.user_key_exists().unwrap());
    }

    #[test]
    fn fake_fonts_report_loaded_files_and_the_registration_key() {
        let f = FakeFonts::new(r"C:\fonts", &["Arial"]);
        f.knows_file("n.ttf", "CaskaydiaCove NF");
        f.load(Path::new(r"C:\fonts\n.ttf")).unwrap();
        assert_eq!(
            f.families().unwrap(),
            vec!["Arial".to_string(), "CaskaydiaCove NF".to_string()]
        );
        f.unload(Path::new(r"C:\fonts\n.ttf")).unwrap();
        assert_eq!(f.families().unwrap(), vec!["Arial".to_string()]);

        f.set_key_exists(false);
        f.register("N (TrueType)", r"C:\fonts\n.ttf").unwrap();
        assert!(f.registration_key_exists().unwrap());
        f.delete_registration_key_if_empty().unwrap();
        assert!(
            f.registration_key_exists().unwrap(),
            "a key with values stays"
        );
        f.unregister("N (TrueType)").unwrap();
        f.delete_registration_key_if_empty().unwrap();
        assert!(!f.registration_key_exists().unwrap());
    }

    #[test]
    fn fake_fonts_can_fail_the_family_list() {
        // The real adapter fails like this when GDI lists nothing.
        let f = FakeFonts::new(r"C:\fonts", &["Arial"]);
        f.register("A (TrueType)", r"C:\fonts\a.ttf").unwrap();
        let before = f.snapshot();
        assert_eq!(f.families().unwrap(), vec!["Arial".to_string()]);
        f.fail_families();
        let e = f.families().unwrap_err();
        assert_eq!(e.context, "EnumFontFamiliesExW");
        assert!(e.message.contains("injected"), "{e}");
        assert!(f.families().is_err(), "it keeps failing");
        // Nothing else is affected: the registry, loading and the key still work.
        assert_eq!(
            f.registration("A (TrueType)").unwrap().as_deref(),
            Some(r"C:\fonts\a.ttf")
        );
        f.load(Path::new(r"C:\fonts\a.ttf")).unwrap();
        f.unload(Path::new(r"C:\fonts\a.ttf")).unwrap();
        assert_eq!(f.snapshot(), before);
    }

    #[test]
    fn fake_fonts_can_fail_the_registration_of_one_value() {
        // The real adapter fails like this for a value that is not plain REG_SZ text.
        let f = FakeFonts::new(r"C:\fonts", &["Arial"]);
        f.register("A (TrueType)", r"C:\fonts\a.ttf").unwrap();
        f.register("B (TrueType)", r"C:\fonts\b.ttf").unwrap();
        let before = f.snapshot();
        f.fail_registration_of("B (TrueType)");
        let e = f.registration("B (TrueType)").unwrap_err();
        assert!(e.context.contains("B (TrueType)"), "{e}");
        assert!(e.message.contains("injected"), "{e}");
        assert!(f.registration("B (TrueType)").is_err(), "it keeps failing");
        // Only that value name is affected: another value, an absent one, the family list and
        // the stored data are not.
        assert_eq!(
            f.registration("A (TrueType)").unwrap().as_deref(),
            Some(r"C:\fonts\a.ttf")
        );
        assert_eq!(f.registration("C (TrueType)").unwrap(), None);
        assert_eq!(f.families().unwrap(), vec!["Arial".to_string()]);
        assert_eq!(f.snapshot(), before);
    }

    #[test]
    fn fake_process_answers_by_exe_and_first_argument() {
        let p = FakeProcess::new();
        p.answer("oh-my-posh.exe", "version", 0, "31.5.0\n");
        let out = p
            .run(Path::new(r"C:\r\oh-my-posh.exe"), &[OsStr::new("version")])
            .unwrap();
        assert_eq!(out.stdout, "31.5.0\n");
        assert_eq!(p.calls().len(), 1);
    }
}
