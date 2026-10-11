//! Machine adapters (spec §5.2 `adapters`): the only layer that touches the
//! machine. Every method takes `&self`, so the executor, the verifier and the
//! journal store can share one adapter; the fakes use interior mutability.
//! Each Windows adapter takes its locations as parameters, so tests point it
//! at a throwaway registry key and temp folders (Phase 1b test safety).
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// A failure inside an adapter. Callers map it to a spec §5.9 code, because
/// only they know which step failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{context}: {message}")]
pub struct AdapterError {
    pub context: String,
    pub message: String,
}

impl AdapterError {
    pub fn new(context: impl Into<String>, message: impl Into<String>) -> Self {
        AdapterError {
            context: context.into(),
            message: message.into(),
        }
    }
}

/// One entry of a folder listing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirEntry {
    pub path: PathBuf,
    pub is_dir: bool,
}

/// Files and folders (spec §5.4 `Fs`).
pub trait Fs {
    /// The file's bytes; `None` when it does not exist.
    fn read(&self, path: &Path) -> Result<Option<Vec<u8>>, AdapterError>;
    /// Replaces the file in one step: a temp file in the same folder, flushed,
    /// then renamed over the target. Creates missing parent folders. When the
    /// target is a symbolic link, the link's target is written and the link stays.
    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<(), AdapterError>;
    /// Deletes a file; `Ok` when it is already gone.
    fn remove_file(&self, path: &Path) -> Result<(), AdapterError>;
    fn exists(&self, path: &Path) -> bool;
    fn is_dir(&self, path: &Path) -> bool;
    fn create_dir_all(&self, path: &Path) -> Result<(), AdapterError>;
    /// Deletes an empty folder; `Ok` when it is gone; an error when it is not empty or is a file.
    fn remove_dir(&self, path: &Path) -> Result<(), AdapterError>;
    /// Deletes a folder and everything in it; `Ok` when it is already gone.
    fn remove_dir_all(&self, path: &Path) -> Result<(), AdapterError>;
    /// Renames a file or folder; `to` must not exist, and the folder that is to hold it must.
    fn rename(&self, from: &Path, to: &Path) -> Result<(), AdapterError>;
    /// The entries directly inside a folder, sorted by path; empty when the folder does not exist.
    fn list_dir(&self, path: &Path) -> Result<Vec<DirEntry>, AdapterError>;
}

/// `REG_SZ`.
pub const REG_SZ: u32 = 1;
/// `REG_EXPAND_SZ`.
pub const REG_EXPAND_SZ: u32 = 2;

/// A registry value exactly as stored: its type and its raw bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegValue {
    pub kind: u32,
    pub bytes: Vec<u8>,
}

impl RegValue {
    /// UTF-16LE text plus one NUL, the way Windows stores `REG_SZ`.
    pub fn from_text(kind: u32, text: &str) -> RegValue {
        let mut bytes: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
        bytes.extend_from_slice(&[0, 0]);
        RegValue { kind, bytes }
    }

    /// The text of a `REG_SZ` or `REG_EXPAND_SZ` value, up to its first NUL,
    /// unexpanded. `None` for any other type or for bytes that are not UTF-16.
    pub fn text(&self) -> Option<String> {
        if self.kind != REG_SZ && self.kind != REG_EXPAND_SZ {
            return None;
        }
        let (pairs, _) = self.bytes.as_chunks::<2>();
        let units: Vec<u16> = pairs
            .iter()
            .map(|p| u16::from_le_bytes(*p))
            .take_while(|u| *u != 0)
            .collect();
        String::from_utf16(&units).ok()
    }
}

