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

/// The limits of the format (spec §3.3), each written once. The `schemars` attributes on
/// the fields read them to state them in the published JSON Schema, and
/// [`Theme::validate`] reads them to enforce them, so the schema and the parser cannot
/// drift apart. Lengths count characters (Unicode code points), as JSON Schema does.
///
/// The schema describes the file as written. `validate` judges every text as import keeps
/// it, cleaned (and, for the name, the author and the font, trimmed), and that is a little
/// more lenient than the schema where a file has control characters or blanks in those.
/// The files `to_file_bytes` writes never do: they hold the cleaned form.
pub mod limits {
    /// `name`: at least this many characters.
    pub const NAME_MIN_CHARS: usize = 1;
    /// `name`: at most this many characters.
    pub const NAME_MAX_CHARS: usize = 64;
    /// `author`: at most this many characters (it may be empty).
    pub const AUTHOR_MAX_CHARS: usize = 64;
    /// `simple.tint`: at least this much.
    pub const TINT_MIN: f64 = 0.0;
    /// `simple.tint`: at most this much.
    pub const TINT_MAX: f64 = 0.30;
    /// `terminal.font`: at least this many characters.
    pub const FONT_MIN_CHARS: usize = 1;
    /// `terminal.font`: at most this many characters.
    pub const FONT_MAX_CHARS: usize = 64;
    /// `terminal.fontSize`: at least this.
    pub const FONT_SIZE_MIN: u8 = 8;
    /// `terminal.fontSize`: at most this.
    pub const FONT_SIZE_MAX: u8 = 32;
    /// `terminal.opacity`: at least this.
    pub const OPACITY_MIN: u8 = 30;
    /// `terminal.opacity`: at most this.
    pub const OPACITY_MAX: u8 = 100;
    /// `terminal.padding`: each of the two values is at most this (and at least 0).
    pub const PADDING_MAX: u8 = 40;
    /// `banner.title`: at most this many characters.
    pub const TITLE_MAX_CHARS: usize = 80;
    /// `banner.lines`: at most this many lines.
    pub const LINES_MAX: usize = 4;
    /// `banner.lines`: each line has at most this many characters.
    pub const LINE_MAX_CHARS: usize = 80;
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Theme {
    /// The version of the file format.
    #[schemars(range(min = SCHEMA_VERSION, max = SCHEMA_VERSION))]
    pub schema_version: u32,
    /// A random (version 4) UUID, written as 8-4-4-4-12 hexadecimal digits with hyphens.
    /// An import that clashes with an existing theme gets a new one.
    #[serde(deserialize_with = "canonical_id")]
    #[schemars(regex(pattern = ID_PATTERN))]
    pub id: uuid::Uuid,
    #[schemars(length(min = limits::NAME_MIN_CHARS, max = limits::NAME_MAX_CHARS))]
    pub name: String,
    #[serde(default)]
    #[schemars(length(max = limits::AUTHOR_MAX_CHARS))]
    pub author: String,
    pub tone: Tone,
    /// The inputs of simple mode; null once any color has been edited by hand.
    pub simple: Option<SimpleRecipe>,
    pub colors: Colors,
    pub terminal: TerminalSettings,
    pub prompt: PromptSettings,
    pub banner: BannerSettings,
    /// Where editors find the JSON Schema of this file. Optional; CMD Studio reads it,
    /// ignores it and does not write it back.
    // The unit type is the point: nothing is kept, so this never changes what a theme equals.
    // Hidden from the API docs: it holds nothing, and code that builds a theme writes
    // `schema: ()` only because a struct literal has to name every field.
    #[serde(
        rename = "$schema",
        default,
        skip_serializing,
        deserialize_with = "string_and_forget"
    )]
    #[schemars(with = "String")]
    #[doc(hidden)]
    pub schema: (),
}

/// Reads a string and drops it; for the one key that is allowed in a file and ignored.
fn string_and_forget<'de, D: serde::Deserializer<'de>>(d: D) -> Result<(), D::Error> {
    String::deserialize(d).map(drop)
}

/// How an id is written in a file, and the only way: the 8-4-4-4-12 form with hyphens,
/// of version 4 (the third group starts with 4) and the RFC 4122 variant (the fourth
/// group starts with 8, 9, a or b). The JSON Schema states this as the pattern of `id`.
const ID_PATTERN: &str =
    r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-4[0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$";

/// The message for an id that is not in the 8-4-4-4-12 form. It does not quote the id.
const ID_FORM: &str = "must be 8-4-4-4-12 hexadecimal digits separated by hyphens, \
                       with no braces and no urn:uuid: prefix";

/// Reads an id. The `uuid` crate would also take braces, a `urn:uuid:` prefix and no
/// hyphens at all, but a public file format should have one spelling for an id. Whether it
/// is version 4 with the right variant is for [`Theme::validate`], which also holds a theme
/// that was built in code.
fn canonical_id<'de, D: serde::Deserializer<'de>>(d: D) -> Result<uuid::Uuid, D::Error> {
    let text = String::deserialize(d)?;
    let canonical = text.len() == 36
        && text.bytes().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_hexdigit(),
        });
    if !canonical {
        return Err(serde::de::Error::custom(ID_FORM));
    }
    uuid::Uuid::parse_str(&text).map_err(serde::de::Error::custom)
}

