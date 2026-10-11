//! Hostile theme files (spec §7: "a schema test with a hostile sample file for every item of
//! §3.7").
//!
//! `tests/fixtures/hostile/` holds one `.cmdtheme` file for each way a shared theme file can try
//! to do harm, or to get past the strict reader. Nearly all of them are the Terracotta example
//! (`tests/fixtures/terracotta-mine.cmdtheme`) with one change made to it. The table in
//! `sample_tests!` states, next to the name of each file, what it must come to, and makes one
//! test of the row. The tests after the table look closer at the files that are accepted. A new
//! sample is a new file and a new row. The file that is over 64 KB is made by its test and not
//! kept in the repo.
//!
//! What the files try, by the numbers of spec §3.7 (and §3.3 for the strictness of the reader):
//! 1. A theme is data and nothing in it runs: `lua-breakers`, whose lines would set a global
//!    in the script if any of them got out of its string.
//! 2. No Oh My Posh config, and no `command` segment: `command-segment`,
//!    `omp-config-injected` and `omp-config-as-theme`.
//! 3. Every text is cleaned of control characters, escape sequences, line breaks and direction
//!    controls: `escape-in-name`, `escape-in-text-fields`, `newline-in-banner` and `bidi-name`.
//! 4. Lua strings are escaped in one place: `lua-breakers`, run in Lua 5.2.
//! 5. At most 64 KB, and a file that is not JSON or does not fit the format is refused, with
//!    the field that is wrong: `malformed-json`, `bad-color` and `bad-id`, and the files that
//!    `a_file_over_64_kb_is_refused_and_one_of_exactly_64_kb_is_not` makes.
//!
//! The strictness of the format (§3.3): `unknown-field`, `hostile-key`, `duplicate-key`,
//! `banner-as-array` and `newer-schema` are refused; `schema-line` is the one extra key that is
//! allowed.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use cmdstudio_core::generate::lua::{banner_lua, Lang};
use cmdstudio_core::generate::{clink, omp, wt};
use cmdstudio_core::model::{parse_theme, Layout, Segment, Target, Theme, ThemeError};
use mlua::{Function, Lua, Table, Value as LuaValue};
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

// ---- the table of samples ----

