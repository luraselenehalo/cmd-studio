//! Text helpers shared by the theme model and the generators (spec §3.7).

/// The line and paragraph separators (U+2028, U+2029) and the bidirectional embeddings,
/// overrides and isolates (U+202A-U+202E, U+2066-U+2069): line breaks and reordering
/// controls that are not C0/C1 controls. `sanitize_text` removes them and
/// `safe_file_stem` replaces them, so there is one list of them.
fn is_separator_or_bidi(c: char) -> bool {
    matches!(
        u32::from(c),
        0x2028 | 0x2029 | 0x202A..=0x202E | 0x2066..=0x2069
    )
}

/// Removes escape sequences and every C0/C1 control character (U+0000-U+001F
/// and U+007F-U+009F, newlines included), so text from a shared theme file can
/// never drive the terminal. ESC and `[` start a CSI sequence, which ends at its
/// final character (U+0040-U+007E); ESC and any other character is a
/// two-character escape. A sequence that never ends takes the rest of the text.
///
/// Two more groups go as well, because they are line breaks and bidirectional
/// controls that are not C0/C1: the line and paragraph separators U+2028 and
/// U+2029, and the bidi embeddings, overrides and isolates U+202A-U+202E and
/// U+2066-U+2069. With them gone, a field has one line and reads in the order it
/// is stored. Other invisible characters (the joiners that emoji and Thai need,
/// U+200D and U+FE0F) stay.
pub fn sanitize_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                // CSI: skip parameters and intermediates up to the final byte.
                for c in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&c) {
                        break;
                    }
                }
            } else {
                chars.next();
            }
            continue;
        }
        if !matches!(u32::from(ch), 0x00..=0x1F | 0x7F..=0x9F) && !is_separator_or_bidi(ch) {
            out.push(ch);
        }
    }
    out
}

