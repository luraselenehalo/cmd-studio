//! Parity test for the welcome box (spec §3.6, §5.6): `preview::render_banner` against the
//! banner.lua that `generate::lua` writes, run in Lua 5.2 with stand-ins for Clink as
//! `tests/banner_lua.rs` runs it. The stand-ins answer with the sample: the environment
//! (`USERNAME`, `COMPUTERNAME`, `WT_SESSION`), the folder and the clock. `console.cellcount`
//! is `preview::banner_cells` on the text without its colors, so this test holds the box's
//! layout, colors, variables, dates, tips and icon to the script; how Clink counts cells in a
//! real window is a separate question (see `preview::banner_cells`).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod sgr;

use cmdstudio_core::generate::lua::banner_lua;
use cmdstudio_core::model::Theme;
use cmdstudio_core::presets;
use cmdstudio_core::preview::{banner_cells, render_banner, Clock, Line, PreviewContext};
use mlua::{Function, Lua, Table};
use sgr::{difference, parse_sgr};

const STAND_INS: &str = r#"
__out = {}
NONL = {}
clink = {
  print = function(s, flag) if flag ~= NONL then __out[#__out + 1] = s end end,
  oninject = function(f) __oninject = f end,
}
console = {
  cellcount = function(s) return __cells((s:gsub("\27%[[%d;]*m", ""))) end,
  getlinetext = function() return nil end,
}
log = { info = function() end }
os.getenv = function(k) return __env[k] end
os.setenv = function(k, v) __env[k] = v end
os.getcwd = function() return __cwd end
os.date = function(fmt)
  if fmt == "*t" then return __now end
  if fmt == "%H:%M" then return string.format("%02d:%02d", __now.hour, __now.min) end
  error("os.date(" .. tostring(fmt) .. ") follows the language of Windows")
end
"#;

/// What the generated banner.lua prints for `theme` in a window like `ctx`, whose date falls
/// on `wday` (1 is Sunday). Empty when it prints nothing. With `unset_empty`, a user name,
/// computer name or folder that is empty is not set at all (`os.getenv` and `os.getcwd` answer
/// `nil`, as they do for a variable that does not exist) instead of being set to "".
fn printed(theme: &Theme, ctx: &PreviewContext, wday: u8, unset_empty: bool) -> String {
    let lua = Lua::new();
    let cells = lua
        .create_function(|_, s: String| Ok(banner_cells(&s)))
        .unwrap();
    lua.globals().set("__cells", cells).unwrap();
    let env = lua.create_table().unwrap();
    for (name, value) in [("USERNAME", &ctx.user), ("COMPUTERNAME", &ctx.host)] {
        if !(unset_empty && value.is_empty()) {
            env.set(name, value.as_str()).unwrap();
        }
    }
    if ctx.windows_terminal {
        env.set("WT_SESSION", "8b6e7d11-0000-4000-8000-000000000000")
            .unwrap();
    }
    lua.globals().set("__env", env).unwrap();
    if !(unset_empty && ctx.cwd.is_empty()) {
        lua.globals().set("__cwd", ctx.cwd.as_str()).unwrap();
    }
    let now = lua.create_table().unwrap();
    let c = &ctx.clock;
    now.set("year", c.year).unwrap();
    now.set("month", c.month).unwrap();
    now.set("day", c.day).unwrap();
    now.set("wday", wday).unwrap();
    now.set("hour", c.hour).unwrap();
    now.set("min", c.minute).unwrap();
    lua.globals().set("__now", now).unwrap();
    lua.load(STAND_INS).exec().unwrap();
    lua.load(banner_lua(theme))
        .set_name("banner.lua")
        .exec()
        .unwrap();
    if let Ok(callback) = lua.globals().get::<Function>("__oninject") {
        callback.call::<()>(()).unwrap();
    }
    let out: Table = lua.globals().get("__out").unwrap();
    let all: Vec<String> = out.sequence_values().collect::<mlua::Result<_>>().unwrap();
    assert!(all.len() <= 1, "printed more than once: {all:?}");
    all.concat()
}

/// Reads what the script printed as `parse_sgr` does and holds it to the emulator: panics,
/// naming `why`, at the first line that differs, and never loosens the comparison. Gives the
/// lines the script printed (none when it printed nothing).
fn compare(why: &str, theme: &Theme, ctx: &PreviewContext, out: &str) -> Vec<Line> {
    let want = render_banner(theme, ctx);
    if out.is_empty() {
        assert!(want.is_empty(), "{why}: banner.lua printed nothing");
        return Vec::new();
    }
    let got = parse_sgr(out, theme.colors.foreground, theme.colors.background)
        .unwrap_or_else(|e| panic!("{why}: {e}"));
    if let Some(diff) = difference(&want, &got) {
        panic!("{why}: {diff}");
    }
    got
}

/// Runs the script for `ctx` on a day of the week `wday` and holds it to the emulator.
fn check(why: &str, theme: &Theme, ctx: &PreviewContext, wday: u8) -> Vec<Line> {
    compare(why, theme, ctx, &printed(theme, ctx, wday, false))
}

/// The text of each line, without its colors.
fn texts(lines: &[Line]) -> Vec<String> {
    lines.iter().map(Line::text).collect()
}

/// Dates whose day of the week is known (1 is Sunday).
const DATES: [(u16, u8, u8, u8); 5] = [
    (2026, 10, 9, 6),  // Friday, spec §3.6
    (2026, 10, 11, 1), // Sunday
    (2024, 2, 29, 5),  // Thursday, a leap day
    (1999, 12, 31, 6), // Friday
    (2026, 1, 1, 5),   // Thursday
];

/// Clocks with the text `%H:%M` gives them: one digit in the hour, one in the minute, both,
/// midnight, the last minute of a day, and two digits each.
const TIMES: [(u8, u8, &str); 6] = [
    (7, 5, "07:05"),
    (0, 0, "00:00"),
    (9, 0, "09:00"),
    (0, 9, "00:09"),
    (23, 59, "23:59"),
    (12, 30, "12:30"),
];

/// How many samples [`contexts`] gives and how many banners [`banners`] gives: the numbers the
/// test counts its cases against, so a case added or lost on the way is noticed.
const SAMPLES: usize = 5;
const BANNERS: usize = 15;

/// The samples: the editor's own, conhost (no WT_SESSION), Thai names and a Thai folder (the
/// user's own data, shown as it is), names with control and direction characters that
/// banner.lua cleans, and names and a folder that are empty.
fn contexts() -> Vec<PreviewContext> {
    let sample = PreviewContext::sample();
    vec![
        sample.clone(),
        PreviewContext {
            windows_terminal: false,
            ..sample.clone()
        },
        PreviewContext {
            user: "ผู้ทดสอบ ไทย".to_string(),
            host: "เครื่อง-01".to_string(),
            cwd: r"C:\Users\ผู้ทดสอบ ไทย\เอกสาร".to_string(),
            ..sample.clone()
        },
        PreviewContext {
            user: "ad\u{202e}min\u{85}x\u{1b}[31mred".to_string(),
            host: "PC\u{2066}-\u{1b}01\u{7}".to_string(),
            cwd: "C:\\work\u{202e}fdp.exe\\\u{85}proj".to_string(),
            ..sample.clone()
        },
        PreviewContext {
            user: String::new(),
            host: String::new(),
            cwd: String::new(),
            ..sample
        },
    ]
}

/// The banners: each preset's own, and changes that move the box's layout.
fn banners(preset: &Theme) -> Vec<(String, Theme)> {
    let with = |what: &str, edit: &dyn Fn(&mut Theme)| {
        let mut theme = preset.clone();
        edit(&mut theme);
        (what.to_string(), theme)
    };
    vec![
        with("as the preset has it", &|_| {}),
        with("turned off", &|t| t.banner.enabled = false),
        with("no lines and no tips", &|t| {
            t.banner.lines.clear();
            t.banner.tips = false;
        }),
        with("four lines of every variable", &|t| {
            t.banner.lines = vec![
                "{user}@{host} in {cwd}".to_string(),
                "{date} {time}".to_string(),
                "{{user}} {USER} {foo} {".to_string(),
                String::new(),
            ];
        }),
        with("Thai with marks", &|t| {
            t.banner.title = "สวัสดี {user} ที่นี่ปี {date}".to_string()
        }),
        with("emoji", &|t| {
            t.banner.title =
                "\u{1f44b} {user} \u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467} \u{2764}\u{fe0f}"
                    .to_string()
        }),
        with("escape sequences in the theme's text", &|t| {
            t.banner.title = "hi\u{1b}]0;evil\u{7} {user}".to_string();
            t.banner.lines = vec!["a\u{1b}[31mb\u{202e}c".to_string()];
        }),
        with("the longest title", &|t| t.banner.title = "W".repeat(80)),
        with("clearLogo off", &|t| t.banner.clear_logo = false),
        // sanitize_text takes a whole escape sequence out before the template is cut, so a
        // variable with one in the middle of its name is that variable
        with("variables cut by escape sequences", &|t| {
            t.banner.title = "{us\u{1b}[31mer}".to_string();
            t.banner.lines = vec![
                "{ho\u{1b}[0mst} in {c\u{1b}[1;32mwd}".to_string(),
                "{da\u{1b}[mte} {ti\u{1b}[0mme}".to_string(),
                "{ti\u{1b}]0;x\u{7}me} stays text".to_string(),
            ];
        }),
        with("a title and lines that sanitise to nothing", &|t| {
            t.banner.title = "\u{1b}[31m\u{202e}".to_string();
            t.banner.lines = vec![
                "\u{1b}[0m".to_string(),
                "\u{7}".to_string(),
                "{user}".to_string(),
            ];
        }),
        with("an empty title", &|t| t.banner.title = String::new()),
        with("no lines, tips on", &|t| {
            t.banner.lines.clear();
            t.banner.tips = true;
        }),
        with("lines, tips off", &|t| {
            t.banner.lines = vec!["cwd: {cwd}".to_string(), "{time}".to_string()];
            t.banner.tips = false;
        }),
        with("wide characters and a joined emoji in a line", &|t| {
            t.banner.lines = vec![
                "中文宽度 ｗｉｄｅ".to_string(),
                "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467} family".to_string(),
                "한국어 ﾊ {user}".to_string(),
            ];
        }),
    ]
}

#[test]
fn the_emulator_draws_what_banner_lua_prints() {
    let mut count = 0;
    for (key, preset) in presets::builtin().unwrap() {
        let banners = banners(&preset);
        assert_eq!(banners.len(), BANNERS);
        for (what, theme) in banners {
            let samples = contexts();
            assert_eq!(samples.len(), SAMPLES);
            for (i, base) in samples.into_iter().enumerate() {
                // The dates and the clocks take turns, and no sample is always on the same day:
                // the date also moves on by one with every banner.
                let (year, month, day, wday) = DATES[(count + count / SAMPLES) % DATES.len()];
                let (hour, minute, _) = TIMES[count % TIMES.len()];
                let ctx = PreviewContext {
                    clock: Clock {
                        year,
                        month,
                        day,
                        hour,
                        minute,
                    },
                    ..base
                };
                check(&format!("{key}, {what}, sample {i}"), &theme, &ctx, wday);
                count += 1;
            }
        }
    }
    // every preset, every banner, every sample
    assert_eq!(count, presets::builtin().unwrap().len() * BANNERS * SAMPLES);
}

/// `preset` with a box that shows the three names: `Welcome back, {user}!` and, in the lines,
/// the folder, then the date, the time and the computer.
fn names_box(preset: &Theme) -> Theme {
    let mut theme = preset.clone();
    theme.banner.enabled = true;
    theme.banner.title = "Welcome back, {user}!".to_string();
    theme.banner.lines = vec![
        "cwd: {cwd}".to_string(),
        "{date} \u{b7} {time} \u{b7} {host}".to_string(),
    ];
    theme.banner.tips = true;
    theme
}

/// The characters on either side of the ends of the ranges banner.lua's `clean` drops, and
/// whether `clean` drops each: C0 (U+0000 to U+001F), DEL (U+007F), C1 (U+0080 to U+009F),
/// U+2028 to U+202E and U+2066 to U+2069.
const EDGES: [(char, bool); 16] = [
    ('\u{0}', true),
    ('\u{1f}', true),
    ('\u{20}', false),
    ('\u{7e}', false),
    ('\u{7f}', true),
    ('\u{80}', true),
    ('\u{9f}', true),
    ('\u{a0}', false),
    ('\u{2027}', false),
    ('\u{2028}', true),
    ('\u{202e}', true),
    ('\u{202f}', false),
    ('\u{2065}', false),
    ('\u{2066}', true),
    ('\u{2069}', true),
    ('\u{206a}', false),
];

#[test]
fn clean_ends_where_the_emulator_ends_it_in_the_user_the_computer_and_the_folder() {
    // The sample is Friday 9 October 2026 at 16:47. Each edge character sits between two
    // letters in each of the three names in turn, and the script and the emulator must keep
    // or drop it alike; the expected text is written out here too, so a character that both
    // keep (or both drop) wrongly cannot slip through.
    let sample = PreviewContext::sample();
    let mut cases = 0;
    for (key, preset) in presets::builtin().unwrap() {
        let theme = names_box(&preset);
        for (c, dropped) in EDGES {
            let name = format!("a{c}b");
            let shown = if dropped { "ab" } else { name.as_str() };
            for (field, ctx, expected) in [
                (
                    "user",
                    PreviewContext {
                        user: name.clone(),
                        ..sample.clone()
                    },
                    format!("Welcome back, {shown}!"),
                ),
                (
                    "computer",
                    PreviewContext {
                        host: name.clone(),
                        ..sample.clone()
                    },
                    format!("16:47 \u{b7} {shown}"),
                ),
                (
                    "folder",
                    PreviewContext {
                        cwd: name.clone(),
                        ..sample.clone()
                    },
                    format!("cwd: {shown}"),
                ),
            ] {
                let why = format!("{key}, U+{:04X} in the {field}", u32::from(c));
                let got = texts(&check(&why, &theme, &ctx, 6)).join("\n");
                assert!(got.contains(&expected), "{why}: not {expected:?} in\n{got}");
                if dropped {
                    assert!(!got.contains(&name), "{why}: still in\n{got:?}");
                }
                cases += 1;
            }
        }
    }
    assert_eq!(cases, presets::builtin().unwrap().len() * EDGES.len() * 3);
}

#[test]
fn an_empty_or_unset_name_or_folder_leaves_nothing_in_its_place() {
    // USERNAME, COMPUTERNAME and the folder, every way of leaving some of them empty, set to
    // "" and not set at all (`os.getenv(...) or ""`)
    let sample = PreviewContext::sample();
    for (key, preset) in presets::builtin().unwrap() {
        let mut theme = names_box(&preset);
        theme.banner.title = "[{user}|{host}|{cwd}]".to_string();
        theme.banner.lines.clear();
        for mask in 1..8_u8 {
            let leave = |bit: u8, name: &str| {
                if mask & bit == 0 {
                    name.to_string()
                } else {
                    String::new()
                }
            };
            let ctx = PreviewContext {
                user: leave(1, &sample.user),
                host: leave(2, &sample.host),
                cwd: leave(4, &sample.cwd),
                ..sample.clone()
            };
            let expected = format!("[{}|{}|{}]", ctx.user, ctx.host, ctx.cwd);
            for unset in [false, true] {
                let why = format!("{key}, mask {mask}, unset {unset}");
                let got = compare(&why, &theme, &ctx, &printed(&theme, &ctx, 6, unset));
                let got = texts(&got).join("\n");
                assert!(got.contains(&expected), "{why}: not {expected:?} in\n{got}");
            }
        }
    }
}

#[test]
fn a_variable_cut_by_an_escape_sequence_is_that_variable() {
    // sanitize_text takes `ESC [ 31 m` out of `{us<here>er}` before the template is cut, so the
    // title is `{user}` and shows the name; the real script says the same as the emulator
    let sample = PreviewContext::sample();
    for (key, preset) in presets::builtin().unwrap() {
        let mut theme = preset.clone();
        theme.banner.enabled = true;
        theme.banner.title = "{us\u{1b}[31mer}".to_string();
        theme.banner.lines = vec!["{c\u{1b}[1;32mwd}".to_string()];
        let rows = texts(&check(&format!("{key} split"), &theme, &sample, 6));
        assert!(rows[1].contains("\u{273b} you"), "{rows:?}");
        assert!(rows[3].contains("  C:\\Users\\you\\projects\\cmd-studio"));
        assert!(!rows.concat().contains('{'), "{rows:?}");
    }
}

#[test]
fn a_title_and_lines_that_sanitise_to_nothing_leave_the_icon_and_the_indent() {
    let sample = PreviewContext::sample();
    for (key, preset) in presets::builtin().unwrap() {
        let mut theme = preset.clone();
        theme.banner.enabled = true;
        theme.banner.title = "\u{1b}[31m\u{202e}".to_string();
        theme.banner.lines = vec!["\u{1b}[0m".to_string()];
        theme.banner.tips = false;
        let rows = texts(&check(&format!("{key} nothing"), &theme, &sample, 6));
        // content: `✻ `, an empty line and two spaces, 2 cells at the widest; the box is 4 cells
        // inside and 6 across
        let blank = format!("\u{2502}{}\u{2502}", " ".repeat(6));
        assert_eq!(
            rows,
            vec![
                format!("\u{256d}{}\u{256e}", "\u{2500}".repeat(6)),
                format!("\u{2502} \u{273b}{}\u{2502}", " ".repeat(4)),
                blank.clone(),
                blank,
                format!("\u{2570}{}\u{256f}", "\u{2500}".repeat(6)),
                " ".to_string(),
            ],
            "{key}"
        );
    }
}

#[test]
fn clocks_are_written_with_two_digits_for_the_hour_and_the_minute() {
    let sample = PreviewContext::sample();
    for (key, preset) in presets::builtin().unwrap() {
        let mut theme = preset.clone();
        theme.banner.enabled = true;
        theme.banner.title = "at {time}".to_string();
        theme.banner.lines.clear();
        for (hour, minute, text) in TIMES {
            let ctx = PreviewContext {
                clock: Clock {
                    hour,
                    minute,
                    ..sample.clock
                },
                ..sample.clone()
            };
            let rows = texts(&check(&format!("{key}, {text}"), &theme, &ctx, 6));
            assert!(rows[1].contains(&format!("at {text} ")), "{rows:?}");
        }
    }
}

#[test]
fn wide_characters_and_a_joined_emoji_line_up_in_the_real_box() {
    // In the script's own output every framed row takes as many cells as the top one (by the
    // count the stand-in gives, which is the emulator's), CJK and a family emoji included.
    let sample = PreviewContext::sample();
    for (key, preset) in presets::builtin().unwrap() {
        let mut theme = preset.clone();
        theme.banner.enabled = true;
        theme.banner.title = "\u{4e2d}\u{6587} {user}".to_string();
        theme.banner.tips = true;
        theme.banner.lines = vec![
            "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467} family".to_string(),
            "\u{d55c}\u{ad6d}\u{c5b4} \u{ff8a}".to_string(),
        ];
        let rows = texts(&check(&format!("{key} wide"), &theme, &sample, 6));
        let framed: Vec<usize> = rows[..rows.len() - 2]
            .iter()
            .map(|row| banner_cells(row))
            .collect();
        assert!(
            framed.windows(2).all(|w| w[0] == w[1]),
            "{framed:?} {rows:?}"
        );
        // the family emoji is printed whole, joiners and all
        assert!(
            rows[3].contains("\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467} family"),
            "{rows:?}"
        );
    }
}

/// The names of the days (Sunday first) and of the months, as spec §3.6 writes the date.
const NAMES_OF_DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const NAMES_OF_MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

#[test]
fn every_date_is_written_as_banner_lua_writes_it() {
    // every day of a leap year: the English day and month names and the Gregorian year
    let theme = {
        let mut t = presets::builtin().unwrap().remove(0).1;
        t.banner.title = "{date}".to_string();
        t.banner.lines.clear();
        t
    };
    // 1 January 2024 was a Monday (wday 2)
    let mut wday = 2;
    let mut days = 0;
    let days_in = [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    for (m, in_month) in days_in.iter().enumerate() {
        for day in 1..=*in_month {
            let month = u8::try_from(m + 1).unwrap();
            let ctx = PreviewContext {
                clock: Clock {
                    year: 2024,
                    month,
                    day,
                    hour: 0,
                    minute: 0,
                },
                ..PreviewContext::sample()
            };
            let why = format!("2024-{month}-{day}");
            let got = check(&why, &theme, &ctx, wday);
            // and the date as spec §3.6 writes it, from the names written out here
            let date = format!(
                "{} {day:02} {} 2024",
                NAMES_OF_DAYS[usize::from(wday) - 1],
                NAMES_OF_MONTHS[m]
            );
            assert!(got[1].text().contains(&date), "{why}: not {date:?}");
            wday = wday % 7 + 1;
            days += 1;
        }
    }
    assert_eq!(days, 366);
    // the day after 31 December 2024, a Tuesday, is a Wednesday: the count of days held
    assert_eq!(wday, 4);
}
