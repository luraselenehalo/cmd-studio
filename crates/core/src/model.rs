//! The `.cmdtheme` v1 model (spec §3.3): parsing, sanitizing, validation.
use crate::color::Rgb;
use crate::palette::{Colors, Tone};
use crate::text::sanitize_text;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// The `schemaVersion` this build reads and writes.
pub const SCHEMA_VERSION: u32 = 1;
/// The largest theme file `parse_theme` takes (spec §3.7 #5).
pub const MAX_FILE_BYTES: usize = 64 * 1024;

// The limits below are attributes as well as checks in `validate`, because the
// published JSON Schema (spec §3.3) is generated from these types and has to state
// them. Lengths count characters (Unicode code points), as JSON Schema does.
// The schema describes the file as written. On import `validate` is the judge, and it
// is a little more lenient in two places the schema cannot express: it measures text
// after `sanitize` has removed control characters, and the font name after trimming.

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Theme {
    /// The version of the file format.
    #[schemars(range(min = SCHEMA_VERSION, max = SCHEMA_VERSION))]
    pub schema_version: u32,
    /// A random (version 4) UUID. An import that clashes with an existing theme gets a new one.
    pub id: uuid::Uuid,
    #[schemars(length(min = 1, max = 64))]
    pub name: String,
    #[serde(default)]
    #[schemars(length(max = 64))]
    pub author: String,
    pub tone: Tone,
    /// The inputs of simple mode; null once any color has been edited by hand.
    pub simple: Option<SimpleRecipe>,
    pub colors: Colors,
    pub terminal: TerminalSettings,
    pub prompt: PromptSettings,
    pub banner: BannerSettings,
}

