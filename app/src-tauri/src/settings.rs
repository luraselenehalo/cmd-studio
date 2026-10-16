//! The app's own settings (spec §4.3 "ตั้งค่า"): the UI's language, where themes go, whether
//! Windows Terminal opens with cmd, how updates are handled, and whether the first-run wizard
//! was finished. In `<data>\settings.json`, not in state.json: an undo puts state.json back as
//! it was before an apply, and the settings must not go back with it. The UI always opens in
//! English unless Thai was chosen here (founder, 2026-10-16); the language of Windows is not
//! looked at.
use crate::error::AppError;
use cmdstudio_core::adapters::Fs;
use cmdstudio_core::model::Target;
use serde::{Deserialize, Serialize};
use std::path::Path;
use ts_rs::TS;

pub const SETTINGS_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Language {
    En,
    Th,
}

/// Spec §6.2: automatic (the default, founder 2026-10-16), notify first, or off. Stored and shown
/// here; Phase 4's updater reads it, which is what makes it live.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum UpdateMode {
    Auto,
    Notify,
    Off,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct AppSettings {
    pub language: Language,
    /// Where an apply puts the theme (the wizard's page 4 and the Settings page).
    pub target: Target,
    /// Make the cmd profile Windows Terminal's default profile when applying.
    pub set_wt_default_profile: bool,
    pub updates: UpdateMode,
    /// The first-run wizard was finished or skipped: the app opens on the home page.
    pub wizard_done: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            language: Language::En,
            target: Target::All,
            set_wt_default_profile: true,
            updates: UpdateMode::Auto,
            wizard_done: false,
        }
    }
}

/// `settings.json` on disk: a version in front of the settings.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsFile {
    version: u32,
    #[serde(flatten)]
    settings: AppSettings,
}

/// What `settings_get` returns: the settings, and the name of the damaged file that was set
/// aside when the defaults had to stand in (spec §2.1: no silent change).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SettingsLoad {
    pub settings: AppSettings,
    pub reset_from: Option<String>,
}

/// Reads the settings. No file gives the defaults. A file that cannot be read is an error
/// (`E_MACHINE_READ`). A damaged file, or one from another version, is renamed
/// `settings.json.damaged-<ms>` (kept for a look) and the defaults stand in, which the result
/// says; when the rename fails, that is an error (`E_DATA_WRITE`) and the file stays.
pub fn load(fs: &dyn Fs, path: &Path, now_ms: u64) -> Result<SettingsLoad, AppError> {
    let bytes = fs.read(path).map_err(|e| {
        AppError::coded(
            "E_MACHINE_READ",
            format!("{} could not be read: {e}", path.display()),
        )
    })?;
    let Some(bytes) = bytes else {
        return Ok(SettingsLoad {
            settings: AppSettings::default(),
            reset_from: None,
        });
    };
    let parsed: Result<SettingsFile, _> = serde_json::from_slice(&bytes);
    let why = match parsed {
        Ok(file) if file.version == SETTINGS_VERSION => {
            return Ok(SettingsLoad {
                settings: file.settings,
                reset_from: None,
            })
        }
        Ok(file) => format!(
            "version {} (this CMD Studio writes {SETTINGS_VERSION})",
            file.version
        ),
        Err(e) => e.to_string(),
    };
    let aside = set_aside_name(fs, path, now_ms);
    fs.rename(path, &aside).map_err(|e| {
        AppError::coded(
            "E_DATA_WRITE",
            format!(
                "{} is damaged ({why}) and could not be set aside as {}: {e}",
                path.display(),
                aside.display()
            ),
        )
    })?;
    Ok(SettingsLoad {
        settings: AppSettings::default(),
        reset_from: aside.file_name().map(|n| n.to_string_lossy().into_owned()),
    })
}

/// `<file>.damaged-<ms>`, with the milliseconds moved on while that name is taken.
fn set_aside_name(fs: &dyn Fs, path: &Path, now_ms: u64) -> std::path::PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "settings.json".to_string());
    let parent = path.parent().unwrap_or(Path::new(""));
    let mut ms = now_ms;
    loop {
        let candidate = parent.join(format!("{name}.damaged-{ms}"));
        if !fs.exists(&candidate) {
            return candidate;
        }
        ms = ms.saturating_add(1);
    }
}

