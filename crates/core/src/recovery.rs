//! The theme's pause and the start counter's files (recovery spec §3.1, §4.1, §7). The folder
//! `<data>\health\` is CMD Studio's alone:
//! - `start-<pid>`: init.lua's tag for one cmd window, the text `v1 <unix seconds>`, from the
//!   window's start to its first prompt;
//! - `paused.json`: the pause. init.lua writes it when [`PAUSE_AFTER`] windows in a row never
//!   reached their prompt, and loads no theme while the file is there, whatever it holds; the app
//!   writes it when it pauses the theme on this computer (Revert everything underneath), with the
//!   theme to apply again.
//!
//! The numbers live here once: the generator writes them into init.lua
//! (`generate::lua::init_lua`).
use crate::adapters::{AdapterError, Fs};
use crate::model::Target;
use crate::plan::Paths;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A tag older than this many seconds is a window that never reached its first prompt.
pub const STALE_AFTER_SECS: u64 = 60;
/// How many windows in a row that never reached their prompt pause the theme in cmd; also how
/// many crashes of cmd in Clink's DLL pause it on this computer (recovery spec §4.3) and make
/// doctor D12 fail.
pub const PAUSE_AFTER: u32 = 3;
/// How far back the app looks for cmd's crashes in Clink: seven days (recovery spec §4.3).
pub const CRASH_WINDOW_MS: u64 = 7 * 24 * 60 * 60 * 1000;
/// The version of `paused.json` this build writes and reads.
pub const PAUSED_VERSION: u32 = 1;
/// A start tag's name: this, then the window's process id.
pub const TAG_PREFIX: &str = "start-";

impl Paths {
    /// `<data>\health`: the start counter's tags and the pause (recovery spec §3.1).
    pub fn health(&self) -> PathBuf {
        self.data.join("health")
    }
    /// `<data>\health\paused.json`.
    pub fn paused_file(&self) -> PathBuf {
        self.health().join("paused.json")
    }
}

/// Why the theme is paused (recovery spec §3.1), as `paused.json` names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum PauseReason {
    /// init.lua: windows in a row that never reached their prompt.
    CmdStarts,
    /// The app: cmd crashed in Clink's DLL, as Windows' crash reports say.
    ClinkCrash,
    /// The app, started from the CMD Studio Recovery shortcut (phase 3b-2).
    Recovery,
    /// The user pressed "Pause the theme".
    User,
}

impl PauseReason {
    /// The name `paused.json` and the log use.
    pub fn as_str(self) -> &'static str {
        match self {
            PauseReason::CmdStarts => "cmdStarts",
            PauseReason::ClinkCrash => "clinkCrash",
            PauseReason::Recovery => "recovery",
            PauseReason::User => "user",
        }
    }
}

/// Who paused the theme, which is how far (recovery spec §4.1): init.lua in cmd, nothing on the
/// computer changed; or the app, on this computer, through Revert everything.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum PausedBy {
    Clink,
    App,
}

/// The theme a pause of this computer applies again when the theme is turned back on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resume {
    pub theme_id: uuid::Uuid,
    pub target: Target,
}

/// `paused.json` (recovery spec §3.1): `{"version": 1, "reason": …, "at": <unix ms>,
/// "count": …, "by": "clink" | "app", "resume": null | {"themeId": …, "target": …}}`. init.lua
/// writes the first five fields and no `resume`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PauseRecord {
    pub version: u32,
    pub reason: PauseReason,
    /// When, in milliseconds since 1970.
    pub at: u64,
    /// The windows in a row, or the crashes, that paused it; 0 for the others.
    pub count: u32,
    pub by: PausedBy,
    #[serde(default)]
    pub resume: Option<Resume>,
}

/// The pause as CMD Studio reads `paused.json`. A file that can't be read as a [`PauseRecord`]
/// of this version (damaged, half written, from a newer CMD Studio) is a pause all the same,
/// at the cmd level: init.lua loads no theme while the file is there, and turning the theme back
/// on deletes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paused {
    pub by: PausedBy,
    /// `None` for a file that can't be read as a record.
    pub reason: Option<PauseReason>,
    pub at_ms: Option<u64>,
    pub count: u32,
    pub resume: Option<Resume>,
}

impl From<PauseRecord> for Paused {
    fn from(r: PauseRecord) -> Self {
        Paused {
            by: r.by,
            reason: Some(r.reason),
            at_ms: Some(r.at),
            count: r.count,
            resume: r.resume,
        }
    }
}

/// The pause, `None` when `paused.json` is not there. A read that fails is an error, never "not
/// paused".
pub fn read_pause(fs: &dyn Fs, paths: &Paths) -> Result<Option<Paused>, AdapterError> {
    let Some(bytes) = fs.read(&paths.paused_file())? else {
        return Ok(None);
    };
    let record = serde_json::from_slice::<PauseRecord>(&bytes)
        .ok()
        .filter(|r| r.version == PAUSED_VERSION);
    Ok(Some(record.map_or(
        Paused {
            by: PausedBy::Clink,
            reason: None,
            at_ms: None,
            count: 0,
            resume: None,
        },
        Paused::from,
    )))
}

