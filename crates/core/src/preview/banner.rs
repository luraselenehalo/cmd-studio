//! The welcome box as the generated banner.lua prints it (spec §3.6), line for line:
//! `build()` gives the title and the lines, `print_banner()` frames them. The parity test
//! (`tests/banner_parity.rs`) runs the real script and compares.
//!
//! The box is English, as everything CMD Studio writes into a cmd window is (spec §3.6): the
//! date and the tips come from `generate::lua`'s own tables. Thai or any other script shows
//! only where the user's own data holds it: the theme's texts and the names the variables
//! stand for.
use super::cells::banner_cells;
use super::values::{hh_mm, lua_clean, weekday};
use super::{Line, PreviewContext, Style};
use crate::generate::lua::{self, Piece, Var, DATE_EN, TIPS_EN};
use crate::model::Theme;
use crate::text::sanitize_text;

/// What banner.lua prints when a cmd window starts, one line per row: nothing when the theme
/// turns the welcome box off. When `clearLogo` is off, a real window shows cmd's own two
/// header lines and an empty line above it; they are cmd's, not the script's, and are not
/// here.
pub fn render_banner(theme: &Theme, ctx: &PreviewContext) -> Vec<Line> {
    let banner = &theme.banner;
    if !banner.enabled {
        return Vec::new();
    }
    let accent = Style::fg(theme.colors.accent);
    let muted = Style::fg(theme.colors.muted);
    // RESET: the terminal's own colors
    let plain = Style::fg(theme.colors.foreground);
    let fill = |template: &str| -> String {
        lua::template_pieces(&sanitize_text(template))
            .into_iter()
            .map(|piece| match piece {
                Piece::Text(text) => text.to_string(),
                Piece::Var(var) => value(var, ctx),
            })
            .collect()
    };

    // build(): the icon and the title; then, when there are lines, an empty line and the
    // lines in the muted color, each set in by two spaces
    let mut content = Vec::new();
    let mut title = Line::default();
    let icon = if ctx.windows_terminal {
        lua::ICON
    } else {
        lua::ICON_FALLBACK
    };
    title.push(icon, accent);
    title.push(" ", plain);
    title.push(&fill(&banner.title), plain);
    content.push(title);
    if !banner.lines.is_empty() {
        content.push(Line::default());
        for text in &banner.lines {
            let mut line = Line::default();
            line.push("  ", muted);
            line.push(&fill(text), muted);
            content.push(line);
        }
    }

    // print_banner(): a frame two cells wider than the widest line, by console.cellcount
    let width = content
        .iter()
        .map(|line| banner_cells(&line.text()))
        .max()
        .unwrap_or(0)
        + 2;
    let rule = "\u{2500}".repeat(width + 2);
    let mut rows = Vec::new();
    let mut top = Line::default();
    top.push(&format!("\u{256d}{rule}\u{256e}"), accent);
    rows.push(top);
    for line in &content {
        let mut row = Line::default();
        row.push("\u{2502}", accent);
        row.push(" ", plain);
        row.append(line);
        let pad = width.saturating_sub(banner_cells(&line.text()));
        row.push(&" ".repeat(pad), plain);
        row.push(" ", plain);
        row.push("\u{2502}", accent);
        rows.push(row);
    }
    let mut bottom = Line::default();
    bottom.push(&format!("\u{2570}{rule}\u{256f}"), accent);
    rows.push(bottom);
    if banner.tips {
        let mut tips = Line::default();
        tips.push(TIPS_EN, muted);
        rows.push(tips);
    }
    // one space, never an empty line (spec §2.2 #3)
    let mut last = Line::default();
    last.push(" ", plain);
    rows.push(last);
    rows
}

