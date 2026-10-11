//! Doctrine guard (spec §7 (ก)): the generated banner.lua does nothing when
//! loaded, prints once per session, and keeps hostile text literal. Runs the
//! real script in Lua 5.2 with small stand-ins for Clink's API. The generated
//! init.lua runs here as well, with a stand-in for `io.popen` (spec §5.4, G17).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use cmdstudio_core::generate::lua::{banner_lua, init_lua, Lang};
use cmdstudio_core::model::Theme;
use cmdstudio_core::presets;
use mlua::{Function, Lua, Table, Value};
use std::path::Path;

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

/// The Terracotta preset, picked by its key. This file is a crate of its own, so it cannot
/// see the test-only helper inside `cmdstudio-core`.
fn terracotta() -> Theme {
    presets::builtin()
        .unwrap()
        .into_iter()
        .find(|(key, _)| *key == "terracotta")
        .unwrap()
        .1
}

fn load(script: &str, screen: &[&str]) -> mlua::Result<Lua> {
    let lua = Lua::new();
    lua.load(PRELUDE).exec()?;
    let lines = lua.create_table()?;
    for (i, l) in screen.iter().enumerate() {
        lines.set(i + 1, *l)?;
    }
    lua.globals().set("__lines", lines)?;
    lua.load(script).set_name("banner.lua").exec()?;
    Ok(lua)
}

fn printed(lua: &Lua) -> mlua::Result<Vec<String>> {
    let out: Table = lua.globals().get("__out")?;
    out.sequence_values::<String>().collect()
}

fn inject(lua: &Lua) -> mlua::Result<()> {
    let f: Function = lua.globals().get("__oninject")?;
    f.call::<()>(())
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut in_seq = false;
    for ch in s.chars() {
        match (in_seq, ch) {
            (false, '\u{1b}') => in_seq = true,
            (true, 'm') => in_seq = false,
            (true, _) => {}
            (false, c) => out.push(c),
        }
    }
    out
}

#[test]
fn loading_the_banner_prints_nothing() -> mlua::Result<()> {
    let lua = load(&banner_lua(&terracotta(), Lang::En), &[])?;
    assert!(printed(&lua)?.is_empty());
    Ok(())
}

#[test]
fn banner_prints_once_per_session() -> mlua::Result<()> {
    let lua = load(&banner_lua(&terracotta(), Lang::En), &[])?;
    inject(&lua)?;
    inject(&lua)?;
    assert_eq!(printed(&lua)?.len(), 1);
    Ok(())
}

#[test]
fn box_lines_line_up_and_the_last_line_is_one_space() -> mlua::Result<()> {
    let lua = load(&banner_lua(&terracotta(), Lang::Th), &[])?;
    inject(&lua)?;
    let out = printed(&lua)?;
    let lines: Vec<&str> = out[0].split('\n').collect();
    let bottom = lines
        .iter()
        .position(|l| strip_ansi(l).starts_with('\u{2570}'))
        .unwrap();
    let widths: Vec<usize> = lines[..=bottom]
        .iter()
        .map(|l| strip_ansi(l).chars().count())
        .collect();
    assert!(widths.windows(2).all(|w| w[0] == w[1]), "{widths:?}");
    assert_eq!(*lines.last().unwrap(), " ");
    assert!(out[0].contains("ทดสอบ"));
    Ok(())
}

#[test]
fn clears_the_cmd_header_only_when_it_is_on_top() -> mlua::Result<()> {
    let header = [
        "Microsoft Windows [Version 10.0.26300.9550]",
        "(c) Microsoft Corporation. All rights reserved.",
    ];
    let lua = load(&banner_lua(&terracotta(), Lang::En), &header)?;
    inject(&lua)?;
    assert!(printed(&lua)?[0].starts_with("<NONL>"));

    let lua = load(
        &banner_lua(&terracotta(), Lang::En),
        &["C:\\>dir", "Microsoft Windows [Version 10.0]"],
    )?;
    inject(&lua)?;
    let out = printed(&lua)?;
    assert_eq!(out.len(), 1);
    assert!(!out[0].starts_with("<NONL>"));
    Ok(())
}