/// Writes `paused.json` in one step (`Fs::write_atomic`, which makes `health\` when it is
/// missing).
pub fn write_pause(fs: &dyn Fs, paths: &Paths, record: &PauseRecord) -> Result<(), AdapterError> {
    let file = paths.paused_file();
    // Always this build's version, whatever the caller holds: a record this build wrote must be
    // one it reads back (`read_pause`), not a pause with no reason, time or theme.
    let record = PauseRecord {
        version: PAUSED_VERSION,
        ..record.clone()
    };
    let mut bytes = serde_json::to_vec_pretty(&record)
        .map_err(|e| AdapterError::new(format!("write {}", file.display()), e.to_string()))?;
    bytes.push(b'\n');
    fs.write_atomic(&file, &bytes)
}

/// Lifts the pause: deletes every start tag (a window that comes after counts from nothing), then
/// `paused.json`, and makes sure `health\` is there for init.lua to write its tags in. The tags go
/// first so that a tag which cannot be deleted (a virus scanner holds it) is an error with the
/// pause still in place, to try again: a pause lifted with three stale tags left would pause the
/// next window at once. While `paused.json` is there no window writes a tag, so the order has no
/// race. Files that are gone already are fine; nothing else in the folder is touched.
pub fn clear_pause(fs: &dyn Fs, paths: &Paths) -> Result<(), AdapterError> {
    for entry in fs.list_dir(&paths.health())? {
        if !entry.is_dir && is_tag(&entry.path) {
            fs.remove_file(&entry.path)?;
        }
    }
    fs.remove_file(&paths.paused_file())?;
    fs.create_dir_all(&paths.health())
}

