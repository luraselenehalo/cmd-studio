//! How many cells of the window a text takes, as the two programs that lay out the preview
//! count them.
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

#[cfg(test)]
mod tests {
    use super::*;

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
        ] {
            assert_eq!(banner_cells(text), cells, "{text:?}");
        }
    }
}
