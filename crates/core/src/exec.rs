//! The real `txn::Executor` (spec §5.4, §5.5): runs each plan operation
//! through the adapters. `capture` records the prior state, `apply` makes the
//! change and `restore` puts the prior state back; `apply` and `restore` may
//! each run any number of times with the same result, because the txn engine
//! runs them again after a crash.
//!
//! Codes: a target that cannot be read while its prior state is captured is
//! `E_MACHINE_READ`; a prior state that cannot be turned into a record is
//! `E_SNAPSHOT_WRITE`; a record the restore point does not hold, or holds
//! damaged, is `E_SNAPSHOT_READ`. Each op's own failures carry its spec §5.9
//! code.
use crate::adapters::{
    program_data_dir, Created, DirState, Fs, RegValue, System, REG_EXPAND_SZ, REG_SZ,
};
use crate::plan::{Component, Op, Paths};
use crate::resources::{check_bundle, sha256_hex, Manifest, ResourceError, Resources};
use crate::state::{AutoRunRecord, RegValueRecord};
use crate::txn::{Executor, Failure, Verifier};
use crate::wt_edit::{apply_edit, WtEdit};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

fn failure(code: &'static str, message: impl std::fmt::Display) -> Failure {
    Failure {
        code,
        message: message.to_string(),
    }
}

/// Prior state of an `InstallRuntime` target.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimePrior {
    dir_existed: bool,
    parent_existed: bool,
}

/// Prior state of an `InstallFont` target.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FontPrior {
    file_existed: bool,
    dir_existed: bool,
    /// A new account may not have the registration key yet; restore deletes the one apply made.
    key_existed: bool,
    registration: Option<String>,
    /// The file's bytes, as hex, when it was there with bytes other than the bundled file's
    /// (apply replaces it). A file that was the bundled one comes back from the bundle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    file_hex: Option<String>,
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().saturating_mul(2));
    for b in bytes {
        for nibble in [b >> 4, b & 0x0f] {
            out.push(char::from_digit(u32::from(nibble), 16).unwrap_or('0'));
        }
    }
    out
}

/// `None` for anything but pairs of hex digits.
fn from_hex(text: &str) -> Option<Vec<u8>> {
    fn digit(b: u8) -> Option<u8> {
        char::from(b)
            .to_digit(16)
            .and_then(|d| u8::try_from(d).ok())
    }
    let (pairs, rest) = text.as_bytes().as_chunks::<2>();
    if !rest.is_empty() {
        return None;
    }
    pairs
        .iter()
        .map(|&[hi, lo]| Some(digit(hi)? << 4 | digit(lo)?))
        .collect()
}

/// Where a bundled font goes: (bundled file, installed file, registry value name).
pub fn font_target(
    sys: System<'_>,
    resources: &Resources,
    family: &str,
) -> Option<(PathBuf, PathBuf, String)> {
    let src = resources.font_file(family)?;
    let dest = sys.fonts.user_fonts_dir().join(src.file_name()?);
    Some((src, dest, format!("{family} (TrueType)")))
}

fn to_json<T: Serialize>(v: &T) -> Result<Vec<u8>, Failure> {
    serde_json::to_vec(v).map_err(|e| failure("E_SNAPSHOT_WRITE", e))
}

/// The record of a prior state. One the restore point does not hold, or holds damaged, is a
/// restore point that cannot be read: it is reported, never guessed at.
fn from_json<T: DeserializeOwned>(prior: &Option<Vec<u8>>) -> Result<T, Failure> {
    let bytes = prior.as_deref().ok_or_else(|| {
        failure(
            "E_SNAPSHOT_READ",
            "the restore point has no prior state for this step",
        )
    })?;
    serde_json::from_slice(bytes).map_err(|e| {
        failure(
            "E_SNAPSHOT_READ",
            format!("the restore point's prior state for this step is damaged: {e}"),
        )
    })
}

/// A bundled file's bytes, checked against the hash `resources/manifest.json` pins.
fn pinned_bytes(fs: &dyn Fs, path: &Path, hash: &str) -> Result<Vec<u8>, ResourceError> {
    let bytes = fs
        .read(path)
        .map_err(|e| ResourceError::Io(e.to_string()))?
        .ok_or_else(|| ResourceError::FileMissing(path.to_path_buf()))?;
    if sha256_hex(&bytes) != hash {
        return Err(ResourceError::FileChanged(path.to_path_buf()));
    }
    Ok(bytes)
}

/// A font file's path as the registration holds it.
fn path_text(path: &Path) -> Result<&str, Failure> {
    path.to_str().ok_or_else(|| {
        failure(
            "E_FONT_INSTALL",
            format!("the font's path is not Unicode: {}", path.display()),
        )
    })
}

/// A sibling name in the same folder: `.<name>.<what>-<8 hex>`.
fn sibling(dir: &Path, what: &str) -> PathBuf {
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tail: String = uuid::Uuid::new_v4()
        .simple()
        .to_string()
        .chars()
        .take(8)
        .collect();
    dir.with_file_name(format!(".{name}.{what}-{tail}"))
}

pub struct SystemExecutor<'a> {
    sys: System<'a>,
    resources: &'a Resources,
    manifest: Option<Manifest>,
    /// Font files this executor uninstalled but could not delete. Every method that changes
    /// the list takes `&mut self`, so it needs no interior mutability.
    held: Vec<PathBuf>,
}

impl<'a> SystemExecutor<'a> {
    pub fn new(sys: System<'a>, resources: &'a Resources) -> Self {
        SystemExecutor {
            sys,
            resources,
            manifest: None,
            held: Vec::new(),
        }
    }

    /// Font files that were uninstalled (unloaded and unregistered) but could not be deleted,
    /// because Windows Terminal or GDI still held them. The font counts as uninstalled; the
    /// caller notes each file in `State::pending_deletes` so a later run deletes it. A file
    /// that was installed again since is not listed.
    pub fn held_files(&self) -> Vec<PathBuf> {
        self.held.clone()
    }

    fn note_held(&mut self, file: &Path) {
        if !self.held.iter().any(|h| h == file) {
            self.held.push(file.to_path_buf());
        }
    }

    fn forget_held(&mut self, file: &Path) {
        self.held.retain(|h| h != file);
    }

    fn manifest(&mut self) -> Result<&Manifest, Failure> {
        if self.manifest.is_none() {
            let m = self
                .resources
                .manifest(self.sys.fs)
                .map_err(|e| failure(e.code(), e))?;
            self.manifest = Some(m);
        }
        self.manifest
            .as_ref()
            .ok_or_else(|| failure("E_RUNTIME_MISSING", "no manifest"))
    }

    /// The user's own ProgramData folder when `dir` goes directly inside it (spec §5.4 step 3).
    fn private_parent(&self, dir: &Path) -> Result<Option<PathBuf>, Failure> {
        let host = self.sys.host;
        let pd = host
            .program_data()
            .map_err(|e| failure("E_CLINK_PATH", e))?;
        let sid = host.user_sid().map_err(|e| failure("E_CLINK_PATH", e))?;
        let own = program_data_dir(&pd, &sid);
        Ok((dir.parent() == Some(own.as_path())).then_some(own))
    }

    /// The user's own ProgramData folder: create it private, or check that
    /// an existing one still is (spec §5.4 step 3, TOCTOU).
    fn ensure_private(&self, dir: &Path) -> Result<(), Failure> {
        let created = self
            .sys
            .host
            .create_private_dir(dir)
            .map_err(|e| failure("E_CLINK_PATH", e))?;
        if created == Created::AlreadyExisted {
            self.still_private(dir)?;
        }
        Ok(())
    }

    /// An existing folder of the user's own is still private: owned by the user, SYSTEM or
    /// Administrators, nobody else may change it, and it is not a link to another place.
    fn still_private(&self, dir: &Path) -> Result<(), Failure> {
        match self
            .sys
            .host
            .private_dir_state(dir)
            .map_err(|e| failure("E_CLINK_PATH", e))?
        {
            DirState::Private => Ok(()),
            other => Err(failure(
                "E_CLINK_PATH",
                format!("{} is not private to this user: {other:?}", dir.display()),
            )),
        }
    }

    /// A runtime folder inside the private folder is no link to another place and nobody else
    /// may change it. One made inside the private folder inherits its access list, so it is
    /// `Private`; `Absent` means it went away since `exists` looked, and the check of its files
    /// that follows fails.
    fn not_taken_over(&self, dir: &Path) -> Result<(), Failure> {
        match self
            .sys
            .host
            .private_dir_state(dir)
            .map_err(|e| failure("E_CLINK_PATH", e))?
        {
            DirState::Private | DirState::Absent => Ok(()),
            DirState::Unsafe(why) => Err(failure(
                "E_CLINK_PATH",
                format!("{} is not private to this user: {why}", dir.display()),
            )),
        }
    }

    fn install_runtime(&mut self, component: Component, dir: &Path) -> Result<(), Failure> {
        let bundle = self.manifest()?.bundle(component).clone();
        let fs = self.sys.fs;
        let private_parent = self.private_parent(dir)?;
        if fs.exists(dir) {
            if let Some(parent) = &private_parent {
                // Checked again before the files count as installed (spec §5.4 step 3, TOCTOU):
                // a folder someone else took over, or a link to another place, could hold
                // files that cmd would run in every window.
                self.still_private(parent)?;
                self.not_taken_over(dir)?;
            }
            // Never overwrite a runtime folder (spec §5.4 Fs): the same files is a no-op.
            return check_bundle(fs, dir, &bundle).map_err(|e| {
                failure(
                    "E_RUNTIME_MISSING",
                    format!("{} already exists with other files: {e}", dir.display()),
                )
            });
        }
        if let Some(parent) = &private_parent {
            self.ensure_private(parent)?;
        }
        // Each bundled file is checked against its pinned hash as it is read, so nothing
        // unchecked is installed; the folder appears under its name in one rename.
        let src = self.resources.dir(component);
        let staging = sibling(dir, "partial");
        let copied = (|| -> Result<(), Failure> {
            for (name, hash) in &bundle.files {
                let bytes =
                    pinned_bytes(fs, &src.join(name), hash).map_err(|e| failure(e.code(), e))?;
                fs.write_atomic(&staging.join(name), &bytes)
                    .map_err(|e| failure("E_RUNTIME_MISSING", e))?;
            }
            fs.rename(&staging, dir)
                .map_err(|e| failure("E_RUNTIME_MISSING", e))
        })();
        if copied.is_err() {
            let _ = fs.remove_dir_all(&staging);
        }
        copied
    }

    fn font_target(&self, family: &str) -> Result<(PathBuf, PathBuf, String), Failure> {
        font_target(self.sys, self.resources, family)
            .ok_or_else(|| failure("E_FONT_INSTALL", format!("no bundled file for {family}")))
    }

