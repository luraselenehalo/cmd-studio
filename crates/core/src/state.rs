//! `state.json` (spec §5.3): the applied theme, hashes of the generated
//! files, the runtime versions, `<clink-launch>`, and the baseline: each
//! setting's value before CMD Studio first changed it, plus what CMD Studio
//! last wrote, so "revert all" and uninstall can undo key by key (spec §6.3).
use crate::adapters::{Fs, RegValue};
use crate::model::Target;
use crate::plan::{ClinkLaunch, Paths};
use crate::wt_edit::{WtCreated, WtEdited, WtKey};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const STATE_VERSION: u32 = 1;

impl Paths {
    pub fn state_file(&self) -> PathBuf {
        self.data.join("state.json")
    }
    pub fn snapshots(&self) -> PathBuf {
        self.data.join("snapshots")
    }
    pub fn themes(&self) -> PathBuf {
        self.data.join("themes")
    }
    pub fn runtime_root(&self) -> PathBuf {
        self.data.join("runtime")
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub version: u32,
    pub applied: Option<Applied>,
    #[serde(default)]
    pub baseline: Baseline,
    /// Font files CMD Studio installed, unregistered and could not delete because they were in
    /// use. A later run deletes them. Not written while empty, so a state without any stays
    /// byte for byte what it was before this field existed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_deletes: Vec<PathBuf>,
}

impl Default for State {
    fn default() -> Self {
        State {
            version: STATE_VERSION,
            applied: None,
            baseline: Baseline::default(),
            pending_deletes: Vec::new(),
        }
    }
}

impl State {
    /// Remembers a file to delete later; a path already remembered stays once.
    pub fn note_pending_delete(&mut self, path: PathBuf) {
        if !self.pending_deletes.contains(&path) {
            self.pending_deletes.push(path);
        }
    }
}

/// The theme that is on the machine now.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Applied {
    pub theme_id: uuid::Uuid,
    pub theme_name: String,
    pub target: Target,
    pub set_default: bool,
    /// `th` or `en`: the language the banner was generated in.
    pub lang: String,
    pub launch: Launch,
    pub omp_version: String,
    pub clink_version: String,
    /// Path of each generated file → its SHA-256 (doctor D9).
    pub generated: BTreeMap<PathBuf, String>,
    /// The font face written to Windows Terminal, if any (doctor D8).
    pub wt_font: Option<String>,
    /// How many bytes `clink-profile\clink.log` held when the theme was applied. Doctor D3
    /// reads only what Clink logged after them: the cmd windows started since.
    pub clink_log_len: u64,
    pub snapshot_id: String,
    pub applied_at_ms: u64,
}

/// `plan::ClinkLaunch` as stored.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Launch {
    Direct { exe: PathBuf },
    ShortPath { exe: PathBuf },
    ProgramData { dir: PathBuf },
}

impl From<&ClinkLaunch> for Launch {
    fn from(l: &ClinkLaunch) -> Self {
        match l {
            ClinkLaunch::Direct(exe) => Launch::Direct { exe: exe.clone() },
            ClinkLaunch::ShortPath(exe) => Launch::ShortPath { exe: exe.clone() },
            ClinkLaunch::ProgramData { dir } => Launch::ProgramData { dir: dir.clone() },
        }
    }
}

impl Launch {
    pub fn to_clink(&self) -> ClinkLaunch {
        match self {
            Launch::Direct { exe } => ClinkLaunch::Direct(exe.clone()),
            Launch::ShortPath { exe } => ClinkLaunch::ShortPath(exe.clone()),
            Launch::ProgramData { dir } => ClinkLaunch::ProgramData { dir: dir.clone() },
        }
    }
}

/// A registry value as stored, kept exactly (type + raw bytes as hex).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegValueRecord {
    pub kind: u32,
    pub data_hex: String,
}

impl RegValueRecord {
    pub fn from_reg(v: &RegValue) -> Self {
        RegValueRecord {
            kind: v.kind,
            data_hex: v.bytes.iter().map(|b| format!("{b:02x}")).collect(),
        }
    }

    /// `None` when `data_hex` is not hex: an odd length, or anything but the digits `0-9a-fA-F`
    /// (`u8::from_str_radix` alone would also take a leading `+`).
    pub fn to_reg(&self) -> Option<RegValue> {
        let (pairs, rest) = self.data_hex.as_bytes().as_chunks::<2>();
        if !rest.is_empty() {
            return None;
        }
        let bytes = pairs
            .iter()
            .map(|&[hi, lo]| Some(hex_digit(hi)? << 4 | hex_digit(lo)?))
            .collect::<Option<Vec<u8>>>()?;
        Some(RegValue {
            kind: self.kind,
            bytes,
        })
    }
}

fn hex_digit(b: u8) -> Option<u8> {
    char::from(b)
        .to_digit(16)
        .and_then(|d| u8::try_from(d).ok())
}

/// The user's AutoRun at one moment: whether the key existed and the value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoRunRecord {
    pub key_existed: bool,
    pub value: Option<RegValueRecord>,
}

/// What CMD Studio last wrote to AutoRun; `value: None` means it deleted it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoRunWritten {
    pub value: Option<String>,
}