/// What a sample must come to.
enum Outcome {
    /// Read and kept. `check_sample` holds it to what every accepted theme must satisfy, and
    /// a test of its own says what was cleaned out of it.
    Accepted,
    /// Refused with `E_THEME_INVALID` by the reader. The message starts with the path of the
    /// field that is wrong (`colors.ansi[3]`) and goes on to say what is wrong with it. When
    /// the fault has no path, because it is in the text as a whole or in a key at the top, the
    /// message starts with what is wrong.
    Invalid(&'static str),
    /// Refused with `E_THEME_INVALID` by a rule of the format, which names the field and the
    /// reason.
    Field(&'static str, &'static str),
    /// Refused as written by a newer CMD Studio (`E_THEME_NEWER_SCHEMA`), with this version.
    Newer(u64),
}

use Outcome::{Accepted, Field, Invalid, Newer};

/// One test for each sample: `test_name: "name of the file, without .cmdtheme" => outcome;`.
/// The names of the files are also collected in `SAMPLES`, for the test that looks for a file
/// with no row and a row with no file.
macro_rules! sample_tests {
    ($($test:ident: $file:literal => $outcome:expr;)*) => {
        $(
            #[test]
            fn $test() -> mlua::Result<()> {
                check_sample($file, $outcome)
            }
        )*

        /// The samples that have a row.
        const SAMPLES: &[&str] = &[$($file),*];
    };
}

sample_tests! {
    // -- §3.7 #2: the prompt is built from the whitelist, never from the file --

    // "segments": ["path", "command"]
    command_segment: "command-segment" =>
        Invalid("prompt.segments[1]: unknown variant `command`");
    // an Oh My Posh `blocks` list with a `command` segment in it, under `prompt`
    omp_config_injected: "omp-config-injected" =>
        Invalid("prompt.blocks: unknown field `blocks`");
    // an Oh My Posh config with a `command` segment, renamed to .cmdtheme
    omp_config_as_theme: "omp-config-as-theme" =>
        Field("schemaVersion", "missing or not a whole number");

    // -- §3.7 #3: texts are cleaned, in every field that is text --

    // ESC[31m, a bell and an OSC title sequence in `name`
    escape_in_name: "escape-in-name" => Accepted;
    // the same in `author`, `font`, the banner title and a banner line
    escape_in_text_fields: "escape-in-text-fields" => Accepted;
    // \n, \r, U+2028 and U+2029 in the banner
    newline_in_banner: "newline-in-banner" => Accepted;
    // U+202E in `name`
    bidi_name: "bidi-name" => Accepted;

    // -- §3.7 #1 and #4: the text is data, and Lua keeps it as text --

    // ]] [==[ \ " ' Thai and emoji in the banner, and lines that would run code if they got out
    lua_breakers: "lua-breakers" => Accepted;

    // -- §3.3: the reader is strict --

    unknown_field: "unknown-field" => Invalid("hooks: unknown field `hooks`");
    // an unknown key that has an escape sequence in it and is 400 letters long
    hostile_key: "hostile-key" => Invalid("red0;pwnedkkkk");
    // two `name` keys: a reader that keeps the last one would see a different name. The key is
    // at the top, so the path is empty and the message starts with the field itself.
    duplicate_key: "duplicate-key" => Invalid("duplicate field `name` at line 5 column");
    // the five parts of `banner` as a list, which serde would read by position
    banner_as_array: "banner-as-array" => Invalid("banner: expected an object");
    // schemaVersion 2, with a field of its own that this version does not know
    newer_schema: "newer-schema" => Newer(2);
    // "#12345" in ansi[3]
    bad_color: "bad-color" => Invalid("colors.ansi[3]: not a #rrggbb color");
    // an id in braces
    bad_id: "bad-id" => Invalid("id: must be 8-4-4-4-12 hexadecimal digits");

    // -- §3.7 #5: a file that is not JSON is refused --

    // the file ends in the middle of the `banner` object, as a download that stopped would
    malformed_json: "malformed-json" => Invalid("EOF while parsing");

    // -- §3.3: the one extra key that is allowed --

    schema_line: "schema-line" => Accepted;
}

// ---- reading the samples ----

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

fn hostile_dir() -> PathBuf {
    fixtures().join("hostile")
}

/// The Terracotta example, which the samples are made from.
fn base_text() -> String {
    std::fs::read_to_string(fixtures().join("terracotta-mine.cmdtheme")).unwrap()
}

fn sample_bytes(name: &str) -> Vec<u8> {
    let path = hostile_dir().join(format!("{name}.cmdtheme"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("cannot read sample {name}: {e}"))
}

/// The theme a sample comes to, which the test knows to be accepted.
fn accepted(name: &str) -> Theme {
    parse_theme(&sample_bytes(name))
        .unwrap_or_else(|e| panic!("sample {name} should be accepted, but: {e:?}"))
}

// ---- what is unsafe, and what a refusal may say ----

/// What may not stay in a text from a file, by the list of spec §3.7 #3: the control characters
/// (U+0000 to U+001F and U+007F to U+009F), the line and paragraph separators, and the
/// bidirectional embeddings, overrides and isolates.
fn is_unsafe(c: char) -> bool {
    matches!(
        u32::from(c),
        0x00..=0x1F | 0x7F..=0x9F | 0x2028 | 0x2029 | 0x202A..=0x202E | 0x2066..=0x2069
    )
}

/// The longest refusal: spec §3.3 allows a message 300 characters, and `E_THEME_INVALID` puts
/// "not a valid theme file: " (24 characters) in front of it.
const MAX_REFUSAL_CHARS: usize = 300 + 24;

/// A refusal is printed on a screen and written to a log, and it can quote the file.
fn assert_safe_to_print(name: &str, error: &ThemeError) {
    let shown = error.to_string();
    assert!(
        !shown.chars().any(is_unsafe),
        "{name}: an unsafe character in {shown:?}"
    );
    let chars = shown.chars().count();
    assert!(
        chars <= MAX_REFUSAL_CHARS,
        "{name}: {chars} characters in {shown:?}"
    );
}

/// Checks a refusal against its row.
fn assert_refusal(name: &str, want: Outcome, got: &ThemeError) {
    match (want, got) {
        (Invalid(starts), ThemeError::Invalid(text)) => {
            assert!(
                text.starts_with(starts),
                "{name}: the message {text:?} should start with {starts:?}"
            );
            assert_eq!(got.code(), "E_THEME_INVALID", "{name}");
        }
        (
            Field(field, reason),
            ThemeError::Field {
                field: f,
                reason: r,
            },
        ) => {
            assert_eq!((*f, *r), (field, reason), "{name}");
            assert_eq!(got.code(), "E_THEME_INVALID", "{name}");
        }
        (Newer(version), ThemeError::NewerSchema(v)) => {
            assert_eq!(*v, version, "{name}");
            assert_eq!(got.code(), "E_THEME_NEWER_SCHEMA", "{name}");
        }
        _ => panic!("{name}: refused, but not as its row says: {got:?}"),
    }
    assert_safe_to_print(name, got);
}

// ---- running the generators over a theme ----

/// All seven segments, so that every segment type of the generator is drawn.
fn every_segment() -> Vec<Segment> {
    vec![
        Segment::Path,
        Segment::Git,
        Segment::Time,
        Segment::User,
        Segment::Host,
        Segment::Status,
        Segment::Duration,
    ]
}

/// Every string in `v`, keys included, as the JSON holds it once it is read (so an escape
/// character is one character here, not the six of `\u001b`).
fn json_strings(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) => out.push(s.clone()),
        Value::Array(items) => items.iter().for_each(|x| json_strings(x, out)),
        Value::Object(map) => map.iter().for_each(|(k, x)| {
            out.push(k.clone());
            json_strings(x, out);
        }),
        _ => {}
    }
}