#[test]
fn hostile_titles_stay_literal() -> mlua::Result<()> {
    let mut t = terracotta();
    t.banner.title = r#"a"]]\x {foo} {user}"#.to_string();
    let lua = load(&banner_lua(&t, Lang::En), &[])?;
    inject(&lua)?;
    assert!(printed(&lua)?[0].contains(r#"a"]]\x {foo} ทดสอบ"#));
    Ok(())
}

#[test]
fn thai_dates_use_the_buddhist_year() -> mlua::Result<()> {
    let mut t = terracotta();
    t.banner.lines = vec!["{date}".to_string()];
    let lua = load(&banner_lua(&t, Lang::Th), &[])?;
    let year: i64 = lua.load("return os.date('*t').year + 543").eval()?;
    inject(&lua)?;
    assert!(printed(&lua)?[0].contains(&year.to_string()));
    Ok(())
}

#[test]
fn no_lines_and_no_tips_still_print_a_valid_box() -> mlua::Result<()> {
    // Review Focus 4
    let mut t = terracotta();
    t.banner.lines.clear();
    t.banner.tips = false;
    let lua = load(&banner_lua(&t, Lang::En), &[])?;
    inject(&lua)?;
    let out = printed(&lua)?;
    assert_eq!(strip_ansi(&out[0]).lines().count(), 4);
    Ok(())
}

#[test]
fn disabled_banner_registers_no_callback() -> mlua::Result<()> {
    let mut t = terracotta();
    t.banner.enabled = false;
    let lua = load(&banner_lua(&t, Lang::En), &[])?;
    let cb: Value = lua.globals().get("__oninject")?;
    assert!(cb.is_nil());
    Ok(())
}

// ---- Beyond the eight above: the icon, the dates, the options, text from the theme, init.lua ----

const LANGS: [Lang; 2] = [Lang::Th, Lang::En];

/// What cmd prints on top of a fresh window (spec §3.6, G4).
const CMD_HEADER: [&str; 2] = [
    "Microsoft Windows [Version 10.0.26300.9550]",
    "(c) Microsoft Corporation. All rights reserved.",
];

const ACCENT_SGR: &str = "\u{1b}[38;2;217;119;87m";
const MUTED_SGR: &str = "\u{1b}[38;2;138;135;128m";

/// Sets an environment variable of the stand-in `os.getenv`.
fn set_env(lua: &Lua, key: &str, value: &str) -> mlua::Result<()> {
    let env: Table = lua.globals().get("__env")?;
    env.set(key, value)
}

/// The clock of a script. `os.date` then answers only "*t" (numbers) and "%H:%M" (digits).
/// Any other format would give names in the language of Windows and not of the app, which is
/// what the generator must not depend on (spec §3.6), so the stand-in raises an error.
struct Clock {
    year: u32,
    month: u32,
    day: u32,
    /// 1 is Sunday, as in Lua's `os.date("*t")`.
    wday: u32,
    hour: u32,
    min: u32,
}

impl Clock {
    fn set(&self, lua: &Lua) -> mlua::Result<()> {
        let Clock {
            year,
            month,
            day,
            wday,
            hour,
            min,
        } = self;
        lua.load(format!(
            r#"
            local now = {{ year = {year}, month = {month}, day = {day}, wday = {wday}, hour = {hour}, min = {min} }}
            os.date = function(fmt)
              if fmt == "*t" then return now end
              if fmt == "%H:%M" then return string.format("%02d:%02d", now.hour, now.min) end
              error("os.date(" .. tostring(fmt) .. ") follows the language of Windows")
            end
            "#
        ))
        .exec()
    }
}

/// Friday 9 October 2026 at 16:47, the date that spec §3.6 writes out in both languages.
const FRIDAY: Clock = Clock {
    year: 2026,
    month: 10,
    day: 9,
    wday: 6,
    hour: 16,
    min: 47,
};

/// The rows of the box, with the colors taken off.
fn plain_rows(shown: &str) -> Vec<String> {
    shown.split('\n').map(strip_ansi).collect()
}

/// A theme with these lines and nothing else changed.
fn with_lines(lines: &[&str]) -> Theme {
    let mut t = terracotta();
    t.banner.lines = lines.iter().map(|l| l.to_string()).collect();
    t
}

/// Loads the banner of `theme`, lets `before` set the stage and injects once. Returns what
/// was printed.
fn run(
    theme: &Theme,
    lang: Lang,
    screen: &[&str],
    before: impl FnOnce(&Lua) -> mlua::Result<()>,
) -> mlua::Result<Vec<String>> {
    let lua = load(&banner_lua(theme, lang), screen)?;
    before(&lua)?;
    inject(&lua)?;
    printed(&lua)
}

// -- the icon (spec §3.6) --

#[test]
fn the_icon_is_the_star_glyph_in_windows_terminal() -> mlua::Result<()> {
    // Windows Terminal sets WT_SESSION in every tab, and it falls back to another font
    // for U+273B when the font of the profile has none.
    for lang in LANGS {
        let out = run(&terracotta(), lang, &[], |lua| {
            set_env(lua, "WT_SESSION", "0d4c3ab1-1d2e-4a60-9b6c-6a1f4d9b8e11")
        })?;
        let rows = plain_rows(&out[0]);
        assert!(
            rows[1].starts_with("\u{2502} \u{273b} "),
            "{lang:?}: {}",
            rows[1]
        );
    }
    Ok(())
}

#[test]
fn the_icon_falls_back_to_an_asterisk_outside_windows_terminal() -> mlua::Result<()> {
    // conhost on Windows 10 draws U+273B as an empty box (spec §3.6): without WT_SESSION the
    // glyph must not be printed at all, and the asterisk takes its cell
    for lang in LANGS {
        let out = run(&terracotta(), lang, &[], |_| Ok(()))?;
        let rows = plain_rows(&out[0]);
        assert!(rows[1].starts_with("\u{2502} * "), "{lang:?}: {}", rows[1]);
        assert!(!out[0].contains('\u{273b}'), "{lang:?}");
    }
    Ok(())
}

#[test]
fn the_icon_is_chosen_when_the_box_is_printed_not_when_the_script_loads() -> mlua::Result<()> {
    // `clink set` loads the script too, and the terminal is only known once cmd is running
    let lua = load(&banner_lua(&terracotta(), Lang::En), &[])?;
    set_env(&lua, "WT_SESSION", "x")?;
    inject(&lua)?;
    assert!(printed(&lua)?[0].contains('\u{273b}'));
    Ok(())
}

// -- dates and time (spec §3.6) --

#[test]
fn dates_are_written_in_the_app_language_as_the_spec_shows_them() -> mlua::Result<()> {
    let t = with_lines(&["{date}|{time}"]);
    for (lang, want) in [
        (Lang::Th, "ศ. 9 ต.ค. 2569|16:47"),
        (Lang::En, "Fri 09 Oct 2026|16:47"),
    ] {
        let out = run(&t, lang, &[], |lua| FRIDAY.set(lua))?;
        let shown = plain_rows(&out[0]).join("\n");
        assert!(shown.contains(want), "{lang:?}: {shown}");
    }
    Ok(())
}

#[test]
fn every_day_and_month_has_its_name_in_both_languages() -> mlua::Result<()> {
    // The names are typed again here and not read from the generator, so a slipped entry or an
    // off-by-one in one of its tables fails.
    const DAYS_TH: [&str; 7] = ["อา.", "จ.", "อ.", "พ.", "พฤ.", "ศ.", "ส."];
    const MONTHS_TH: [&str; 12] = [
        "ม.ค.",
        "ก.พ.",
        "มี.ค.",
        "เม.ย.",
        "พ.ค.",
        "มิ.ย.",
        "ก.ค.",
        "ส.ค.",
        "ก.ย.",
        "ต.ค.",
        "พ.ย.",
        "ธ.ค.",
    ];
    const DAYS_EN: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const MONTHS_EN: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let t = with_lines(&["{date}"]);
    for wday in 1..=7u32 {
        for month in 1..=12u32 {
            // 1 to 31, so that days with one digit and with two are both written
            let day = 1 + (wday * 5 + month * 3) % 31;
            let clock = Clock {
                wday,
                month,
                day,
                ..FRIDAY
            };
            let (w, m) = (wday as usize - 1, month as usize - 1);
            for (lang, want) in [
                (
                    Lang::Th,
                    format!("{} {} {} {}", DAYS_TH[w], day, MONTHS_TH[m], 2026 + 543),
                ),
                (
                    Lang::En,
                    format!("{} {:02} {} {}", DAYS_EN[w], day, MONTHS_EN[m], 2026),
                ),
            ] {
                let out = run(&t, lang, &[], |lua| clock.set(lua))?;
                let shown = plain_rows(&out[0]).join("\n");
                assert!(
                    shown.contains(&want),
                    "{lang:?} {wday}/{month}/{day}: {shown}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn the_buddhist_year_follows_the_calendar_year() -> mlua::Result<()> {
    let t = with_lines(&["{date}"]);
    for (year, th) in [(1999, 2542), (2026, 2569), (2099, 2642)] {
        let clock = Clock { year, ..FRIDAY };
        let out = run(&t, Lang::Th, &[], |lua| clock.set(lua))?;
        assert!(out[0].contains(&format!("ต.ค. {th}")), "{year}");
        let out = run(&t, Lang::En, &[], |lua| clock.set(lua))?;
        assert!(out[0].contains(&format!("Oct {year}")), "{year}");
    }
    Ok(())
}

#[test]
fn the_time_is_hours_and_minutes_with_two_digits() -> mlua::Result<()> {
    let t = with_lines(&["{time}"]);
    let clock = Clock {
        hour: 7,
        min: 5,
        ..FRIDAY
    };
    for lang in LANGS {
        let out = run(&t, lang, &[], |lua| clock.set(lua))?;
        assert!(
            plain_rows(&out[0]).join("\n").contains(" 07:05 "),
            "{lang:?}"
        );
    }
    Ok(())
}

// -- variables (spec §3.3) --

#[test]
fn variables_are_read_when_the_box_is_printed_and_stay_literal() -> mlua::Result<()> {
    let mut t = with_lines(&["{user}|{host}|{cwd}|{time}"]);
    t.banner.title = "{user}".to_string();
    // a value full of characters that mean something to Lua, to string.format and to patterns
    let hostile = r#"%s%d]]"\027 (x)[a-z]%1"#;
    for lang in LANGS {
        let out = run(&t, lang, &[], |lua| {
            set_env(lua, "USERNAME", hostile)?;
            set_env(lua, "COMPUTERNAME", "WORKSTATION")?;
            FRIDAY.set(lua)
        })?;
        let rows = plain_rows(&out[0]);
        assert!(
            rows[1].contains(&format!(" {hostile} ")),
            "{lang:?}: {}",
            rows[1]
        );
        let want = format!("{hostile}|WORKSTATION|C:\\Users\\ทดสอบ|16:47");
        assert!(rows[3].contains(&want), "{lang:?}: {}", rows[3]);
    }
    Ok(())
}

#[test]
fn a_missing_variable_is_an_empty_text_and_not_an_error() -> mlua::Result<()> {
    let t = with_lines(&["[{user}][{host}]"]);
    let out = run(&t, Lang::En, &[], |lua| {
        // both variables unset: the stand-in answers nil for a key it does not know
        lua.load("os.getenv = function() return nil end").exec()
    })?;
    assert!(plain_rows(&out[0]).join("\n").contains("[][]"));
    Ok(())
}

// -- text from the theme (spec §3.7 #3 and #4) --

#[test]
fn hostile_lines_stay_literal_too() -> mlua::Result<()> {
    let hostile = [
        r#"a"]]\x"#,
        "--[[ {foo} ]]",
        r"'single' \027[0m \n \\",
        "%s %d {Time} {USER}",
    ];
    for lang in LANGS {
        let out = run(&with_lines(&hostile), lang, &[], |_| Ok(()))?;
        // `\027` and `\n` stay the characters they are written with: had the first become an
        // escape character, strip_ansi would take it out together with `[0m`
        let shown = plain_rows(&out[0]).join("\n");
        for line in hostile {
            assert!(shown.contains(line), "{lang:?}: {line:?} in {shown}");
        }
    }
    Ok(())
}

#[test]
fn control_characters_in_a_theme_never_reach_the_terminal() -> mlua::Result<()> {
    // Import cleans every text (spec §3.7 #3), but a theme built in code has not been through
    // that, and `validate` accepts what import would have cleaned. The generator is the last
    // place that can keep an escape sequence out of the terminal.
    let mut t = with_lines(&["a\u{1b}[31mb\r\nc\u{202e}d\u{7}"]);
    t.banner.title = "hi\u{1b}]0;evil\u{7}\n{user}".to_string();
    for lang in LANGS {
        let out = run(&t, lang, &[], |_| Ok(()))?;
        let rows = plain_rows(&out[0]);
        for row in &rows {
            for bad in ['\u{1b}', '\u{7}', '\r', '\u{202e}'] {
                assert!(!row.contains(bad), "{lang:?}: {bad:?} in {row:?}");
            }
        }
        // top, title, blank, the one line, bottom, tips, the closing space: the newlines in
        // the texts did not add rows
        assert_eq!(rows.len(), 7, "{lang:?}: {rows:?}");
        assert!(rows[1].contains("hi0;evil"), "{lang:?}: {}", rows[1]);
        assert!(
            rows[1].contains("ทดสอบ"),
            "{lang:?}: the variable still works"
        );
        assert!(rows[3].contains("abcd"), "{lang:?}: {}", rows[3]);
    }
    Ok(())
}

// -- colors and tips --

#[test]
fn the_frame_is_the_accent_and_the_extra_rows_are_the_muted_color() -> mlua::Result<()> {
    let out = run(&terracotta(), Lang::En, &[], |_| Ok(()))?;
    let rows: Vec<&str> = out[0].split('\n').collect();
    assert!(rows[0].starts_with(&format!("{ACCENT_SGR}\u{256d}")));
    assert!(rows[1].starts_with(&format!("{ACCENT_SGR}\u{2502}")));
    // the icon is in the accent color as well (it is the asterisk here: no WT_SESSION)
    assert!(rows[1].contains(&format!("{ACCENT_SGR}*")), "{}", rows[1]);
    // the first extra line, then the tips, are in the muted color
    assert!(
        rows[3].contains(&format!("{MUTED_SGR}  cwd: ")),
        "{}",
        rows[3]
    );
    let tips = rows.len() - 2;
    assert!(rows[tips].starts_with(MUTED_SGR), "{}", rows[tips]);

    // and the colors come from the theme, not from the preset
    let mut t = terracotta();
    t.colors.accent = "#010203".parse().unwrap();
    t.colors.muted = "#0a0b0c".parse().unwrap();
    let out = run(&t, Lang::En, &[], |_| Ok(()))?;
    let rows: Vec<&str> = out[0].split('\n').collect();
    assert!(
        rows[0].starts_with("\u{1b}[38;2;1;2;3m\u{256d}"),
        "{}",
        rows[0]
    );
    assert!(
        rows[3].contains("\u{1b}[38;2;10;11;12m  cwd: "),
        "{}",
        rows[3]
    );
    assert!(!out[0].contains(ACCENT_SGR) && !out[0].contains(MUTED_SGR));
    Ok(())
}

#[test]
fn the_tips_line_is_in_the_app_language() -> mlua::Result<()> {
    for (lang, want) in [
        (
            Lang::Th,
            " ทิป: help ดูคำสั่ง \u{b7} cls ล้างจอ \u{b7} \u{2191} ดูประวัติ",
        ),
        (
            Lang::En,
            " Tips: help for commands \u{b7} cls to clear \u{b7} \u{2191} for history",
        ),
    ] {
        let out = run(&terracotta(), lang, &[], |_| Ok(()))?;
        let rows = plain_rows(&out[0]);
        // the box, then the tips, then the one space that closes the print
        assert_eq!(rows[rows.len() - 2], want, "{lang:?}");
        assert_eq!(rows[rows.len() - 1], " ", "{lang:?}");
    }
    Ok(())
}

// -- the cmd header (spec §3.6, G4) --

#[test]
fn the_screen_is_cleared_with_one_sequence_before_the_box() -> mlua::Result<()> {
    let out = run(&terracotta(), Lang::En, &CMD_HEADER, |_| Ok(()))?;
    assert_eq!(out.len(), 2);
    assert_eq!(out[0], "<NONL>\u{1b}[2J\u{1b}[3J\u{1b}[H");
    assert!(out[1].contains("Welcome back"), "{}", out[1]);
    Ok(())
}

#[test]
fn the_screen_is_never_cleared_unless_both_lines_are_the_cmd_header() -> mlua::Result<()> {
    let mut off = terracotta();
    off.banner.clear_logo = false;
    let screens: [(&str, Theme, [&str; 2]); 5] = [
        ("clearLogo is off", off, CMD_HEADER),
        (
            "only the first line",
            terracotta(),
            [CMD_HEADER[0], "C:\\>"],
        ),
        (
            "only the second line",
            terracotta(),
            ["C:\\>", CMD_HEADER[1]],
        ),
        (
            "the two lines swapped",
            terracotta(),
            [CMD_HEADER[1], CMD_HEADER[0]],
        ),
        (
            "a header that is not on top",
            terracotta(),
            ["", CMD_HEADER[0]],
        ),
    ];
    for (why, theme, screen) in screens {
        let out = run(&theme, Lang::En, &screen, |_| Ok(()))?;
        assert_eq!(out.len(), 1, "{why}: {out:?}");
        assert!(!out.iter().any(|s| s.contains("[2J")), "{why}");
    }
    Ok(())
}

// -- every combination of the switches --

/// A theme for every language and every combination of the switches that change the script.
fn variants() -> Vec<(Theme, Lang, String)> {
    let mut all = Vec::new();
    for lang in LANGS {
        for enabled in [true, false] {
            for clear_logo in [true, false] {
                for tips in [true, false] {
                    for count in 0..=4 {
                        let mut t = terracotta();
                        t.banner.enabled = enabled;
                        t.banner.clear_logo = clear_logo;
                        t.banner.tips = tips;
                        t.banner.lines = (0..count)
                            .map(|i| format!("line {i} {{user}} {{date}} {{foo}}"))
                            .collect();
                        let why = format!(
                            "{lang:?} enabled={enabled} clearLogo={clear_logo} tips={tips} lines={count}"
                        );
                        all.push((t, lang, why));
                    }
                }
            }
        }
    }
    all
}

#[test]
fn loading_does_nothing_whatever_the_theme() -> mlua::Result<()> {
    // spec §2.2 #4: `clink set` loads every script, so loading may neither print nor set a
    // variable; the only thing it may do is register the callback, and only when enabled
    for (theme, lang, why) in variants() {
        let lua = load(&banner_lua(&theme, lang), &CMD_HEADER)?;
        assert!(printed(&lua)?.is_empty(), "{why}");
        let env: Table = lua.globals().get("__env")?;
        assert_eq!(
            env.pairs::<Value, Value>().count(),
            0,
            "{why}: a variable was set"
        );
        let callback: Value = lua.globals().get("__oninject")?;
        assert_eq!(!callback.is_nil(), theme.banner.enabled, "{why}");
    }
    Ok(())
}

#[test]
fn every_theme_prints_one_valid_box_once_per_session() -> mlua::Result<()> {
    for (theme, lang, why) in variants().into_iter().filter(|(t, ..)| t.banner.enabled) {
        let b = &theme.banner;
        let lua = load(&banner_lua(&theme, lang), &CMD_HEADER)?;
        inject(&lua)?;
        inject(&lua)?;
        let out = printed(&lua)?;
        // the clear comes first, when it is asked for, and nothing is printed twice
        assert_eq!(out.len(), 1 + usize::from(b.clear_logo), "{why}: {out:?}");
        assert_eq!(out[0].starts_with("<NONL>"), b.clear_logo, "{why}");
        let shown = out.last().unwrap();

        let rows: Vec<&str> = shown.split('\n').collect();
        let plain = plain_rows(shown);
        let content = 1 + if b.lines.is_empty() {
            0
        } else {
            1 + b.lines.len()
        };
        let bottom = 1 + content;
        assert_eq!(
            rows.len(),
            bottom + 1 + usize::from(b.tips) + 1,
            "{why}: {plain:?}"
        );
        assert!(
            plain[0].starts_with('\u{256d}') && plain[0].ends_with('\u{256e}'),
            "{why}"
        );
        for row in &plain[1..bottom] {
            assert!(
                row.starts_with('\u{2502}') && row.ends_with('\u{2502}'),
                "{why}: {row}"
            );
        }
        assert!(
            plain[bottom].starts_with('\u{2570}') && plain[bottom].ends_with('\u{256f}'),
            "{why}"
        );
        let widths: Vec<usize> = plain[..=bottom].iter().map(|r| r.chars().count()).collect();
        assert!(widths.windows(2).all(|w| w[0] == w[1]), "{why}: {widths:?}");
        // the last line is one space, never an empty line (spec §2.2 #3)
        assert_eq!(rows.last(), Some(&" "), "{why}");
    }
    Ok(())
}

// ---- init.lua (spec §5.4, G17) ----

/// A stand-in for `io.popen` that records each command and answers `__popen_result`, or nil
/// when that is unset, which is what `io.popen` gives when the command cannot start. `print`
/// is a stand-in too, so that anything init.lua prints shows up in `printed` like `clink.print`.
const INIT_STUBS: &str = r#"
__popen_cmds = {}
__logs = {}
log = { info = function(s) __logs[#__logs + 1] = s end }
print = function(...) __out[#__out + 1] = "<print>" .. table.concat({...}, " ") end
io.popen = function(cmd)
  __popen_cmds[#__popen_cmds + 1] = cmd
  if __popen_result == nil then return nil end
  return {
    read = function(self, fmt) return __popen_result end,
    close = function(self) end,
  }
end
"#;

/// Loads the generated init.lua for these two paths, with `popen_result` as the text that
/// Oh My Posh prints.
fn load_init(exe: &str, config: &str, popen_result: Option<&str>) -> mlua::Result<Lua> {
    let lua = Lua::new();
    lua.load(PRELUDE).exec()?;
    lua.load(INIT_STUBS).exec()?;
    if let Some(result) = popen_result {
        lua.globals().set("__popen_result", result)?;
    }
    lua.load(init_lua(Path::new(exe), Path::new(config)))
        .set_name("init.lua")
        .exec()?;
    Ok(lua)
}

fn strings(lua: &Lua, global: &str) -> mlua::Result<Vec<String>> {
    let list: Table = lua.globals().get(global)?;
    list.sequence_values::<String>().collect()
}

#[test]
fn init_runs_oh_my_posh_by_its_full_path_in_the_form_cmd_needs() -> mlua::Result<()> {
    // `cmd /c` drops the outer quotes of what io.popen gives it, so the command is
    // `""<exe>" init cmd --config "<config>" 2>nul"` (G17), whatever the two paths hold. The
    // redirect is inside the outer quotes: io.popen pipes stdout only, so cmd's own error text
    // (a missing exe) would otherwise be printed in every window.
    for (exe, config) in [
        (
            r"C:\Users\ทดสอบ ผู้ใช้\AppData\Local\ai.resincore.cmdstudio\runtime\omp-31.5.0\oh-my-posh.exe",
            r"C:\Users\ทดสอบ ผู้ใช้\AppData\Local\ai.resincore.cmdstudio\generated\theme.omp.json",
        ),
        (
            r"C:\a]]b\--[[ x \omp.exe",
            r"D:\x ]] y\\z\ทดสอบ]]\theme.omp.json",
        ),
        (r"E:\100 percent\'it's'\omp.exe", r"E:\[=[ ]=]\t.json"),
    ] {
        let lua = load_init(exe, config, Some("__prompt = 'loaded'"))?;
        let want = format!("\"\"{exe}\" init cmd --config \"{config}\" 2>nul\"");
        assert_eq!(strings(&lua, "__popen_cmds")?, [want]);
        // what Oh My Posh printed was loaded and run
        assert_eq!(lua.globals().get::<String>("__prompt")?, "loaded");
        assert_eq!(strings(&lua, "__logs")?, ["cmdstudio: prompt loaded"]);
        assert!(printed(&lua)?.is_empty());
    }
    Ok(())
}

#[test]
fn init_gives_up_quietly_when_oh_my_posh_prints_nothing_or_nonsense() -> mlua::Result<()> {
    // init.lua runs in every cmd window, so a missing or broken Oh My Posh may only be logged;
    // an error would be printed in every window
    for (result, logged) in [
        (None, "cmdstudio: oh-my-posh init returned nothing"),
        (Some(""), "cmdstudio: oh-my-posh init returned nothing"),
        (
            Some("this is ( not lua"),
            "cmdstudio: cannot load oh-my-posh init: ",
        ),
    ] {
        let lua = load_init(r"C:\omp.exe", r"C:\t.json", result)?;
        let logs = strings(&lua, "__logs")?;
        assert_eq!(logs.len(), 1, "{result:?}: {logs:?}");
        assert!(logs[0].starts_with(logged), "{result:?}: {logs:?}");
        let prompt: Value = lua.globals().get("__prompt")?;
        assert!(prompt.is_nil(), "{result:?}");
        assert!(printed(&lua)?.is_empty(), "{result:?}");
    }
    Ok(())
}

#[test]
fn init_logs_and_prints_nothing_when_the_oh_my_posh_script_raises() -> mlua::Result<()> {
    // The script parses, so `load` accepts it, and then it raises while it runs. That must not
    // reach the window as an error: init.lua runs in every cmd window, so it only logs. The
    // failures are not all strings: `error({})` and `error(nil)` must not break the log line.
    for (script, detail) in [
        (
            "error('boom from the init script')",
            "boom from the init script",
        ),
        ("local missing = nil\nmissing()", "attempt to call"),
        ("__prompt = 'half done'\nerror('late boom')", "late boom"),
        ("error({})", "table:"),
        ("error(nil)", "nil"),
    ] {
        let lua = load_init(r"C:\omp.exe", r"C:\t.json", Some(script))?;
        let logs = strings(&lua, "__logs")?;
        assert_eq!(logs.len(), 1, "{script:?}: {logs:?}");
        assert!(
            logs[0].starts_with("cmdstudio: oh-my-posh init failed: "),
            "{script:?}: {logs:?}"
        );
        assert!(logs[0].contains(detail), "{script:?}: {logs:?}");
        assert!(printed(&lua)?.is_empty(), "{script:?}");
    }
    Ok(())
}