/// Quotes `s` as a double-quoted Lua 5.2 string literal. Control characters
/// become three-digit decimal escapes; UTF-8 text passes through.
pub fn lua_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            c if u32::from(c) < 0x20 || u32::from(c) == 0x7F => {
                out.push_str(&format!("\\{:03}", u32::from(c)))
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Names Windows opens as devices and not as files, whatever follows them after
/// a dot. Microsoft's "Naming Files, Paths, and Namespaces" lists CON, PRN, AUX,
/// NUL, COM1-COM9, LPT1-LPT9 and the superscript forms `COM¹ COM² COM³ LPT¹
/// LPT² LPT³` (U+00B9, U+00B2 and U+00B3, which are not the digits 1, 2 and 3).
/// COM0 and LPT0 are not on that list; they are in the table on the side of
/// caution.
const RESERVED_NAMES: [&str; 30] = [
    "CON", "PRN", "AUX", "NUL", "COM0", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
    "COM8", "COM9", "COM¹", "COM²", "COM³", "LPT0", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6",
    "LPT7", "LPT8", "LPT9", "LPT¹", "LPT²", "LPT³",
];

/// The most characters `safe_file_stem` returns.
const MAX_STEM_CHARS: usize = 100;

/// Whether Windows would open `stem` as a device and not as a file. It looks
/// only at the part before the first dot (`NUL.txt` is NUL), ignores spaces at
/// the end of that part, and ignores case.
fn is_device_name(stem: &str) -> bool {
    let base = stem.split_once('.').map_or(stem, |(base, _)| base);
    let base = base.trim_end_matches(' ');
    RESERVED_NAMES
        .iter()
        .any(|name| name.eq_ignore_ascii_case(base))
}

/// Keeps the first `MAX_STEM_CHARS` characters, then drops the dots and spaces
/// at the end. Windows drops those without a word, so the file would not have
/// the name we asked for; the cut can leave them there as well as the name.
fn cut(s: &str) -> String {
    let kept: String = s.chars().take(MAX_STEM_CHARS).collect();
    kept.trim_end_matches(['.', ' ']).to_string()
}

/// Turns a theme name into a file name stem that Windows accepts: at most 100
/// characters, none of `< > : " / \ | ? *`, a control character, a line or
/// paragraph separator or a bidirectional control (U+2028, U+2029, U+202A-U+202E,
/// U+2066-U+2069), no dot or space at the end, and not a device name. A name with
/// nothing usable left becomes `theme`.
///
/// The bidirectional controls matter most here: a right-to-left override
/// (U+202E) makes the end of one file name read as the end of another in a file
/// manager. A name from a theme file has been cleaned of them already, but a name
/// built in code has not.
///
/// Windows opens `CON`, `nul.txt` and `com1.tar.gz` as devices, so a stem whose
/// part before the first dot is a device name gets an underscore right after
/// that part: `CON.foo` becomes `CON_.foo`. An underscore at the end of the
/// stem would not help, because the dot still comes right after `CON`.
pub fn safe_file_stem(name: &str) -> String {
    let replaced: String = name
        .chars()
        .map(|c| {
            if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
                || u32::from(c) < 0x20
                || is_separator_or_bidi(c)
            {
                '_'
            } else {
                c
            }
        })
        .collect();
    // `cut` also drops a dot or space at the end, whether the name or the cut left it.
    let stem = cut(replaced.trim());
    if stem.is_empty() {
        return "theme".to_string();
    }
    if !is_device_name(&stem) {
        return stem;
    }
    let guarded = match stem.split_once('.') {
        Some((base, rest)) => format!("{base}_.{rest}"),
        None => format!("{stem}_"),
    };
    // The underscore can make a full-length stem one character too long.
    cut(&guarded)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// The 30 device names the guard must catch, built here and not read from
    /// the module's own table.
    fn device_names() -> Vec<String> {
        let mut names: Vec<String> = ["CON", "PRN", "AUX", "NUL"]
            .into_iter()
            .map(String::from)
            .collect();
        for prefix in ["COM", "LPT"] {
            // 0-9, then the superscripts 1, 2 and 3 (U+00B9, U+00B2, U+00B3)
            for suffix in ('0'..='9').chain(['\u{b9}', '\u{b2}', '\u{b3}']) {
                names.push(format!("{prefix}{suffix}"));
            }
        }
        names
    }

    /// What `safe_file_stem` must never return: a stem whose part before the
    /// first dot, without spaces at its end, is a reserved name in any case.
    fn is_device(stem: &str, devices: &[String]) -> bool {
        let base = stem
            .split('.')
            .next()
            .unwrap_or_default()
            .trim_end_matches(' ');
        devices.contains(&base.to_uppercase())
    }

    // ---- sanitize_text ----

    #[test]
    fn strips_escape_sequences_and_controls() {
        assert_eq!(sanitize_text("a\u{1b}[31mred\u{1b}[0m\u{7}b\n"), "aredb");
        assert_eq!(sanitize_text("x\u{1b}cy"), "xy");
        assert_eq!(sanitize_text("x\u{9b}31my"), "x31my");
    }

    #[test]
    fn keeps_thai_and_emoji() {
        assert_eq!(sanitize_text("สวัสดี 👋"), "สวัสดี 👋");
    }

    #[test]
    fn exactly_the_c0_and_c1_controls_are_removed() {
        // spec §3.7 #3: U+0000-U+001F and U+007F-U+009F. ESC is left out here,
        // because it also takes the next character; the escape tests cover it.
        for code in (0x00..=0x1F)
            .chain(0x7F..=0x9F)
            .filter(|&code| code != 0x1B)
        {
            let c = char::from_u32(code).unwrap();
            assert_eq!(sanitize_text(&format!("a{c}b")), "ab", "U+{code:04X}");
        }
        // the characters just outside the two ranges stay
        for c in [' ', '~', '\u{a0}'] {
            let s = format!("a{c}b");
            assert_eq!(sanitize_text(&s), s, "U+{:04X}", u32::from(c));
        }
    }

    #[test]
    fn line_separators_and_bidi_controls_are_removed() {
        // spec §3.7 #3 allows no line break inside a field, and U+2028 and U+2029 are the
        // line and paragraph separators of Unicode. The bidi controls make a name read in
        // another order than it is stored, which is how a file name is made to look like
        // another one (U+202E) or an import preview is made to lie.
        let removed: Vec<u32> = [0x2028, 0x2029]
            .into_iter()
            .chain(0x202A..=0x202E)
            .chain(0x2066..=0x2069)
            .collect();
        assert_eq!(removed.len(), 11);
        for code in removed {
            let c = char::from_u32(code).unwrap();
            assert_eq!(sanitize_text(&format!("a{c}b")), "ab", "U+{code:04X}");
        }
        // the characters next to those ranges stay: U+2027 and U+202F on either side of the
        // first two, U+2065 and U+206A on either side of the third
        for c in ['\u{2027}', '\u{202f}', '\u{2065}', '\u{206a}'] {
            let s = format!("a{c}b");
            assert_eq!(sanitize_text(&s), s, "U+{:04X}", u32::from(c));
        }
        // and they go together with the sequences that were already removed
        assert_eq!(
            sanitize_text("a\u{1b}[31m\u{202e}b\u{2028}\nc\u{2066}"),
            "abc"
        );
        // Thai, emoji and the joiners that glue them stay
        assert_eq!(
            sanitize_text("ไทย 👋 👨\u{200d}👩 ❤\u{fe0f}"),
            "ไทย 👋 👨\u{200d}👩 ❤\u{fe0f}"
        );
    }

    #[test]
    fn csi_sequences_end_at_their_final_byte() {
        // Parameter bytes (0x30-0x3F) and intermediate bytes (0x20-0x2F) are
        // skipped; the final byte is anything in 0x40-0x7E, both ends included.
        for (input, want) in [
            ("a\u{1b}[38;2;217;119;87mb", "ab"), // ';' is a parameter byte
            ("a\u{1b}[?25lb", "ab"),             // so is '?' (0x3F)
            ("a\u{1b}[2 qb", "ab"),              // an intermediate byte, then 'q'
            ("a\u{1b}[1@b", "ab"),               // final byte '@' (0x40)
            ("a\u{1b}[3~b", "ab"),               // final byte '~' (0x7E)
            ("a\u{1b}[3\u{7f}mb", "ab"),         // DEL (0x7F) is not a final byte
        ] {
            assert_eq!(sanitize_text(input), want, "{input:?}");
        }
        // a sequence that is cut short takes nothing but itself...
        assert_eq!(sanitize_text("ab\u{1b}"), "ab");
        assert_eq!(sanitize_text("ab\u{1b}[31"), "ab");
        // ...but one that never reaches a final byte takes the rest of the text
        assert_eq!(sanitize_text("ab\u{1b}[31 ไทย"), "ab");
    }

    #[test]
    fn no_control_character_survives_whatever_the_input() {
        // spec §3.7 #3 as a property: every string of up to four of these pieces
        let pieces = [
            "\u{1b}", "[", "31", "m", "\u{9b}", "\n", "\u{7f}", "ก", "👋", "~", "?", " ",
        ];
        for input in combos(&pieces, 4) {
            let out = sanitize_text(&input);
            assert!(!out.chars().any(char::is_control), "{input:?} -> {out:?}");
            assert_eq!(sanitize_text(&out), out, "not stable for {input:?}");
        }
    }

    // ---- lua_str ----

    #[test]
    fn lua_str_escapes_quotes_backslashes_and_controls() {
        assert_eq!(lua_str(r#"a"b\c"#), r#""a\"b\\c""#);
        assert_eq!(lua_str("x]]y"), "\"x]]y\"");
        assert_eq!(lua_str("t\tz"), "\"t\\009z\"");
        assert_eq!(lua_str("ไทย"), "\"ไทย\"");
        assert_eq!(lua_str("\u{1b}[0m"), "\"\\027[0m\"");
        // spec §3.7 #4 names emoji as well
        assert_eq!(lua_str("👋"), "\"👋\"");
    }

    #[test]
    fn lua_str_leaves_no_raw_control_character() {
        // A raw newline would end a short Lua string, so every ASCII control
        // becomes a decimal escape...
        assert_eq!(lua_str("a\nb\rc\0d"), "\"a\\010b\\013c\\000d\"");
        // ...U+001F and DEL included, while the space between them is plain text...
        assert_eq!(lua_str("\u{1f} \u{7f}"), "\"\\031 \\127\"");
        // ...and the escape always has three digits, so a digit after it is not part of it.
        assert_eq!(lua_str("\u{1}2"), "\"\\0012\"");
    }

    #[test]
    fn joined_emoji_pass_through_both_helpers() {
        // U+200D (joiner) and U+FE0F (emoji variation selector) are format
        // characters, not controls: stripping or escaping them would break an emoji.
        for emoji in ["👨\u{200d}👩\u{200d}👧", "❤\u{fe0f}"] {
            assert_eq!(sanitize_text(emoji), emoji);
            assert_eq!(lua_str(emoji), format!("\"{emoji}\""));
        }
    }

    // ---- safe_file_stem ----

    #[test]
    fn file_stems_are_windows_safe() {
        // Review Focus 5
        assert_eq!(safe_file_stem("a/b:c*?"), "a_b_c__");
        assert_eq!(safe_file_stem("CON"), "CON_");
        assert_eq!(safe_file_stem("lpt1"), "lpt1_");
        assert_eq!(safe_file_stem("  ธีม.  "), "ธีม");
        assert_eq!(safe_file_stem("..."), "theme");
        assert_eq!(safe_file_stem(&"x".repeat(150)).chars().count(), 100);
    }

    #[test]
    fn every_forbidden_character_becomes_an_underscore() {
        for c in [
            '<', '>', ':', '"', '/', '\\', '|', '?', '*', '\u{0}', '\t', '\n', '\u{1f}',
        ] {
            assert_eq!(safe_file_stem(&format!("a{c}b")), "a_b", "{c:?}");
        }
        // U+0020 is the first character that is allowed
        assert_eq!(safe_file_stem("a b"), "a b");
    }

    #[test]
    fn line_separators_and_bidi_controls_become_underscores_in_a_file_stem() {
        // Defense in depth: names are cleaned on import, but a name built in code is not, and a
        // right-to-left override (U+202E) in a file name is the classic way to make the end of
        // one name read as the end of another in a file manager.
        let codes: Vec<u32> = [0x2028, 0x2029]
            .into_iter()
            .chain(0x202A..=0x202E)
            .chain(0x2066..=0x2069)
            .collect();
        assert_eq!(codes.len(), 11);
        for code in codes {
            let c = char::from_u32(code).unwrap();
            assert_eq!(safe_file_stem(&format!("a{c}b")), "a_b", "U+{code:04X}");
        }
        // the characters on either side of the ranges stay, and so do the joiners
        for c in [
            '\u{2027}', '\u{202f}', '\u{2065}', '\u{206a}', '\u{200d}', '\u{fe0f}',
        ] {
            let s = format!("a{c}b");
            assert_eq!(safe_file_stem(&s), s, "U+{:04X}", u32::from(c));
        }
        assert_eq!(safe_file_stem("report\u{202e}fdp.exe"), "report_fdp.exe");
        // a name of nothing else is underscores, like one of any other forbidden character
        assert_eq!(safe_file_stem("\u{202e}\u{2028}"), "__");
        // and it stays a stem Windows accepts: no dot or space left at the end
        assert_eq!(safe_file_stem("name.\u{202e}"), "name._");
        assert_eq!(safe_file_stem("CON\u{2066}.txt"), "CON_.txt");
    }

    #[test]
    fn a_name_with_nothing_usable_left_becomes_theme() {
        for name in ["", "   ", ".", "..", " . ", ". . ."] {
            assert_eq!(safe_file_stem(name), "theme", "{name:?}");
        }
    }

    #[test]
    fn a_device_name_is_judged_by_the_part_before_the_first_dot() {
        // Windows opens `CON.foo` as the device CON, whatever follows the dot,
        // so the underscore goes after the base. `CON.foo_` would still be CON.
        assert_eq!(safe_file_stem("CON.foo"), "CON_.foo");
        assert_eq!(safe_file_stem("nul.txt"), "nul_.txt");
        assert_eq!(safe_file_stem("Aux.cmdtheme"), "Aux_.cmdtheme");
        assert_eq!(safe_file_stem("com1.tar.gz"), "com1_.tar.gz");
        // Windows versions differ on spaces before the dot (`nul .txt`), so the
        // guard takes the strict reading and treats them as no help
        assert_eq!(safe_file_stem("NUL .txt"), "NUL _.txt");
        // a name that starts with the dot has no base, and a later dot ends a longer one
        assert_eq!(safe_file_stem(".con"), ".con");
        assert_eq!(safe_file_stem("a.con"), "a.con");
    }

    #[test]
    fn every_windows_device_name_is_reserved() {
        // COM0, LPT0 and the superscript forms count too
        let names = device_names();
        assert_eq!(names.len(), 30);
        for name in names {
            for spelled in [name.clone(), name.to_lowercase()] {
                assert_eq!(safe_file_stem(&spelled), format!("{spelled}_"));
                assert_eq!(
                    safe_file_stem(&format!("{spelled}.cmdtheme")),
                    format!("{spelled}_.cmdtheme")
                );
            }
        }
    }

    #[test]
    fn names_that_only_resemble_a_device_are_left_alone() {
        for name in [
            "CONSOLE",
            "COM",
            "COM10",
            "LPT10",
            "NULL",
            "xcon",
            "CON_",
            "COM\u{b9}\u{b9}",
        ] {
            assert_eq!(safe_file_stem(name), name);
        }
    }

    #[test]
    fn the_cut_to_100_characters_is_trimmed_again() {
        // a dot or a space that the cut leaves last would be dropped by Windows
        let a = "a".repeat(99);
        assert_eq!(safe_file_stem(&format!("{a}.bbb")), a);
        assert_eq!(safe_file_stem(&format!("{a} bbb")), a);
        // and when the cut leaves nothing, the name is `theme`, not empty
        assert_eq!(safe_file_stem(&format!("{}x", ".".repeat(100))), "theme");
        // the 100 counts characters, not bytes
        assert_eq!(safe_file_stem(&"ก".repeat(150)), "ก".repeat(100));
    }

    #[test]
    fn a_device_name_that_only_shows_after_the_cut_is_still_caught() {
        // this is no device name until the cut and the trim leave just `CON`
        assert_eq!(safe_file_stem(&format!("CON{}x", " ".repeat(100))), "CON_");
    }

    #[test]
    fn the_underscore_for_a_device_name_never_breaks_the_length_limit() {
        // `CON.` and 96 more characters is 100 already; the underscore makes 101
        let out = safe_file_stem(&format!("CON.{}", "x".repeat(150)));
        assert_eq!(out, format!("CON_.{}", "x".repeat(95)));
        assert_eq!(out.chars().count(), 100);
    }

    #[test]
    fn whatever_the_name_the_stem_is_safe() {
        // Review Focus 5 as a property: every string of up to four of these
        // pieces. The long piece makes the cut land on a dot, on a space and
        // inside a device name (96 + `CON` = 99, so the next character is the 100th).
        let long = "x".repeat(96);
        let pieces = [
            "CON",
            "nul",
            "com",
            "\u{b9}",
            "1",
            ".",
            " ",
            "/",
            "\t",
            "ก",
            "\u{202e}",
            "\u{2028}",
            long.as_str(),
        ];
        let devices = device_names();
        for input in combos(&pieces, 4) {
            let out = safe_file_stem(&input);
            let why = format!("{input:?} -> {out:?}");
            assert!(!out.is_empty(), "empty: {why}");
            assert!(out.chars().count() <= 100, "too long: {why}");
            assert!(
                !out.chars().any(|c| "<>:\"/\\|?*".contains(c)
                    || u32::from(c) < 0x20
                    || matches!(
                        u32::from(c),
                        0x2028 | 0x2029 | 0x202A..=0x202E | 0x2066..=0x2069
                    )),
                "forbidden character: {why}"
            );
            assert!(!out.ends_with(['.', ' ']), "ends in a dot or space: {why}");
            assert!(!is_device(&out, &devices), "device name: {why}");
        }
    }
}