/// Whether `path` is named like a start tag: `start-` and the digits of a process id.
pub fn is_tag(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.strip_prefix(TAG_PREFIX))
        .is_some_and(|pid| !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::fake::FakeFs;

    fn paths() -> Paths {
        Paths {
            data: PathBuf::from(r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio"),
        }
    }

    /// What init.lua writes when it pauses the theme (`generate::lua`, HEALTH_BODY).
    const FROM_LUA: &[u8] =
        b"{\"version\":1,\"reason\":\"cmdStarts\",\"at\":1791694709000,\"count\":3,\"by\":\"clink\"}\n";

    #[test]
    fn the_folder_and_its_files_are_under_the_data_folder() {
        let p = paths();
        assert_eq!(p.health(), p.data.join("health"));
        assert_eq!(p.paused_file(), p.data.join(r"health\paused.json"));
        assert_eq!((STALE_AFTER_SECS, PAUSE_AFTER), (60, 3));
        assert_eq!(CRASH_WINDOW_MS, 604_800_000);
    }

    #[test]
    fn a_pause_init_lua_wrote_reads_as_one_in_cmd() {
        let (fs, p) = (FakeFs::new(), paths());
        fs.put(p.paused_file(), FROM_LUA);
        assert_eq!(
            read_pause(&fs, &p).unwrap(),
            Some(Paused {
                by: PausedBy::Clink,
                reason: Some(PauseReason::CmdStarts),
                at_ms: Some(1_791_694_709_000),
                count: 3,
                resume: None,
            })
        );
    }

    #[test]
    fn a_pause_the_app_wrote_round_trips_with_its_theme() {
        let (fs, p) = (FakeFs::new(), paths());
        let record = PauseRecord {
            version: PAUSED_VERSION,
            reason: PauseReason::ClinkCrash,
            at: 1_791_694_709_000,
            count: 4,
            by: PausedBy::App,
            resume: Some(Resume {
                theme_id: uuid::Uuid::nil(),
                target: Target::Wt,
            }),
        };
        write_pause(&fs, &p, &record).unwrap();
        let text = String::from_utf8(fs.file(p.paused_file()).unwrap()).unwrap();
        // camelCase, the names of recovery spec §3.1
        for name in [
            "\"version\": 1",
            "\"reason\": \"clinkCrash\"",
            "\"by\": \"app\"",
            "\"themeId\"",
            "\"target\": \"wt\"",
        ] {
            assert!(text.contains(name), "{name} in {text}");
        }
        assert_eq!(read_pause(&fs, &p).unwrap(), Some(Paused::from(record)));
    }

    #[test]
    fn a_pause_file_that_cannot_be_read_as_one_is_still_a_pause_in_cmd() {
        // init.lua only asks whether the file is there (Review Focus 3): a damaged one, a half
        // written one, one from a newer CMD Studio all keep the theme out of cmd, so the app must
        // say "paused" too, and turning the theme back on deletes the file.
        let in_cmd = Paused {
            by: PausedBy::Clink,
            reason: None,
            at_ms: None,
            count: 0,
            resume: None,
        };
        for text in [
            &b""[..],
            b"{ not json",
            b"{\"version\":1,\"reason\":\"cmdStarts\",\"at\":17916",
            b"{}",
            b"{\"version\":2,\"reason\":\"user\",\"at\":1,\"count\":0,\"by\":\"app\"}",
            b"{\"version\":1,\"reason\":\"later\",\"at\":1,\"count\":0,\"by\":\"clink\"}",
        ] {
            let (fs, p) = (FakeFs::new(), paths());
            fs.put(p.paused_file(), text);
            assert_eq!(
                read_pause(&fs, &p).unwrap(),
                Some(in_cmd.clone()),
                "{:?}",
                String::from_utf8_lossy(text)
            );
        }
    }

    #[test]
    fn no_file_is_no_pause_and_a_read_that_fails_is_an_error() {
        let (fs, p) = (FakeFs::new(), paths());
        assert_eq!(read_pause(&fs, &p).unwrap(), None);
        fs.put(p.paused_file(), FROM_LUA);
        fs.fail_reads_of(p.paused_file());
        assert!(read_pause(&fs, &p).is_err());
    }

    #[test]
    fn lifting_a_pause_deletes_the_file_and_every_tag_and_nothing_else() {
        let (fs, p) = (FakeFs::new(), paths());
        fs.put(p.paused_file(), FROM_LUA);
        for name in ["start-1", "start-4242", "start-", "start-x1", "notes.txt"] {
            fs.put(p.health().join(name), b"v1 1791694709\n");
        }
        fs.create_dir_all(&p.health().join("start-7")).unwrap();
        // A folder named like a tag is left alone: removing it as a file would fail, so this
        // pins the `is_dir` guard in `clear_pause`.
        fs.fail_removes_of(p.health().join("start-7"));
        clear_pause(&fs, &p).unwrap();
        let left: Vec<String> = fs
            .list_dir(&p.health())
            .unwrap()
            .iter()
            .map(|e| e.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left, ["notes.txt", "start-", "start-7", "start-x1"]);
        // Nothing to lift is no error, and the folder is made for init.lua.
        let (fs, p) = (FakeFs::new(), paths());
        clear_pause(&fs, &p).unwrap();
        assert!(fs.is_dir(&p.health()));
    }

    #[test]
    fn a_tag_that_cannot_be_deleted_leaves_the_pause_in_place_to_try_again() {
        // The tags go first and the pause last: with a pause lifted and three stale tags left,
        // the next cmd window would pause the theme again at once. While `paused.json` is
        // there no window writes a tag, so the order has no race.
        let (fs, p) = (FakeFs::new(), paths());
        fs.put(p.paused_file(), FROM_LUA);
        for name in ["start-1", "start-2", "start-3"] {
            fs.put(p.health().join(name), b"v1 1791694709\n");
        }
        fs.fail_removes_of(p.health().join("start-2"));
        assert!(clear_pause(&fs, &p).is_err());
        assert!(
            fs.file(p.paused_file()).is_some(),
            "the pause is still there"
        );
        // Once the tag lets go, the same call lifts everything.
        fs.stop_failing(p.health().join("start-2"));
        clear_pause(&fs, &p).unwrap();
        assert!(fs.file(p.paused_file()).is_none());
        assert!(fs.list_dir(&p.health()).unwrap().is_empty());
    }

    #[test]
    fn a_record_is_written_as_this_version_whatever_the_caller_passes() {
        // A record that read back as unreadable would be a pause with no reason, no time and
        // no theme to turn back on.
        for version in [0, 2, 7] {
            let (fs, p) = (FakeFs::new(), paths());
            let record = PauseRecord {
                version,
                reason: PauseReason::User,
                at: 1_791_694_709_000,
                count: 0,
                by: PausedBy::App,
                resume: None,
            };
            write_pause(&fs, &p, &record).unwrap();
            let text = String::from_utf8(fs.file(p.paused_file()).unwrap()).unwrap();
            assert!(text.contains("\"version\": 1"), "{text}");
            assert_eq!(
                read_pause(&fs, &p).unwrap(),
                Some(Paused::from(PauseRecord {
                    version: PAUSED_VERSION,
                    ..record
                })),
                "version {version}"
            );
        }
    }

    #[test]
    fn a_tag_is_start_and_the_digits_of_a_process_id() {
        for (name, tag) in [
            ("start-4242", true),
            ("start-1", true),
            ("start-", false),
            ("start-12a", false),
            ("Start-1", false),
            ("paused.json", false),
        ] {
            assert_eq!(is_tag(Path::new(name)), tag, "{name}");
        }
    }

    #[test]
    fn the_reasons_are_named_as_paused_json_names_them() {
        for reason in [
            PauseReason::CmdStarts,
            PauseReason::ClinkCrash,
            PauseReason::Recovery,
            PauseReason::User,
        ] {
            assert_eq!(
                serde_json::to_value(reason).unwrap(),
                serde_json::Value::from(reason.as_str())
            );
        }
    }
}