    /// The hash `resources/manifest.json` pins for the bundled font file `src`.
    fn pinned_font_hash(&mut self, src: &Path) -> Result<String, Failure> {
        let name = src
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        self.manifest()?
            .fonts
            .files
            .get(&name)
            .cloned()
            .ok_or_else(|| {
                failure(
                    "E_RUNTIME_MISSING",
                    format!("resources/manifest.json pins no font file {name}"),
                )
            })
    }

    /// The bundled font file's bytes, checked against their pinned hash.
    fn bundled_font(&mut self, src: &Path) -> Result<Vec<u8>, Failure> {
        let hash = self.pinned_font_hash(src)?;
        pinned_bytes(self.sys.fs, src, &hash).map_err(|e| failure(e.code(), e))
    }

    fn install_font(&mut self, family: &str) -> Result<(), Failure> {
        let (src, dest, value_name) = self.font_target(family)?;
        let bytes = self.bundled_font(&src)?;
        let fs = self.sys.fs;
        if fs
            .read(&dest)
            .map_err(|e| failure("E_FONT_INSTALL", e))?
            .as_deref()
            != Some(bytes.as_slice())
        {
            fs.write_atomic(&dest, &bytes)
                .map_err(|e| failure("E_FONT_INSTALL", e))?;
        }
        // The file is the installed font's now, not one left to delete.
        self.forget_held(&dest);
        let fonts = self.sys.fonts;
        fonts
            .register(&value_name, path_text(&dest)?)
            .map_err(|e| failure("E_FONT_INSTALL", e))?;
        fonts.load(&dest).map_err(|e| failure("E_FONT_INSTALL", e))
    }

    /// Puts the font's registration value back as it was: the old data, or none.
    fn put_registration_back(&self, p: &FontPrior, value_name: &str) -> Result<(), Failure> {
        let fonts = self.sys.fonts;
        match &p.registration {
            Some(file) => fonts.register(value_name, file),
            None => fonts.unregister(value_name),
        }
        .map_err(|e| failure("E_FONT_INSTALL", e))
    }

    /// Deletes the registration key when apply made it and it is empty again.
    fn drop_created_key(&self, p: &FontPrior) -> Result<(), Failure> {
        if p.key_existed {
            return Ok(());
        }
        self.sys
            .fonts
            .delete_registration_key_if_empty()
            .map_err(|e| failure("E_FONT_INSTALL", e))
    }

    fn restore_font(&mut self, family: &str, prior: &Option<Vec<u8>>) -> Result<(), Failure> {
        let p: FontPrior = from_json(prior)?;
        let (src, dest, value_name) = self.font_target(family)?;
        if p.file_existed {
            self.put_font_back(&p, &src, &dest, &value_name)
        } else {
            self.remove_font(&p, &dest, &value_name)
        }
    }

    /// The font file was not there before: the font is uninstalled in the order G12 proved
    /// (spec §5.4): out of the session's font table (`unload`, which also broadcasts the
    /// change), then the registration, then the file. Once the registration is gone the font is
    /// uninstalled. A file that would not unload or delete (Windows Terminal may hold it) is
    /// left for a later delete and listed by `held_files`.
    fn remove_font(&mut self, p: &FontPrior, dest: &Path, value_name: &str) -> Result<(), Failure> {
        let (fs, fonts) = (self.sys.fs, self.sys.fonts);
        // An unload that fails leaves the font in this session only; the delete below then
        // fails too, and the file is listed.
        let _ = fonts.unload(dest);
        self.put_registration_back(p, value_name)?;
        if fs.remove_file(dest).is_err() && fs.exists(dest) {
            self.note_held(dest);
        } else {
            self.forget_held(dest);
        }
        if !p.dir_existed {
            // Only when empty: another per-user font, or a held file, may be in it.
            let _ = fs.remove_dir(&fonts.user_fonts_dir());
        }
        self.drop_created_key(p)
    }

    /// The font file was there before (an undo that uninstalled the font is rolled back, or
    /// apply replaced another file of the same name): its bytes come back (its own, or the
    /// bundled file it was), then its registration, and it is loaded again when that
    /// registration names it, as a runtime folder is put back.
    fn put_font_back(
        &mut self,
        p: &FontPrior,
        src: &Path,
        dest: &Path,
        value_name: &str,
    ) -> Result<(), Failure> {
        let (fs, fonts) = (self.sys.fs, self.sys.fonts);
        let now = fs.read(dest).map_err(|e| failure("E_FONT_INSTALL", e))?;
        let replacement = match &p.file_hex {
            Some(hex) => {
                let bytes = from_hex(hex).ok_or_else(|| {
                    failure(
                        "E_SNAPSHOT_READ",
                        "the restore point's copy of the font file is not hex",
                    )
                })?;
                (now.as_deref() != Some(bytes.as_slice())).then_some(bytes)
            }
            None => {
                // The bundled file: read it only when the one in place is not it.
                let in_place = match &now {
                    Some(bytes) => self
                        .pinned_font_hash(src)
                        .is_ok_and(|hash| sha256_hex(bytes) == hash),
                    None => false,
                };
                if in_place {
                    None
                } else {
                    Some(self.bundled_font(src)?)
                }
            }
        };
        if let Some(bytes) = replacement {
            // A loaded font file cannot be replaced; when the unload fails, so does the write,
            // and that is reported.
            let _ = fonts.unload(dest);
            fs.write_atomic(dest, &bytes)
                .map_err(|e| failure("E_FONT_INSTALL", e))?;
        }
        self.forget_held(dest);
        self.put_registration_back(p, value_name)?;
        self.drop_created_key(p)?;
        let dest_text = path_text(dest)?;
        if p.registration
            .as_deref()
            .is_some_and(|r| r.eq_ignore_ascii_case(dest_text))
        {
            fonts.load(dest).map_err(|e| failure("E_FONT_INSTALL", e))
        } else {
            // There but not installed, as a file left by an earlier uninstall is: not loaded
            // either. Without a registration the font counts as uninstalled even if this fails.
            let _ = fonts.unload(dest);
            Ok(())
        }
    }

    fn restore_runtime(
        &mut self,
        component: Component,
        dir: &Path,
        prior: &Option<Vec<u8>>,
    ) -> Result<(), Failure> {
        let p: RuntimePrior = from_json(prior)?;
        let fs = self.sys.fs;
        if p.dir_existed {
            // An undo that removed the folder and is rolled back puts it back, from
            // the bundled files with their pinned hashes.
            if !fs.exists(dir) {
                self.install_runtime(component, dir)?;
            }
            return Ok(());
        }
        if fs.exists(dir) {
            // Rename first: it fails cleanly while a file inside is in use,
            // so a half-deleted runtime is never left behind.
            let tomb = sibling(dir, "removing");
            fs.rename(dir, &tomb).map_err(|e| {
                failure(
                    "E_RUNTIME_MISSING",
                    format!("{e} (close every cmd window, then try again)"),
                )
            })?;
            // Best effort: `sweep` deletes what is left at the next start.
            let _ = fs.remove_dir_all(&tomb);
        }
        if !p.parent_existed {
            if let Some(parent) = dir.parent() {
                // Only when empty: the other runtime may live there too.
                let _ = fs.remove_dir(parent);
            }
        }
        Ok(())
    }
}

impl Executor for SystemExecutor<'_> {
    fn capture(&mut self, op: &Op) -> Result<Option<Vec<u8>>, Failure> {
        let fs = self.sys.fs;
        let read = |e| failure("E_MACHINE_READ", e);
        match op {
            Op::InstallRuntime { dir, .. } => {
                let parent_existed = dir.parent().is_none_or(|p| fs.exists(p));
                to_json(&RuntimePrior {
                    dir_existed: fs.exists(dir),
                    parent_existed,
                })
                .map(Some)
            }
            Op::WriteFile { path, .. } => fs.read(path).map_err(read),
            Op::WtApply { settings, .. } => fs.read(settings).map_err(read),
            Op::SetAutoRun { .. } => {
                let a = self.sys.autorun;
                let key_existed = a.user_key_exists().map_err(read)?;
                let value = a
                    .read_user()
                    .map_err(read)?
                    .as_ref()
                    .map(RegValueRecord::from_reg);
                to_json(&AutoRunRecord { key_existed, value }).map(Some)
            }
            Op::InstallFont { family } => {
                let (src, dest, value_name) = self.font_target(family)?;
                let fonts = self.sys.fonts;
                // A value that is there but is not plain REG_SZ text is an error, never "absent":
                // a restore would delete it.
                let registration = fonts.registration(&value_name).map_err(read)?;
                let key_existed = fonts.registration_key_exists().map_err(read)?;
                let dir_existed = fs.is_dir(&fonts.user_fonts_dir());
                let file = fs.read(&dest).map_err(read)?;
                // The bundled file comes back from the bundle; any other is kept here. Without
                // a readable pin the bytes are kept too, so an undo never needs the bundle.
                let file_hex = match &file {
                    Some(bytes)
                        if !self
                            .pinned_font_hash(&src)
                            .is_ok_and(|hash| sha256_hex(bytes) == hash) =>
                    {
                        Some(to_hex(bytes))
                    }
                    _ => None,
                };
                to_json(&FontPrior {
                    file_existed: file.is_some(),
                    dir_existed,
                    key_existed,
                    registration,
                    file_hex,
                })
                .map(Some)
            }
        }
    }

    fn apply(&mut self, op: &Op) -> Result<(), Failure> {
        let fs = self.sys.fs;
        match op {
            Op::InstallRuntime { component, dir } => self.install_runtime(*component, dir),
            Op::WriteFile { path, contents } => fs
                .write_atomic(path, contents.as_bytes())
                .map_err(|e| failure("E_DATA_WRITE", e)),
            Op::WtApply {
                settings,
                scheme,
                profile,
                set_default,
            } => {
                let bytes = fs
                    .read(settings)
                    .map_err(|e| failure("E_WT_WRITE", e))?
                    .ok_or_else(|| {
                        failure("E_WT_WRITE", format!("{} is gone", settings.display()))
                    })?;
                let text = String::from_utf8(bytes).map_err(|e| failure("E_WT_PARSE", e))?;
                let edited = apply_edit(
                    &text,
                    &WtEdit {
                        scheme,
                        profile,
                        set_default: *set_default,
                    },
                )
                .map_err(|e| failure(e.code(), e))?;
                if edited.text == text {
                    return Ok(());
                }
                fs.write_atomic(settings, edited.text.as_bytes())
                    .map_err(|e| failure("E_WT_WRITE", e))
            }
            Op::SetAutoRun { value } => {
                let a = self.sys.autorun;
                let result = match value {
                    Some(text) => {
                        // Keep REG_EXPAND_SZ when the value already has it (spec §5.4).
                        let current = a.read_user().map_err(|e| failure("E_AUTORUN_WRITE", e))?;
                        let kind = if current.is_some_and(|v| v.kind == REG_EXPAND_SZ) {
                            REG_EXPAND_SZ
                        } else {
                            REG_SZ
                        };
                        a.write_user(&RegValue::from_text(kind, text))
                    }
                    None => a.delete_user(),
                };
                result.map_err(|e| failure("E_AUTORUN_WRITE", e))
            }
            Op::InstallFont { family } => self.install_font(family),
        }
    }

    fn restore(&mut self, op: &Op, prior: &Option<Vec<u8>>) -> Result<(), Failure> {
        let fs = self.sys.fs;
        match op {
            Op::InstallRuntime { component, dir } => self.restore_runtime(*component, dir, prior),
            Op::WriteFile { path, .. } => match prior {
                Some(bytes) => fs.write_atomic(path, bytes),
                None => fs.remove_file(path),
            }
            .map_err(|e| failure("E_DATA_WRITE", e)),
            Op::WtApply { settings, .. } => match prior {
                Some(bytes)
                    if fs.read(settings).ok().flatten().as_deref() != Some(bytes.as_slice()) =>
                {
                    fs.write_atomic(settings, bytes)
                        .map_err(|e| failure("E_WT_WRITE", e))
                }
                // CMD Studio never creates or deletes a settings.json.
                _ => Ok(()),
            },
            Op::SetAutoRun { .. } => {
                let a = self.sys.autorun;
                let p: AutoRunRecord = from_json(prior)?;
                match &p.value {
                    Some(rec) => {
                        let value = rec.to_reg().ok_or_else(|| {
                            failure(
                                "E_SNAPSHOT_READ",
                                "the restore point's AutoRun value is not hex",
                            )
                        })?;
                        a.write_user(&value)
                    }
                    None => a.delete_user(),
                }
                .map_err(|e| failure("E_AUTORUN_WRITE", e))?;
                if !p.key_existed {
                    a.delete_user_key_if_empty()
                        .map_err(|e| failure("E_AUTORUN_WRITE", e))?;
                }
                Ok(())
            }
            Op::InstallFont { family } => self.restore_font(family, prior),
        }
    }
}