/// The color sequences the banner writes itself: the accent and muted colors of Terracotta,
/// and the reset. Every sample has Terracotta's colors.
const OUR_COLORS: [&str; 3] = [
    "\u{1b}[38;2;217;119;87m",
    "\u{1b}[38;2;138;135;128m",
    "\u{1b}[0m",
];

/// The rows of a printed box with those sequences taken off. An escape character that is left
/// in a row came from somewhere else.
fn plain_rows(printed: &str) -> Vec<String> {
    let mut plain = printed.to_string();
    for color in OUR_COLORS {
        plain = plain.replace(color, "");
    }
    plain.split('\n').map(String::from).collect()
}

/// Stand-ins for the parts of Clink and Windows that `banner.lua` uses, so that it runs in
/// plain Lua 5.2. `tests/banner_lua.rs` has the same ones; every file under `tests/` is a
/// crate of its own, so this file keeps a copy.
const PRELUDE: &str = r#"
__out = {}
__env = {}
__lines = {}
NONL = {}
clink = {
  print = function(s, flag)
    if flag == NONL then s = "<NONL>" .. s end
    __out[#__out + 1] = s
  end,
  oninject = function(f) __oninject = f end,
}
console = {
  cellcount = function(s)
    local plain = s:gsub("\27%[[%d;]*m", "")
    local _, n = plain:gsub("[^\128-\191]", "")
    return n
  end,
  getlinetext = function(n) return __lines[n] end,
}
os.getenv = function(k)
  if __env[k] ~= nil then return __env[k] end
  if k == "USERNAME" then return "ทดสอบ" end
  if k == "COMPUTERNAME" then return "PC-01" end
  return nil
end
os.setenv = function(k, v) __env[k] = v end
os.getcwd = function() return "C:\\Users\\ทดสอบ" end
log = { info = function() end }
"#;

/// The user name that the stand-in `os.getenv` gives for `{user}`.
const STAND_IN_USER: &str = "ทดสอบ";

/// Loads the banner script of `theme`, lets the session start, and returns the Lua state and
/// what the script printed with `clink.print`.
fn run_banner(theme: &Theme, lang: Lang) -> mlua::Result<(Lua, Vec<String>)> {
    let lua = Lua::new();
    lua.load(PRELUDE).exec()?;
    lua.load(banner_lua(theme, lang))
        .set_name("banner.lua")
        .exec()?;
    let on_inject: Function = lua.globals().get("__oninject")?;
    on_inject.call::<()>(())?;
    let out: Table = lua.globals().get("__out")?;
    let printed = out
        .sequence_values::<String>()
        .collect::<mlua::Result<_>>()?;
    Ok((lua, printed))
}

/// The box that the banner of `theme` prints, as its rows without our colors.
fn banner_rows(theme: &Theme, lang: Lang) -> mlua::Result<Vec<String>> {
    let (_, printed) = run_banner(theme, lang)?;
    assert_eq!(printed.len(), 1, "one print, and no clearing of the screen");
    Ok(plain_rows(&printed[0]))
}

/// Everything the generators write for `theme`, with a label for each, as the text of the
/// file.
fn generated_files(theme: &Theme) -> Vec<(String, String)> {
    let mut files = vec![(
        "theme.omp.json (the theme's own layout)".to_string(),
        omp::config_text(theme).unwrap(),
    )];
    for layout in [Layout::Box, Layout::Line, Layout::Powerline] {
        let mut every = theme.clone();
        every.prompt.layout = layout;
        every.prompt.segments = every_segment();
        files.push((
            format!("theme.omp.json ({layout:?}, every segment)"),
            omp::config_text(&every).unwrap(),
        ));
    }
    for lang in [Lang::Th, Lang::En] {
        files.push((format!("banner.lua ({lang:?})"), banner_lua(theme, lang)));
    }
    files.push((
        "clink_settings".to_string(),
        clink::settings(theme.prompt.transient, Path::new(r"C:\data\generated")),
    ));
    files.push((
        "the Windows Terminal scheme".to_string(),
        wt::scheme(theme).to_string(),
    ));
    for target in [Target::All, Target::Wt] {
        let keys = wt::profile_values(
            theme,
            target,
            Path::new(r"C:\data\runtime\clink-1.9.34\clink_x64.exe"),
            Path::new(r"C:\data\clink-profile"),
        );
        files.push((
            format!("the Windows Terminal profile ({target:?})"),
            Value::Object(keys).to_string(),
        ));
    }
    files
}

/// What holds for every theme that was accepted, whatever its file said: its texts are clean,
/// it passes `validate` and comes back as it is from a saved file, and nothing unsafe is in
/// what the generators write for it, or in what its banner prints.
fn assert_a_clean_theme(name: &str, theme: &Theme) -> mlua::Result<()> {
    let banner = &theme.banner;
    let mut texts = vec![
        ("name", &theme.name),
        ("author", &theme.author),
        ("terminal.font", &theme.terminal.font),
        ("banner.title", &banner.title),
    ];
    texts.extend(banner.lines.iter().map(|line| ("banner.lines", line)));
    for (field, text) in texts {
        assert!(
            !text.chars().any(is_unsafe),
            "{name}: {field} keeps an unsafe character: {text:?}"
        );
    }
    theme
        .validate()
        .unwrap_or_else(|e| panic!("{name}: does not validate: {e:?}"));
    let saved = theme.to_file_bytes().unwrap();
    assert_eq!(
        &parse_theme(&saved).unwrap(),
        theme,
        "{name}: not the same after it is saved and read again"
    );

    for (label, text) in generated_files(theme) {
        // an escape character or a bell, as the file holds them
        assert!(
            !text.chars().any(|c| is_unsafe(c) && c != '\n'),
            "{name}: an unsafe character in {label}"
        );
        // and as a JSON file means them: `\u001b` is one escape character
        if let Ok(json) = serde_json::from_str::<Value>(&text) {
            let mut strings = Vec::new();
            json_strings(&json, &mut strings);
            assert!(
                !strings.iter().any(|s| s.chars().any(is_unsafe)),
                "{name}: an unsafe character in a string of {label}"
            );
        }
    }
    // and what the banner prints is the box, with nothing in it for the terminal to act on
    for lang in [Lang::Th, Lang::En] {
        for row in banner_rows(theme, lang)? {
            assert!(
                !row.chars().any(is_unsafe),
                "{name}: the {lang:?} banner prints an unsafe character in {row:?}"
            );
        }
    }
    Ok(())
}

// ---- the tests ----

/// The test of one row of the table.
fn check_sample(name: &str, want: Outcome) -> mlua::Result<()> {
    match (parse_theme(&sample_bytes(name)), want) {
        (Ok(theme), Accepted) => assert_a_clean_theme(name, &theme)?,
        (Ok(_), _) => panic!("{name}: should be refused, but it was accepted"),
        (Err(e), Accepted) => panic!("{name}: should be accepted, but: {e:?}"),
        (Err(e), want) => assert_refusal(name, want, &e),
    }
    Ok(())
}

#[test]
fn every_file_in_the_hostile_folder_has_a_row_and_every_row_a_file() {
    let rows: BTreeSet<&str> = SAMPLES.iter().copied().collect();
    assert_eq!(rows.len(), SAMPLES.len(), "a sample is in the table twice");
    let files: BTreeSet<String> = std::fs::read_dir(hostile_dir())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    let wanted: BTreeSet<String> = rows.iter().map(|name| format!("{name}.cmdtheme")).collect();
    let without_row: Vec<_> = files.difference(&wanted).collect();
    let without_file: Vec<_> = wanted.difference(&files).collect();
    assert!(
        without_row.is_empty() && without_file.is_empty(),
        "files with no row in the table: {without_row:?}; rows with no file: {without_file:?}"
    );
}

#[test]
fn the_file_the_samples_are_made_from_is_accepted() {
    // so that a refusal in the table is the doing of the change made to it
    parse_theme(base_text().as_bytes()).unwrap();
}

// -- §3.7 #3: texts are cleaned --

#[test]
fn an_escape_sequence_in_the_name_is_cut_out_and_never_reaches_a_generated_file() {
    let theme = accepted("escape-in-name");
    // ESC[31m, ESC[0m and the bell are gone, and so are the escape character and `]` of the
    // title sequence; the `0;pwned` that was its text is only text now
    assert_eq!(theme.name, "Red 0;pwnedTheme");
    assert!(!theme.name.chars().any(is_unsafe));
    for lang in [Lang::Th, Lang::En] {
        let lua = banner_lua(&theme, lang);
        assert!(
            !lua.contains('\u{1b}') && !lua.contains('\u{7}'),
            "{lang:?}"
        );
    }
    let config = omp::config_text(&theme).unwrap();
    assert!(!config.contains('\u{1b}') && !config.contains('\u{7}'));
    assert!(
        !config.contains("pwned"),
        "the name is not in the config at all"
    );
}

#[test]
fn every_text_field_is_cleaned_and_the_font_reaches_windows_terminal_clean() -> mlua::Result<()> {
    let theme = accepted("escape-in-text-fields");
    assert_eq!(theme.author, "clear");
    assert_eq!(theme.terminal.font, "Cascadia Mono");
    assert_eq!(theme.banner.title, "0;pwnedHi {user}");
    assert_eq!(theme.banner.lines, ["red", "bell and tab and CR."]);
    // the font is the one text of these that a generator writes, to the cmd profile
    let keys = wt::profile_values(
        &theme,
        Target::All,
        Path::new(r"C:\c\clink_x64.exe"),
        Path::new(r"C:\c\p"),
    );
    assert_eq!(keys["font"]["face"], "Cascadia Mono");
    // and the banner prints the cleaned title with the variable in it still working
    for lang in [Lang::Th, Lang::En] {
        let rows = banner_rows(&theme, lang)?;
        assert!(
            rows[1].contains(&format!("0;pwnedHi {STAND_IN_USER}")),
            "{lang:?}: {}",
            rows[1]
        );
        assert!(rows[3].contains("red"), "{lang:?}: {}", rows[3]);
        assert!(
            rows[4].contains("bell and tab and CR."),
            "{lang:?}: {}",
            rows[4]
        );
    }
    Ok(())
}

#[test]
fn line_breaks_in_the_banner_are_cut_out_and_the_box_keeps_its_rows() -> mlua::Result<()> {
    let theme = accepted("newline-in-banner");
    assert_eq!(theme.banner.title, "Welcomeback, {user}!");
    assert_eq!(
        theme.banner.lines,
        ["firstsecond", "carriagereturn", "separator!", "crlfend"]
    );
    for text in std::iter::once(&theme.banner.title).chain(&theme.banner.lines) {
        assert!(
            !text.contains(['\n', '\r', '\u{2028}', '\u{2029}']),
            "{text:?}"
        );
    }
    for lang in [Lang::Th, Lang::En] {
        let rows = banner_rows(&theme, lang)?;
        // top, title, blank, the four lines, bottom, tips and the closing space: a line break
        // that got through would be one row more
        assert_eq!(rows.len(), 10, "{lang:?}: {rows:?}");
        assert!(
            rows[1].contains(&format!("Welcomeback, {STAND_IN_USER}!")),
            "{lang:?}: {}",
            rows[1]
        );
        for (row, line) in rows[3..7].iter().zip(&theme.banner.lines) {
            assert!(row.contains(line.as_str()), "{lang:?}: {row:?} / {line:?}");
        }
    }
    Ok(())
}

#[test]
fn a_direction_control_in_the_name_is_cut_out() {
    let theme = accepted("bidi-name");
    // U+202E would make the end of "Calm\u{202e}txt.exe" read as "exe.txt" in a file manager
    assert_eq!(theme.name, "Calmtxt.exe");
    assert!(!theme.name.contains('\u{202e}'));
}

// -- §3.7 #1 and #4: the text is data, and Lua keeps it as text --

#[test]
fn text_that_breaks_a_lua_string_is_printed_as_it_is_and_never_runs() -> mlua::Result<()> {
    let theme = accepted("lua-breakers");
    // what the file says, spelled out here so that a change to the file shows
    let title = r#"Hi ]] [==[ \ " ' ไทย 👋"#;
    let lines = [
        "\"); __pwned = true; --",
        "']]; __pwned = true; --[[ [==[ ]==]",
        r"C:\Users\นักทดสอบ\",
        concat!(
            "สวัสดี 👋 👨\u{200d}👩\u{200d}👧 ❤\u{fe0f} {user} ",
            r"\\ \n \027 \x41"
        ),
    ];
    assert_eq!(theme.banner.title, title);
    assert_eq!(theme.banner.lines, lines);

    for lang in [Lang::Th, Lang::En] {
        let (lua, printed) = run_banner(&theme, lang)?;
        // nothing of the texts ran: the lines that try to would have set this global
        let pwned: LuaValue = lua.globals().get("__pwned")?;
        assert!(pwned.is_nil(), "{lang:?}: text from the theme ran as code");
        let rows = plain_rows(&printed[0]);
        // the title row has the icon and then the title, and each line has its row after the
        // blank one; the variable is the only part that is not as written
        assert!(rows[1].contains(title), "{lang:?}: {}", rows[1]);
        for (row, line) in rows[3..].iter().zip(lines) {
            let want = line.replace("{user}", STAND_IN_USER);
            assert!(
                row.contains(&want),
                "{lang:?}: {row:?} should hold {want:?}"
            );
        }
    }
    Ok(())
}

// -- §3.3: the extra key that is allowed --

#[test]
fn a_schema_line_is_accepted_ignored_and_not_written_back() {
    let theme = accepted("schema-line");
    assert_eq!(
        theme,
        parse_theme(base_text().as_bytes()).unwrap(),
        "nothing of the line is kept"
    );
    let saved = String::from_utf8(theme.to_file_bytes().unwrap()).unwrap();
    assert!(!saved.contains("$schema"), "{saved}");
}

// -- §3.7 #5: the size of the file --

/// The Terracotta example with this many bytes in all, made by padding with spaces after the
/// closing brace, which JSON allows.
fn padded_to(len: usize) -> Vec<u8> {
    let mut bytes = base_text().into_bytes();
    assert!(bytes.len() <= len);
    bytes.resize(len, b' ');
    bytes
}

#[test]
fn a_file_over_64_kb_is_refused_and_one_of_exactly_64_kb_is_not() {
    const LIMIT: usize = 64 * 1024;
    // one byte over, made of a name that is far too long: the size is judged first
    let base = base_text();
    let empty_name = base.replacen("Terracotta ของฉัน", "", 1);
    let long_name = "x".repeat(LIMIT + 1 - empty_name.len());
    let huge = base.replacen("Terracotta ของฉัน", &long_name, 1);
    assert_eq!(huge.len(), LIMIT + 1);
    let refused = parse_theme(huge.as_bytes()).unwrap_err();
    assert_eq!(refused, ThemeError::TooLarge);
    assert_eq!(refused.code(), "E_THEME_INVALID");
    assert_safe_to_print("too large", &refused);
    // the same example, padded to one byte over and to exactly the limit
    assert_eq!(
        parse_theme(&padded_to(LIMIT + 1)).unwrap_err(),
        ThemeError::TooLarge
    );
    assert_eq!(
        parse_theme(&padded_to(LIMIT)).unwrap(),
        parse_theme(base.as_bytes()).unwrap()
    );
}

// -- §3.3: what a refusal says when the file is the one that wrote it --

#[test]
fn a_hostile_key_of_400_letters_gives_a_message_that_is_clean_and_cut_short() {
    let error = parse_theme(&sample_bytes("hostile-key")).unwrap_err();
    let ThemeError::Invalid(text) = &error else {
        panic!("not an Invalid refusal: {error:?}");
    };
    // the escape sequence is cut out of the key, the key is the path, and the message is cut
    // at 300 characters with three dots
    assert!(text.starts_with("red0;pwnedkkkk"), "{text}");
    assert_eq!(text.chars().count(), 300, "{text}");
    assert!(text.ends_with("..."), "{text}");
    assert_eq!(error.to_string().chars().count(), MAX_REFUSAL_CHARS);
    assert_safe_to_print("hostile-key", &error);
}

#[test]
fn a_newer_file_is_newer_even_though_it_has_a_field_this_version_does_not_know() {
    // the sample has `schemaVersion` 2 and an `extensions` field of the newer format
    let error = parse_theme(&sample_bytes("newer-schema")).unwrap_err();
    assert_eq!(error, ThemeError::NewerSchema(2));
    assert_eq!(
        error.to_string(),
        "theme file comes from a newer CMD Studio (schemaVersion 2)"
    );
}