/// A key's value before CMD Studio first touched it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "was", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Was {
    Absent,
    Present { value: Value },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WtOriginal {
    pub key: WtKey,
    pub original: Was,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WtWritten {
    pub key: WtKey,
    pub value: Value,
}

/// One settings.json: first-touch originals, last-written values, and the
/// containers CMD Studio added.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WtFileBaseline {
    pub settings: PathBuf,
    pub originals: Vec<WtOriginal>,
    pub written: Vec<WtWritten>,
    pub created: Vec<WtCreated>,
}

impl WtFileBaseline {
    pub fn originals_map(&self) -> BTreeMap<WtKey, Option<Value>> {
        self.originals
            .iter()
            .map(|o| {
                let v = match &o.original {
                    Was::Absent => None,
                    Was::Present { value } => Some(value.clone()),
                };
                (o.key.clone(), v)
            })
            .collect()
    }

    pub fn written_map(&self) -> BTreeMap<WtKey, Value> {
        self.written
            .iter()
            .map(|w| (w.key.clone(), w.value.clone()))
            .collect()
    }
}

/// A font CMD Studio installed (spec §5.4: uninstall only what we installed).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FontRecord {
    pub family: String,
    pub file: PathBuf,
    pub value_name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Baseline {
    /// AutoRun before CMD Studio first changed it; `None`: never changed.
    pub autorun: Option<AutoRunRecord>,
    pub autorun_written: Option<AutoRunWritten>,
    pub wt: Vec<WtFileBaseline>,
    pub fonts: Vec<FontRecord>,
    /// The user's ProgramData folder, when CMD Studio created it (spec §6.3 step 4).
    pub program_data_dir: Option<PathBuf>,
}

impl Baseline {
    /// Keeps the first-ever original; always records the latest write.
    pub fn note_autorun(&mut self, before: &AutoRunRecord, written: Option<&str>) {
        if self.autorun.is_none() {
            self.autorun = Some(before.clone());
        }
        self.autorun_written = Some(AutoRunWritten {
            value: written.map(str::to_string),
        });
    }

    /// Keeps each key's first-ever original and its latest written value, and the containers
    /// CMD Studio added, once each, across any number of applies.
    pub fn note_wt(&mut self, settings: &Path, edited: &WtEdited) {
        let index = match self.wt.iter().position(|f| f.settings == settings) {
            Some(i) => i,
            None => {
                self.wt.push(WtFileBaseline {
                    settings: settings.to_path_buf(),
                    originals: Vec::new(),
                    written: Vec::new(),
                    created: Vec::new(),
                });
                self.wt.len().saturating_sub(1)
            }
        };
        let Some(file) = self.wt.get_mut(index) else {
            return;
        };
        for change in &edited.changes {
            if !file.originals.iter().any(|o| o.key == change.key) {
                let original = match &change.original {
                    None => Was::Absent,
                    Some(value) => Was::Present {
                        value: value.clone(),
                    },
                };
                file.originals.push(WtOriginal {
                    key: change.key.clone(),
                    original,
                });
            }
            match file.written.iter_mut().find(|w| w.key == change.key) {
                Some(w) => w.value = change.written.clone(),
                None => file.written.push(WtWritten {
                    key: change.key.clone(),
                    value: change.written.clone(),
                }),
            }
        }
        for c in &edited.created {
            if !file.created.contains(c) {
                file.created.push(*c);
            }
        }
    }

    pub fn note_font(&mut self, font: FontRecord) {
        if !self.fonts.contains(&font) {
            self.fonts.push(font);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StateError {
    /// The file is there but could not be read (locked, no access, a folder in its place).
    #[error("state.json could not be read: {0}")]
    Read(String),
    /// The file was read but is damaged: not JSON, not a state file, or a version no release
    /// wrote. The message says which.
    #[error("{0}")]
    Invalid(String),
    /// The file is a valid state of a newer CMD Studio (`version` above `STATE_VERSION`). It
    /// is not damaged and holds a baseline this version could lose on a save, so a caller must
    /// leave it where it is, not set it aside as it may a damaged one.
    #[error(
        "state.json was written by a newer CMD Studio (version {found}); \
         this version understands version {STATE_VERSION}"
    )]
    Newer { found: u64 },
    /// A save failed.
    #[error("{0}")]
    Io(String),
}

impl StateError {
    /// Stable code shown to users (spec §5.9). A read never carries a write code.
    pub fn code(&self) -> &'static str {
        match self {
            StateError::Read(_) | StateError::Invalid(_) | StateError::Newer { .. } => {
                "E_STATE_INVALID"
            }
            StateError::Io(_) => "E_DATA_WRITE",
        }
    }
}