/// Writes the settings, whole, in one step (`write_atomic`).
pub fn save(fs: &dyn Fs, path: &Path, settings: &AppSettings) -> Result<(), AppError> {
    let file = SettingsFile {
        version: SETTINGS_VERSION,
        settings: settings.clone(),
    };
    let mut bytes = serde_json::to_vec_pretty(&file).map_err(|e| {
        AppError::coded(
            "E_DATA_WRITE",
            format!("settings could not be produced: {e}"),
        )
    })?;
    bytes.push(b'\n');
    fs.write_atomic(path, &bytes).map_err(|e| {
        AppError::coded(
            "E_DATA_WRITE",
            format!("{} could not be written: {e}", path.display()),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmdstudio_core::adapters::fake::FakeFs;
    use std::path::PathBuf;

    fn file() -> PathBuf {
        PathBuf::from(r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio\settings.json")
    }

    #[test]
    fn the_defaults_are_english_every_window_automatic_updates_and_the_wizard_not_done() {
        let fs = FakeFs::new();
        let loaded = load(&fs, &file(), 1).unwrap();
        assert_eq!(loaded.reset_from, None);
        assert_eq!(
            loaded.settings,
            AppSettings {
                language: Language::En,
                target: Target::All,
                set_wt_default_profile: true,
                updates: UpdateMode::Auto,
                wizard_done: false,
            }
        );
    }

    #[test]
    fn settings_round_trip_as_camel_case_json_with_a_version() {
        let fs = FakeFs::new();
        let mine = AppSettings {
            language: Language::Th,
            target: Target::Wt,
            set_wt_default_profile: false,
            updates: UpdateMode::Off,
            wizard_done: true,
        };
        save(&fs, &file(), &mine).unwrap();
        let text = String::from_utf8(fs.file(file()).unwrap()).unwrap();
        assert!(
            text.starts_with("{\n  \"version\": 1,\n  \"language\": \"th\",\n"),
            "{text}"
        );
        assert!(
            text.contains("\"setWtDefaultProfile\": false") && text.ends_with("}\n"),
            "{text}"
        );
        assert_eq!(load(&fs, &file(), 2).unwrap().settings, mine);
        // A field missing from an older file reads as its default.
        fs.put(file(), b"{\"version\": 1, \"language\": \"th\"}");
        let loaded = load(&fs, &file(), 3).unwrap();
        assert_eq!(loaded.settings.language, Language::Th);
        assert_eq!(loaded.settings.target, Target::All);
    }

    #[test]
    fn a_damaged_or_newer_file_is_set_aside_and_said() {
        let fs = FakeFs::new();
        fs.put(file(), b"{ not json");
        let loaded = load(&fs, &file(), 1_791_694_709_000).unwrap();
        assert_eq!(loaded.settings, AppSettings::default());
        assert_eq!(
            loaded.reset_from.as_deref(),
            Some("settings.json.damaged-1791694709000")
        );
        assert!(!fs.exists(&file()));
        assert!(fs.exists(&file().with_file_name("settings.json.damaged-1791694709000")));
        // The same millisecond again: the name moves on instead of failing.
        fs.put(file(), b"{\"version\": 9}");
        let loaded = load(&fs, &file(), 1_791_694_709_000).unwrap();
        assert_eq!(
            loaded.reset_from.as_deref(),
            Some("settings.json.damaged-1791694709001")
        );
        // A rename that fails leaves the file and is an error, not a silent reset.
        fs.put(file(), b"{ not json");
        fs.fail_removes_of(file());
        let e = load(&fs, &file(), 5).unwrap_err();
        assert_eq!(e.code, "E_DATA_WRITE");
        assert!(fs.exists(&file()));
    }

    #[test]
    fn a_file_that_cannot_be_read_is_a_read_error() {
        let fs = FakeFs::new();
        fs.put(file(), b"{}");
        fs.fail_reads_of(file());
        assert_eq!(load(&fs, &file(), 1).unwrap_err().code, "E_MACHINE_READ");
    }
}