/// Inputs of simple mode, kept so the theme can be edited there again.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SimpleRecipe {
    pub accent: Rgb,
    #[schemars(range(min = limits::TINT_MIN, max = limits::TINT_MAX))]
    pub tint: f64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TerminalSettings {
    /// Font family name. When it is not installed, Windows Terminal gets the bundled fallback font.
    #[schemars(length(min = limits::FONT_MIN_CHARS, max = limits::FONT_MAX_CHARS))]
    pub font: String,
    #[schemars(range(min = limits::FONT_SIZE_MIN, max = limits::FONT_SIZE_MAX))]
    pub font_size: u8,
    pub cursor_shape: CursorShape,
    #[schemars(range(min = limits::OPACITY_MIN, max = limits::OPACITY_MAX))]
    pub opacity: u8,
    pub acrylic: bool,
    /// `[horizontal, vertical]`.
    #[schemars(inner(range(min = 0, max = limits::PADDING_MAX)))]
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
    #[schemars(length(max = limits::TITLE_MAX_CHARS))]
    pub title: String,
    #[schemars(length(max = limits::LINES_MAX), inner(length(max = limits::LINE_MAX_CHARS)))]
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
    /// The file is not JSON, or does not fit the format. The text starts with the path of the
    /// field that is wrong (`colors.ansi[3]: ...`) and usually ends with a line and column.
    /// It is safe to print: the file can be quoted in it, so it has been cleaned like the
    /// file's own text, and the whole message, `not a valid theme file: ` included, is at
    /// most 300 characters (spec §3.3).
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

/// What an `Invalid` error says in front of the parser's message (see its `#[error]`).
const INVALID_START: &str = "not a valid theme file: ";
/// The most characters an `Invalid` error's whole message has, its start included (spec §3.3).
const MAX_MESSAGE_CHARS: usize = 300;
/// The most characters of the parser's message that an `Invalid` error keeps. The start is
/// ASCII, so its length in bytes is its length in characters.
const MAX_KEPT_CHARS: usize = MAX_MESSAGE_CHARS - INVALID_START.len();

/// The `Invalid` error for the message a parser gave. That message can quote the file
/// itself (an unknown key, an unknown variant), and the file is not to be trusted with
/// what a screen or a log prints, so the message is cleaned like the file's own text and
/// cut to `MAX_KEPT_CHARS`, which keeps the whole message within `MAX_MESSAGE_CHARS`. The
/// field path is what the message starts with, and the cut is made at the end, so the path
/// stays.
fn invalid(message: &str) -> ThemeError {
    let clean = sanitize_text(message);
    if clean.chars().count() <= MAX_KEPT_CHARS {
        return ThemeError::Invalid(clean);
    }
    let mut cut: String = clean.chars().take(MAX_KEPT_CHARS - 3).collect();
    cut.push_str("...");
    ThemeError::Invalid(cut)
}

/// A name, an author or a font as import keeps it: cleaned of escape sequences and control
/// characters (see [`sanitize_text`]) and trimmed. `Theme::sanitize` stores this, and
/// `Theme::validate` judges it, so there is one definition of what import makes of the text.
fn cleaned(text: &str) -> String {
    sanitize_text(text).trim().to_string()
}

/// A file of more than `MAX_FILE_BYTES` is refused (spec §3.7 #5). `parse_theme` applies this
/// to what it is given and `to_file_bytes` to what it writes, so the two cannot disagree
/// about the limit.
fn check_size(len: usize) -> Result<(), ThemeError> {
    if len > MAX_FILE_BYTES {
        return Err(ThemeError::TooLarge);
    }
    Ok(())
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

/// The parts of a theme that are objects have to be written as objects. serde also reads a
/// struct from a JSON list, field by field, so without this `"banner": [true, "t", [], false,
/// false]` would parse; the format and its published schema say object. `simple` may be null,
/// too. A part that is missing is for serde to report, so only what is there is looked at.
fn check_objects(value: &serde_json::Value) -> Result<(), ThemeError> {
    for part in ["colors", "terminal", "prompt", "banner"] {
        if value.get(part).is_some_and(|v| !v.is_object()) {
            return Err(invalid(&format!("{part}: expected an object")));
        }
    }
    if value
        .get("simple")
        .is_some_and(|v| !v.is_null() && !v.is_object())
    {
        return Err(invalid("simple: expected an object or null"));
    }
    Ok(())
}

/// The words of the format, as JSON pointers and as the paths an error names them by. Each of
/// `prompt.segments` is a word as well.
const WORDS: [(&str, &str); 3] = [
    ("/tone", "tone"),
    ("/terminal/cursorShape", "terminal.cursorShape"),
    ("/prompt/layout", "prompt.layout"),
];

/// A word of the format (`tone`, `terminal.cursorShape`, `prompt.layout` and each of
/// `prompt.segments`) is a string. serde also reads a word from a one-key object, so without
/// this `"layout": {"box": null}` would parse; the format and its published schema say string.
/// Only objects are refused here: any other wrong value is for serde to report.
fn check_words(value: &serde_json::Value) -> Result<(), ThemeError> {
    for (pointer, path) in WORDS {
        if value
            .pointer(pointer)
            .is_some_and(serde_json::Value::is_object)
        {
            return Err(invalid(&format!("{path}: expected a string")));
        }
    }
    let segments = value
        .pointer("/prompt/segments")
        .and_then(serde_json::Value::as_array);
    if let Some(i) = segments.and_then(|s| s.iter().position(serde_json::Value::is_object)) {
        return Err(invalid(&format!("prompt.segments[{i}]: expected a string")));
    }
    Ok(())
}

/// An id is a random UUID: version 4, of the RFC 4122 variant (spec §3.3).
fn check_id(id: &uuid::Uuid) -> Result<(), ThemeError> {
    if id.get_version() != Some(uuid::Version::Random) {
        return field(
            "id",
            "must be a version 4 UUID (the third group starts with 4)",
        );
    }
    if id.get_variant() != uuid::Variant::RFC4122 {
        return field(
            "id",
            "must be an RFC 4122 UUID (the fourth group starts with 8, 9, a or b)",
        );
    }
    Ok(())
}

impl Theme {
    /// Cleans every text in the theme with [`sanitize_text`]: name, author, font, banner
    /// title and banner lines (spec §3.7 #3, and the font, which ends up in files and
    /// settings of its own). That takes out escape sequences, control characters, line
    /// breaks and bidirectional controls. Name, author and font are trimmed as well, so
    /// a name of nothing but blanks is empty and fails [`Theme::validate`]. The banner
    /// is laid out by its text, so its spaces stay.
    pub fn sanitize(&mut self) {
        self.name = cleaned(&self.name);
        self.author = cleaned(&self.author);
        self.terminal.font = cleaned(&self.terminal.font);
        self.banner.title = sanitize_text(&self.banner.title);
        for line in &mut self.banner.lines {
            *line = sanitize_text(line);
        }
    }

    /// Checks every rule of spec §3.3 that the types do not already enforce, so a theme
    /// built in code is held to the same rules as a parsed one.
    ///
    /// Every text is judged as import keeps it, so the answer is the same before and after
    /// [`Theme::sanitize`], and a theme that passes is one that `parse_theme` takes back
    /// and `to_file_bytes` writes. Name, author and font are cleaned and trimmed (a name of
    /// nothing but blanks, or an author that is 65 characters once its control characters
    /// are gone, does not pass); the banner texts are cleaned only, because a banner is
    /// laid out by its spaces.
    pub fn validate(&self) -> Result<(), ThemeError> {
        check_schema_version(u64::from(self.schema_version))?;
        check_id(&self.id)?;
        let name_len = cleaned(&self.name).chars().count();
        if !(limits::NAME_MIN_CHARS..=limits::NAME_MAX_CHARS).contains(&name_len) {
            return field("name", "must be 1–64 characters");
        }
        if cleaned(&self.author).chars().count() > limits::AUTHOR_MAX_CHARS {
            return field("author", "must be at most 64 characters");
        }
        if let Some(s) = &self.simple {
            if !(limits::TINT_MIN..=limits::TINT_MAX).contains(&s.tint) {
                return field("simple.tint", "must be 0–0.30");
            }
        }
        let t = &self.terminal;
        let font_len = cleaned(&t.font).chars().count();
        if !(limits::FONT_MIN_CHARS..=limits::FONT_MAX_CHARS).contains(&font_len) {
            return field("terminal.font", "must be 1–64 characters");
        }
        if !(limits::FONT_SIZE_MIN..=limits::FONT_SIZE_MAX).contains(&t.font_size) {
            return field("terminal.fontSize", "must be 8–32");
        }
        if !(limits::OPACITY_MIN..=limits::OPACITY_MAX).contains(&t.opacity) {
            return field("terminal.opacity", "must be 30–100");
        }
        if t.padding.iter().any(|p| *p > limits::PADDING_MAX) {
            return field("terminal.padding", "each value must be 0–40");
        }
        let mut seen = HashSet::new();
        if !self.prompt.segments.iter().all(|s| seen.insert(*s)) {
            return field("prompt.segments", "must not repeat");
        }
        if sanitize_text(&self.banner.title).chars().count() > limits::TITLE_MAX_CHARS {
            return field("banner.title", "must be at most 80 characters");
        }
        let lines = &self.banner.lines;
        if lines.len() > limits::LINES_MAX
            || lines
                .iter()
                .any(|l| sanitize_text(l).chars().count() > limits::LINE_MAX_CHARS)
        {
            return field("banner.lines", "at most 4 lines of at most 80 characters");
        }
        Ok(())
    }

    /// The bytes written when the theme is exported or saved. The file holds the theme as
    /// import would keep it: a copy of it that has been through [`Theme::sanitize`], so
    /// every text in the file is clean and within the limits the published schema states
    /// (a name that is 64 characters once cleaned is 64 in the file, whatever padding or
    /// control characters it had). The theme itself is not changed.
    ///
    /// This never writes a file that `parse_theme` refuses. A theme that does not
    /// [`validate`](Theme::validate) is refused with the error `validate` gives
    /// (`serde_json` would write a NaN tint as `null`, which nobody could read back), and
    /// a file over the size limit is [`ThemeError::TooLarge`]. The size check is a guard:
    /// every text is cut to its limit, so no theme that validates comes near it.
    pub fn to_file_bytes(&self) -> Result<Vec<u8>, ThemeError> {
        let mut clean = self.clone();
        clean.sanitize();
        clean.validate()?;
        let mut bytes = serde_json::to_vec_pretty(&clean).map_err(|e| invalid(&e.to_string()))?;
        bytes.push(b'\n');
        check_size(bytes.len())?;
        Ok(bytes)
    }
}

/// Parses, sanitizes and validates a `.cmdtheme` file.
///
/// `schemaVersion` is read before anything else, so a file from a newer CMD Studio is
/// reported as newer and not as having unknown fields. A file that does not fit the
/// format fails with [`ThemeError::Invalid`]: the message starts with the path of the
/// field that is wrong, for example `colors.ansi[3]: not a #rrggbb color` (spec §3.7 #5),
/// and ends with the line and column. A key that appears twice is refused, and so is a
/// part that should be an object but is a list or anything else (`banner: expected an
/// object`), and a word of the format written as an object (`prompt.layout: expected a
/// string`). The message quotes the file at times, so it is cleaned and cut short. Text is
/// sanitized before the limits are checked.
pub fn parse_theme(bytes: &[u8]) -> Result<Theme, ThemeError> {
    check_size(bytes.len())?;
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    // First pass: is it JSON (and only one value of it), and which version does it say it is?
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| invalid(&e.to_string()))?;
    let version = value
        .get("schemaVersion")
        .and_then(serde_json::Value::as_u64)
        .ok_or(ThemeError::Field {
            field: "schemaVersion",
            reason: "missing or not a whole number",
        })?;
    check_schema_version(version)?;
    check_objects(&value)?;
    check_words(&value)?;
    // Second pass: the theme itself, from the bytes and not from `value`. A `Value` keeps
    // the last of two equal keys and no longer knows where anything was, so it could not
    // refuse a repeated key or give a line and column. The first pass has seen the whole
    // input, so there is nothing left after the value to check for.
    let mut de = serde_json::Deserializer::from_slice(bytes);
    let mut theme: Theme =
        serde_path_to_error::deserialize(&mut de).map_err(|e| invalid(&e.to_string()))?;
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
    fn a_word_of_the_format_is_a_string_and_never_an_object() {
        // serde reads `{"box": null}` as the word `box`; the format and its schema do not.
        for (pointer, word, path) in [
            ("/tone", "dark", "tone"),
            ("/terminal/cursorShape", "filledBox", "terminal.cursorShape"),
            ("/prompt/layout", "box", "prompt.layout"),
        ] {
            let mut object = serde_json::Map::new();
            object.insert(word.to_string(), Value::Null);
            assert_eq!(
                at(pointer, Value::Object(object)).unwrap_err(),
                ThemeError::Invalid(format!("{path}: expected a string")),
                "{pointer}"
            );
            assert!(at(pointer, json!(word)).is_ok(), "{pointer}");
        }
        assert_eq!(
            at("/prompt/segments", json!(["path", { "git": null }])).unwrap_err(),
            ThemeError::Invalid("prompt.segments[1]: expected a string".into())
        );
        // any other wrong value is still reported by the reader, with its line and column
        let text = invalid_text_with(|v| v["prompt"]["layout"] = json!(5));
        assert!(
            text.starts_with("prompt.layout: expected value at line 1"),
            "{text}"
        );
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
                "id: must be 8-4-4-4-12 hexadecimal digits",
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
        // 1–64 characters, judged on the text that is kept: cleaned, then trimmed
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

    // ---- fix round: what a file can make an error say, cleaning and trimming, "$schema",
    // ---- saving, repeated keys, the limits at both ends, and the form of an id

    /// Sets the value at a JSON pointer in the fixture, then parses it.
    fn at(pointer: &str, value: Value) -> Result<Theme, ThemeError> {
        with(|v| *v.pointer_mut(pointer).unwrap() = value)
    }

    /// The text of the `Invalid` error that `bytes` get: what a screen or a log would show.
    fn invalid_text(bytes: &[u8]) -> String {
        match parse_theme(bytes) {
            Err(ThemeError::Invalid(text)) => text,
            other => panic!("expected an Invalid error, got {other:?}"),
        }
    }

    fn invalid_text_with(edit: impl FnOnce(&mut Value)) -> String {
        let mut v: Value = serde_json::from_str(FIXTURE).unwrap();
        edit(&mut v);
        invalid_text(&serde_json::to_vec(&v).unwrap())
    }

    /// The fixture's text after `edit`, parsed: for files `with` cannot make, such as one
    /// with a repeated key.
    fn with_text(edit: impl FnOnce(&str) -> String) -> Result<Theme, ThemeError> {
        parse_theme(edit(FIXTURE).as_bytes())
    }

    /// An error text must be safe to print and short enough to read.
    fn assert_clean(text: &str) {
        assert!(
            !text.chars().any(char::is_control),
            "a control character in {text:?}"
        );
        let n = text.chars().count();
        assert!(n <= 300, "{n} characters");
    }

    // -- an error message is text from the file, so it is cleaned and kept short --

    #[test]
    fn hostile_text_never_reaches_an_error_message() {
        let esc = "\u{1b}[31m";
        let osc = "\u{1b}]0;pwn\u{7}";
        for (text, want) in [
            // an unknown key is its own path, at the root and nested
            (
                invalid_text_with(|v| v[format!("a{osc}b\nc").as_str()] = json!(1)),
                "a0;pwnbc: unknown field `a0;pwnbc`",
            ),
            (
                invalid_text_with(|v| v["banner"][format!("bad{esc}key").as_str()] = json!(true)),
                "banner.badkey: unknown field `badkey`",
            ),
            // an unknown variant is quoted by the parser
            (
                invalid_text_with(|v| v["prompt"]["layout"] = json!(format!("fancy{esc}\nlayout"))),
                "prompt.layout: unknown variant `fancylayout`",
            ),
            (
                invalid_text_with(|v| v["tone"] = json!(format!("night{osc}"))),
                "tone: unknown variant `night0;pwn`",
            ),
            (
                invalid_text_with(|v| v["terminal"]["cursorShape"] = json!("beam\u{7}")),
                "terminal.cursorShape: unknown variant `beam`",
            ),
            (
                invalid_text_with(|v| v["prompt"]["segments"] = json!(["path", "a\nb"])),
                "prompt.segments[1]: unknown variant `ab`",
            ),
            // a bad id is not quoted at all, but its path is
            (
                invalid_text_with(|v| {
                    v["id"] = json!(format!("{esc}2f0c8f0e-6a8e-4b7e-9a4f-3f1f0b9c1d2e"))
                }),
                "id: ",
            ),
        ] {
            assert_clean(&text);
            assert!(text.contains(want), "{text:?} should contain {want:?}");
        }
        // an escape sequence that never ends takes the rest of its text with it, and still
        // leaves nothing behind
        let text = invalid_text_with(|v| v["x\u{1b}["] = json!(1));
        assert_clean(&text);
        assert!(text.starts_with('x'), "{text:?}");
    }

    #[test]
    fn a_huge_key_gives_a_short_message() {
        let mut v: Value = serde_json::from_str(FIXTURE).unwrap();
        v["k".repeat(60_000).as_str()] = json!(1);
        let bytes = serde_json::to_vec(&v).unwrap();
        assert!(
            bytes.len() <= MAX_FILE_BYTES,
            "the file is allowed in: {} bytes",
            bytes.len()
        );
        let text = invalid_text(&bytes);
        assert_clean(&text);
        assert!(
            text.starts_with(&"k".repeat(50)),
            "the path is kept: {text}"
        );
        assert!(text.ends_with("..."), "{text}");
    }

    #[test]
    fn error_messages_are_cleaned_and_capped() {
        assert_eq!(
            invalid("a\u{1b}[31mb\nc\u{7}"),
            ThemeError::Invalid("abc".into())
        );
        // the whole message is at most 300 characters (spec §3.3), and its start takes 24 of
        // them; the cap counts characters, not bytes: 276 stay whole, 277 are cut to 276 with
        // dots
        assert_eq!(INVALID_START.chars().count(), 24);
        assert_eq!(
            ThemeError::Invalid(String::new()).to_string(),
            INVALID_START
        );
        for c in ["x", "ก", "👋"] {
            assert_eq!(invalid(&c.repeat(276)), ThemeError::Invalid(c.repeat(276)));
            let cut = invalid(&c.repeat(277));
            assert_eq!(cut, ThemeError::Invalid(format!("{}...", c.repeat(273))));
            assert_eq!(cut.to_string().chars().count(), 300);
            assert!(invalid(&c.repeat(10_000)).to_string().chars().count() <= 300);
        }
        // the cut is made after the cleaning, so controls do not use up the room
        assert_eq!(
            invalid(&format!("{}{}", "\u{7}".repeat(1000), "y".repeat(276))),
            ThemeError::Invalid("y".repeat(276))
        );
        assert_eq!(invalid("x").code(), "E_THEME_INVALID");
    }

    // -- the separators and bidi controls go, and name, author and font are trimmed --

    #[test]
    fn sanitize_removes_separators_and_bidi_controls_from_every_text() {
        let mut t = parse_theme(FIXTURE.as_bytes()).unwrap();
        t.name = "a\u{202e}b\u{2028}c\u{2066}d".to_string();
        t.author = "x\u{2029}y".to_string();
        t.terminal.font = "Cas\u{202a}cadia".to_string();
        t.banner.title = "t\u{2067}t".to_string();
        t.banner.lines = vec!["l\u{2069}l".to_string(), "\u{202d}m".to_string()];
        t.sanitize();
        assert_eq!(t.name, "abcd");
        assert_eq!(t.author, "xy");
        assert_eq!(t.terminal.font, "Cascadia");
        assert_eq!(t.banner.title, "tt");
        assert_eq!(t.banner.lines, ["ll", "m"]);
    }

    #[test]
    fn name_author_and_font_are_cleaned_and_trimmed() {
        let t = with(|v| {
            v["name"] = json!("  \u{a0}Terracotta  mine\u{3000} ");
            v["author"] = json!("\u{2003}Ada\u{1b}[0m ");
            v["terminal"]["font"] = json!("\t Cascadia\u{1b}[31m Mono\u{7} ");
        })
        .unwrap();
        // spaces inside stay, and so does everything that is not a blank at an end
        assert_eq!(t.name, "Terracotta  mine");
        assert_eq!(t.author, "Ada");
        assert_eq!(t.terminal.font, "Cascadia Mono");
        // a padded file and a plain one are the same theme
        let padded = with(|v| {
            v["name"] = json!("  Terracotta ของฉัน ");
            v["terminal"]["font"] = json!(" Cascadia Mono  ");
        })
        .unwrap();
        assert_eq!(padded, parse_theme(FIXTURE.as_bytes()).unwrap());
    }

    #[test]
    fn a_name_of_nothing_but_blanks_is_empty() {
        let name = field_err("name", "must be 1–64 characters");
        let font = field_err("terminal.font", "must be 1–64 characters");
        for blank in [
            " ",
            "   ",
            "\u{a0}",
            "\u{3000} \u{2003}",
            "\t\n ",
            "\u{1b}[0m  ",
            "\u{202e} \u{2028}",
        ] {
            assert_eq!(
                with(|v| v["name"] = json!(blank)).unwrap_err(),
                name,
                "{blank:?}"
            );
            assert_eq!(
                with(|v| v["terminal"]["font"] = json!(blank)).unwrap_err(),
                font,
                "{blank:?}"
            );
        }
        // an author may be empty, so a blank one just becomes empty
        assert_eq!(
            with(|v| v["author"] = json!(" \u{3000}")).unwrap().author,
            ""
        );
    }

    #[test]
    fn a_banner_keeps_its_spaces() {
        // the box is laid out by its text, so its spaces belong to it
        let t = with(|v| {
            v["banner"]["title"] = json!("  Welcome  ");
            v["banner"]["lines"] = json!(["  indented", "trailing  ", "\u{a0}nbsp"]);
        })
        .unwrap();
        assert_eq!(t.banner.title, "  Welcome  ");
        assert_eq!(t.banner.lines, ["  indented", "trailing  ", "\u{a0}nbsp"]);
    }

    #[test]
    fn malformed_json_gives_clean_short_messages() {
        // serde_json's own messages for broken JSON do not quote the file today. They go
        // through the same cleaning anyway, and this keeps it that way if they ever do.
        let mut inputs: Vec<Vec<u8>> = vec![
            b"{\"a\x07\": }".to_vec(),
            b"{\"a\": \"\\q\"}".to_vec(),
            b"{\"a\": \"\xff\xfe\"}".to_vec(),
            b"{\"a\": 1e999}".to_vec(),
            b"[1 2]".to_vec(),
            b"{\"a\":1,}".to_vec(),
            b"\x1b[31m{".to_vec(),
            b"{\"schemaVersion\": 1} \x1b]0;pwn\x07 trailing".to_vec(),
            b"\0\0\0".to_vec(),
            b"{\"schemaVersion\": 1, \"\xed\xa0\x80\": 1}".to_vec(),
            vec![b'['; 60_000],
        ];
        inputs.push(format!("{{\"k\": \"{}", "x".repeat(60_000)).into_bytes());
        for input in inputs {
            let text = invalid_text(&input);
            assert_clean(&text);
        }
    }

    #[test]
    fn a_newer_file_is_reported_as_newer_whatever_else_is_in_it() {
        // a later version may add fields and change types; none of that is this version's
        // business, so the version is read first and says so
        let text = with(|v| {
            v["schemaVersion"] = json!(2);
            v["futureThing"] = json!({"a": 1});
            v["terminal"]["fontSize"] = json!("big");
            v["id"] = json!("not even an id");
        });
        assert_eq!(text.unwrap_err(), ThemeError::NewerSchema(2));
    }

    #[test]
    fn validate_judges_a_code_built_font_without_its_blanks() {
        // `sanitize` trims the font on import; a theme built in code has not been through it,
        // and a font of nothing but blanks must not pass for one
        let mut t = parse_theme(FIXTURE.as_bytes()).unwrap();
        t.terminal.font = "   ".to_string();
        assert_eq!(
            t.validate(),
            Err(field_err("terminal.font", "must be 1–64 characters"))
        );
        t.terminal.font = format!("  {}  ", "f".repeat(64));
        assert_eq!(t.validate(), Ok(()));
    }

    // -- validate judges name, author and font as import would keep them --

    /// Every concatenation of up to `max` of `pieces`, shortest first.
    fn combos(pieces: &[&str], max: usize) -> Vec<String> {
        let mut all = vec![String::new()];
        let mut level = vec![String::new()];
        for _ in 0..max {
            level = level
                .iter()
                .flat_map(|prefix| pieces.iter().map(move |piece| format!("{prefix}{piece}")))
                .collect();
            all.extend(level.iter().cloned());
        }
        all
    }

    #[test]
    fn validate_judges_name_author_and_font_as_import_would_keep_them() {
        let good = parse_theme(FIXTURE.as_bytes()).unwrap();
        let name_err = field_err("name", "must be 1–64 characters");
        let font_err = field_err("terminal.font", "must be 1–64 characters");
        let author_err = field_err("author", "must be at most 64 characters");
        // nothing is left of these once they are cleaned and trimmed
        for blank in [
            "   ",
            "\u{7}",
            "\u{a0}\u{3000}",
            "\u{1b}[31m",
            "\u{202e}\u{2028}",
            "\t \n",
        ] {
            let mut t = good.clone();
            t.name = blank.to_string();
            assert_eq!(t.validate(), Err(name_err.clone()), "name {blank:?}");
            assert_eq!(t.to_file_bytes(), Err(name_err.clone()), "name {blank:?}");
            let mut t = good.clone();
            t.terminal.font = blank.to_string();
            assert_eq!(t.validate(), Err(font_err.clone()), "font {blank:?}");
            assert_eq!(t.to_file_bytes(), Err(font_err.clone()), "font {blank:?}");
            // an author may be empty, so there is nothing to refuse
            let mut t = good.clone();
            t.author = blank.to_string();
            assert_eq!(t.validate(), Ok(()), "author {blank:?}");
            assert!(t.to_file_bytes().is_ok(), "author {blank:?}");
        }
        // what counts is the length after cleaning: 65 are too many however they are padded
        // or interleaved, and 64 are fine whatever surrounds them
        let a64 = "a".repeat(64);
        let too_long = [
            "a".repeat(65),
            format!("\u{7}  {}b\u{1b}[0m", a64),
            format!(" {}\u{7}b", a64),
        ];
        let fine = [
            a64.clone(),
            format!("\u{7}  {a64}\u{1b}[0m\u{3000}"),
            format!("{a64}{}", "\u{7}".repeat(100)),
        ];
        for text in too_long {
            let mut t = good.clone();
            t.author = text.clone();
            assert_eq!(t.validate(), Err(author_err.clone()), "author {text:?}");
            assert_eq!(
                t.to_file_bytes(),
                Err(author_err.clone()),
                "author {text:?}"
            );
            let mut t = good.clone();
            t.name = text.clone();
            assert_eq!(t.validate(), Err(name_err.clone()), "name {text:?}");
            let mut t = good.clone();
            t.terminal.font = text.clone();
            assert_eq!(t.validate(), Err(font_err.clone()), "font {text:?}");
        }
        for text in fine {
            let mut t = good.clone();
            t.author = text.clone();
            t.name = text.clone();
            t.terminal.font = text.clone();
            assert_eq!(t.validate(), Ok(()), "{text:?}");
            assert!(t.to_file_bytes().is_ok(), "{text:?}");
        }
    }

    #[test]
    fn validate_and_import_agree_about_every_text() {
        // validate() on a theme built in code and parse_theme() on that theme's own JSON must
        // say the same, so that a theme that passes can be saved and read back, and one that
        // does not could not have been read. What is saved is the cleaned theme: the file
        // holds its texts, and saving the theme or its cleaned self gives the same file.
        let thirty = "x".repeat(30);
        let pieces = [
            "a",
            " ",
            "\u{7}",
            "\u{1b}[31m",
            "\u{202e}",
            "ก",
            "\u{3000}",
            thirty.as_str(),
        ];
        let good = parse_theme(FIXTURE.as_bytes()).unwrap();
        let mut checked = 0;
        for text in combos(&pieces, 3) {
            for which in ["name", "author", "font", "title", "line"] {
                let mut t = good.clone();
                match which {
                    "name" => t.name = text.clone(),
                    "author" => t.author = text.clone(),
                    "font" => t.terminal.font = text.clone(),
                    "title" => t.banner.title = text.clone(),
                    _ => t.banner.lines = vec![text.clone()],
                }
                let imported = parse_theme(&serde_json::to_vec(&t).unwrap());
                let why = format!("{which} = {text:?}");
                assert_eq!(t.validate().is_ok(), imported.is_ok(), "{why}");
                let mut cleaned = t.clone();
                cleaned.sanitize();
                if let Ok(imported) = imported {
                    assert_eq!(imported, cleaned, "{why}");
                    let bytes = t.to_file_bytes().unwrap();
                    assert_eq!(parse_theme(&bytes).unwrap(), cleaned, "{why}");
                    assert_eq!(bytes, cleaned.to_file_bytes().unwrap(), "{why}");
                    let file: Value = serde_json::from_slice(&bytes).unwrap();
                    assert_eq!(file["name"], json!(cleaned.name), "{why}");
                    assert_eq!(file["author"], json!(cleaned.author), "{why}");
                    assert_eq!(
                        file["terminal"]["font"],
                        json!(cleaned.terminal.font),
                        "{why}"
                    );
                    assert_eq!(
                        file["banner"]["title"],
                        json!(cleaned.banner.title),
                        "{why}"
                    );
                    assert_eq!(
                        file["banner"]["lines"],
                        json!(cleaned.banner.lines),
                        "{why}"
                    );
                } else {
                    assert!(t.to_file_bytes().is_err(), "{why}");
                }
                checked += 1;
            }
        }
        assert_eq!(checked, 585 * 5);
    }

    #[test]
    fn validate_judges_the_banner_as_import_keeps_it() {
        // import cleans the banner texts without trimming them, so validate() counts them
        // that way: the same answer before and after sanitize, as for the other texts
        let good = parse_theme(FIXTURE.as_bytes()).unwrap();
        let title_err = field_err("banner.title", "must be at most 80 characters");
        let lines_err = field_err("banner.lines", "at most 4 lines of at most 80 characters");
        // 80 characters with controls around them are 80 once cleaned
        let mut t = good.clone();
        t.banner.title = format!("\u{7}{}\u{202e}", "t".repeat(80));
        t.banner.lines = vec![format!("\u{1b}[0m{}\u{2028}", "l".repeat(80)); 4];
        assert_eq!(t.validate(), Ok(()));
        // 81 once cleaned are too many, whatever surrounds them
        t.banner.title = format!("\u{7}{}\u{202e}", "t".repeat(81));
        assert_eq!(t.validate(), Err(title_err.clone()));
        t.banner.title = "ok".to_string();
        t.banner.lines = vec![format!("\u{1b}[0m{}\u{2028}", "l".repeat(81))];
        assert_eq!(t.validate(), Err(lines_err.clone()));
        // a banner is laid out by its spaces, so blanks around a text are part of its length
        t.banner.lines = vec![format!(" {}", "l".repeat(80))];
        assert_eq!(t.validate(), Err(lines_err));
        t.banner.lines = vec![];
        t.banner.title = format!("{} ", "t".repeat(80));
        assert_eq!(t.validate(), Err(title_err));
    }

    #[test]
    fn what_is_saved_reads_back_equal() {
        // a theme from a file reads back equal, whatever it is
        for (what, edit) in [
            (
                "as it is",
                Box::new(|_: &mut Value| {}) as Box<dyn FnOnce(&mut Value)>,
            ),
            (
                "without simple inputs",
                Box::new(|v: &mut Value| v["simple"] = json!(null)),
            ),
            (
                "a long Thai name",
                Box::new(|v: &mut Value| v["name"] = json!("ก".repeat(64))),
            ),
            (
                "every segment, another layout",
                Box::new(|v: &mut Value| {
                    v["prompt"]["layout"] = json!("powerline");
                    v["prompt"]["segments"] =
                        json!(["path", "git", "time", "user", "host", "status", "duration"]);
                }),
            ),
            (
                "a full banner",
                Box::new(|v: &mut Value| {
                    v["banner"]["title"] = json!("t".repeat(80));
                    v["banner"]["lines"] = json!(["  indented", "l".repeat(80), "", "👋"]);
                }),
            ),
            (
                "with a schema line",
                Box::new(|v: &mut Value| v["$schema"] = json!("https://x")),
            ),
        ] {
            let t = with(edit).unwrap();
            assert_eq!(
                parse_theme(&t.to_file_bytes().unwrap()).unwrap(),
                t,
                "{what}"
            );
        }
        // one built in code that is not clean reads back as its cleaned self
        let mut dirty = parse_theme(FIXTURE.as_bytes()).unwrap();
        dirty.name = "  Mine\u{7} ".to_string();
        dirty.author = "\u{1b}[0mAda ".to_string();
        dirty.terminal.font = " Cascadia Mono\t".to_string();
        let back = parse_theme(&dirty.to_file_bytes().unwrap()).unwrap();
        assert_eq!(back.name, "Mine");
        assert_eq!(back.author, "Ada");
        assert_eq!(back.terminal.font, "Cascadia Mono");
    }

    #[test]
    fn saving_writes_the_cleaned_form() {
        let mut t = parse_theme(FIXTURE.as_bytes()).unwrap();
        t.name = "  \u{7}Name\u{202e}  ".to_string();
        t.author = "\u{1b}[0m Ada\u{2028} ".to_string();
        t.terminal.font = " Cascadia\u{7} Mono\t".to_string();
        t.banner.title = "  hi\u{2028}  ".to_string();
        t.banner.lines = vec!["a\u{7}b".to_string(), "  \u{202e}c".to_string()];
        let before = t.clone();
        let bytes = t.to_file_bytes().unwrap();
        // the theme itself is left as it was
        assert_eq!(t, before);
        // the file holds the cleaned text, and nothing of what was cleaned away
        let text = String::from_utf8(bytes.clone()).unwrap();
        for want in [
            "\"name\": \"Name\",",
            "\"author\": \"Ada\",",
            "\"font\": \"Cascadia Mono\",",
            "\"title\": \"  hi  \",", // a banner keeps its spaces
            "\"ab\"",
            "\"  c\"",
        ] {
            assert!(text.contains(want), "{want} missing from {text}");
        }
        for gone in ['\u{202e}', '\u{2028}', '\u{7}', '\u{1b}', '\t'] {
            assert!(!text.contains(gone), "{gone:?} in {text}");
        }
        assert!(!text.contains("\\u00") && !text.contains("\\t"), "{text}");
        // and it reads back equal to the cleaned theme
        let mut cleaned = before.clone();
        cleaned.sanitize();
        assert_eq!(parse_theme(&bytes).unwrap(), cleaned);
        assert_eq!(cleaned.name, "Name");
    }

    #[test]
    fn a_saved_file_is_within_the_published_limits() {
        // 64 characters once cleaned and 70 with their padding: the file holds the 64, which is
        // what the published schema's maxLength lets it hold. Same for the others.
        let schema = serde_json::to_value(schemars::schema_for!(Theme)).unwrap();
        let limit = |pointer: &str| {
            schema
                .pointer(pointer)
                .and_then(Value::as_u64)
                .unwrap_or_else(|| panic!("no number at {pointer}")) as usize
        };
        let name_max = limit("/properties/name/maxLength");
        let author_max = limit("/properties/author/maxLength");
        let font_max = limit("/$defs/TerminalSettings/properties/font/maxLength");
        let title_max = limit("/$defs/BannerSettings/properties/title/maxLength");
        let lines_max = limit("/$defs/BannerSettings/properties/lines/maxItems");
        let line_max = limit("/$defs/BannerSettings/properties/lines/items/maxLength");
        let mut t = parse_theme(FIXTURE.as_bytes()).unwrap();
        t.name = format!("   {}   ", "n".repeat(name_max));
        t.author = format!("\u{7}\u{7}{}\u{1b}[0m  ", "a".repeat(author_max));
        t.terminal.font = format!(" \t{}  ", "f".repeat(font_max));
        t.banner.title = format!("{}\u{202e}\u{7}\u{2028}", "t".repeat(title_max));
        t.banner.lines = vec![format!("\u{7}{}\u{2066}", "l".repeat(line_max)); lines_max];
        assert_eq!(t.name.chars().count(), 70, "the premise: 64 and padding");
        assert!(t.banner.title.chars().count() > title_max);
        let file: Value = serde_json::from_slice(&t.to_file_bytes().unwrap()).unwrap();
        let chars = |pointer: &str| {
            file.pointer(pointer)
                .and_then(Value::as_str)
                .unwrap_or_else(|| panic!("no text at {pointer}"))
                .chars()
                .count()
        };
        assert_eq!(chars("/name"), name_max);
        assert_eq!(chars("/author"), author_max);
        assert_eq!(chars("/terminal/font"), font_max);
        assert_eq!(chars("/banner/title"), title_max);
        let lines = file
            .pointer("/banner/lines")
            .and_then(Value::as_array)
            .unwrap();
        assert_eq!(lines.len(), lines_max);
        for line in lines {
            assert_eq!(line.as_str().unwrap().chars().count(), line_max);
        }
    }

    #[test]
    fn blanks_around_a_name_never_reach_the_file() {
        // what is written is the cleaned form, so a name padded with 70,000 blanks and bells
        // makes the very file the plain name makes, and the size limit has nothing to do with it
        let plain = parse_theme(FIXTURE.as_bytes()).unwrap();
        let mut padded = plain.clone();
        padded.name = format!(
            "{}{}{}",
            " ".repeat(70_000),
            plain.name,
            "\u{7}".repeat(70_000)
        );
        assert_eq!(
            padded.to_file_bytes().unwrap(),
            plain.to_file_bytes().unwrap()
        );
        assert_eq!(
            padded.name.chars().count(),
            140_000 + plain.name.chars().count()
        );
    }

    #[test]
    fn reading_and_writing_share_one_size_limit() {
        // spec §3.7 #5, "at most 64 KB": exactly that many bytes is fine and one more is not
        assert_eq!(check_size(0), Ok(()));
        assert_eq!(check_size(MAX_FILE_BYTES), Ok(()));
        assert_eq!(check_size(MAX_FILE_BYTES + 1), Err(ThemeError::TooLarge));
        assert_eq!(check_size(usize::MAX), Err(ThemeError::TooLarge));
    }

    #[test]
    fn the_largest_theme_that_validates_is_far_below_the_size_limit() {
        // Every text is cut to its limit when it is saved, so a saved file is small however
        // the theme was built, and the size check in to_file_bytes() is a guard for a limit
        // that is raised one day. This is the worst case: every text at its limit, in
        // four-byte characters, all seven segments and the most lines.
        let mut t = parse_theme(FIXTURE.as_bytes()).unwrap();
        t.name = "👋".repeat(64);
        t.author = "👋".repeat(64);
        t.terminal.font = "👋".repeat(64);
        t.banner.title = "👋".repeat(80);
        t.banner.lines = vec!["👋".repeat(80); 4];
        t.prompt.segments = vec![
            Segment::Path,
            Segment::Git,
            Segment::Time,
            Segment::User,
            Segment::Host,
            Segment::Status,
            Segment::Duration,
        ];
        let bytes = t.to_file_bytes().unwrap();
        assert!(bytes.len() < MAX_FILE_BYTES / 8, "{} bytes", bytes.len());
        assert_eq!(parse_theme(&bytes).unwrap(), t);
    }

    // -- the parts of a theme are objects --

    #[test]
    fn the_parts_of_a_theme_must_be_objects() {
        // serde reads a struct from a JSON list too, field by field, so each of these lists
        // would parse as it is. The format and its published schema say object.
        let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
        let by_position = |part: &str, fields: &[&str]| -> Value {
            Value::Array(fields.iter().map(|f| fixture[part][*f].clone()).collect())
        };
        let colors = by_position(
            "colors",
            &[
                "background",
                "foreground",
                "cursor",
                "selection",
                "accent",
                "muted",
                "frame",
                "ansi",
            ],
        );
        let terminal = by_position(
            "terminal",
            &[
                "font",
                "fontSize",
                "cursorShape",
                "opacity",
                "acrylic",
                "padding",
            ],
        );
        let prompt = by_position("prompt", &["layout", "segments", "transient"]);
        let banner = json!([true, "t", [], false, false]); // the example from the review
        let simple = by_position("simple", &["accent", "tint"]);
        // the premise: serde does take each of them
        assert!(serde_json::from_value::<Colors>(colors.clone()).is_ok());
        assert!(serde_json::from_value::<TerminalSettings>(terminal.clone()).is_ok());
        assert!(serde_json::from_value::<PromptSettings>(prompt.clone()).is_ok());
        assert!(serde_json::from_value::<BannerSettings>(banner.clone()).is_ok());
        assert!(serde_json::from_value::<SimpleRecipe>(simple.clone()).is_ok());
        for (key, list) in [
            ("colors", colors),
            ("terminal", terminal),
            ("prompt", prompt),
            ("banner", banner.clone()),
        ] {
            for bad in [
                list,
                json!("x"),
                json!(5),
                json!(true),
                json!(null),
                json!([]),
            ] {
                let text = invalid_text_with(|v| v[key] = bad.clone());
                assert_eq!(text, format!("{key}: expected an object"), "{key} = {bad}");
            }
        }
        // `simple` may be an object or null, and may be left out; nothing else
        for bad in [simple, json!("x"), json!(5), json!(true), json!([])] {
            let text = invalid_text_with(|v| v["simple"] = bad.clone());
            assert_eq!(text, "simple: expected an object or null", "simple = {bad}");
        }
        // the message is the usual one, with its usual start
        let error = with(|v| v["banner"] = banner).unwrap_err();
        assert_eq!(
            error.to_string(),
            "not a valid theme file: banner: expected an object"
        );
        assert_eq!(error.code(), "E_THEME_INVALID");
    }

    #[test]
    fn objects_still_parse_and_a_newer_file_is_still_reported_first() {
        // the fixture has all five parts as objects
        assert!(parse_theme(FIXTURE.as_bytes()).is_ok());
        assert!(with(|v| v["simple"] = json!(null)).is_ok());
        // a missing part is serde's to report, and so is an object with the wrong insides
        let missing = message(|v| {
            v.as_object_mut().unwrap().remove("banner");
        });
        assert!(missing.contains("missing field `banner`"), "{missing}");
        let hollow = message(|v| v["banner"] = json!({}));
        assert!(
            hollow.contains("banner: missing field `enabled`"),
            "{hollow}"
        );
        // the version is read before the parts, so a newer file is a newer file
        let newer = with(|v| {
            v["schemaVersion"] = json!(2);
            v["banner"] = json!([true]);
        });
        assert_eq!(newer.unwrap_err(), ThemeError::NewerSchema(2));
    }

    // -- an optional "$schema" line is read and dropped --

    #[test]
    fn a_schema_line_is_accepted_and_ignored() {
        // editors want `"$schema": "<url>"` in the file; it is not data
        let url = "https://example.com/cmdtheme-1.schema.json";
        let t = with(|v| v["$schema"] = json!(url)).unwrap();
        assert_eq!(
            t,
            parse_theme(FIXTURE.as_bytes()).unwrap(),
            "nothing is kept"
        );
        let saved = String::from_utf8(t.to_file_bytes().unwrap()).unwrap();
        assert!(!saved.contains("$schema"), "and none is written: {saved}");
        // its text is dropped unread, so what is in it does not matter
        assert!(with(|v| v["$schema"] = json!("x\u{1b}[31m\u{7}\n".repeat(100))).is_ok());
        assert!(with(|v| v["$schema"] = json!("")).is_ok());
    }

    #[test]
    fn only_a_string_under_exactly_that_key_is_a_schema_line() {
        for (what, bad) in [
            ("a number", json!(5)),
            ("null", json!(null)),
            ("an object", json!({"a": 1})),
            ("a list", json!(["x"])),
            ("a bool", json!(true)),
        ] {
            let text = invalid_text_with(|v| v["$schema"] = bad);
            assert!(text.contains("$schema: invalid type"), "{what}: {text}");
        }
        let text = invalid_text_with(|v| v["$schemaX"] = json!("x"));
        assert!(
            text.contains("$schemaX: unknown field `$schemaX`"),
            "{text}"
        );
        for key in ["schema", "$Schema", "$schema ", "$ref", "$id", "x$schema"] {
            assert!(with(|v| v[key] = json!("x")).is_err(), "{key:?}");
        }
        // a repeated one is a repeated key like any other
        let twice = with_text(|t| {
            t.replacen(
                "{\n",
                "{\n  \"$schema\": \"a\",\n  \"$schema\": \"b\",\n",
                1,
            )
        });
        assert!(
            matches!(twice, Err(ThemeError::Invalid(ref text)) if text.contains("duplicate field `$schema`")),
            "{twice:?}"
        );
    }

    #[test]
    fn the_json_schema_allows_a_schema_line_as_a_string() {
        let schema = serde_json::to_value(schemars::schema_for!(Theme)).unwrap();
        assert_eq!(
            schema.pointer("/properties/$schema/type"),
            Some(&json!("string"))
        );
        let required = schema
            .pointer("/required")
            .and_then(Value::as_array)
            .unwrap();
        assert!(!required.contains(&json!("$schema")), "it is optional");
        assert_eq!(
            schema.pointer("/additionalProperties"),
            Some(&json!(false)),
            "everything else is still refused"
        );
    }

    // -- saving --

    #[test]
    fn saving_refuses_a_theme_that_does_not_validate() {
        let good = parse_theme(FIXTURE.as_bytes()).unwrap();
        assert!(good.to_file_bytes().is_ok());
        let mut empty_name = good.clone();
        empty_name.name.clear();
        let mut huge_font = good.clone();
        huge_font.terminal.font_size = 200;
        let mut newer = good.clone();
        newer.schema_version = 2;
        let mut nil_id = good.clone();
        nil_id.id = uuid::Uuid::nil();
        // serde_json writes a NaN as null, which would be a file nobody can read back
        let mut nan_tint = good.clone();
        if let Some(simple) = &mut nan_tint.simple {
            simple.tint = f64::NAN;
        }
        for (what, bad, want) in [
            (
                "an empty name",
                empty_name,
                field_err("name", "must be 1–64 characters"),
            ),
            (
                "a font size of 200",
                huge_font,
                field_err("terminal.fontSize", "must be 8–32"),
            ),
            ("a newer schema", newer, ThemeError::NewerSchema(2)),
            (
                "the nil id",
                nil_id,
                field_err(
                    "id",
                    "must be a version 4 UUID (the third group starts with 4)",
                ),
            ),
            (
                "a tint that is not a number",
                nan_tint,
                field_err("simple.tint", "must be 0–0.30"),
            ),
        ] {
            assert_eq!(bad.to_file_bytes().unwrap_err(), want, "{what}");
            assert_eq!(
                bad.validate().unwrap_err(),
                want,
                "{what}: validate says the same"
            );
        }
    }

    // -- a repeated key is refused, and errors say where --

    #[test]
    fn repeated_keys_are_refused() {
        // `serde_json::Value` keeps the last of two equal keys, so a parse that went through it
        // would take the second one and never know about the first
        for (what, edited, key) in [
            (
                "name",
                FIXTURE.replacen(
                    "\"name\": \"Terracotta ของฉัน\",",
                    "\"name\": \"First\",\n  \"name\": \"Second\",",
                    1,
                ),
                "name",
            ),
            (
                "a nested title",
                FIXTURE.replacen(
                    "\"title\": \"Welcome back, {user}!\",",
                    "\"title\": \"one\",\n    \"title\": \"two\",",
                    1,
                ),
                "title",
            ),
            (
                "the version, a newer one first",
                FIXTURE.replacen(
                    "\"schemaVersion\": 1,",
                    "\"schemaVersion\": 7,\n  \"schemaVersion\": 1,",
                    1,
                ),
                "schemaVersion",
            ),
        ] {
            assert_ne!(edited, FIXTURE, "{what}: the edit changed nothing");
            let text = invalid_text(edited.as_bytes());
            assert!(
                text.contains(&format!("duplicate field `{key}`")),
                "{what}: {text}"
            );
        }
        // the other order is read as the newer file it claims to be
        let one_then_seven = FIXTURE.replacen(
            "\"schemaVersion\": 1,",
            "\"schemaVersion\": 1,\n  \"schemaVersion\": 7,",
            1,
        );
        assert_eq!(
            parse_theme(one_then_seven.as_bytes()).unwrap_err(),
            ThemeError::NewerSchema(7)
        );
    }

    #[test]
    fn a_bad_color_is_quoted_as_written_and_still_cleaned() {
        // Thai letters come out as they are, not as `\u{..}` escapes...
        let thai = FIXTURE.replacen("\"#e3b66b\"", "\"#สีส้ม\"", 1);
        let text = invalid_text(thai.as_bytes());
        assert!(
            text.starts_with("colors.ansi[3]: not a #rrggbb color: \"#สีส้ม\""),
            "{text}"
        );
        assert!(!text.contains("\\u{"), "{text}");
        // ...and an escape sequence in the value never reaches the terminal.
        let hostile = FIXTURE.replacen("\"#e3b66b\"", "\"\\u001b[31m#e3b66\"", 1);
        let text = invalid_text(hostile.as_bytes());
        assert!(
            text.starts_with("colors.ansi[3]: not a #rrggbb color: \"#e3b66\""),
            "{text}"
        );
        assert!(!text.contains('\u{1b}') && !text.contains("\\u{"), "{text}");
    }

    #[test]
    fn errors_carry_line_and_column() {
        let bad = FIXTURE.replacen("\"#e3b66b\"", "\"#e3b66\"", 1);
        assert_ne!(bad, FIXTURE);
        let line = FIXTURE.lines().position(|l| l.contains("#e3b66b")).unwrap() + 1;
        let text = invalid_text(bad.as_bytes());
        assert!(
            text.starts_with("colors.ansi[3]: not a #rrggbb color"),
            "{text}"
        );
        assert!(text.contains(&format!(" at line {line} column ")), "{text}");
        // an unknown key and a repeated key say where, too
        let unknown = FIXTURE.replacen("\"tips\": true,", "\"tips\": true,\n    \"sound\": 1,", 1);
        let text = invalid_text(unknown.as_bytes());
        let line = unknown.lines().position(|l| l.contains("sound")).unwrap() + 1;
        assert!(
            text.starts_with("banner.sound: unknown field `sound`"),
            "{text}"
        );
        assert!(text.contains(&format!(" at line {line} column ")), "{text}");
        // a syntax error already did
        let text = invalid_text(b"{\n  \"schemaVersion\": 1,\n  oops\n}");
        assert!(text.contains(" at line 3 column "), "{text}");
    }

    // -- every limit holds at both of its ends --

    #[test]
    fn every_limit_holds_at_both_ends() {
        // The numbers are written out here on purpose: they are the contract of spec §3.3,
        // while the `limits` constants are how the code says them.
        let text = |c: &str, n: usize| json!(c.repeat(n));
        let cases: Vec<(&str, ThemeError, Vec<Value>, Vec<Value>)> = vec![
            (
                "/name",
                field_err("name", "must be 1–64 characters"),
                vec![text("a", 1), text("a", 64), text("ก", 64), text("👋", 64)],
                vec![
                    json!(""),
                    json!("   "),
                    text("a", 65),
                    text("ก", 65),
                    text("👋", 65),
                ],
            ),
            (
                "/author",
                field_err("author", "must be at most 64 characters"),
                vec![json!(""), text("a", 64), text("ก", 64)],
                vec![text("a", 65), text("ก", 65)],
            ),
            (
                "/simple/tint",
                field_err("simple.tint", "must be 0–0.30"),
                vec![json!(0.0), json!(0.1), json!(0.30)],
                vec![json!(-0.01), json!(0.31), json!(-1.0), json!(1.0)],
            ),
            (
                "/terminal/font",
                field_err("terminal.font", "must be 1–64 characters"),
                vec![text("f", 1), text("f", 64), text("ก", 64)],
                vec![json!(""), json!("  "), text("f", 65), text("ก", 65)],
            ),
            (
                "/terminal/fontSize",
                field_err("terminal.fontSize", "must be 8–32"),
                vec![json!(8), json!(12), json!(32)],
                vec![json!(7), json!(33), json!(0), json!(255)],
            ),
            (
                "/terminal/opacity",
                field_err("terminal.opacity", "must be 30–100"),
                vec![json!(30), json!(96), json!(100)],
                vec![json!(29), json!(101), json!(0), json!(255)],
            ),
            (
                "/terminal/padding",
                field_err("terminal.padding", "each value must be 0–40"),
                vec![
                    json!([0, 0]),
                    json!([40, 40]),
                    json!([0, 40]),
                    json!([40, 0]),
                ],
                vec![
                    json!([41, 0]),
                    json!([0, 41]),
                    json!([41, 41]),
                    json!([255, 255]),
                ],
            ),
            (
                "/banner/title",
                field_err("banner.title", "must be at most 80 characters"),
                vec![json!(""), text("t", 80), text("ก", 80), text("👋", 80)],
                vec![text("t", 81), text("ก", 81), text("👋", 81)],
            ),
            (
                "/banner/lines",
                field_err("banner.lines", "at most 4 lines of at most 80 characters"),
                vec![
                    json!([]),
                    json!(["a", "b", "c", "d"]),
                    json!(["", "", "", ""]),
                    json!([text("l", 80)]),
                    json!([text("ก", 80), text("👋", 80)]),
                ],
                vec![
                    json!(["a", "b", "c", "d", "e"]),
                    json!(["", "", "", "", ""]),
                    json!([text("l", 81)]),
                    json!(["ok", text("ก", 81)]),
                    json!([text("👋", 81)]),
                ],
            ),
        ];
        for (pointer, refused_with, inside, outside) in cases {
            for value in inside {
                assert!(
                    at(pointer, value.clone()).is_ok(),
                    "{pointer} should take {value}"
                );
            }
            for value in outside {
                assert_eq!(
                    at(pointer, value.clone()).unwrap_err(),
                    refused_with,
                    "{pointer} should refuse {value}"
                );
            }
        }
    }

    #[test]
    fn a_theme_without_simple_inputs_has_no_tint_to_check() {
        assert!(with(|v| v["simple"] = json!(null)).is_ok());
        assert!(with(|v| {
            v.as_object_mut().unwrap().remove("simple");
        })
        .is_ok());
    }

    #[test]
    fn the_parser_enforces_exactly_what_the_schema_states() {
        // The limits are written once, as the `limits` constants, and the schema attributes and
        // validate() both read them. This reads the numbers back out of the generated schema and
        // holds the parser to them, so a number typed into only one of the two places fails here.
        let schema = serde_json::to_value(schemars::schema_for!(Theme)).unwrap();
        let bound = |pointer: String| -> f64 {
            schema
                .pointer(&pointer)
                .and_then(Value::as_f64)
                .unwrap_or_else(|| panic!("no number at {pointer}"))
        };
        let font = "/$defs/TerminalSettings/properties/font";
        let title = "/$defs/BannerSettings/properties/title";
        // text: the schema's longest is taken and one more is refused
        for (edit, schema_at) in [
            ("/name", "/properties/name"),
            ("/author", "/properties/author"),
            ("/terminal/font", font),
            ("/banner/title", title),
        ] {
            let max = bound(format!("{schema_at}/maxLength")) as usize;
            assert!(at(edit, json!("a".repeat(max))).is_ok(), "{edit}: {max}");
            assert!(
                at(edit, json!("a".repeat(max + 1))).is_err(),
                "{edit}: {}",
                max + 1
            );
        }
        // and the shortest, where the schema has one
        for (edit, schema_at) in [("/name", "/properties/name"), ("/terminal/font", font)] {
            let min = bound(format!("{schema_at}/minLength")) as usize;
            assert!(at(edit, json!("a".repeat(min))).is_ok(), "{edit}: {min}");
            assert!(
                at(edit, json!("a".repeat(min - 1))).is_err(),
                "{edit}: {}",
                min - 1
            );
        }
        // whole numbers
        for (edit, schema_at) in [
            (
                "/terminal/fontSize",
                "/$defs/TerminalSettings/properties/fontSize",
            ),
            (
                "/terminal/opacity",
                "/$defs/TerminalSettings/properties/opacity",
            ),
        ] {
            let min = bound(format!("{schema_at}/minimum")) as u64;
            let max = bound(format!("{schema_at}/maximum")) as u64;
            assert!(at(edit, json!(min)).is_ok(), "{edit}: {min}");
            assert!(at(edit, json!(max)).is_ok(), "{edit}: {max}");
            assert!(at(edit, json!(min - 1)).is_err(), "{edit}: {}", min - 1);
            assert!(at(edit, json!(max + 1)).is_err(), "{edit}: {}", max + 1);
        }
        // padding: the bound is for each of the two values
        let padding = "/terminal/padding";
        let top =
            bound("/$defs/TerminalSettings/properties/padding/items/maximum".to_string()) as u64;
        assert!(at(padding, json!([top, 0])).is_ok() && at(padding, json!([0, top])).is_ok());
        assert!(at(padding, json!([top + 1, 0])).is_err());
        assert!(at(padding, json!([0, top + 1])).is_err());
        // tint
        let tint = "/$defs/SimpleRecipe/properties/tint";
        let (lo, hi) = (
            bound(format!("{tint}/minimum")),
            bound(format!("{tint}/maximum")),
        );
        assert!(at("/simple/tint", json!(lo)).is_ok() && at("/simple/tint", json!(hi)).is_ok());
        assert!(at("/simple/tint", json!(lo - 0.01)).is_err());
        assert!(at("/simple/tint", json!(hi + 0.01)).is_err());
        // banner lines: how many, and how long
        let lines = "/$defs/BannerSettings/properties/lines";
        let count = bound(format!("{lines}/maxItems")) as usize;
        let long = bound(format!("{lines}/items/maxLength")) as usize;
        assert!(at("/banner/lines", json!(vec!["a"; count])).is_ok());
        assert!(at("/banner/lines", json!(vec!["a"; count + 1])).is_err());
        assert!(at("/banner/lines", json!(["a".repeat(long)])).is_ok());
        assert!(at("/banner/lines", json!(["a".repeat(long + 1)])).is_err());
    }

    // -- an id is a version 4 UUID in its canonical form --

    const ID: &str = "2f0c8f0e-6a8e-4b7e-9a4f-3f1f0b9c1d2e";

    #[test]
    fn ids_are_accepted_only_in_the_canonical_form() {
        // lower, upper and mixed case, and one that was just made; saved in lower case
        let fresh = uuid::Uuid::new_v4().to_string();
        for id in [
            ID.to_string(),
            ID.to_uppercase(),
            "2f0C8F0e-6A8e-4B7e-9a4F-3f1F0b9C1d2E".to_string(),
            fresh,
        ] {
            let t = with(|v| v["id"] = json!(id)).unwrap();
            assert_eq!(t.id.to_string(), id.to_lowercase());
        }
        // the other ways to write a UUID, and strings that only look like one
        let compact = ID.replace('-', "");
        let short: String = ID.chars().take(35).collect();
        for (what, id) in [
            ("braced", format!("{{{ID}}}")),
            ("a urn", format!("urn:uuid:{ID}")),
            ("without hyphens", compact.clone()),
            ("without hyphens, upper case", compact.to_uppercase()),
            ("with spaces around", format!(" {ID} ")),
            ("with a newline", format!("{ID}\n")),
            ("a digit short", short),
            ("a digit long", format!("{ID}0")),
            (
                "hyphens in the wrong places",
                "2f0c8f0e6-a8e-4b7e-9a4f-3f1f0b9c1d2e".to_string(),
            ),
            ("a letter that is no digit", ID.replace('f', "g")),
            ("underscores", ID.replace('-', "_")),
            (
                "one wide character for two digits",
                ID.replacen("2f", "é", 1),
            ),
            ("empty", String::new()),
        ] {
            let text = invalid_text_with(|v| v["id"] = json!(id));
            assert!(
                text.contains("id: must be 8-4-4-4-12 hexadecimal digits"),
                "{what}: {text}"
            );
            assert_clean(&text);
        }
        // not a string at all
        for bad in [json!(5), json!(null), json!(["x"])] {
            let text = invalid_text_with(|v| v["id"] = bad);
            assert!(text.starts_with("id: invalid type"), "{text}");
        }
    }

    #[test]
    fn ids_must_be_version_4_with_the_rfc_4122_variant() {
        let version = field_err(
            "id",
            "must be a version 4 UUID (the third group starts with 4)",
        );
        let variant = field_err(
            "id",
            "must be an RFC 4122 UUID (the fourth group starts with 8, 9, a or b)",
        );
        for (what, id) in [
            ("nil", "00000000-0000-0000-0000-000000000000"),
            ("v1", "6ba7b810-9dad-11d1-80b4-00c04fd430c8"),
            ("v3", "9073926b-929f-31c2-abc9-fad77ae3e8eb"),
            ("v5", "886313e1-3b8a-5372-9b90-0c9aee199e5d"),
            ("v7", "017f22e2-79b0-7cc3-98c4-dc0c0c07398f"),
            ("max", "ffffffff-ffff-ffff-ffff-ffffffffffff"),
        ] {
            assert_eq!(
                with(|v| v["id"] = json!(id)).unwrap_err(),
                version,
                "{what}"
            );
        }
        // every version digit with every variant digit: only 4 with 8, 9, a or b gets in,
        // and the version is the first thing said when both are wrong
        for version_digit in "0123456789abcdef".chars() {
            for variant_digit in "0123456789abcdefABCDEF".chars() {
                let id =
                    format!("2f0c8f0e-6a8e-{version_digit}b7e-{variant_digit}a4f-3f1f0b9c1d2e");
                let got = with(|v| v["id"] = json!(id)).map(|_| ());
                let want = if version_digit != '4' {
                    Err(version.clone())
                } else if "89abAB".contains(variant_digit) {
                    Ok(())
                } else {
                    Err(variant.clone())
                };
                assert_eq!(got, want, "{id}");
            }
        }
    }

    #[test]
    fn a_theme_built_in_code_is_held_to_the_id_rules_too() {
        let mut t = parse_theme(FIXTURE.as_bytes()).unwrap();
        t.id = uuid::Uuid::new_v4();
        assert_eq!(t.validate(), Ok(()));
        t.id = uuid::Uuid::nil();
        assert!(matches!(
            t.validate(),
            Err(ThemeError::Field { field: "id", .. })
        ));
        // right version, wrong variant
        t.id = uuid::Uuid::from_u128(0x2f0c_8f0e_6a8e_4b7e_7a4f_3f1f_0b9c_1d2e);
        assert_eq!(
            t.validate(),
            Err(field_err(
                "id",
                "must be an RFC 4122 UUID (the fourth group starts with 8, 9, a or b)"
            ))
        );
    }

    #[test]
    fn the_schema_states_the_id_pattern_and_the_parser_agrees_with_it() {
        use proptest::strategy::{Strategy, ValueTree};
        let schema = serde_json::to_value(schemars::schema_for!(Theme)).unwrap();
        let pattern = schema
            .pointer("/properties/id/pattern")
            .and_then(Value::as_str)
            .unwrap();
        assert_eq!(
            pattern,
            r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-4[0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"
        );
        assert_eq!(
            schema.pointer("/properties/id/format"),
            Some(&json!("uuid"))
        );
        // every string the pattern describes is an id the parser takes
        let body = pattern
            .strip_prefix('^')
            .and_then(|p| p.strip_suffix('$'))
            .unwrap();
        let strategy = proptest::string::string_regex(body).unwrap();
        let mut runner = proptest::test_runner::TestRunner::deterministic();
        for _ in 0..500 {
            let id = strategy.new_tree(&mut runner).unwrap().current();
            assert!(with(|v| v["id"] = json!(id.clone())).is_ok(), "{id}");
        }
    }
}