/// Undo: "applying" an op of the reversed journal puts its target back to
/// the state that journal captured.
pub struct UndoExecutor<'a> {
    inner: SystemExecutor<'a>,
    priors: Vec<(Op, Option<Vec<u8>>)>,
}

impl<'a> UndoExecutor<'a> {
    pub fn new(inner: SystemExecutor<'a>, priors: Vec<(Op, Option<Vec<u8>>)>) -> Self {
        UndoExecutor { inner, priors }
    }

    /// The inner executor's [`SystemExecutor::held_files`].
    pub fn held_files(&self) -> Vec<PathBuf> {
        self.inner.held_files()
    }
}

impl Executor for UndoExecutor<'_> {
    fn capture(&mut self, op: &Op) -> Result<Option<Vec<u8>>, Failure> {
        self.inner.capture(op)
    }

    fn apply(&mut self, op: &Op) -> Result<(), Failure> {
        let prior = self
            .priors
            .iter()
            .find(|(o, _)| o == op)
            .map(|(_, p)| p.clone())
            .ok_or_else(|| {
                failure(
                    "E_SNAPSHOT_READ",
                    "the restore point holds no prior state for this step",
                )
            })?;
        self.inner.restore(op, &prior)
    }

    fn restore(&mut self, op: &Op, prior: &Option<Vec<u8>>) -> Result<(), Failure> {
        self.inner.restore(op, prior)
    }
}

/// A short English name for an op's target, for messages.
pub fn op_label(op: &Op) -> String {
    match op {
        Op::InstallRuntime { dir, .. } => format!("runtime folder {}", dir.display()),
        Op::WriteFile { path, .. } => format!("file {}", path.display()),
        Op::WtApply { settings, .. } => {
            format!("Windows Terminal settings {}", settings.display())
        }
        Op::SetAutoRun { .. } => "cmd AutoRun".to_string(),
        Op::InstallFont { family } => format!("font {family}"),
    }
}

/// The registry key that holds AutoRun, as Registry Editor shows it.
const COMMAND_PROCESSOR_KEY: &str = r"HKEY_CURRENT_USER\Software\Microsoft\Command Processor";

/// How a person puts an op's target back by hand, in English and in Thai (spec §5.9: every
/// problem the user sees has both).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FixByHand {
    pub en: String,
    pub th: String,
}

/// How to put `op`'s target back by hand when a rollback could not (spec §5.5: say which
/// parts, and how to fix them by hand). `prior` is what the transaction captured before the op,
/// and `payload` the restore point's file that holds it.
pub fn fix_by_hand(op: &Op, prior: &Option<Vec<u8>>, payload: &Path) -> FixByHand {
    let cleanup_en = "run `cmdstudio --cleanup`, which takes out what CMD Studio changed";
    let cleanup_th = "รันคำสั่ง `cmdstudio --cleanup` ซึ่งจะเอาสิ่งที่ CMD Studio เปลี่ยนออก";
    let (payload, key) = (payload.display(), COMMAND_PROCESSOR_KEY);
    let (en, th) = match op {
        Op::InstallRuntime { dir, .. } => (
            format!(
                "Close every cmd window and Windows Terminal, then {cleanup_en}. If {} is still \
                 there afterwards, delete it.",
                dir.display()
            ),
            format!(
                "ปิดหน้าต่าง cmd และ Windows Terminal ทุกหน้าต่าง แล้ว{cleanup_th} \
                 ถ้ายังมี {} อยู่ ให้ลบโฟลเดอร์นั้นทิ้ง",
                dir.display()
            ),
        ),
        Op::WriteFile { path, .. } => match prior {
            Some(_) => (
                format!("Copy {payload} over {}.", path.display()),
                format!("คัดลอก {payload} ไปวางทับ {}", path.display()),
            ),
            None => (
                format!("Delete {}.", path.display()),
                format!("ลบ {}", path.display()),
            ),
        },
        Op::WtApply { settings, .. } => match prior {
            Some(_) => (
                format!(
                    "Close Windows Terminal, then copy {payload} over {}.",
                    settings.display()
                ),
                format!(
                    "ปิด Windows Terminal แล้วคัดลอก {payload} ไปวางทับ {}",
                    settings.display()
                ),
            ),
            None => (
                format!(
                    "Close Windows Terminal, then delete {}.",
                    settings.display()
                ),
                format!("ปิด Windows Terminal แล้วลบ {}", settings.display()),
            ),
        },
        Op::SetAutoRun { .. } => {
            let text = from_json::<AutoRunRecord>(prior)
                .ok()
                .map(|r| r.value.map(|v| v.to_reg().and_then(|v| v.text())));
            match text {
                Some(None) => (
                    format!("In Registry Editor, open {key} and delete the AutoRun value."),
                    format!("เปิด Registry Editor (regedit) ไปที่ {key} แล้วลบค่า AutoRun"),
                ),
                Some(Some(Some(text))) => (
                    format!("In Registry Editor, open {key} and set AutoRun back to: {text}"),
                    format!(
                        "เปิด Registry Editor (regedit) ไปที่ {key} แล้วแก้ค่า AutoRun กลับเป็น: {text}"
                    ),
                ),
                // A record that cannot be read, or a value that is not text.
                Some(Some(None)) | None => (
                    format!("Close every cmd window, then {cleanup_en}."),
                    format!("ปิดหน้าต่าง cmd ทุกหน้าต่าง แล้ว{cleanup_th}"),
                ),
            }
        }
        Op::InstallFont { family } => (
            format!(
                "Close Windows Terminal, then {cleanup_en}: it uninstalls the font {family} once \
                 nothing uses it."
            ),
            format!(
                "ปิด Windows Terminal แล้ว{cleanup_th} คำสั่งนี้จะถอนฟอนต์ {family} \
                 เมื่อไม่มีโปรแกรมไหนใช้ฟอนต์นี้อยู่"
            ),
        ),
    };
    FixByHand { en, th }
}

/// Whether two captured states of `op`'s target mean the same thing. Files and settings
/// compare byte for byte; AutoRun by the value's type and bytes; for a font only the
/// registration counts (a file Windows Terminal still holds is deleted later); for a runtime
/// only whether the folder exists. A record that does not decode matches nothing.
pub fn same_state(op: &Op, a: &Option<Vec<u8>>, b: &Option<Vec<u8>>) -> bool {
    fn both<T: DeserializeOwned, K: PartialEq>(
        a: &Option<Vec<u8>>,
        b: &Option<Vec<u8>>,
        key: impl Fn(T) -> K,
    ) -> bool {
        match (from_json::<T>(a), from_json::<T>(b)) {
            (Ok(a), Ok(b)) => key(a) == key(b),
            _ => false,
        }
    }
    match op {
        Op::WriteFile { .. } | Op::WtApply { .. } => a == b,
        Op::SetAutoRun { .. } => both(a, b, |r: AutoRunRecord| r.value),
        Op::InstallFont { .. } => both(a, b, |p: FontPrior| p.registration),
        Op::InstallRuntime { .. } => both(a, b, |p: RuntimePrior| p.dir_existed),
    }
}

/// Checks that undo put every target back to what the undone apply captured
/// (the spec §8 Phase 1 criterion: byte-identical after undo).
pub struct UndoVerifier<'a> {
    exec: SystemExecutor<'a>,
    expected: Vec<(Op, Option<Vec<u8>>)>,
}

impl<'a> UndoVerifier<'a> {
    pub fn new(exec: SystemExecutor<'a>, expected: Vec<(Op, Option<Vec<u8>>)>) -> Self {
        UndoVerifier { exec, expected }
    }
}

impl Verifier for UndoVerifier<'_> {
    fn verify(&mut self, _ops: &[Op]) -> Result<(), Failure> {
        for (op, want) in &self.expected {
            let now = self.exec.capture(op)?;
            if !same_state(op, &now, want) {
                return Err(failure(
                    "E_VERIFY_READBACK",
                    format!("undo did not restore {}", op_label(op)),
                ));
            }
        }
        Ok(())
    }
}

