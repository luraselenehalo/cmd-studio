//! How many cells of the window a text takes, as the two programs that lay out the preview
//! count them, and how a line longer than the window wraps.
use super::{Line, Style};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// The cells of one character as Oh My Posh counts them: 0 for a combining mark (the Thai
/// vowel and tone marks above and below a letter), a joiner or a variation selector, 2 for a
/// wide character (Chinese, Japanese, Korean, most emoji), 1 for the rest.
fn char_cells(c: char) -> usize {
    c.width().unwrap_or(0)
}

/// The cells `text` takes as Oh My Posh 31.5.0 counts them when it fills the box's top line:
/// character by character, so a joined emoji counts each of its parts. Checked against the
/// real binary in `tests/parity.rs` (Thai, Chinese, an emoji and a family emoji in the path).
pub fn prompt_cells(text: &str) -> usize {
    text.chars().map(char_cells).sum()
}

/// The cells `text` takes as `console.cellcount` counts them in a Windows Terminal tab, which
/// banner.lua pads the welcome box by: Clink 1.9.34 on Windows 10 and 11 gives a combining
/// mark no cell of its own and a joined emoji two cells (its `wcwidth_iter`). conhost is
/// different: there Clink gives each combining mark a cell, because conhost draws it in one.
/// The preview draws Windows Terminal.
pub fn banner_cells(text: &str) -> usize {
    text.width()
}

/// `line` cut into rows of at most `width` cells, the way the window wraps it: a character
/// that does not fit starts the next row, and a character with no cells of its own stays
/// with the one before it. A width of 0 is taken as 1.
pub(crate) fn wrap(line: &Line, width: usize) -> Vec<Line> {
    let width = width.max(1);
    let mut rows = vec![Line::default()];
    let mut col = 0;
    for span in &line.spans {
        let style: Style = span.style();
        for c in span.text.chars() {
            let cells = char_cells(c);
            if cells > 0 && col > 0 && col + cells > width {
                rows.push(Line::default());
                col = 0;
            }
            if let Some(row) = rows.last_mut() {
                row.push(c.encode_utf8(&mut [0; 4]), style);
            }
            col += cells;
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgb;

    #[test]
    fn prompt_cells_count_thai_marks_as_nothing_and_wide_characters_as_two() {
        // measured on Oh My Posh 31.5.0 through the box filler (see tests/parity.rs)
        for (text, cells) in [
            ("abc", 3),
            ("ผู้ทดสอบ", 6),
            ("ที่นี่", 2),
            ("中文", 4),
            ("📁x", 3),
            ("·×①", 3),
            ("\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}", 6),
            ("\u{2764}\u{fe0f}", 1),
            ("ﾊ가", 3),
            ("\u{e0b0}\u{e0a0}╭─✓✗❯", 7),
            // SARA AM takes a cell of its own, after the letter and its tone mark; the preview
            // (app/ui/src/components/cells.ts) boxes น้ำ as one glyph two cells wide
            ("น้ำ", 2),
            ("สำนักงาน", 7),
            // a zero-width space takes none; U+2329 and U+232A are wide
            ("a\u{200b}b", 2),
            ("\u{2329}x\u{232a}", 5),
        ] {
            assert_eq!(prompt_cells(text), cells, "{text:?}");
        }
    }

    #[test]
    fn banner_cells_join_emoji_sequences_like_clink_in_windows_terminal() {
        for (text, cells) in [
            ("abc", 3),
            ("มี.ค.", 4),
            ("ผู้ทดสอบ", 6),
            ("中文", 4),
            ("👋", 2),
            ("\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}", 2),
            ("\u{2764}\u{fe0f}", 2),
            ("✻ Welcome", 9),
            ("น้ำ", 2),
            ("สำนักงาน", 7),
            ("a\u{200b}b", 2),
            ("\u{2329}x\u{232a}", 5),
        ] {
            assert_eq!(banner_cells(text), cells, "{text:?}");
        }
    }

    #[test]
    fn wrap_cuts_at_the_width_and_keeps_wide_characters_whole() {
        let s = Style::fg(Rgb { r: 1, g: 2, b: 3 });
        let mut line = Line::default();
        line.push("abcde中f", s);
        let rows: Vec<String> = wrap(&line, 3).iter().map(Line::text).collect();
        // `中` needs two cells and only one is left after `abcde`, so it starts a row
        assert_eq!(rows, ["abc", "de", "中f"]);
        // a combining mark stays on the row of its letter
        let mut thai = Line::default();
        thai.push("กี่ขค", s);
        let rows: Vec<String> = wrap(&thai, 2).iter().map(Line::text).collect();
        assert_eq!(rows, ["กี่ข", "ค"]);
        // a line that fits is one row, and an empty line is one empty row
        assert_eq!(wrap(&line, 80).len(), 1);
        assert_eq!(wrap(&Line::default(), 80), vec![Line::default()]);
        // the width is at least 1
        assert_eq!(wrap(&line, 0).len(), 7);
    }

    #[test]
    fn wrap_keeps_a_mark_with_a_wide_character_even_in_a_window_too_narrow_for_it() {
        // at width 1 the wide character already overfills its row, and its mark still joins it
        let mut line = Line::default();
        line.push("中\u{301}x", Style::fg(Rgb { r: 1, g: 2, b: 3 }));
        let rows: Vec<String> = wrap(&line, 1).iter().map(Line::text).collect();
        assert_eq!(rows, ["中\u{301}", "x"]);
    }

    #[test]
    fn wrap_keeps_every_style() {
        let (a, b) = (
            Style::fg(Rgb { r: 1, g: 1, b: 1 }),
            Style::fg(Rgb { r: 2, g: 2, b: 2 }),
        );
        let mut line = Line::default();
        line.push("ab", a);
        line.push("cd", b);
        let rows = wrap(&line, 3);
        assert_eq!(rows[0].spans.len(), 2);
        assert_eq!(rows[0].spans[1].text, "c");
        assert_eq!(rows[1].spans[0].fg, Rgb { r: 2, g: 2, b: 2 });
    }
}