impl State {
    /// The saved state; a fresh one when there is no file yet. A file that cannot be read or is
    /// damaged is an error, never replaced by a fresh state: that would forget the baseline.
    /// Reading only: this never writes the file.
    ///
    /// The `version` field decides first, before the rest is read:
    /// - `STATE_VERSION` (1) loads;
    /// - a larger number is a state from a newer CMD Studio: `StateError::Newer`, not
    ///   `Invalid`, so a caller can tell it from damage. It is refused, because saving it again
    ///   would drop the fields this version does not know, and with them the baseline that
    ///   undoing everything needs. Its shape is not looked at, so the error names the version;
    /// - 0 was never written by any version, so it is damage;
    /// - a missing `version` (serde: "missing field `version`") or one that is not a whole
    ///   number is damage. `applied` and `baseline` may be missing: they read as none and as the
    ///   default.
    pub fn load(fs: &dyn Fs, path: &Path) -> Result<State, StateError> {
        let Some(bytes) = fs.read(path).map_err(|e| StateError::Read(e.to_string()))? else {
            return Ok(State::default());
        };
        let damaged = |e: serde_json::Error| {
            StateError::Invalid(format!("state.json is not a valid state file: {e}"))
        };
        let value: Value = serde_json::from_slice(&bytes).map_err(damaged)?;
        match value.get("version").and_then(Value::as_u64) {
            Some(0) => {
                return Err(StateError::Invalid(format!(
                    "state.json has version 0, which no CMD Studio wrote; \
                     this version understands version {STATE_VERSION}"
                )))
            }
            Some(found) if found > u64::from(STATE_VERSION) => {
                return Err(StateError::Newer { found })
            }
            _ => {}
        }
        serde_json::from_value(value).map_err(damaged)
    }