/// What banner.lua puts in place of a variable: `V.user()`, `V.host()` and `V.cwd()` clean
/// what they read, `V.date()` writes the date in English (the one style banner.lua has),
/// `V.time()` is `HH:MM`.
fn value(var: Var, ctx: &PreviewContext) -> String {
    let clock = &ctx.clock;
    match var {
        Var::User => lua_clean(&ctx.user),
        Var::Host => lua_clean(&ctx.host),
        Var::Cwd => lua_clean(&ctx.cwd),
        Var::Date => DATE_EN.format(weekday(clock), clock.day, clock.month, clock.year),
        Var::Time => hh_mm(clock),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::terracotta;
    use crate::preview::Clock;

    const ACCENT: &str = "#d97757";
    const MUTED: &str = "#8a8780";
    const FG: &str = "#faf9f5";

    fn texts(rows: &[Line]) -> Vec<String> {
        rows.iter().map(Line::text).collect()
    }

    /// A row as (text, foreground) pairs.
    fn spans(row: &Line) -> Vec<(String, String)> {
        row.spans
            .iter()
            .map(|s| (s.text.clone(), s.fg.to_string()))
            .collect()
    }

    fn s(text: &str, fg: &str) -> (String, String) {
        (text.to_string(), fg.to_string())
    }

    /// The Thai characters in `text` (U+0E00 to U+0E7F), in order.
    fn thai(text: &str) -> String {
        text.chars()
            .filter(|c| ('\u{0e00}'..='\u{0e7f}').contains(c))
            .collect()
    }

    #[test]
    fn terracotta_draws_the_box_of_the_sample() {
        // the widest line is `  cwd: C:\Users\you\projects\cmd-studio`, 39 cells, so the
        // box is 41 cells inside and 45 across
        let rows = render_banner(&terracotta(), &PreviewContext::sample());
        let rule = "\u{2500}".repeat(43);
        assert_eq!(
            texts(&rows),
            vec![
                format!("\u{256d}{rule}\u{256e}"),
                format!(
                    "\u{2502} \u{273b} Welcome back, you!{}\u{2502}",
                    " ".repeat(22)
                ),
                format!("\u{2502}{}\u{2502}", " ".repeat(43)),
                "\u{2502}   cwd: C:\\Users\\you\\projects\\cmd-studio   \u{2502}".to_string(),
                format!(
                    "\u{2502}   Fri 09 Oct 2026 \u{b7} 16:47 \u{b7} PC{}\u{2502}",
                    " ".repeat(12)
                ),
                format!("\u{2570}{rule}\u{256f}"),
                " Tips: help for commands \u{b7} cls to clear \u{b7} \u{2191} for history"
                    .to_string(),
                " ".to_string(),
            ]
        );
        // the frame and the icon in the accent, the title in the terminal's own color, the
        // lines and the tips muted
        assert_eq!(
            spans(&rows[1]),
            vec![
                s("\u{2502}", ACCENT),
                s(" ", FG),
                s("\u{273b}", ACCENT),
                s(&format!(" Welcome back, you!{}", " ".repeat(22)), FG),
                s("\u{2502}", ACCENT),
            ]
        );
        assert_eq!(
            spans(&rows[3])[2],
            s("  cwd: C:\\Users\\you\\projects\\cmd-studio", MUTED)
        );
        assert_eq!(spans(&rows[6]), vec![s(&texts(&rows)[6], MUTED)]);
        assert_eq!(spans(&rows[7]), vec![s(" ", FG)]);
    }

    #[test]
    fn the_box_is_english_and_thai_shows_only_where_the_user_put_it() {
        // The founder's rule (spec §3.6): what CMD Studio writes into cmd is English on every
        // Windows, so the date is `Fri 09 Oct 2026` and the tips are English, and the sample
        // (ASCII names and paths) gives a box with no Thai character in it at all.
        let rows = texts(&render_banner(&terracotta(), &PreviewContext::sample()));
        assert!(
            rows[4].contains("Fri 09 Oct 2026 \u{b7} 16:47 \u{b7} PC"),
            "{}",
            rows[4]
        );
        assert_eq!(
            rows[6],
            " Tips: help for commands \u{b7} cls to clear \u{b7} \u{2191} for history"
        );
        assert_eq!(thai(&rows.concat()), "", "{rows:?}");
        // A Thai user name is the user's own data, shown as it is: the name is the only Thai
        // in the box, and the date beside it stays English.
        let ctx = PreviewContext {
            user: "ผู้ทดสอบ".to_string(),
            ..PreviewContext::sample()
        };
        let rows = texts(&render_banner(&terracotta(), &ctx));
        assert_eq!(thai(&rows.concat()), "ผู้ทดสอบ", "{rows:?}");
        assert!(rows[1].contains("Welcome back, ผู้ทดสอบ!"), "{}", rows[1]);
        assert!(rows[4].contains("Fri 09 Oct 2026"), "{}", rows[4]);
    }

    #[test]
    fn outside_windows_terminal_the_icon_is_an_asterisk() {
        let ctx = PreviewContext {
            windows_terminal: false,
            ..PreviewContext::sample()
        };
        let rows = texts(&render_banner(&terracotta(), &ctx));
        assert!(rows[1].starts_with("\u{2502} * Welcome"), "{}", rows[1]);
    }

    #[test]
    fn a_turned_off_banner_draws_nothing_and_no_lines_no_tips_is_a_small_box() {
        let mut theme = terracotta();
        theme.banner.enabled = false;
        assert!(render_banner(&theme, &PreviewContext::sample()).is_empty());
        theme.banner.enabled = true;
        theme.banner.lines.clear();
        theme.banner.tips = false;
        let rows = texts(&render_banner(&theme, &PreviewContext::sample()));
        assert_eq!(
            rows.len(),
            4,
            "top, title, bottom and the closing space: {rows:?}"
        );
        assert_eq!(rows[3], " ");
    }

    #[test]
    fn the_frame_pads_by_cells_so_thai_marks_and_emoji_line_up() {
        // Thai in the theme's text and in the user name is user data (spec §3.6) and still has
        // to line up: the marks take no cell and a joined emoji takes two
        let mut theme = terracotta();
        theme.banner.title = "สวัสดี {user} ที่นี่ \u{1f44b}".to_string();
        theme.banner.lines = vec!["\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467} 中文".to_string()];
        let ctx = PreviewContext {
            user: "ผู้ทดสอบ".to_string(),
            ..PreviewContext::sample()
        };
        let rows = texts(&render_banner(&theme, &ctx));
        let framed = &rows[..rows.len() - 2];
        let cells: Vec<usize> = framed.iter().map(|r| banner_cells(r)).collect();
        assert!(
            cells.windows(2).all(|w| w[0] == w[1]),
            "{cells:?} {framed:?}"
        );
        // the Thai marks take no cell, so the title row has more characters than cells
        assert!(rows[1].chars().count() > cells[1]);
    }

    #[test]
    fn variables_are_filled_once_and_other_braces_stay() {
        let mut theme = terracotta();
        theme.banner.title = "{{user}} {foo} {USER} {host}".to_string();
        theme.banner.lines = vec!["{time}|{cwd}".to_string()];
        // a user whose name looks like a variable stays as it is
        let ctx = PreviewContext {
            user: "{host}".to_string(),
            ..PreviewContext::sample()
        };
        let rows = texts(&render_banner(&theme, &ctx));
        assert!(rows[1].contains("{{host}} {foo} {USER} PC"), "{}", rows[1]);
        assert!(
            rows[3].contains("16:47|C:\\Users\\you\\projects\\cmd-studio"),
            "{}",
            rows[3]
        );
    }

    #[test]
    fn names_and_the_folder_lose_what_banner_lua_cleans() {
        let ctx = PreviewContext {
            user: "ad\u{202e}min\u{85}x\u{1b}[31mred".to_string(),
            cwd: "C:\\work\u{2066}\\\u{7}proj".to_string(),
            ..PreviewContext::sample()
        };
        let rows = texts(&render_banner(&terracotta(), &ctx));
        assert!(
            rows[1].contains("Welcome back, adminx[31mred!"),
            "{}",
            rows[1]
        );
        assert!(rows[3].contains("cwd: C:\\work\\proj"), "{}", rows[3]);
        // and the theme's own text loses whole escape sequences, as `sanitize_text` does
        let mut theme = terracotta();
        theme.banner.title = "hi\u{1b}[31m there".to_string();
        let rows = texts(&render_banner(&theme, &PreviewContext::sample()));
        assert!(rows[1].contains("hi there"), "{}", rows[1]);
    }

    #[test]
    fn a_control_character_in_a_name_or_the_text_does_not_shift_the_frame() {
        // `banner_cells` gives a control character a cell, so a name is cleaned as banner.lua
        // cleans it before the frame is measured, and the theme's text as `sanitize_text`
        // cleans it: thirty of them in the user name would otherwise make the title the widest
        // line and push the right side out
        assert_eq!(banner_cells("\u{1}"), 1);
        let controls: String = (0x01..=0x1E).filter_map(char::from_u32).collect();
        let dirty = PreviewContext {
            user: format!("you{controls}"),
            host: "P\u{9b}C\u{7f}".to_string(),
            cwd: format!("C:\\Users\\you\\projects\\cmd-studio\u{2028}{controls}"),
            ..PreviewContext::sample()
        };
        let mut dirty_theme = terracotta();
        dirty_theme.banner.title = format!("Welcome back,{controls} {{user}}!");
        assert_eq!(
            render_banner(&dirty_theme, &dirty),
            render_banner(&terracotta(), &PreviewContext::sample())
        );
        // and no row holds a control character
        for row in texts(&render_banner(&dirty_theme, &dirty)) {
            assert!(!row.chars().any(char::is_control), "{row:?}");
        }
    }

    #[test]
    fn a_clock_out_of_range_still_draws_a_box_and_never_panics() {
        // The app sends the clock (spec §5.6), and nothing checks its fields: a day of the
        // week or a month out of range is named `?`, the day, the year and the time are written
        // as the numbers they are, and the box still lines up
        for (clock, line) in [
            (
                Clock {
                    year: 0,
                    month: 0,
                    day: 0,
                    hour: 99,
                    minute: 99,
                },
                "  Sun 00 ? 0 \u{b7} 99:99 \u{b7} PC",
            ),
            (
                Clock {
                    year: u16::MAX,
                    month: u8::MAX,
                    day: u8::MAX,
                    hour: u8::MAX,
                    minute: u8::MAX,
                },
                "  Fri 255 ? 65535 \u{b7} 255:255 \u{b7} PC",
            ),
            (
                Clock {
                    year: 2026,
                    month: 13,
                    day: 32,
                    hour: 24,
                    minute: 60,
                },
                "  Mon 32 ? 2026 \u{b7} 24:60 \u{b7} PC",
            ),
        ] {
            let ctx = PreviewContext {
                clock,
                ..PreviewContext::sample()
            };
            let rows = texts(&render_banner(&terracotta(), &ctx));
            assert!(rows[4].contains(line), "{clock:?}: {}", rows[4]);
            let framed = &rows[..rows.len() - 2];
            let cells: Vec<usize> = framed.iter().map(|r| banner_cells(r)).collect();
            assert!(
                cells.windows(2).all(|w| w[0] == w[1]),
                "{clock:?}: {cells:?} {framed:?}"
            );
        }
    }
}