/// cmd's AutoRun value under `Command Processor` (spec §5.4 AutoRun).
pub trait AutoRun {
    /// Whether the user's `Command Processor` key exists.
    fn user_key_exists(&self) -> Result<bool, AdapterError>;
    /// The user's AutoRun value as stored; `None` when absent.
    fn read_user(&self) -> Result<Option<RegValue>, AdapterError>;
    /// Writes the user's AutoRun value exactly as given. Opens the existing key
    /// and creates it only when missing; never recreates it (spec §2.2 lesson 9).
    fn write_user(&self, value: &RegValue) -> Result<(), AdapterError>;
    /// Deletes the user's AutoRun value; `Ok` when it is absent.
    fn delete_user(&self) -> Result<(), AdapterError>;
    /// Deletes the user's `Command Processor` key only when it holds no values and no subkeys.
    fn delete_user_key_if_empty(&self) -> Result<(), AdapterError>;
    /// The machine-wide (HKLM) AutoRun value. Read only: CMD Studio never writes HKLM.
    fn read_machine(&self) -> Result<Option<RegValue>, AdapterError>;
}

/// Per-user fonts (spec §5.4 `Fonts`, G12).
pub trait Fonts {
    /// Family names GDI lists in this session (`EnumFontFamiliesExW`), sorted, without duplicates.
    fn families(&self) -> Result<Vec<String>, AdapterError>;
    /// `%LOCALAPPDATA%\Microsoft\Windows\Fonts`, or the test's folder.
    fn user_fonts_dir(&self) -> PathBuf;
    /// Data of a per-user font registration value (the font file's path); `None` when absent.
    fn registration(&self, value_name: &str) -> Result<Option<String>, AdapterError>;
    fn register(&self, value_name: &str, file: &str) -> Result<(), AdapterError>;
    /// Deletes the registration value; `Ok` when it is absent.
    fn unregister(&self, value_name: &str) -> Result<(), AdapterError>;
    /// Whether the registration key exists; a new account may not have it yet.
    fn registration_key_exists(&self) -> Result<bool, AdapterError>;
    /// Deletes the registration key only when it holds no values and no subkeys.
    fn delete_registration_key_if_empty(&self) -> Result<(), AdapterError>;
    /// Makes the font usable now: `AddFontResourceW`, then broadcast `WM_FONTCHANGE`.
    fn load(&self, file: &Path) -> Result<(), AdapterError>;
    /// `RemoveFontResourceW`, then broadcast; `Ok` when it was not loaded.
    fn unload(&self, file: &Path) -> Result<(), AdapterError>;
}

/// The Windows Terminal variants spec §5.4 lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WtVariant {
    Store,
    Preview,
    Unpackaged,
}

/// Where one Windows Terminal variant keeps its settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WtLocation {
    pub variant: WtVariant,
    /// The variant's own folder exists.
    pub installed: bool,
    pub settings: PathBuf,
}

/// Windows Terminal's settings files (spec §5.4 `WtSettings`). Reading and
/// writing go through `Fs`; editing is `wt_edit`.
pub trait WtSettings {
    fn locations(&self) -> Vec<WtLocation>;
}

/// What a finished process printed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Output {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Runs CMD Studio's own bundled programs with fixed arguments: no shell,
/// no window, a time limit (spec §5.10 last bullet).
pub trait Process {
    fn run(&self, exe: &Path, args: &[&OsStr]) -> Result<Output, AdapterError>;
}

/// Who can change a folder that must belong to this user alone (spec §5.4 step 3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DirState {
    Absent,
    /// Owned by the user, SYSTEM or Administrators, and nobody else may write, append or delete.
    Private,
    /// Anything else, with the reason.
    Unsafe(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Created {
    New,
    AlreadyExisted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Arch {
    X64,
    Arm64,
    Other,
}

/// Windows 11's "default terminal application" (spec §5.8 D11).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DefaultTerminal {
    /// Not set: Windows decides (Windows Terminal on Windows 11 22H2 and later).
    LetWindowsDecide,
    WindowsTerminal,
    WindowsTerminalPreview,
    Conhost,
    Other,
}

/// Facts for the wizard's machine check and for doctor (spec §4.2 page 2, §5.8).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    /// `CurrentBuildNumber`; `None` when it could not be read.
    pub build: Option<u32>,
    pub arch: Arch,
    pub default_terminal: DefaultTerminal,
    /// WebView2 Runtime version (`pv`), for Phase 4.
    pub webview2: Option<String>,
    /// The app runs inside an MSIX package (spec §2.2 lesson 1, D10).
    pub packaged: bool,
    /// `GetUserDefaultUILanguage`; Thai is 0x041E.
    pub ui_language: u16,
}