/// Inputs of simple mode, kept so the theme can be edited there again.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SimpleRecipe {
    pub accent: Rgb,
    #[schemars(range(min = 0.0, max = 0.30))]
    pub tint: f64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TerminalSettings {
    /// Font family name. When it is not installed, Windows Terminal gets the bundled fallback font.
    #[schemars(length(min = 1, max = 64))]
    pub font: String,
    #[schemars(range(min = 8, max = 32))]
    pub font_size: u8,
    pub cursor_shape: CursorShape,
    #[schemars(range(min = 30, max = 100))]
    pub opacity: u8,
    pub acrylic: bool,
    /// `[horizontal, vertical]`.
    #[schemars(inner(range(min = 0, max = 40)))]
    pub padding: [u8; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum CursorShape {
    FilledBox,
    Bar,
    Underscore,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PromptSettings {
    pub layout: Layout,
    /// Shown in list order; each segment may appear once.
    #[schemars(extend("uniqueItems" = true))]
    pub segments: Vec<Segment>,
    pub transient: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Layout {
    Box,
    Line,
    Powerline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Segment {
    Path,
    Git,
    Time,
    User,
    Host,
    Status,
    Duration,
}

/// The welcome box printed when cmd starts. `title` and `lines` may use the variables
/// `{user}`, `{host}`, `{cwd}`, `{date}` and `{time}`; any other `{word}` stays as written.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BannerSettings {
    pub enabled: bool,
    #[schemars(length(max = 80))]
    pub title: String,
    #[schemars(length(max = 4), inner(length(max = 80)))]
    pub lines: Vec<String>,
    pub tips: bool,
    pub clear_logo: bool,
}

/// Where a theme is applied (spec §4.2 page 4; CLI `--target`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Target {
    All,
    Wt,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ThemeError {
    #[error("theme file is larger than 64 KB")]
    TooLarge,
    #[error("theme file comes from a newer CMD Studio (schemaVersion {0})")]
    NewerSchema(u64),
    #[error("not a valid theme file: {0}")]
    Invalid(String),
    #[error("invalid {field}: {reason}")]
    Field {
        field: &'static str,
        reason: &'static str,
    },
}

impl ThemeError {
    /// Stable code shown to users (spec §5.9).
    pub fn code(&self) -> &'static str {
        match self {
            ThemeError::NewerSchema(_) => "E_THEME_NEWER_SCHEMA",
            _ => "E_THEME_INVALID",
        }
    }
}

fn field(field: &'static str, reason: &'static str) -> Result<(), ThemeError> {
    Err(ThemeError::Field { field, reason })
}

/// 0 is not a version, and a number above ours comes from a newer CMD Studio.
fn check_schema_version(version: u64) -> Result<(), ThemeError> {
    if version == 0 {
        return field("schemaVersion", "must be 1 or higher");
    }
    if version > u64::from(SCHEMA_VERSION) {
        return Err(ThemeError::NewerSchema(version));
    }
    Ok(())
}

impl Theme {
    /// Removes escape sequences and control characters (newlines included) from the
    /// free text: name, author, banner title and banner lines (spec §3.7 #3).
    /// `terminal.font` is not touched; the spec does not list it.
    pub fn sanitize(&mut self) {
        self.name = sanitize_text(&self.name);
        self.author = sanitize_text(&self.author);
        self.banner.title = sanitize_text(&self.banner.title);
        for line in &mut self.banner.lines {
            *line = sanitize_text(line);
        }
    }

    /// Checks every rule of spec §3.3 that the types do not already enforce, so a theme
    /// built in code is held to the same rules as a parsed one. Run it after
    /// [`Theme::sanitize`]: lengths are counted on the text that is kept.
    pub fn validate(&self) -> Result<(), ThemeError> {
        check_schema_version(u64::from(self.schema_version))?;
        if self.id.get_version() != Some(uuid::Version::Random) {
            return field("id", "must be a version 4 UUID");
        }
        let name_len = self.name.chars().count();
        if name_len == 0 || name_len > 64 {
            return field("name", "must be 1–64 characters");
        }
        if self.author.chars().count() > 64 {
            return field("author", "must be at most 64 characters");
        }
        if let Some(s) = &self.simple {
            if !(0.0..=0.30).contains(&s.tint) {
                return field("simple.tint", "must be 0–0.30");
            }
        }
        let t = &self.terminal;
        let font_len = t.font.trim().chars().count();
        if font_len == 0 || font_len > 64 {
            return field("terminal.font", "must be 1–64 characters");
        }
        if !(8..=32).contains(&t.font_size) {
            return field("terminal.fontSize", "must be 8–32");
        }
        if !(30..=100).contains(&t.opacity) {
            return field("terminal.opacity", "must be 30–100");
        }
        if t.padding.iter().any(|p| *p > 40) {
            return field("terminal.padding", "each value must be 0–40");
        }
        let mut seen = HashSet::new();
        if !self.prompt.segments.iter().all(|s| seen.insert(*s)) {
            return field("prompt.segments", "must not repeat");
        }
        if self.banner.title.chars().count() > 80 {
            return field("banner.title", "must be at most 80 characters");
        }
        if self.banner.lines.len() > 4 || self.banner.lines.iter().any(|l| l.chars().count() > 80) {
            return field("banner.lines", "at most 4 lines of at most 80 characters");
        }
        Ok(())
    }

    /// The bytes written when the theme is exported or saved.
    pub fn to_file_bytes(&self) -> Result<Vec<u8>, ThemeError> {
        let mut bytes =
            serde_json::to_vec_pretty(self).map_err(|e| ThemeError::Invalid(e.to_string()))?;
        bytes.push(b'\n');
        Ok(bytes)
    }
}

/// Parses, sanitizes and validates a `.cmdtheme` file.
///
/// `schemaVersion` is read before anything else, so a file from a newer CMD Studio is
/// reported as newer and not as having unknown fields. A file that does not fit the
/// format fails with [`ThemeError::Invalid`], and the message starts with the path of
/// the field that is wrong, for example `colors.ansi[3]: not a #rrggbb color` (spec
/// §3.7 #5). Text is sanitized before the limits are checked.
pub fn parse_theme(bytes: &[u8]) -> Result<Theme, ThemeError> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(ThemeError::TooLarge);
    }
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| ThemeError::Invalid(e.to_string()))?;
    let version = value
        .get("schemaVersion")
        .and_then(serde_json::Value::as_u64)
        .ok_or(ThemeError::Field {
            field: "schemaVersion",
            reason: "missing or not a whole number",
        })?;
    check_schema_version(version)?;
    let mut theme: Theme =
        serde_path_to_error::deserialize(value).map_err(|e| ThemeError::Invalid(e.to_string()))?;
    theme.sanitize();
    theme.validate()?;
    Ok(theme)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    const FIXTURE: &str = include_str!("../tests/fixtures/terracotta-mine.cmdtheme");

    fn with(edit: impl FnOnce(&mut Value)) -> Result<Theme, ThemeError> {
        let mut v: Value = serde_json::from_str(FIXTURE).unwrap();
        edit(&mut v);
        parse_theme(&serde_json::to_vec(&v).unwrap())
    }

    fn field_err(field: &'static str, reason: &'static str) -> ThemeError {
        ThemeError::Field { field, reason }
    }

    #[test]
    fn parses_the_spec_example() {
        let t = parse_theme(FIXTURE.as_bytes()).unwrap();
        assert_eq!(t.name, "Terracotta ของฉัน");
        assert_eq!(t.tone, Tone::Dark);
        assert_eq!(t.prompt.layout, Layout::Box);
        assert_eq!(
            t.prompt.segments,
            vec![Segment::Path, Segment::Git, Segment::Time]
        );
        assert_eq!(t.terminal.cursor_shape, CursorShape::FilledBox);
        assert!(t.banner.clear_logo);
    }

    #[test]
    fn rejects_unknown_fields_at_any_depth() {
        assert_eq!(
            with(|v| v["extra"] = json!(1)).unwrap_err().code(),
            "E_THEME_INVALID"
        );
        assert!(with(|v| v["banner"]["sound"] = json!(true)).is_err());
        assert!(with(|v| v["colors"]["glow"] = json!("#ffffff")).is_err());
    }

    #[test]
    fn rejects_newer_and_zero_schema_versions() {
        let e = with(|v| v["schemaVersion"] = json!(2)).unwrap_err();
        assert_eq!(e, ThemeError::NewerSchema(2));
        assert_eq!(e.code(), "E_THEME_NEWER_SCHEMA");
        assert!(with(|v| v["schemaVersion"] = json!(0)).is_err());
        assert!(with(|v| v["schemaVersion"] = json!("1")).is_err());
    }

    #[test]
    fn rejects_bad_colors_and_short_ansi() {
        assert!(with(|v| v["colors"]["accent"] = json!("#12345")).is_err());
        assert!(with(|v| {
            v["colors"]["ansi"].as_array_mut().unwrap().pop();
        })
        .is_err());
    }

    #[test]
    fn rejects_values_outside_the_enums() {
        assert!(with(|v| v["prompt"]["layout"] = json!("fancy")).is_err());
        assert!(with(|v| v["prompt"]["segments"] = json!(["path", "command"])).is_err());
        assert!(with(|v| v["terminal"]["cursorShape"] = json!("vintage")).is_err());
    }

    #[test]
    fn strips_control_characters_from_text() {
        let t = with(|v| {
            v["name"] = json!("A\u{1b}[31mB\u{7}");
            v["banner"]["title"] = json!("hi\u{1b}]0;evil\u{7}");
        })
        .unwrap();
        assert_eq!(t.name, "AB");
        assert_eq!(t.banner.title, "hi0;evil");
    }

    #[test]
    fn enforces_limits() {
        let name = field_err("name", "must be 1–64 characters");
        assert_eq!(
            with(|v| v["name"] = json!("x".repeat(65))).unwrap_err(),
            name
        );
        assert_eq!(with(|v| v["name"] = json!("\u{7}\u{7}")).unwrap_err(), name);
        assert_eq!(
            with(|v| v["terminal"]["fontSize"] = json!(7)).unwrap_err(),
            field_err("terminal.fontSize", "must be 8–32")
        );
        assert_eq!(
            with(|v| v["terminal"]["opacity"] = json!(29)).unwrap_err(),
            field_err("terminal.opacity", "must be 30–100")
        );
        assert_eq!(
            with(|v| v["terminal"]["padding"] = json!([41, 10])).unwrap_err(),
            field_err("terminal.padding", "each value must be 0–40")
        );
        assert_eq!(
            with(|v| v["simple"]["tint"] = json!(0.31)).unwrap_err(),
            field_err("simple.tint", "must be 0–0.30")
        );
        assert_eq!(
            with(|v| v["prompt"]["segments"] = json!(["git", "git"])).unwrap_err(),
            field_err("prompt.segments", "must not repeat")
        );
        assert_eq!(
            with(|v| v["banner"]["title"] = json!("t".repeat(81))).unwrap_err(),
            field_err("banner.title", "must be at most 80 characters")
        );
        assert_eq!(
            with(|v| v["banner"]["lines"] = json!(["a", "b", "c", "d", "e"])).unwrap_err(),
            field_err("banner.lines", "at most 4 lines of at most 80 characters")
        );
        assert_eq!(
            with(|v| v["banner"]["lines"] = json!(["l".repeat(81)])).unwrap_err(),
            field_err("banner.lines", "at most 4 lines of at most 80 characters")
        );
    }

    #[test]
    fn accepts_a_utf8_bom() {
        // Review Focus 1
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(FIXTURE.as_bytes());
        assert!(parse_theme(&bytes).is_ok());
    }

    #[test]
    fn rejects_utf16_cleanly() {
        // Review Focus 1
        let bytes: Vec<u8> = [0xFF, 0xFE]
            .into_iter()
            .chain(FIXTURE.encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        assert!(matches!(parse_theme(&bytes), Err(ThemeError::Invalid(_))));
    }

    #[test]
    fn rejects_oversized_files() {
        let bytes = vec![b' '; MAX_FILE_BYTES + 1];
        assert_eq!(parse_theme(&bytes).unwrap_err(), ThemeError::TooLarge);
    }

    #[test]
    fn round_trips_through_the_file_format() {
        let t = parse_theme(FIXTURE.as_bytes()).unwrap();
        assert_eq!(parse_theme(&t.to_file_bytes().unwrap()).unwrap(), t);
    }

    #[test]
    fn target_uses_cli_words() {
        assert_eq!(serde_json::to_string(&Target::All).unwrap(), "\"all\"");
        assert_eq!(serde_json::to_string(&Target::Wt).unwrap(), "\"wt\"");
    }

    // ---- field paths in errors, schema limits, id, schemaVersion, sanitize-then-validate ----

    fn message(edit: impl FnOnce(&mut Value)) -> String {
        with(edit).unwrap_err().to_string()
    }

    #[test]
    fn parse_errors_name_the_field_path() {
        // spec §3.7 #5: "say which field is wrong"
        let bad_color = with(|v| v["colors"]["ansi"][3] = json!("#12345")).unwrap_err();
        assert_eq!(bad_color.code(), "E_THEME_INVALID");
        assert!(
            bad_color
                .to_string()
                .contains("colors.ansi[3]: not a #rrggbb color"),
            "{bad_color}"
        );
        for (got, want) in [
            (
                message(|v| {
                    v["colors"]["ansi"].as_array_mut().unwrap().pop();
                }),
                "colors.ansi: invalid length 15",
            ),
            (
                message(|v| v["terminal"]["fontSize"] = json!("big")),
                "terminal.fontSize: invalid type: string \"big\", expected u8",
            ),
            (
                // `simple` is an Option, which must not leave a gap in the path
                message(|v| v["simple"]["accent"] = json!("nope")),
                "simple.accent: not a #rrggbb color",
            ),
            (
                message(|v| v["prompt"]["segments"] = json!(["path", "command"])),
                "prompt.segments[1]: unknown variant `command`",
            ),
            (
                message(|v| v["id"] = json!("not-a-uuid")),
                "id: UUID parsing failed",
            ),
        ] {
            assert!(got.contains(want), "{got:?} should contain {want:?}");
        }
        // a missing field is named by serde itself, with the object it belongs to
        let missing = message(|v| {
            v.as_object_mut().unwrap().remove("name");
        });
        assert!(missing.contains("missing field `name`"), "{missing}");
        let nested = message(|v| {
            v["banner"].as_object_mut().unwrap().remove("title");
        });
        assert!(nested.contains("banner: missing field `title`"), "{nested}");
    }

    #[test]
    fn unknown_fields_are_named_with_their_place() {
        // the path ends with the unknown key itself, so it says where it sits
        for (got, want) in [
            (
                message(|v| v["extra"] = json!(1)),
                "extra: unknown field `extra`",
            ),
            (
                message(|v| v["banner"]["sound"] = json!(true)),
                "banner.sound: unknown field `sound`",
            ),
            (
                message(|v| v["colors"]["glow"] = json!("#ffffff")),
                "colors.glow: unknown field `glow`",
            ),
            (
                message(|v| v["simple"]["shade"] = json!(1)),
                "simple.shade: unknown field `shade`",
            ),
        ] {
            assert!(got.contains(want), "{got:?} should contain {want:?}");
        }
    }

    #[test]
    fn the_json_schema_states_the_limits() {
        // spec §3.3: the published schema is generated from these types, so the
        // limits have to live in attributes. One field per mechanism: a string length,
        // a float range, a range on array items, an items length, and uniqueItems.
        let schema = serde_json::to_value(schemars::schema_for!(Theme)).unwrap();
        let num = |pointer: &str| {
            schema
                .pointer(pointer)
                .and_then(Value::as_f64)
                .unwrap_or_else(|| panic!("no number at {pointer}"))
        };
        assert_eq!(num("/properties/name/minLength"), 1.0);
        assert_eq!(num("/properties/name/maxLength"), 64.0);
        assert_eq!(num("/properties/author/maxLength"), 64.0);
        assert_eq!(num("/properties/schemaVersion/minimum"), 1.0);
        assert_eq!(num("/properties/schemaVersion/maximum"), 1.0);
        assert_eq!(
            num("/$defs/TerminalSettings/properties/font/minLength"),
            1.0
        );
        assert_eq!(
            num("/$defs/TerminalSettings/properties/font/maxLength"),
            64.0
        );
        assert_eq!(num("/$defs/SimpleRecipe/properties/tint/minimum"), 0.0);
        assert_eq!(num("/$defs/SimpleRecipe/properties/tint/maximum"), 0.3);
        assert_eq!(
            num("/$defs/TerminalSettings/properties/fontSize/minimum"),
            8.0
        );
        assert_eq!(
            num("/$defs/TerminalSettings/properties/fontSize/maximum"),
            32.0
        );
        assert_eq!(
            num("/$defs/TerminalSettings/properties/padding/items/maximum"),
            40.0
        );
        assert_eq!(num("/$defs/BannerSettings/properties/lines/maxItems"), 4.0);
        assert_eq!(
            num("/$defs/BannerSettings/properties/lines/items/maxLength"),
            80.0
        );
        assert_eq!(
            schema.pointer("/$defs/PromptSettings/properties/segments/uniqueItems"),
            Some(&json!(true))
        );
    }

    #[test]
    fn the_id_must_be_a_version_4_uuid() {
        // spec §3.3: "id: UUID v4"
        let not_v4 = field_err("id", "must be a version 4 UUID");
        for (what, id) in [
            ("nil", "00000000-0000-0000-0000-000000000000"),
            ("v1", "6ba7b810-9dad-11d1-80b4-00c04fd430c8"),
            ("v3", "9073926b-929f-31c2-abc9-fad77ae3e8eb"),
            ("v5", "886313e1-3b8a-5372-9b90-0c9aee199e5d"),
            ("v7", "017f22e2-79b0-7cc3-98c4-dc0c0c07398f"),
            ("max", "ffffffff-ffff-ffff-ffff-ffffffffffff"),
        ] {
            assert_eq!(with(|v| v["id"] = json!(id)).unwrap_err(), not_v4, "{what}");
        }
        // the fixture's own id, an upper-case one, and a fresh one all pass
        let fresh = uuid::Uuid::new_v4().to_string();
        for id in [
            "2f0c8f0e-6a8e-4b7e-9a4f-3f1f0b9c1d2e",
            "2F0C8F0E-6A8E-4B7E-9A4F-3F1F0B9C1D2E",
            fresh.as_str(),
        ] {
            let t = with(|v| v["id"] = json!(id)).unwrap();
            assert_eq!(t.id.to_string(), id.to_lowercase());
        }
    }

    #[test]
    fn validate_itself_checks_the_schema_version() {
        // a Theme built in code is held to the same rule as a parsed one
        let mut t = parse_theme(FIXTURE.as_bytes()).unwrap();
        assert_eq!(t.validate(), Ok(()));
        t.schema_version = 2;
        assert_eq!(t.validate(), Err(ThemeError::NewerSchema(2)));
        t.schema_version = u32::MAX;
        assert_eq!(
            t.validate(),
            Err(ThemeError::NewerSchema(u64::from(u32::MAX)))
        );
        t.schema_version = 0;
        let zero = t.validate().unwrap_err();
        assert_eq!(zero, field_err("schemaVersion", "must be 1 or higher"));
        assert_eq!(zero.code(), "E_THEME_INVALID");
    }

    #[test]
    fn the_font_is_one_to_sixty_four_characters_after_trimming() {
        // 1–64 characters, judged after trimming; the font is trimmed to be measured, not changed
        let font = field_err("terminal.font", "must be 1–64 characters");
        for bad in ["", " ", "\t \u{a0}\u{3000}"] {
            assert_eq!(
                with(|v| v["terminal"]["font"] = json!(bad)).unwrap_err(),
                font,
                "{bad:?}"
            );
        }
        assert_eq!(
            with(|v| v["terminal"]["font"] = json!("f".repeat(65))).unwrap_err(),
            font
        );
        // the 64 are counted as characters, not bytes, and padding does not count
        for good in [
            "f".to_string(),
            "f".repeat(64),
            "ก".repeat(64),
            format!("  {}\t", "f".repeat(64)),
        ] {
            assert!(
                with(|v| v["terminal"]["font"] = json!(good)).is_ok(),
                "{good:?}"
            );
        }
    }

    #[test]
    fn the_author_is_at_most_sixty_four_characters() {
        // spec §3.3: author is 0–64 characters
        let author = field_err("author", "must be at most 64 characters");
        assert_eq!(
            with(|v| v["author"] = json!("a".repeat(65))).unwrap_err(),
            author
        );
        assert_eq!(
            with(|v| v["author"] = json!("ก".repeat(65))).unwrap_err(),
            author
        );
        for good in ["", "a", &"a".repeat(64), &"ก".repeat(64)] {
            assert!(with(|v| v["author"] = json!(good)).is_ok(), "{good:?}");
        }
        // `author` may be left out of the file
        let t = with(|v| {
            v.as_object_mut().unwrap().remove("author");
        })
        .unwrap();
        assert_eq!(t.author, "");
    }

    #[test]
    fn lengths_are_checked_after_control_characters_are_removed() {
        // sanitizing comes first, so the limits count what is kept
        let t = with(|v| {
            v["name"] = json!(format!("{}\u{1b}[31m\u{7}", "n".repeat(64)));
            v["author"] = json!(format!("\u{7}{}", "a".repeat(64)));
            v["banner"]["title"] = json!(format!("{}\n\r", "t".repeat(80)));
            v["banner"]["lines"] = json!(["l1", format!("{}\u{1b}[0m", "l".repeat(80))]);
        })
        .unwrap();
        assert_eq!(t.name, "n".repeat(64));
        assert_eq!(t.author, "a".repeat(64));
        assert_eq!(t.banner.title, "t".repeat(80));
        assert_eq!(t.banner.lines, ["l1".to_string(), "l".repeat(80)]);
        // a name of nothing but controls is empty once they are gone, so it fails as empty
        let name_err = field_err("name", "must be 1–64 characters");
        assert_eq!(
            with(|v| v["name"] = json!("\u{1b}[31m\u{7}\n")).unwrap_err(),
            name_err
        );
        // and one visible character too many is still too many
        assert_eq!(
            with(|v| v["name"] = json!(format!("{}\u{7}", "n".repeat(65)))).unwrap_err(),
            name_err
        );
    }

    #[test]
    fn the_size_limit_is_inclusive() {
        // spec §3.7 #5: "at most 64 KB", so exactly that many bytes is still a theme
        let mut bytes = FIXTURE.as_bytes().to_vec();
        bytes.resize(MAX_FILE_BYTES, b' ');
        assert!(parse_theme(&bytes).is_ok());
        bytes.push(b' ');
        assert_eq!(parse_theme(&bytes).unwrap_err(), ThemeError::TooLarge);
    }

    #[test]
    fn error_codes_are_stable() {
        // spec §5.9: users see these codes
        assert_eq!(ThemeError::TooLarge.code(), "E_THEME_INVALID");
        assert_eq!(ThemeError::Invalid("x".into()).code(), "E_THEME_INVALID");
        assert_eq!(field_err("name", "x").code(), "E_THEME_INVALID");
        assert_eq!(ThemeError::NewerSchema(9).code(), "E_THEME_NEWER_SCHEMA");
    }

    #[test]
    fn exported_files_are_plain_utf8_with_a_final_newline() {
        let t = parse_theme(FIXTURE.as_bytes()).unwrap();
        let bytes = t.to_file_bytes().unwrap();
        assert!(!bytes.starts_with(&[0xEF, 0xBB, 0xBF]), "no BOM");
        assert_eq!(bytes.last(), Some(&b'\n'));
        let text = String::from_utf8(bytes).unwrap();
        assert!(
            text.contains("Terracotta ของฉัน"),
            "Thai is written as is, not escaped"
        );
        for key in [
            "\"schemaVersion\": 1",
            "\"fontSize\": 12",
            "\"cursorShape\": \"filledBox\"",
            "\"clearLogo\": true",
        ] {
            assert!(text.contains(key), "{key} missing from {text}");
        }
    }
}