    /// Pretty JSON with a final newline. A failure here is a failure to save (the in-memory
    /// state is not text), not a damaged file.
    pub fn to_bytes(&self) -> Result<Vec<u8>, StateError> {
        let mut bytes = serde_json::to_vec_pretty(self)
            .map_err(|e| StateError::Io(format!("state.json could not be produced: {e}")))?;
        bytes.push(b'\n');
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::fake::FakeFs;
    use crate::adapters::{REG_EXPAND_SZ, REG_SZ};
    use crate::wt_edit::WtChange;
    use serde_json::json;

    /// `state.json` as version 1 wrote it before `pendingDeletes` existed. The 1 is deliberate:
    /// it is a stored file, not "the current version".
    const STATE_WITHOUT_PENDING: &str = "{\n  \"version\": 1,\n  \"applied\": null,\n  \"baseline\": {\n    \"autorun\": null,\n    \"autorunWritten\": null,\n    \"wt\": [],\n    \"fonts\": [],\n    \"programDataDir\": null\n  }\n}\n";

    fn edited(changes: Vec<WtChange>, created: Vec<WtCreated>) -> WtEdited {
        WtEdited {
            text: String::new(),
            changes,
            created,
        }
    }

    fn change(key: &WtKey, original: Option<Value>, written: Value) -> WtChange {
        WtChange {
            key: key.clone(),
            original,
            written,
        }
    }

    fn applied(launch: Launch) -> Applied {
        Applied {
            theme_id: uuid::Uuid::from_u128(0x1234),
            theme_name: "Aurora".into(),
            target: Target::All,
            set_default: true,
            lang: "th".into(),
            launch,
            omp_version: "31.5.0".into(),
            clink_version: "1.9.34".into(),
            generated: BTreeMap::from([(
                PathBuf::from(r"C:\d\generated\theme.omp.json"),
                "ab".repeat(32),
            )]),
            wt_font: Some("CaskaydiaCove NF".into()),
            clink_log_len: 4096,
            snapshot_id: "20261011-120000-ab12".into(),
            applied_at_ms: 1_791_720_000_000,
        }
    }

    #[test]
    fn a_missing_file_is_a_fresh_state_and_junk_is_refused() {
        let fs = FakeFs::new();
        let p = Path::new(r"C:\d\state.json");
        assert_eq!(State::load(&fs, p).unwrap(), State::default());
        fs.put(p, b"{ nope");
        assert_eq!(State::load(&fs, p).unwrap_err().code(), "E_STATE_INVALID");
    }

    #[test]
    fn reg_records_round_trip_every_byte() {
        let mut v = RegValue::from_text(REG_EXPAND_SZ, "%X% & ไทย");
        v.bytes.push(0xff);
        let rec = RegValueRecord::from_reg(&v);
        assert_eq!(rec.to_reg(), Some(v));
        assert_eq!(
            RegValueRecord {
                kind: REG_SZ,
                data_hex: "zz".into()
            }
            .to_reg(),
            None
        );
        assert_eq!(
            RegValueRecord {
                kind: REG_SZ,
                data_hex: "abc".into()
            }
            .to_reg(),
            None
        );
    }

    #[test]
    fn the_first_original_wins_and_the_last_write_is_kept() {
        let mut b = Baseline::default();
        let before = AutoRunRecord {
            key_existed: true,
            value: None,
        };
        b.note_autorun(&before, Some("first"));
        b.note_autorun(
            &AutoRunRecord {
                key_existed: true,
                value: Some(RegValueRecord {
                    kind: 1,
                    data_hex: "00".into(),
                }),
            },
            None,
        );
        assert_eq!(b.autorun, Some(before));
        assert_eq!(b.autorun_written, Some(AutoRunWritten { value: None }));

        let key = WtKey::Profile {
            key: "opacity".into(),
        };
        let first = WtEdited {
            text: String::new(),
            changes: vec![WtChange {
                key: key.clone(),
                original: Some(json!(80)),
                written: json!(96),
            }],
            created: vec![WtCreated::Schemes],
        };
        let second = WtEdited {
            text: String::new(),
            changes: vec![WtChange {
                key: key.clone(),
                original: Some(json!(96)),
                written: json!(90),
            }],
            created: vec![],
        };
        let f = Path::new(r"C:\wt\settings.json");
        b.note_wt(f, &first);
        b.note_wt(f, &second);
        let file = &b.wt[0];
        assert_eq!(file.originals_map()[&key], Some(json!(80)));
        assert_eq!(file.written_map()[&key], json!(90));
        assert_eq!(file.created, vec![WtCreated::Schemes]);
    }

    #[test]
    fn json_uses_camel_case_and_tells_absent_from_null() {
        let mut s = State::default();
        let edited = WtEdited {
            text: String::new(),
            changes: vec![
                WtChange {
                    key: WtKey::Profile { key: "icon".into() },
                    original: Some(Value::Null),
                    written: json!("x"),
                },
                WtChange {
                    key: WtKey::ProfileSub {
                        key: "font".into(),
                        sub: "face".into(),
                    },
                    original: None,
                    written: json!("F"),
                },
            ],
            created: vec![WtCreated::CmdProfile],
        };
        s.baseline
            .note_wt(Path::new(r"C:\wt\settings.json"), &edited);
        s.baseline.note_font(FontRecord {
            family: "CaskaydiaCove NF".into(),
            file: PathBuf::from(r"C:\f.ttf"),
            value_name: "CaskaydiaCove NF (TrueType)".into(),
        });
        let text = String::from_utf8(s.to_bytes().unwrap()).unwrap();
        for word in [
            "\"originals\"",
            "\"valueName\"",
            "\"programDataDir\"",
            "\"was\": \"absent\"",
            "\"was\": \"present\"",
            "\"at\": \"profileSub\"",
            "\"cmdProfile\"",
        ] {
            assert!(text.contains(word), "{word} missing in {text}");
        }
        let back: State = serde_json::from_str(&text).unwrap();
        assert_eq!(back, s);
        assert_eq!(
            back.baseline.wt[0].originals_map()[&WtKey::Profile { key: "icon".into() }],
            Some(Value::Null)
        );
    }

    // --- A read that fails is not a write error. ---

    #[test]
    fn a_failed_read_is_reported_as_an_unreadable_state_not_as_a_write_error() {
        let fs = FakeFs::new();
        let p = Path::new(r"C:\d\state.json");
        fs.put(p, State::default().to_bytes().unwrap());
        fs.fail_reads_of(p);
        let err = State::load(&fs, p).unwrap_err();
        assert!(matches!(err, StateError::Read(_)), "{err:?}");
        assert_eq!(err.code(), "E_STATE_INVALID");
        assert!(err.to_string().contains("(injected)"), "{err}");
    }

    #[test]
    fn write_errors_keep_their_own_code() {
        assert_eq!(StateError::Io("disk full".into()).code(), "E_DATA_WRITE");
        assert_eq!(StateError::Invalid("x".into()).code(), "E_STATE_INVALID");
        assert_eq!(StateError::Read("x".into()).code(), "E_STATE_INVALID");
        assert_eq!(StateError::Newer { found: 2 }.code(), "E_STATE_INVALID");
    }

    // --- Font files that could not be deleted are remembered. ---

    #[test]
    fn an_old_state_file_without_pending_deletes_loads_with_an_empty_list() {
        let fs = FakeFs::new();
        let p = Path::new(r"C:\d\state.json");
        fs.put(p, STATE_WITHOUT_PENDING);
        let s = State::load(&fs, p).unwrap();
        assert!(s.pending_deletes.is_empty());
        assert_eq!(s, State::default());
    }

    #[test]
    fn pending_deletes_round_trip() {
        let fs = FakeFs::new();
        let p = Path::new(r"C:\d\state.json");
        let mut s = State::default();
        s.note_pending_delete(PathBuf::from(r"C:\f\b.ttf"));
        s.note_pending_delete(PathBuf::from(r"C:\f\a.ttf"));
        let text = String::from_utf8(s.to_bytes().unwrap()).unwrap();
        assert!(text.contains("\"pendingDeletes\""), "{text}");
        fs.put(p, text);
        let back = State::load(&fs, p).unwrap();
        assert_eq!(back, s);
        // The order CMD Studio noted them in stays.
        assert_eq!(
            back.pending_deletes,
            vec![PathBuf::from(r"C:\f\b.ttf"), PathBuf::from(r"C:\f\a.ttf")]
        );
    }

    #[test]
    fn a_path_noted_twice_is_kept_once() {
        let mut s = State::default();
        s.note_pending_delete(PathBuf::from(r"C:\f\a.ttf"));
        s.note_pending_delete(PathBuf::from(r"C:\f\b.ttf"));
        s.note_pending_delete(PathBuf::from(r"C:\f\a.ttf"));
        assert_eq!(
            s.pending_deletes,
            vec![PathBuf::from(r"C:\f\a.ttf"), PathBuf::from(r"C:\f\b.ttf")]
        );
    }

    #[test]
    fn an_empty_pending_list_is_not_written_so_the_file_stays_as_it_was() {
        let text = String::from_utf8(State::default().to_bytes().unwrap()).unwrap();
        assert_eq!(text, STATE_WITHOUT_PENDING);
        let mut s = State::default();
        s.note_pending_delete(PathBuf::from(r"C:\f\a.ttf"));
        s.pending_deletes.clear();
        assert_eq!(
            String::from_utf8(s.to_bytes().unwrap()).unwrap(),
            STATE_WITHOUT_PENDING
        );
    }

    // --- The file's version. ---

    #[test]
    fn a_state_from_a_newer_cmd_studio_is_refused_and_left_alone() {
        let fs = FakeFs::new();
        let p = Path::new(r"C:\d\state.json");
        let newer = u64::from(STATE_VERSION) + 1;
        let files: [(String, u64); 4] = [
            // The shape of today's state plus a field a newer version added.
            (
                format!(
                    r#"{{"version":{newer},"applied":null,"baseline":{{"autorun":null,"autorunWritten":null,"wt":[],"fonts":[],"programDataDir":null}},"extra":[1]}}"#
                ),
                newer,
            ),
            // A shape this version cannot read at all: the version is what gets reported.
            (
                format!(r#"{{"version":{newer},"applied":"another shape","baseline":[]}}"#),
                newer,
            ),
            (format!(r#"{{"version":{}}}"#, newer + 1), newer + 1),
            // Larger than `u32`.
            (r#"{"version":99999999999}"#.to_string(), 99_999_999_999),
        ];
        for (text, found) in files {
            let bytes = text.as_bytes();
            fs.put(p, bytes);
            let err = State::load(&fs, p).unwrap_err();
            // Neither `Invalid` nor `Read`: a caller that renames a damaged file aside must be
            // able to tell a newer state, which it has to leave alone.
            assert_eq!(err, StateError::Newer { found }, "{text}");
            assert_eq!(err.code(), "E_STATE_INVALID");
            assert_eq!(
                fs.file(p).as_deref(),
                Some(bytes),
                "load must not touch the file"
            );
        }
    }

    #[test]
    fn the_newer_version_message_names_both_versions() {
        // The numbers are written out on purpose: raising STATE_VERSION must show up here, so
        // the new version's handling of old files is decided.
        assert_eq!(STATE_VERSION, 1);
        assert_eq!(
            StateError::Newer { found: 2 }.to_string(),
            "state.json was written by a newer CMD Studio (version 2); this version understands version 1"
        );
    }

    #[test]
    fn only_the_current_version_is_understood_a_missing_or_odd_version_is_damage() {
        let fs = FakeFs::new();
        let p = Path::new(r"C:\d\state.json");
        fs.put(p, format!(r#"{{"version":{STATE_VERSION}}}"#));
        assert_eq!(State::load(&fs, p).unwrap().version, STATE_VERSION);
        // `applied` and `baseline` may be missing (serde reads a missing Option as None and the
        // baseline has a default); `version` may not: serde's "missing field `version`".
        let bad: [&[u8]; 8] = [
            br#"{"version":0}"#,
            br#"{"applied":null}"#,
            br#"{"version":null}"#,
            br#"{"version":"1"}"#,
            br#"{"version":-1}"#,
            br#"{"version":1.5}"#,
            br#"{"version":true}"#,
            br#"{"version":[1]}"#,
        ];
        for bytes in bad {
            fs.put(p, bytes);
            let err = State::load(&fs, p).unwrap_err();
            assert!(matches!(err, StateError::Invalid(_)), "{bytes:?}: {err:?}");
            assert_eq!(err.code(), "E_STATE_INVALID");
            assert!(!err.to_string().contains("newer"), "{bytes:?}: {err}");
            assert_eq!(
                fs.file(p).as_deref(),
                Some(bytes),
                "load must not touch the file"
            );
        }
        fs.put(p, br#"{"applied":null}"#);
        assert!(State::load(&fs, p)
            .unwrap_err()
            .to_string()
            .contains("version"));
        fs.put(p, br#"{"version":0}"#);
        assert!(State::load(&fs, p)
            .unwrap_err()
            .to_string()
            .contains("version 0"));
    }

    #[test]
    fn a_state_that_is_saved_carries_the_current_version() {
        assert_eq!(State::default().version, STATE_VERSION);
        let text = String::from_utf8(State::default().to_bytes().unwrap()).unwrap();
        assert!(
            text.contains(&format!("\"version\": {STATE_VERSION},")),
            "{text}"
        );
    }

    #[test]
    fn a_damaged_file_is_reported_and_left_alone() {
        let fs = FakeFs::new();
        let p = Path::new(r"C:\d\state.json");
        // Empty (a crash during a write), not JSON, not an object, and an object that is not a state.
        for junk in [&b""[..], b"{ nope", b"[]", b"{}", b"{\"version\":\"one\"}"] {
            fs.put(p, junk);
            let err = State::load(&fs, p).unwrap_err();
            assert!(matches!(err, StateError::Invalid(_)), "{junk:?}: {err:?}");
            assert_eq!(err.code(), "E_STATE_INVALID", "{junk:?}");
            assert_eq!(
                fs.file(p).as_deref(),
                Some(junk),
                "load must not touch the file"
            );
        }
    }

    #[test]
    fn hex_is_decoded_only_from_hex_digits() {
        let rec = |hex: &str| RegValueRecord {
            kind: REG_SZ,
            data_hex: hex.into(),
        };
        assert_eq!(
            rec("").to_reg(),
            Some(RegValue {
                kind: REG_SZ,
                bytes: vec![]
            })
        );
        assert_eq!(
            rec("00fF7a").to_reg(),
            Some(RegValue {
                kind: REG_SZ,
                bytes: vec![0x00, 0xff, 0x7a]
            })
        );
        // `u8::from_str_radix` would take a sign; a restore must not guess at damaged bytes.
        for bad in [
            "+1",
            "1+",
            "-1",
            "0x",
            " 1",
            "1 ",
            "0g",
            "éé",
            "é0",
            "\u{0661}\u{0661}",
        ] {
            assert_eq!(rec(bad).to_reg(), None, "{bad:?}");
        }
    }

    #[test]
    fn autorun_type_and_raw_bytes_survive_the_json_round_trip() {
        // A REG_EXPAND_SZ value with an unpaired byte at the end, an empty REG_SZ and a DWORD.
        let mut odd = RegValue::from_text(REG_EXPAND_SZ, r"%USERPROFILE%\a & ไทย");
        odd.bytes.extend_from_slice(&[0x41, 0x00, 0xff]);
        let values = [
            odd,
            RegValue {
                kind: REG_SZ,
                bytes: vec![],
            },
            RegValue {
                kind: 4,
                bytes: vec![1, 0, 0, 0],
            },
        ];
        for v in values {
            let mut s = State::default();
            s.baseline.note_autorun(
                &AutoRunRecord {
                    key_existed: true,
                    value: Some(RegValueRecord::from_reg(&v)),
                },
                Some(r#"(if exist "C:\c.exe" x) & old"#),
            );
            let back: State = serde_json::from_slice(&s.to_bytes().unwrap()).unwrap();
            let record = back.baseline.autorun.unwrap();
            assert!(record.key_existed);
            assert_eq!(record.value.unwrap().to_reg(), Some(v));
            assert_eq!(
                back.baseline.autorun_written,
                Some(AutoRunWritten {
                    value: Some(r#"(if exist "C:\c.exe" x) & old"#.into())
                })
            );
        }
    }

    #[test]
    fn the_autorun_original_stays_even_when_it_was_a_missing_key() {
        let mut b = Baseline::default();
        let none = AutoRunRecord {
            key_existed: false,
            value: None,
        };
        b.note_autorun(&none, Some("ours"));
        b.note_autorun(
            &AutoRunRecord {
                key_existed: true,
                value: None,
            },
            Some("ours again"),
        );
        b.note_autorun(
            &AutoRunRecord {
                key_existed: true,
                value: None,
            },
            Some("ours 3"),
        );
        assert_eq!(b.autorun, Some(none));
        assert_eq!(
            b.autorun_written,
            Some(AutoRunWritten {
                value: Some("ours 3".into())
            })
        );
    }

    #[test]
    fn several_applies_keep_each_keys_first_original_and_last_write() {
        let font = WtKey::Profile { key: "font".into() };
        let face = WtKey::ProfileSub {
            key: "font".into(),
            sub: "face".into(),
        };
        let icon = WtKey::Profile { key: "icon".into() };
        let scheme = WtKey::Scheme;
        let mut b = Baseline::default();
        let f = Path::new(r"C:\wt\settings.json");
        // Apply 1: the profile had no `font`, an `icon` of null and no scheme.
        b.note_wt(
            f,
            &edited(
                vec![
                    change(&font, None, json!({"face": "A"})),
                    change(&face, None, json!("A")),
                    change(&icon, Some(Value::Null), json!("i1")),
                    change(&scheme, None, json!({"name": "CMD Studio", "v": 1})),
                ],
                vec![WtCreated::CmdProfile],
            ),
        );
        // Apply 2 sees what apply 1 wrote as the "original".
        b.note_wt(
            f,
            &edited(
                vec![
                    change(&font, Some(json!({"face": "A"})), json!({"face": "B"})),
                    change(&face, Some(json!("A")), json!("B")),
                    change(&icon, Some(json!("i1")), json!("i2")),
                    change(
                        &scheme,
                        Some(json!({"name": "CMD Studio", "v": 1})),
                        json!({"name": "CMD Studio", "v": 2}),
                    ),
                ],
                vec![WtCreated::CmdProfile, WtCreated::Schemes],
            ),
        );
        // Apply 3 touches only the face, and the file has no new key.
        b.note_wt(
            f,
            &edited(vec![change(&face, Some(json!("B")), json!("C"))], vec![]),
        );

        assert_eq!(b.wt.len(), 1);
        let file = &b.wt[0];
        let originals = file.originals_map();
        assert_eq!(originals.len(), 4);
        assert_eq!(originals[&font], None, "absent stays absent");
        assert_eq!(originals[&face], None);
        assert_eq!(originals[&icon], Some(Value::Null), "null stays null");
        assert_eq!(originals[&scheme], None);
        let written = file.written_map();
        assert_eq!(written.len(), 4);
        assert_eq!(written[&font], json!({"face": "B"}));
        assert_eq!(written[&face], json!("C"));
        assert_eq!(written[&icon], json!("i2"));
        assert_eq!(written[&scheme], json!({"name": "CMD Studio", "v": 2}));
        // Each key is listed once, first seen first; containers are listed once.
        let order: Vec<&WtKey> = file.originals.iter().map(|o| &o.key).collect();
        assert_eq!(order, vec![&font, &face, &icon, &scheme]);
        assert_eq!(
            file.created,
            vec![WtCreated::CmdProfile, WtCreated::Schemes]
        );
    }

    #[test]
    fn a_later_original_that_differs_does_not_replace_the_first_one() {
        // The user edited the key between two applies; the baseline still holds the very first value.
        let key = WtKey::DefaultProfile;
        let mut b = Baseline::default();
        let f = Path::new(r"C:\wt\settings.json");
        b.note_wt(
            f,
            &edited(
                vec![change(&key, Some(json!("{old}")), json!("{cmd}"))],
                vec![],
            ),
        );
        b.note_wt(
            f,
            &edited(
                vec![change(&key, Some(json!("{users-own}")), json!("{cmd}"))],
                vec![],
            ),
        );
        assert_eq!(b.wt[0].originals_map()[&key], Some(json!("{old}")));
        assert_eq!(b.wt[0].written_map()[&key], json!("{cmd}"));
    }

    #[test]
    fn each_settings_file_has_its_own_baseline() {
        let key = WtKey::Scheme;
        let store = Path::new(r"C:\wt\store.json");
        let preview = Path::new(r"C:\wt\preview.json");
        let mut b = Baseline::default();
        b.note_wt(
            store,
            &edited(vec![change(&key, None, json!(1))], vec![WtCreated::Schemes]),
        );
        b.note_wt(
            preview,
            &edited(vec![change(&key, Some(json!(7)), json!(2))], vec![]),
        );
        b.note_wt(
            store,
            &edited(vec![change(&key, Some(json!(1)), json!(3))], vec![]),
        );
        assert_eq!(b.wt.len(), 2);
        assert_eq!(b.wt[0].settings, store);
        assert_eq!(b.wt[1].settings, preview);
        assert_eq!(b.wt[0].originals_map()[&key], None);
        assert_eq!(b.wt[0].written_map()[&key], json!(3));
        assert_eq!(b.wt[1].originals_map()[&key], Some(json!(7)));
        assert_eq!(b.wt[0].created, vec![WtCreated::Schemes]);
        assert!(b.wt[1].created.is_empty());
    }

    #[test]
    fn an_edit_with_no_changes_still_records_the_file_and_its_containers() {
        let mut b = Baseline::default();
        let f = Path::new(r"C:\wt\settings.json");
        b.note_wt(
            f,
            &edited(vec![], vec![WtCreated::Profiles, WtCreated::ProfileList]),
        );
        assert_eq!(b.wt.len(), 1);
        assert!(b.wt[0].originals.is_empty() && b.wt[0].written.is_empty());
        assert_eq!(
            b.wt[0].created,
            vec![WtCreated::Profiles, WtCreated::ProfileList]
        );
    }

    #[test]
    fn a_font_is_noted_once() {
        let font = FontRecord {
            family: "CaskaydiaCove NF".into(),
            file: PathBuf::from(r"C:\f\a.ttf"),
            value_name: "CaskaydiaCove NF (TrueType)".into(),
        };
        let mut b = Baseline::default();
        b.note_font(font.clone());
        b.note_font(font.clone());
        let other = FontRecord {
            file: PathBuf::from(r"C:\f\b.ttf"),
            ..font.clone()
        };
        b.note_font(other.clone());
        assert_eq!(b.fonts, vec![font, other]);
    }

    #[test]
    fn launch_converts_to_and_from_clink_launch() {
        let launches = [
            ClinkLaunch::Direct(PathBuf::from(r"C:\d\runtime\clink-1.9.34\clink_x64.exe")),
            ClinkLaunch::ShortPath(PathBuf::from(
                r"C:\USERS\X~1\runtime\clink-1.9.34\clink_x64.exe",
            )),
            ClinkLaunch::ProgramData {
                dir: PathBuf::from(r"C:\ProgramData\ai.resincore.cmdstudio-S-1-5-21-1"),
            },
        ];
        for l in &launches {
            assert_eq!(&Launch::from(l).to_clink(), l);
        }
    }

    #[test]
    fn an_applied_theme_is_stored_with_stable_names_and_loads_back() {
        let launches = [
            (
                Launch::Direct {
                    exe: PathBuf::from(r"C:\a.exe"),
                },
                "direct",
                "\"exe\"",
            ),
            (
                Launch::ShortPath {
                    exe: PathBuf::from(r"C:\b.exe"),
                },
                "shortPath",
                "\"exe\"",
            ),
            (
                Launch::ProgramData {
                    dir: PathBuf::from(r"C:\p"),
                },
                "programData",
                "\"dir\"",
            ),
        ];
        for (launch, kind, field) in launches {
            let s = State {
                applied: Some(applied(launch)),
                ..State::default()
            };
            let text = String::from_utf8(s.to_bytes().unwrap()).unwrap();
            for word in [
                "\"themeId\": \"00000000-0000-0000-0000-000000001234\"",
                "\"themeName\"",
                "\"target\": \"all\"",
                "\"setDefault\": true",
                "\"lang\": \"th\"",
                "\"ompVersion\"",
                "\"clinkVersion\"",
                "\"generated\"",
                "\"wtFont\": \"CaskaydiaCove NF\"",
                "\"clinkLogLen\": 4096",
                "\"snapshotId\"",
                "\"appliedAtMs\": 1791720000000",
                &format!("\"kind\": \"{kind}\""),
                field,
            ] {
                assert!(text.contains(word), "{word} missing in {text}");
            }
            assert_eq!(serde_json::from_str::<State>(&text).unwrap(), s);
        }
    }

    /// Other files store these types as they are (the journal keeps an `AutoRunRecord`), so a
    /// renamed field would make files written by an earlier version unreadable. This only pins
    /// what the code does today: it passed as soon as it was written.
    #[test]
    fn the_names_that_other_files_store_are_pinned() {
        fn pin<T>(value: &T, expected: Value)
        where
            T: Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
        {
            assert_eq!(serde_json::to_value(value).unwrap(), expected);
            // And the stored text reads back as the same value.
            let back: T = serde_json::from_str(&expected.to_string()).unwrap();
            assert_eq!(&back, value);
        }

        pin(
            &AutoRunRecord {
                key_existed: true,
                value: Some(RegValueRecord {
                    kind: REG_EXPAND_SZ,
                    data_hex: "2500".into(),
                }),
            },
            json!({"keyExisted": true, "value": {"kind": 2, "dataHex": "2500"}}),
        );
        pin(
            &AutoRunRecord {
                key_existed: false,
                value: None,
            },
            json!({"keyExisted": false, "value": null}),
        );
        pin(
            &AutoRunWritten {
                value: Some("x".into()),
            },
            json!({"value": "x"}),
        );
        pin(&AutoRunWritten { value: None }, json!({"value": null}));
        pin(
            &FontRecord {
                family: "F".into(),
                file: PathBuf::from("f.ttf"),
                value_name: "F (TrueType)".into(),
            },
            json!({"family": "F", "file": "f.ttf", "valueName": "F (TrueType)"}),
        );

        // One settings.json with every kind of key, an absent original, a null one, and every
        // container.
        let mut b = Baseline::default();
        b.note_wt(
            Path::new(r"C:\wt\settings.json"),
            &edited(
                vec![
                    change(&WtKey::Scheme, None, json!({"name": "CMD Studio"})),
                    change(&WtKey::DefaultProfile, Some(json!("{x}")), json!("{y}")),
                    change(
                        &WtKey::Profile {
                            key: "opacity".into(),
                        },
                        Some(json!(80)),
                        json!(96),
                    ),
                    change(
                        &WtKey::Profile { key: "icon".into() },
                        Some(Value::Null),
                        json!("i"),
                    ),
                    change(
                        &WtKey::ProfileSub {
                            key: "font".into(),
                            sub: "face".into(),
                        },
                        None,
                        json!("F"),
                    ),
                ],
                vec![
                    WtCreated::CmdProfile,
                    WtCreated::ProfileList,
                    WtCreated::Profiles,
                    WtCreated::Schemes,
                ],
            ),
        );
        let scheme = json!({"at": "scheme"});
        let default_profile = json!({"at": "defaultProfile"});
        let opacity = json!({"at": "profile", "key": "opacity"});
        let icon = json!({"at": "profile", "key": "icon"});
        let face = json!({"at": "profileSub", "key": "font", "sub": "face"});
        pin(
            &b.wt[0],
            json!({
                "settings": r"C:\wt\settings.json",
                "originals": [
                    {"key": scheme, "original": {"was": "absent"}},
                    {"key": default_profile, "original": {"was": "present", "value": "{x}"}},
                    {"key": opacity, "original": {"was": "present", "value": 80}},
                    {"key": icon, "original": {"was": "present", "value": null}},
                    {"key": face, "original": {"was": "absent"}},
                ],
                "written": [
                    {"key": scheme, "value": {"name": "CMD Studio"}},
                    {"key": default_profile, "value": "{y}"},
                    {"key": opacity, "value": 96},
                    {"key": icon, "value": "i"},
                    {"key": face, "value": "F"},
                ],
                "created": ["cmdProfile", "profileList", "profiles", "schemes"],
            }),
        );
        pin(
            &Baseline::default(),
            json!({
                "autorun": null,
                "autorunWritten": null,
                "wt": [],
                "fonts": [],
                "programDataDir": null,
            }),
        );
    }

    #[test]
    fn paths_point_into_the_data_folder() {
        let p = Paths {
            data: PathBuf::from(r"C:\d"),
        };
        assert_eq!(p.state_file(), PathBuf::from(r"C:\d\state.json"));
        assert_eq!(p.snapshots(), PathBuf::from(r"C:\d\snapshots"));
        assert_eq!(p.themes(), PathBuf::from(r"C:\d\themes"));
        assert_eq!(p.runtime_root(), PathBuf::from(r"C:\d\runtime"));
        // The runtime folders of `Paths` live under it.
        assert!(p.omp_dir().starts_with(p.runtime_root()));
        assert!(p.clink_dir().starts_with(p.runtime_root()));
    }

    /// A path with an unpaired surrogate is legal on Windows but is not text, so JSON cannot hold it.
    #[cfg(windows)]
    #[test]
    fn a_state_that_cannot_be_turned_into_json_is_a_save_failure_not_a_damaged_file() {
        use std::os::windows::ffi::OsStringExt;
        let mut s = State::default();
        s.note_pending_delete(PathBuf::from(std::ffi::OsString::from_wide(&[
            0x43, 0x3a, 0x5c, 0xd800,
        ])));
        assert_eq!(s.to_bytes().unwrap_err().code(), "E_DATA_WRITE");
    }
}