/// Identity, known folders, 8.3 names and the private ProgramData folder.
pub trait Host {
    /// `FOLDERID_LocalAppData`.
    fn local_app_data(&self) -> Result<PathBuf, AdapterError>;
    /// `FOLDERID_ProgramData`.
    fn program_data(&self) -> Result<PathBuf, AdapterError>;
    /// The current user's SID as text, such as `S-1-5-21-1-2-3-1001`.
    fn user_sid(&self) -> Result<String, AdapterError>;
    /// `GetShortPathNameW` of an existing path. On a volume that makes no 8.3
    /// names it returns the long path itself (G19).
    fn short_path(&self, path: &Path) -> Result<PathBuf, AdapterError>;
    fn private_dir_state(&self, dir: &Path) -> Result<DirState, AdapterError>;
    /// Creates the folder and its protected security descriptor in one call:
    /// `D:P(A;OICI;FA;;;<user SID>)(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)`.
    fn create_private_dir(&self, dir: &Path) -> Result<Created, AdapterError>;
    fn system_info(&self) -> SystemInfo;
}

/// Everything that touches the machine, borrowed for one command.
#[derive(Clone, Copy)]
pub struct System<'a> {
    pub fs: &'a dyn Fs,
    pub autorun: &'a dyn AutoRun,
    pub fonts: &'a dyn Fonts,
    pub wt: &'a dyn WtSettings,
    pub process: &'a dyn Process,
    pub host: &'a dyn Host,
}

/// `%ProgramData%\ai.resincore.cmdstudio-<SID>`: the user's own folder for
/// Clink when no ASCII path under the data folder exists (spec §5.4 step 3).
pub fn program_data_dir(program_data: &Path, sid: &str) -> PathBuf {
    program_data.join(format!("ai.resincore.cmdstudio-{sid}"))
}

#[cfg(any(test, feature = "fake"))]
pub mod fake;

#[cfg(windows)]
pub mod windows;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reg_text_round_trips_thai_and_stops_at_the_first_nul() {
        let v = RegValue::from_text(REG_SZ, "if exist \"C:\\ผู้ใช้\\x.exe\" x");
        assert_eq!(v.text().as_deref(), Some("if exist \"C:\\ผู้ใช้\\x.exe\" x"));
        assert_eq!(v.bytes.len() % 2, 0);
        assert_eq!(&v.bytes[v.bytes.len() - 2..], &[0, 0]);
        let mut odd = RegValue::from_text(REG_EXPAND_SZ, "%USERPROFILE%\\a");
        odd.bytes.extend_from_slice(&[0x41, 0x00, 0x00, 0x00]);
        assert_eq!(odd.text().as_deref(), Some("%USERPROFILE%\\a"));
        assert_eq!(
            RegValue {
                kind: 4,
                bytes: vec![1, 0, 0, 0]
            }
            .text(),
            None
        );
        assert_eq!(
            RegValue {
                kind: REG_SZ,
                bytes: vec![0x00, 0xD8, 0, 0]
            }
            .text(),
            None
        );
    }

    #[test]
    fn program_data_dir_has_no_shared_parent() {
        let d = program_data_dir(Path::new(r"C:\ProgramData"), "S-1-5-21-1-2-3-1001");
        assert_eq!(
            d,
            PathBuf::from(r"C:\ProgramData\ai.resincore.cmdstudio-S-1-5-21-1-2-3-1001")
        );
    }
}