/// Deletes runtime folders that earlier runs left half made or half removed
/// (`.<name>.partial-…`, `.<name>.removing-…`). Best effort.
pub fn sweep(sys: System<'_>, paths: &Paths) {
    let mut roots = vec![paths.runtime_root()];
    if let (Ok(pd), Ok(sid)) = (sys.host.program_data(), sys.host.user_sid()) {
        roots.push(program_data_dir(&pd, &sid));
    }
    for root in roots {
        for e in sys.fs.list_dir(&root).unwrap_or_default() {
            let name = e
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if e.is_dir
                && name.starts_with('.')
                && (name.contains(".partial-") || name.contains(".removing-"))
            {
                let _ = sys.fs.remove_dir_all(&e.path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::fake::{FakeFonts, FakeFs, FakeMachine};
    use crate::adapters::{AdapterError, AutoRun, DirEntry, Fonts, Fs};
    use crate::plan::NERD_FONT_FAMILY;
    use crate::resources::fake_resources;
    use crate::resources::NERD_FONT_FILE;
    use crate::txn::{self, Journal, JournalStore, Outcome};
    use serde_json::{json, Map};
    use std::cell::RefCell;

    const RES: &str = r"C:\app\resources";

    fn setup() -> (FakeMachine, Resources, Paths) {
        let m = FakeMachine::new();
        // `%LOCALAPPDATA%\Microsoft\Windows` exists on every account; `Fonts` under it may not.
        m.fs.create_dir_all(Path::new(
            r"C:\Users\tester\AppData\Local\Microsoft\Windows",
        ))
        .unwrap();
        let res = fake_resources(&m.fs, Path::new(RES));
        m.fonts.knows_file(NERD_FONT_FILE, NERD_FONT_FAMILY);
        (
            m,
            res,
            Paths {
                data: PathBuf::from(r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio"),
            },
        )
    }

    /// capture → apply → restore leaves the target as it was, twice over.
    fn round_trip(m: &FakeMachine, res: &Resources, op: &Op) {
        let fs_before = m.fs.snapshot();
        let reg_before = m.autorun.snapshot();
        let fonts_before = m.fonts.snapshot();
        let mut ex = SystemExecutor::new(m.system(), res);
        let prior = ex.capture(op).unwrap();
        ex.apply(op).unwrap();
        ex.apply(op).unwrap();
        ex.restore(op, &prior).unwrap();
        ex.restore(op, &prior).unwrap();
        assert_eq!(m.fs.snapshot(), fs_before, "{op:?}");
        assert_eq!(m.autorun.snapshot(), reg_before, "{op:?}");
        assert_eq!(m.fonts.snapshot(), fonts_before, "{op:?}");
    }

    #[test]
    fn every_op_restores_byte_for_byte() {
        let (m, res, paths) = setup();
        m.fs.put(paths.generated().join("init.lua"), b"old");
        m.fs.put(
            r"C:\wt\settings.json",
            br#"{ "profiles": { "list": [] } } // c"#,
        );
        m.autorun
            .write_user(&RegValue::from_text(REG_EXPAND_SZ, "%A% & echo hi"))
            .unwrap();
        let mut profile = Map::new();
        profile.insert("colorScheme".into(), json!("CMD Studio"));
        for op in [
            Op::InstallRuntime {
                component: Component::Omp,
                dir: paths.omp_dir(),
            },
            Op::InstallRuntime {
                component: Component::Clink,
                dir: paths.clink_dir(),
            },
            Op::WriteFile {
                path: paths.generated().join("init.lua"),
                contents: "new".into(),
            },
            Op::WriteFile {
                path: paths.generated().join("banner.lua"),
                contents: "new".into(),
            },
            Op::WtApply {
                settings: PathBuf::from(r"C:\wt\settings.json"),
                scheme: json!({ "name": "CMD Studio" }),
                profile,
                set_default: true,
            },
            Op::SetAutoRun {
                value: Some("%A% & echo hi & ours".into()),
            },
            Op::SetAutoRun { value: None },
            Op::InstallFont {
                family: NERD_FONT_FAMILY.into(),
            },
        ] {
            round_trip(&m, &res, &op);
        }
        // An account that never had a per-user font has no registration key: it goes again.
        m.fonts.set_key_exists(false);
        round_trip(
            &m,
            &res,
            &Op::InstallFont {
                family: NERD_FONT_FAMILY.into(),
            },
        );
    }

    #[test]
    fn autorun_keeps_expand_sz_and_a_created_key_goes_again() {
        let (m, res, _) = setup();
        m.autorun
            .write_user(&RegValue::from_text(REG_EXPAND_SZ, "%X%"))
            .unwrap();
        let mut ex = SystemExecutor::new(m.system(), &res);
        ex.apply(&Op::SetAutoRun {
            value: Some("%X% & ours".into()),
        })
        .unwrap();
        assert_eq!(m.autorun.read_user().unwrap().unwrap().kind, REG_EXPAND_SZ);

        let (m, res, _) = setup();
        m.autorun.delete_user().unwrap();
        m.autorun.set_key_exists(false);
        round_trip(
            &m,
            &res,
            &Op::SetAutoRun {
                value: Some("ours".into()),
            },
        );
        assert_eq!(m.autorun.snapshot(), (false, None));
    }

    #[test]
    fn runtime_install_is_a_no_op_when_identical_and_refuses_a_different_folder() {
        let (m, res, paths) = setup();
        let mut ex = SystemExecutor::new(m.system(), &res);
        let op = Op::InstallRuntime {
            component: Component::Clink,
            dir: paths.clink_dir(),
        };
        ex.apply(&op).unwrap();
        assert_eq!(m.fs.file(paths.clink_exe()), Some(b"clink-exe".to_vec()));
        ex.apply(&op).unwrap();
        m.fs.put(paths.clink_dir().join("clink_dll_x64.dll"), b"other");
        assert_eq!(ex.apply(&op).unwrap_err().code, "E_RUNTIME_MISSING");
        assert_eq!(
            m.fs.file(paths.clink_dir().join("clink_dll_x64.dll")),
            Some(b"other".to_vec()),
            "never overwritten"
        );
    }

    #[test]
    fn a_runtime_in_use_is_not_half_deleted() {
        let (m, res, paths) = setup();
        let mut ex = SystemExecutor::new(m.system(), &res);
        let op = Op::InstallRuntime {
            component: Component::Clink,
            dir: paths.clink_dir(),
        };
        let prior = ex.capture(&op).unwrap();
        ex.apply(&op).unwrap();
        m.fs.fail_removes_of(paths.clink_dir().join("clink_dll_x64.dll"));
        assert!(ex.restore(&op, &prior).is_err());
        assert_eq!(m.fs.file(paths.clink_exe()), Some(b"clink-exe".to_vec()));
    }

    #[test]
    fn program_data_runtime_gets_a_private_folder_and_refuses_a_foreign_one() {
        let (m, res, _) = setup();
        let pd = PathBuf::from(r"C:\ProgramData\ai.resincore.cmdstudio-S-1-5-21-1-2-3-1001");
        let op = Op::InstallRuntime {
            component: Component::Clink,
            dir: pd.join("clink-1.9.34"),
        };
        let mut ex = SystemExecutor::new(m.system(), &res);
        ex.apply(&op).unwrap();
        assert_eq!(m.host.dir_state(&pd), DirState::Private);

        let (m, res, _) = setup();
        // Someone created the folder after detect looked (spec §5.4 TOCTOU).
        m.host
            .set_dir_state(&pd, DirState::Unsafe("S-1-5-32-545 may change it".into()));
        let mut ex = SystemExecutor::new(m.system(), &res);
        assert_eq!(ex.apply(&op).unwrap_err().code, "E_CLINK_PATH");
        assert!(!m.fs.exists(&pd.join("clink-1.9.34")));
    }

    #[test]
    fn wt_apply_refuses_a_broken_file_and_never_writes_it() {
        let (m, res, _) = setup();
        m.fs.put(r"C:\wt\settings.json", b"{ \"profiles\": ");
        let op = Op::WtApply {
            settings: PathBuf::from(r"C:\wt\settings.json"),
            scheme: json!({ "name": "CMD Studio" }),
            profile: Map::new(),
            set_default: false,
        };
        let mut ex = SystemExecutor::new(m.system(), &res);
        assert_eq!(ex.apply(&op).unwrap_err().code, "E_WT_PARSE");
        assert_eq!(
            m.fs.file(r"C:\wt\settings.json"),
            Some(b"{ \"profiles\": ".to_vec())
        );
    }

    #[test]
    fn a_font_file_in_use_still_ends_uninstalled() {
        let (m, res, _) = setup();
        let op = Op::InstallFont {
            family: NERD_FONT_FAMILY.into(),
        };
        let mut ex = SystemExecutor::new(m.system(), &res);
        let prior = ex.capture(&op).unwrap();
        ex.apply(&op).unwrap();
        assert!(m
            .fonts
            .families()
            .unwrap()
            .contains(&NERD_FONT_FAMILY.to_string()));
        let dest = m.fonts.user_fonts_dir().join(NERD_FONT_FILE);
        m.fs.fail_removes_of(&dest);
        ex.restore(&op, &prior).unwrap();
        assert!(!m
            .fonts
            .families()
            .unwrap()
            .contains(&NERD_FONT_FAMILY.to_string()));
        assert_eq!(
            m.fonts.registration("CaskaydiaCove NF (TrueType)").unwrap(),
            None
        );
    }

    #[test]
    fn undo_executor_applies_the_captured_priors() {
        let (m, res, paths) = setup();
        let op = Op::WriteFile {
            path: paths.generated().join("a.lua"),
            contents: "new".into(),
        };
        let mut undo = UndoExecutor::new(
            SystemExecutor::new(m.system(), &res),
            vec![(op.clone(), Some(b"old".to_vec()))],
        );
        m.fs.put(paths.generated().join("a.lua"), b"new");
        undo.apply(&op).unwrap();
        assert_eq!(
            m.fs.file(paths.generated().join("a.lua")),
            Some(b"old".to_vec())
        );
        let other = Op::SetAutoRun { value: None };
        // The restore point holds no prior for this step: a restore point read problem.
        assert_eq!(undo.apply(&other).unwrap_err().code, "E_SNAPSHOT_READ");
    }

    #[test]
    fn failures_carry_the_spec_codes() {
        let (m, res, paths) = setup();
        m.autorun.fail_writes();
        let mut ex = SystemExecutor::new(m.system(), &res);
        let err = ex
            .apply(&Op::SetAutoRun {
                value: Some("x".into()),
            })
            .unwrap_err();
        assert_eq!(err.code, "E_AUTORUN_WRITE");
        let file = paths.generated().join("init.lua");
        m.fs.fail_writes_to(&file);
        let err = ex
            .apply(&Op::WriteFile {
                path: file,
                contents: "x".into(),
            })
            .unwrap_err();
        assert_eq!(err.code, "E_DATA_WRITE");
        let err = ex
            .apply(&Op::WtApply {
                settings: PathBuf::from(r"C:\gone\settings.json"),
                scheme: json!({}),
                profile: Map::new(),
                set_default: false,
            })
            .unwrap_err();
        assert_eq!(err.code, "E_WT_WRITE");
    }

    #[test]
    fn a_runtime_that_was_there_is_put_back() {
        // An undo removes the runtime folder. If a later step of the undo fails, its rollback
        // must bring the folder back: the theme would otherwise point at a runtime that is gone.
        let (m, res, paths) = setup();
        let mut ex = SystemExecutor::new(m.system(), &res);
        let op = Op::InstallRuntime {
            component: Component::Clink,
            dir: paths.clink_dir(),
        };
        ex.apply(&op).unwrap();
        let there = ex.capture(&op).unwrap();
        m.fs.remove_dir_all(&paths.clink_dir()).unwrap();
        ex.restore(&op, &there).unwrap();
        ex.restore(&op, &there).unwrap();
        assert_eq!(m.fs.file(paths.clink_exe()), Some(b"clink-exe".to_vec()));
    }

    #[test]
    fn every_step_has_a_way_to_fix_it_by_hand() {
        let payload = Path::new(r"C:\d\snapshots\0000000000001-a\payload\3.bin");
        let file = PathBuf::from(r"C:\d\generated\init.lua");
        let write = Op::WriteFile {
            path: file.clone(),
            contents: "x".into(),
        };
        assert_eq!(
            fix_by_hand(&write, &Some(b"old".to_vec()), payload).en,
            format!("Copy {} over {}.", payload.display(), file.display())
        );
        assert_eq!(
            fix_by_hand(&write, &None, payload).en,
            format!("Delete {}.", file.display())
        );
        let settings = PathBuf::from(r"C:\wt\settings.json");
        let wt = Op::WtApply {
            settings: settings.clone(),
            scheme: serde_json::json!({}),
            profile: serde_json::Map::new(),
            set_default: false,
        };
        assert_eq!(
            fix_by_hand(&wt, &Some(b"{}".to_vec()), payload).en,
            format!(
                "Close Windows Terminal, then copy {} over {}.",
                payload.display(),
                settings.display()
            )
        );
        assert_eq!(
            fix_by_hand(&wt, &None, payload).en,
            format!(
                "Close Windows Terminal, then delete {}.",
                settings.display()
            )
        );
        let autorun = Op::SetAutoRun {
            value: Some("x".into()),
        };
        let before = to_json(&AutoRunRecord {
            key_existed: true,
            value: Some(RegValueRecord::from_reg(&RegValue::from_text(
                REG_EXPAND_SZ,
                "%A% & echo hi",
            ))),
        })
        .unwrap();
        assert!(fix_by_hand(&autorun, &Some(before), payload)
            .en
            .ends_with("set AutoRun back to: %A% & echo hi"));
        let absent = to_json(&AutoRunRecord {
            key_existed: true,
            value: None,
        })
        .unwrap();
        assert!(fix_by_hand(&autorun, &Some(absent), payload)
            .en
            .ends_with("delete the AutoRun value."));
        for op in [
            Op::InstallRuntime {
                component: Component::Omp,
                dir: PathBuf::from(r"C:\d\runtime\omp-31.5.0"),
            },
            Op::InstallFont {
                family: NERD_FONT_FAMILY.into(),
            },
        ] {
            assert!(
                fix_by_hand(&op, &None, payload)
                    .en
                    .contains("cmdstudio --cleanup"),
                "{op:?}"
            );
        }
    }

    #[test]
    fn every_fix_by_hand_has_a_thai_text_with_the_same_places() {
        let payload = Path::new(r"C:\d\snapshots\0000000000001-a\payload\3.bin");
        let file = PathBuf::from(r"C:\d\generated\init.lua");
        let write = Op::WriteFile {
            path: file.clone(),
            contents: "x".into(),
        };
        let th = fix_by_hand(&write, &Some(b"old".to_vec()), payload).th;
        assert!(
            th.contains(&payload.display().to_string()) && th.contains(&file.display().to_string()),
            "{th}"
        );
        let th = fix_by_hand(&write, &None, payload).th;
        assert!(
            th.contains("ลบ") && th.contains(&file.display().to_string()),
            "{th}"
        );
        let settings = PathBuf::from(r"C:\wt\settings.json");
        let wt = Op::WtApply {
            settings: settings.clone(),
            scheme: json!({}),
            profile: Map::new(),
            set_default: false,
        };
        let th = fix_by_hand(&wt, &Some(b"{}".to_vec()), payload).th;
        assert!(
            th.contains("Windows Terminal") && th.contains(&settings.display().to_string()),
            "{th}"
        );
        let autorun = Op::SetAutoRun {
            value: Some("x".into()),
        };
        let before = to_json(&AutoRunRecord {
            key_existed: true,
            value: Some(RegValueRecord::from_reg(&RegValue::from_text(
                REG_SZ, "echo hi",
            ))),
        })
        .unwrap();
        let fix = fix_by_hand(&autorun, &Some(before), payload);
        assert!(fix.th.contains("Registry Editor (regedit)"), "{}", fix.th);
        assert!(
            fix.th.contains(COMMAND_PROCESSOR_KEY) && fix.th.ends_with("echo hi"),
            "{}",
            fix.th
        );
        let absent = to_json(&AutoRunRecord {
            key_existed: true,
            value: None,
        })
        .unwrap();
        let fix = fix_by_hand(&autorun, &Some(absent), payload);
        assert!(
            fix.th.contains("Registry Editor (regedit)") && fix.th.contains("AutoRun"),
            "{}",
            fix.th
        );
        let runtime = PathBuf::from(r"C:\d\runtime\omp-31.5.0");
        for op in [
            Op::InstallRuntime {
                component: Component::Omp,
                dir: runtime.clone(),
            },
            Op::InstallFont {
                family: NERD_FONT_FAMILY.into(),
            },
        ] {
            let fix = fix_by_hand(&op, &None, payload);
            assert!(fix.th.contains("cmdstudio --cleanup"), "{op:?}: {}", fix.th);
            assert_ne!(fix.th, fix.en);
        }
        assert!(fix_by_hand(
            &Op::InstallRuntime {
                component: Component::Omp,
                dir: runtime.clone()
            },
            &None,
            payload
        )
        .th
        .contains(&runtime.display().to_string()));
        // An AutoRun prior that is not readable as a record still gets advice in both languages.
        let fix = fix_by_hand(&autorun, &Some(b"not json".to_vec()), payload);
        assert!(fix.en.contains("cmdstudio --cleanup") && fix.th.contains("cmdstudio --cleanup"));
    }

    // ---- The rulings that came after the brief ----

    const PD: &str = r"C:\ProgramData\ai.resincore.cmdstudio-S-1-5-21-1-2-3-1001";

    #[test]
    fn a_program_data_runtime_is_checked_again_before_it_counts_as_installed() {
        let (m, res, paths) = setup();
        let pd = PathBuf::from(PD);
        let dir = pd.join("clink-1.9.34");
        let op = Op::InstallRuntime {
            component: Component::Clink,
            dir: dir.clone(),
        };
        let mut ex = SystemExecutor::new(m.system(), &res);
        ex.apply(&op).unwrap();
        ex.apply(&op).unwrap();
        assert_eq!(
            m.fs.file(dir.join("clink_x64.exe")),
            Some(b"clink-exe".to_vec())
        );

        // Someone else took over the folder after the install (spec §5.4 step 3): the files are
        // the same, but they no longer count as installed.
        m.host
            .set_dir_state(&pd, DirState::Unsafe("S-1-5-32-545 may change it".into()));
        let err = ex.apply(&op).unwrap_err();
        assert_eq!(err.code, "E_CLINK_PATH");
        assert!(err.message.contains("S-1-5-32-545"), "{}", err.message);

        // The parent is private again, but the runtime folder itself is a link to another place.
        m.host.set_dir_state(&pd, DirState::Private);
        m.host.set_dir_state(
            &dir,
            DirState::Unsafe("it is a link to another place".into()),
        );
        assert_eq!(ex.apply(&op).unwrap_err().code, "E_CLINK_PATH");
        // A runtime folder that is private, as one made inside the private folder is, passes.
        m.host.set_dir_state(&dir, DirState::Private);
        ex.apply(&op).unwrap();

        // An undo that removed the folder and is rolled back puts it back only into a private
        // folder.
        let there = ex.capture(&op).unwrap();
        m.fs.remove_dir_all(&dir).unwrap();
        m.host.set_dir_state(&dir, DirState::Absent);
        m.host
            .set_dir_state(&pd, DirState::Unsafe("owned by S-1-5-21-9-9-9-1002".into()));
        assert_eq!(ex.restore(&op, &there).unwrap_err().code, "E_CLINK_PATH");
        assert!(!m.fs.exists(&dir));

        // Runtimes under the data folder are not ProgramData ones: the host is not asked about them.
        m.host
            .set_dir_state(paths.clink_dir(), DirState::Unsafe("not looked at".into()));
        let data = Op::InstallRuntime {
            component: Component::Clink,
            dir: paths.clink_dir(),
        };
        ex.apply(&data).unwrap();
        ex.apply(&data).unwrap();
    }

    #[test]
    fn a_program_data_runtime_round_trips_and_takes_the_folder_it_made_with_it() {
        let (m, res, _) = setup();
        // %ProgramData% itself is on every Windows.
        m.fs.create_dir_all(Path::new(r"C:\ProgramData")).unwrap();
        let op = Op::InstallRuntime {
            component: Component::Clink,
            dir: PathBuf::from(PD).join("clink-1.9.34"),
        };
        round_trip(&m, &res, &op);
        assert!(!m.fs.exists(Path::new(PD)));
    }

    #[test]
    fn a_font_that_was_there_is_put_back_and_loaded() {
        // An undo uninstalls the font. If a later step of the undo fails, its rollback must put
        // the font back as it was: the theme's Windows Terminal profile names it.
        let (m, res, _) = setup();
        let op = Op::InstallFont {
            family: NERD_FONT_FAMILY.into(),
        };
        let dest = m.fonts.user_fonts_dir().join(NERD_FONT_FILE);
        let mut ex = SystemExecutor::new(m.system(), &res);
        let before = ex.capture(&op).unwrap();
        ex.apply(&op).unwrap();
        let themed = ex.capture(&op).unwrap();
        let (fs_themed, fonts_themed) = (m.fs.snapshot(), m.fonts.snapshot());
        // The undo.
        ex.restore(&op, &before).unwrap();
        assert!(!m.fs.exists(&dest));
        assert!(!m
            .fonts
            .families()
            .unwrap()
            .contains(&NERD_FONT_FAMILY.to_string()));
        // Its rollback, twice.
        ex.restore(&op, &themed).unwrap();
        ex.restore(&op, &themed).unwrap();
        assert_eq!(m.fs.file(&dest), Some(b"font-bytes".to_vec()));
        assert_eq!(
            m.fonts.registration("CaskaydiaCove NF (TrueType)").unwrap(),
            Some(dest.display().to_string())
        );
        assert!(
            m.fonts
                .families()
                .unwrap()
                .contains(&NERD_FONT_FAMILY.to_string()),
            "loaded again"
        );
        assert_eq!(m.fs.snapshot(), fs_themed);
        assert_eq!(m.fonts.snapshot(), fonts_themed);
    }

    #[test]
    fn a_font_file_that_was_there_round_trips_byte_for_byte() {
        let op = Op::InstallFont {
            family: NERD_FONT_FAMILY.into(),
        };
        // The font was installed already: the bundled file, registered and loaded.
        let (m, res, _) = setup();
        let dest = m.fonts.user_fonts_dir().join(NERD_FONT_FILE);
        m.fs.put(&dest, b"font-bytes");
        m.fonts
            .register("CaskaydiaCove NF (TrueType)", dest.to_str().unwrap())
            .unwrap();
        m.fonts.load(&dest).unwrap();
        round_trip(&m, &res, &op);
        // A file left behind by an earlier uninstall: there, but neither registered nor loaded.
        let (m, res, _) = setup();
        m.fs.put(&dest, b"font-bytes");
        round_trip(&m, &res, &op);
        // Another file under the same name (an older Nerd Font): apply replaces it, and every
        // restore brings its bytes back.
        let (m, res, _) = setup();
        m.fs.put(&dest, b"an older font");
        round_trip(&m, &res, &op);
        assert_eq!(m.fs.file(&dest), Some(b"an older font".to_vec()));
        // ... also when it was registered (the restore loads it again).
        let (m, res, _) = setup();
        m.fs.put(&dest, b"an older font");
        m.fonts
            .register("CaskaydiaCove NF (TrueType)", dest.to_str().unwrap())
            .unwrap();
        m.fonts.load(&dest).unwrap();
        round_trip(&m, &res, &op);
    }

    /// Passes every call on to the fakes, and writes down the ones that change the font table,
    /// the registration or delete a file, in order. `unload` can be made to fail.
    struct Recorder<'a> {
        fs: &'a FakeFs,
        fonts: &'a FakeFonts,
        log: RefCell<Vec<String>>,
        fail_unload: bool,
    }

    impl<'a> Recorder<'a> {
        fn new(m: &'a FakeMachine) -> Self {
            Recorder {
                fs: &m.fs,
                fonts: &m.fonts,
                log: RefCell::new(Vec::new()),
                fail_unload: false,
            }
        }
        fn note(&self, what: String) {
            self.log.borrow_mut().push(what);
        }
        fn system(&'a self, m: &'a FakeMachine) -> System<'a> {
            System {
                fs: self,
                fonts: self,
                ..m.system()
            }
        }
    }

    impl Fs for Recorder<'_> {
        fn read(&self, path: &Path) -> Result<Option<Vec<u8>>, AdapterError> {
            self.fs.read(path)
        }
        fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<(), AdapterError> {
            self.fs.write_atomic(path, bytes)
        }
        fn remove_file(&self, path: &Path) -> Result<(), AdapterError> {
            self.note(format!(
                "delete {}",
                path.file_name().unwrap().to_string_lossy()
            ));
            self.fs.remove_file(path)
        }
        fn exists(&self, path: &Path) -> bool {
            self.fs.exists(path)
        }
        fn is_dir(&self, path: &Path) -> bool {
            self.fs.is_dir(path)
        }
        fn create_dir_all(&self, path: &Path) -> Result<(), AdapterError> {
            self.fs.create_dir_all(path)
        }
        fn remove_dir(&self, path: &Path) -> Result<(), AdapterError> {
            self.fs.remove_dir(path)
        }
        fn remove_dir_all(&self, path: &Path) -> Result<(), AdapterError> {
            self.fs.remove_dir_all(path)
        }
        fn rename(&self, from: &Path, to: &Path) -> Result<(), AdapterError> {
            self.fs.rename(from, to)
        }
        fn list_dir(&self, path: &Path) -> Result<Vec<DirEntry>, AdapterError> {
            self.fs.list_dir(path)
        }
    }

    impl Fonts for Recorder<'_> {
        fn families(&self) -> Result<Vec<String>, AdapterError> {
            self.fonts.families()
        }
        fn user_fonts_dir(&self) -> PathBuf {
            self.fonts.user_fonts_dir()
        }
        fn registration(&self, value_name: &str) -> Result<Option<String>, AdapterError> {
            self.fonts.registration(value_name)
        }
        fn register(&self, value_name: &str, file: &str) -> Result<(), AdapterError> {
            self.note("register".into());
            self.fonts.register(value_name, file)
        }
        fn unregister(&self, value_name: &str) -> Result<(), AdapterError> {
            self.note("unregister".into());
            self.fonts.unregister(value_name)
        }
        fn registration_key_exists(&self) -> Result<bool, AdapterError> {
            self.fonts.registration_key_exists()
        }
        fn delete_registration_key_if_empty(&self) -> Result<(), AdapterError> {
            self.fonts.delete_registration_key_if_empty()
        }
        fn load(&self, file: &Path) -> Result<(), AdapterError> {
            self.note("load".into());
            self.fonts.load(file)
        }
        fn unload(&self, file: &Path) -> Result<(), AdapterError> {
            self.note("unload".into());
            if self.fail_unload {
                return Err(AdapterError::new(
                    "RemoveFontResourceW",
                    "still loaded after 32 removals (injected)",
                ));
            }
            self.fonts.unload(file)
        }
    }

    #[test]
    fn a_font_is_removed_in_the_order_g12_proved() {
        // Unload (which broadcasts), then the registration, then the file (spec §5.4, G12).
        let (m, res, _) = setup();
        let op = Op::InstallFont {
            family: NERD_FONT_FAMILY.into(),
        };
        let rec = Recorder::new(&m);
        let mut ex = SystemExecutor::new(rec.system(&m), &res);
        let prior = ex.capture(&op).unwrap();
        ex.apply(&op).unwrap();
        rec.log.borrow_mut().clear();
        ex.restore(&op, &prior).unwrap();
        assert_eq!(
            *rec.log.borrow(),
            [
                "unload",
                "unregister",
                "delete CaskaydiaCoveNerdFont-Regular.ttf"
            ]
        );
    }

    #[test]
    fn a_held_font_file_is_listed_for_a_later_delete() {
        let (m, res, _) = setup();
        let op = Op::InstallFont {
            family: NERD_FONT_FAMILY.into(),
        };
        let dest = m.fonts.user_fonts_dir().join(NERD_FONT_FILE);
        let mut ex = SystemExecutor::new(m.system(), &res);
        let prior = ex.capture(&op).unwrap();
        ex.apply(&op).unwrap();
        assert!(ex.held_files().is_empty());
        // Windows Terminal holds the file.
        m.fs.fail_removes_of(&dest);
        ex.restore(&op, &prior).unwrap();
        ex.restore(&op, &prior).unwrap();
        // Uninstalled all the same: unloaded and unregistered. Only the file is left, and it is
        // listed once, for Task 13 to note in state.json and delete later.
        assert_eq!(
            m.fonts.registration("CaskaydiaCove NF (TrueType)").unwrap(),
            None
        );
        assert!(!m
            .fonts
            .families()
            .unwrap()
            .contains(&NERD_FONT_FAMILY.to_string()));
        assert!(m.fs.exists(&dest));
        assert_eq!(ex.held_files(), vec![dest.clone()]);
        // Installing it again makes the file ours to keep: nothing is left to delete.
        ex.apply(&op).unwrap();
        assert!(ex.held_files().is_empty());
    }

    #[test]
    fn a_font_that_will_not_unload_still_ends_unregistered() {
        let op = Op::InstallFont {
            family: NERD_FONT_FAMILY.into(),
        };
        // The unload fails, and so does the delete, as on Windows for a file still loaded.
        let (m, res, _) = setup();
        let dest = m.fonts.user_fonts_dir().join(NERD_FONT_FILE);
        let mut rec = Recorder::new(&m);
        rec.fail_unload = true;
        let mut ex = SystemExecutor::new(rec.system(&m), &res);
        let prior = ex.capture(&op).unwrap();
        ex.apply(&op).unwrap();
        m.fs.fail_removes_of(&dest);
        rec.log.borrow_mut().clear();
        ex.restore(&op, &prior).unwrap();
        assert_eq!(
            *rec.log.borrow(),
            [
                "unload",
                "unregister",
                "delete CaskaydiaCoveNerdFont-Regular.ttf"
            ]
        );
        assert_eq!(
            m.fonts.registration("CaskaydiaCove NF (TrueType)").unwrap(),
            None
        );
        assert_eq!(ex.held_files(), vec![dest.clone()]);
        // When the delete works despite the failed unload, nothing is left to delete.
        let (m, res, _) = setup();
        let mut rec = Recorder::new(&m);
        rec.fail_unload = true;
        let mut ex = SystemExecutor::new(rec.system(&m), &res);
        let prior = ex.capture(&op).unwrap();
        ex.apply(&op).unwrap();
        ex.restore(&op, &prior).unwrap();
        assert!(!m.fs.exists(&dest));
        assert_eq!(
            m.fonts.registration("CaskaydiaCove NF (TrueType)").unwrap(),
            None
        );
        assert!(ex.held_files().is_empty());
    }

    #[test]
    fn a_font_put_back_is_not_deleted_later_and_the_undo_executor_reports_held_files() {
        let (m, res, _) = setup();
        let op = Op::InstallFont {
            family: NERD_FONT_FAMILY.into(),
        };
        let dest = m.fonts.user_fonts_dir().join(NERD_FONT_FILE);
        let mut ex = SystemExecutor::new(m.system(), &res);
        let before = ex.capture(&op).unwrap();
        ex.apply(&op).unwrap();
        // The undo uninstalls the font while Windows Terminal holds its file ...
        let mut undo = UndoExecutor::new(
            SystemExecutor::new(m.system(), &res),
            vec![(op.clone(), before)],
        );
        let themed = undo.capture(&op).unwrap();
        m.fs.fail_removes_of(&dest);
        undo.apply(&op).unwrap();
        assert_eq!(undo.held_files(), vec![dest.clone()]);
        assert_eq!(
            m.fonts.registration("CaskaydiaCove NF (TrueType)").unwrap(),
            None
        );
        // ... then a later step of the undo fails, and its rollback puts the font back. The file
        // is installed again, so it must not be deleted later.
        undo.restore(&op, &themed).unwrap();
        assert!(undo.held_files().is_empty());
        assert_eq!(
            m.fonts.registration("CaskaydiaCove NF (TrueType)").unwrap(),
            Some(dest.display().to_string())
        );
        assert!(m
            .fonts
            .families()
            .unwrap()
            .contains(&NERD_FONT_FAMILY.to_string()));
    }

    // ---- Capturing ----

    /// An AutoRun whose reads fail: the key check (`key`), or else the value.
    struct UnreadableAutoRun {
        key: bool,
    }

    impl AutoRun for UnreadableAutoRun {
        fn user_key_exists(&self) -> Result<bool, AdapterError> {
            if self.key {
                Err(AdapterError::new(
                    "Command Processor",
                    "access denied (injected)",
                ))
            } else {
                Ok(true)
            }
        }
        fn read_user(&self) -> Result<Option<RegValue>, AdapterError> {
            Err(AdapterError::new("AutoRun", "access denied (injected)"))
        }
        fn write_user(&self, _: &RegValue) -> Result<(), AdapterError> {
            panic!("a capture writes nothing")
        }
        fn delete_user(&self) -> Result<(), AdapterError> {
            panic!("a capture deletes nothing")
        }
        fn delete_user_key_if_empty(&self) -> Result<(), AdapterError> {
            panic!("a capture deletes nothing")
        }
        fn read_machine(&self) -> Result<Option<RegValue>, AdapterError> {
            Ok(None)
        }
    }

    #[test]
    fn a_target_that_cannot_be_read_is_a_machine_read_failure_never_absent() {
        let (m, res, paths) = setup();
        let file = paths.generated().join("init.lua");
        m.fs.put(&file, b"old");
        m.fs.fail_reads_of(&file);
        m.fs.put(r"C:\wt\settings.json", b"{}");
        m.fs.fail_reads_of(r"C:\wt\settings.json");
        let mut ex = SystemExecutor::new(m.system(), &res);
        let write = Op::WriteFile {
            path: file,
            contents: "x".into(),
        };
        let wt = Op::WtApply {
            settings: PathBuf::from(r"C:\wt\settings.json"),
            scheme: json!({}),
            profile: Map::new(),
            set_default: false,
        };
        for op in [write, wt] {
            let err = ex.capture(&op).unwrap_err();
            assert_eq!(err.code, "E_MACHINE_READ", "{op:?}");
            assert!(err.message.contains("(injected)"), "{}", err.message);
        }
        // A font registration that is there but is not plain REG_SZ text (Task 6): never taken
        // for absent, which a restore would turn into deleting it.
        let font = Op::InstallFont {
            family: NERD_FONT_FAMILY.into(),
        };
        m.fonts.fail_registration_of("CaskaydiaCove NF (TrueType)");
        assert_eq!(ex.capture(&font).unwrap_err().code, "E_MACHINE_READ");
        let (m, res, _) = setup();
        m.fs.put(m.fonts.user_fonts_dir().join(NERD_FONT_FILE), b"font-bytes");
        m.fs.fail_reads_of(m.fonts.user_fonts_dir().join(NERD_FONT_FILE));
        assert_eq!(
            SystemExecutor::new(m.system(), &res)
                .capture(&font)
                .unwrap_err()
                .code,
            "E_MACHINE_READ"
        );
        // AutoRun: the key check and the value.
        let (m, res, _) = setup();
        for key in [true, false] {
            let autorun = UnreadableAutoRun { key };
            let sys = System {
                autorun: &autorun,
                ..m.system()
            };
            let err = SystemExecutor::new(sys, &res)
                .capture(&Op::SetAutoRun { value: None })
                .unwrap_err();
            assert_eq!(err.code, "E_MACHINE_READ", "key: {key}");
        }
    }

    #[test]
    fn an_empty_file_is_not_an_absent_one() {
        let (m, res, paths) = setup();
        let empty = paths.generated().join("empty.lua");
        let absent = paths.generated().join("absent.lua");
        m.fs.put(&empty, b"");
        let mut ex = SystemExecutor::new(m.system(), &res);
        let (on_empty, on_absent) = (
            Op::WriteFile {
                path: empty.clone(),
                contents: "x".into(),
            },
            Op::WriteFile {
                path: absent.clone(),
                contents: "x".into(),
            },
        );
        let (was_empty, was_absent) = (
            ex.capture(&on_empty).unwrap(),
            ex.capture(&on_absent).unwrap(),
        );
        assert_eq!(was_empty, Some(Vec::new()));
        assert_eq!(was_absent, None);
        ex.apply(&on_empty).unwrap();
        ex.apply(&on_absent).unwrap();
        for _ in 0..2 {
            ex.restore(&on_empty, &was_empty).unwrap();
            ex.restore(&on_absent, &was_absent).unwrap();
        }
        assert_eq!(m.fs.file(&empty), Some(Vec::new()));
        assert!(!m.fs.exists(&absent));
        assert!(!same_state(&on_empty, &Some(Vec::new()), &None));
    }

    #[test]
    fn autorun_comes_back_with_its_exact_type_and_bytes() {
        // A REG_EXPAND_SZ with bytes after its text, an empty REG_SZ, a REG_SZ with no closing
        // NUL, and a value that is not text at all: each comes back byte for byte.
        let mut odd = RegValue::from_text(REG_EXPAND_SZ, "%USERPROFILE%\\a & ไทย");
        odd.bytes.extend_from_slice(&[0x41, 0x00, 0xff]);
        let values = [
            odd,
            RegValue {
                kind: REG_SZ,
                bytes: Vec::new(),
            },
            RegValue {
                kind: REG_SZ,
                bytes: vec![0x61, 0x00],
            },
            RegValue {
                kind: 4,
                bytes: vec![1, 0, 0, 0],
            },
        ];
        for value in values {
            for op in [
                Op::SetAutoRun {
                    value: Some("ours".into()),
                },
                Op::SetAutoRun { value: None },
            ] {
                let (m, res, _) = setup();
                m.autorun.write_user(&value).unwrap();
                round_trip(&m, &res, &op);
                assert_eq!(m.autorun.read_user().unwrap(), Some(value.clone()));
            }
        }
        // Apply writes REG_SZ over anything but a REG_EXPAND_SZ.
        let (m, res, _) = setup();
        m.autorun
            .write_user(&RegValue {
                kind: 4,
                bytes: vec![1, 0, 0, 0],
            })
            .unwrap();
        SystemExecutor::new(m.system(), &res)
            .apply(&Op::SetAutoRun {
                value: Some("ours".into()),
            })
            .unwrap();
        assert_eq!(
            m.autorun.read_user().unwrap(),
            Some(RegValue::from_text(REG_SZ, "ours"))
        );
    }

    // ---- Restoring ----

    #[test]
    fn a_damaged_restore_point_is_reported_as_unreadable_and_changes_nothing() {
        let (m, res, paths) = setup();
        m.autorun
            .write_user(&RegValue::from_text(REG_SZ, "keep"))
            .unwrap();
        let ops = [
            Op::InstallRuntime {
                component: Component::Clink,
                dir: paths.clink_dir(),
            },
            Op::SetAutoRun { value: None },
            Op::InstallFont {
                family: NERD_FONT_FAMILY.into(),
            },
        ];
        let bad_hex = to_json(&AutoRunRecord {
            key_existed: true,
            value: Some(RegValueRecord {
                kind: REG_SZ,
                data_hex: "zz".into(),
            }),
        })
        .unwrap();
        let bad_font = br#"{"fileExisted":true,"dirExisted":true,"keyExisted":true,"registration":null,"fileHex":"0g"}"#;
        let before = (m.fs.snapshot(), m.autorun.snapshot(), m.fonts.snapshot());
        let mut ex = SystemExecutor::new(m.system(), &res);
        for op in &ops {
            for prior in [None, Some(b"not json".to_vec()), Some(b"{}".to_vec())] {
                assert_eq!(
                    ex.restore(op, &prior).unwrap_err().code,
                    "E_SNAPSHOT_READ",
                    "{op:?} {prior:?}"
                );
            }
        }
        assert_eq!(
            ex.restore(&ops[1], &Some(bad_hex)).unwrap_err().code,
            "E_SNAPSHOT_READ"
        );
        assert_eq!(
            ex.restore(&ops[2], &Some(bad_font.to_vec()))
                .unwrap_err()
                .code,
            "E_SNAPSHOT_READ"
        );
        assert_eq!(
            (m.fs.snapshot(), m.autorun.snapshot(), m.fonts.snapshot()),
            before
        );
    }

    #[test]
    fn windows_terminal_settings_are_written_only_when_they_change() {
        let (m, res, _) = setup();
        let settings = PathBuf::from(r"C:\wt\settings.json");
        m.fs.put(&settings, br#"{ "profiles": { "list": [] } }"#);
        let op = Op::WtApply {
            settings: settings.clone(),
            scheme: json!({ "name": "CMD Studio" }),
            profile: Map::new(),
            set_default: false,
        };
        let mut ex = SystemExecutor::new(m.system(), &res);
        let prior = ex.capture(&op).unwrap();
        ex.apply(&op).unwrap();
        let applied = ex.capture(&op).unwrap();
        // From here on any write fails: a second apply, and a restore to what is there, write
        // nothing.
        m.fs.fail_writes_to(&settings);
        ex.apply(&op).unwrap();
        ex.restore(&op, &applied).unwrap();
        // A restore that has to write reports the failure with the settings code.
        assert_eq!(ex.restore(&op, &prior).unwrap_err().code, "E_WT_WRITE");
        // A settings.json that was not there is neither created nor deleted.
        let gone = Op::WtApply {
            settings: PathBuf::from(r"C:\wt\other.json"),
            scheme: json!({}),
            profile: Map::new(),
            set_default: false,
        };
        ex.restore(&gone, &None).unwrap();
        assert!(!m.fs.exists(Path::new(r"C:\wt\other.json")));
    }

    #[test]
    fn a_tampered_bundle_installs_nothing() {
        let (m, res, paths) = setup();
        // The second file of the bundle, so the first one is already copied when it fails.
        m.fs.put(Path::new(RES).join(r"clink\clink_x64.exe"), b"tampered");
        let mut ex = SystemExecutor::new(m.system(), &res);
        let op = Op::InstallRuntime {
            component: Component::Clink,
            dir: paths.clink_dir(),
        };
        assert_eq!(ex.apply(&op).unwrap_err().code, "E_RUNTIME_MISSING");
        assert!(!m.fs.exists(&paths.clink_dir()));
        // No half-made folder is left beside it.
        assert!(m.fs.list_dir(&paths.runtime_root()).unwrap().is_empty());

        let font = Op::InstallFont {
            family: NERD_FONT_FAMILY.into(),
        };
        let dest = m.fonts.user_fonts_dir().join(NERD_FONT_FILE);
        m.fs.put(
            Path::new(RES).join("fonts").join(NERD_FONT_FILE),
            b"tampered",
        );
        assert_eq!(ex.apply(&font).unwrap_err().code, "E_RUNTIME_MISSING");
        m.fs.remove_file(&Path::new(RES).join("fonts").join(NERD_FONT_FILE))
            .unwrap();
        assert_eq!(ex.apply(&font).unwrap_err().code, "E_RUNTIME_MISSING");
        assert!(!m.fs.exists(&dest));
        assert_eq!(
            m.fonts.registration("CaskaydiaCove NF (TrueType)").unwrap(),
            None
        );
    }

    // ---- Undo ----

    #[test]
    fn same_state_compares_what_each_kind_of_target_keeps() {
        let autorun = Op::SetAutoRun { value: None };
        let record = |key_existed, value: Option<RegValue>| {
            Some(
                to_json(&AutoRunRecord {
                    key_existed,
                    value: value.as_ref().map(RegValueRecord::from_reg),
                })
                .unwrap(),
            )
        };
        let sz = RegValue::from_text(REG_SZ, "x");
        let expand = RegValue::from_text(REG_EXPAND_SZ, "x");
        // The key may stay when another value lives in it; the value's type and bytes count.
        assert!(same_state(
            &autorun,
            &record(true, Some(sz.clone())),
            &record(false, Some(sz.clone()))
        ));
        assert!(!same_state(
            &autorun,
            &record(true, Some(sz.clone())),
            &record(true, Some(expand))
        ));
        assert!(!same_state(
            &autorun,
            &record(true, Some(sz)),
            &record(true, None)
        ));
        // A font: only the registration counts, not a held file.
        let font = Op::InstallFont {
            family: NERD_FONT_FAMILY.into(),
        };
        let prior = |file_existed, registration: Option<&str>| {
            Some(
                to_json(&FontPrior {
                    file_existed,
                    dir_existed: true,
                    key_existed: true,
                    registration: registration.map(str::to_string),
                    file_hex: None,
                })
                .unwrap(),
            )
        };
        assert!(same_state(&font, &prior(true, None), &prior(false, None)));
        assert!(!same_state(
            &font,
            &prior(true, Some(r"C:\f.ttf")),
            &prior(true, None)
        ));
        // A runtime: whether the folder is there.
        let runtime = Op::InstallRuntime {
            component: Component::Omp,
            dir: PathBuf::from(r"C:\d\runtime\omp-31.5.0"),
        };
        let there = |dir_existed, parent_existed| {
            Some(
                to_json(&RuntimePrior {
                    dir_existed,
                    parent_existed,
                })
                .unwrap(),
            )
        };
        assert!(same_state(
            &runtime,
            &there(false, true),
            &there(false, false)
        ));
        assert!(!same_state(
            &runtime,
            &there(true, true),
            &there(false, true)
        ));
        // A record that does not decode matches nothing, not even itself.
        for op in [&autorun, &font, &runtime] {
            assert!(
                !same_state(op, &Some(b"junk".to_vec()), &Some(b"junk".to_vec())),
                "{op:?}"
            );
            assert!(!same_state(op, &None, &None), "{op:?}");
        }
    }

    #[derive(Default)]
    struct Journals(Vec<Journal>);

    impl JournalStore for Journals {
        fn save(&mut self, journal: &Journal) -> Result<(), Failure> {
            self.0.push(journal.clone());
            Ok(())
        }
        fn load_unfinished(&mut self) -> Result<Option<Journal>, Failure> {
            Ok(None)
        }
    }

    struct Pass;

    impl Verifier for Pass {
        fn verify(&mut self, _: &[Op]) -> Result<(), Failure> {
            Ok(())
        }
    }

    /// Every kind of op, on a machine with a settings.json, an old file and someone's AutoRun.
    fn themed_ops(m: &FakeMachine, paths: &Paths) -> Vec<Op> {
        m.fs.put(paths.generated().join("init.lua"), b"old");
        m.fs.put(
            r"C:\wt\settings.json",
            br#"{ "profiles": { "list": [] } } // c"#,
        );
        m.autorun
            .write_user(&RegValue::from_text(REG_EXPAND_SZ, "%A% & echo hi"))
            .unwrap();
        vec![
            Op::InstallRuntime {
                component: Component::Omp,
                dir: paths.omp_dir(),
            },
            Op::InstallRuntime {
                component: Component::Clink,
                dir: paths.clink_dir(),
            },
            Op::WriteFile {
                path: paths.generated().join("init.lua"),
                contents: "new".into(),
            },
            Op::InstallFont {
                family: NERD_FONT_FAMILY.into(),
            },
            Op::WtApply {
                settings: PathBuf::from(r"C:\wt\settings.json"),
                scheme: json!({ "name": "CMD Studio" }),
                profile: Map::new(),
                set_default: true,
            },
            Op::SetAutoRun {
                value: Some("ours & %A% & echo hi".into()),
            },
        ]
    }

    /// Applies `ops` through the txn engine; returns each op with the prior it captured.
    fn apply_all(m: &FakeMachine, res: &Resources, ops: &[Op]) -> Vec<(Op, Option<Vec<u8>>)> {
        let mut journals = Journals::default();
        let mut ex = SystemExecutor::new(m.system(), res);
        assert_eq!(
            txn::run("apply", ops.to_vec(), &mut ex, &mut Pass, &mut journals),
            Outcome::Committed
        );
        let j = journals.0.last().unwrap();
        j.snapshots
            .iter()
            .map(|s| (j.ops[s.op_index].clone(), s.prior.clone()))
            .collect()
    }

    fn undo_all(
        m: &FakeMachine,
        res: &Resources,
        ops: &[Op],
        priors: Vec<(Op, Option<Vec<u8>>)>,
    ) -> Outcome {
        let mut exec = UndoExecutor::new(SystemExecutor::new(m.system(), res), priors.clone());
        let mut verifier = UndoVerifier::new(SystemExecutor::new(m.system(), res), priors);
        let reversed: Vec<Op> = ops.iter().rev().cloned().collect();
        txn::run(
            "undo",
            reversed,
            &mut exec,
            &mut verifier,
            &mut Journals::default(),
        )
    }

    #[test]
    fn an_undo_puts_every_target_back_byte_for_byte() {
        let (m, res, paths) = setup();
        let ops = themed_ops(&m, &paths);
        let before = (m.fs.snapshot(), m.autorun.snapshot(), m.fonts.snapshot());
        let priors = apply_all(&m, &res, &ops);
        assert_ne!(
            (m.fs.snapshot(), m.autorun.snapshot(), m.fonts.snapshot()),
            before
        );
        assert_eq!(undo_all(&m, &res, &ops, priors), Outcome::Committed);
        assert_eq!(
            (m.fs.snapshot(), m.autorun.snapshot(), m.fonts.snapshot()),
            before
        );
    }

    #[test]
    fn an_undo_that_fails_half_way_leaves_the_theme_as_it_was() {
        // The undo's last step (removing the Oh My Posh runtime) fails because a cmd window
        // runs it. Its rollback puts back everything the undo had already taken out: the
        // Clink runtime from the bundle, the font, the files, the settings and AutoRun.
        let (m, res, paths) = setup();
        let ops = themed_ops(&m, &paths);
        let priors = apply_all(&m, &res, &ops);
        let themed = (m.fs.snapshot(), m.autorun.snapshot(), m.fonts.snapshot());
        m.fs.fail_removes_of(paths.omp_exe());
        let out = undo_all(&m, &res, &ops, priors);
        assert!(
            matches!(
                &out,
                Outcome::RolledBack {
                    code: "E_RUNTIME_MISSING",
                    ..
                }
            ),
            "{out:?}"
        );
        assert_eq!(
            (m.fs.snapshot(), m.autorun.snapshot(), m.fonts.snapshot()),
            themed
        );
    }

    #[test]
    fn restoring_a_step_that_never_ran_changes_nothing() {
        // The txn engine also restores the op that may have been running when it stopped, so a
        // restore must work on a target that apply never touched, any number of times.
        for key in [true, false] {
            let (m, res, paths) = setup();
            m.fonts.set_key_exists(key);
            m.autorun.set_key_exists(key);
            let mut ops = themed_ops(&m, &paths);
            ops.push(Op::InstallRuntime {
                component: Component::Clink,
                dir: PathBuf::from(PD).join("clink-1.9.34"),
            });
            ops.push(Op::SetAutoRun { value: None });
            let before = (m.fs.snapshot(), m.autorun.snapshot(), m.fonts.snapshot());
            let mut ex = SystemExecutor::new(m.system(), &res);
            for op in &ops {
                let prior = ex.capture(op).unwrap();
                ex.restore(op, &prior).unwrap();
                ex.restore(op, &prior).unwrap();
                assert_eq!(
                    (m.fs.snapshot(), m.autorun.snapshot(), m.fonts.snapshot()),
                    before,
                    "{op:?}"
                );
            }
            assert!(ex.held_files().is_empty());
        }
    }

    #[test]
    fn the_undo_verifier_names_a_target_that_did_not_come_back() {
        let (m, res, paths) = setup();
        let file = paths.generated().join("init.lua");
        let op = Op::WriteFile {
            path: file.clone(),
            contents: "new".into(),
        };
        let expected = vec![(op.clone(), Some(b"old".to_vec()))];
        m.fs.put(&file, b"old");
        let mut verifier = UndoVerifier::new(SystemExecutor::new(m.system(), &res), expected);
        verifier.verify(&[]).unwrap();
        m.fs.put(&file, b"edited since");
        let err = verifier.verify(&[]).unwrap_err();
        assert_eq!(err.code, "E_VERIFY_READBACK");
        assert!(err.message.contains(&op_label(&op)), "{}", err.message);
    }

    // ---- Start-up sweep ----

    #[test]
    fn sweep_deletes_only_half_made_and_half_removed_runtime_folders() {
        let (m, _, paths) = setup();
        let root = paths.runtime_root();
        let pd = PathBuf::from(PD);
        let gone = [
            root.join(".clink-1.9.34.partial-1a2b3c4d"),
            root.join(".omp-31.5.0.removing-deadbeef"),
            pd.join(".clink-1.9.34.partial-00ff00ff"),
        ];
        for dir in &gone {
            m.fs.put(dir.join("x.dll"), b"x");
        }
        let kept_files = [
            root.join(r"clink-1.9.34\clink_x64.exe"),
            pd.join(r"clink-1.9.34\clink_x64.exe"),
            // A file, not a folder, and a folder of another name.
            root.join(".x.partial-1"),
            root.join(r".hidden\y"),
        ];
        for file in &kept_files {
            m.fs.put(file, b"keep");
        }
        sweep(m.system(), &paths);
        for dir in &gone {
            assert!(!m.fs.exists(dir), "{}", dir.display());
        }
        for file in &kept_files {
            assert_eq!(
                m.fs.file(file),
                Some(b"keep".to_vec()),
                "{}",
                file.display()
            );
        }
    }

    #[test]
    fn op_labels_name_each_target() {
        let paths = Paths {
            data: PathBuf::from(r"C:\d"),
        };
        assert_eq!(
            op_label(&Op::InstallRuntime {
                component: Component::Omp,
                dir: paths.omp_dir()
            }),
            r"runtime folder C:\d\runtime\omp-31.5.0"
        );
        assert_eq!(op_label(&Op::SetAutoRun { value: None }), "cmd AutoRun");
        assert_eq!(
            op_label(&Op::InstallFont {
                family: NERD_FONT_FAMILY.into()
            }),
            "font CaskaydiaCove NF"
        );
    }
}
