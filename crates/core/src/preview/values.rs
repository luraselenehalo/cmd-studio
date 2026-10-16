//! The text Oh My Posh and banner.lua make of the sample values.
use super::Clock;

/// The `path` segment's text for `cwd` (style `full`, `home_icon` `~`), as Oh My Posh 31.5.0
/// writes it on Windows: `/` becomes `\`, the drive letter is upper case, a trailing `\` goes
/// (a drive's root keeps it: `C:\`, and `D:` becomes `D:\`), and `home` or a folder under it
/// starts with `~`. `.` and `..` stay as they are.
///
/// A `home` that is a drive and nothing more, with or without separators after it (`C:`,
/// `C:\`, `d:/`, `C:\\`), holds every folder on that drive, as Oh My Posh 31.5.0 has it
/// (measured on the pinned binary by `tests/parity.rs`): a folder on that drive, in any case,
/// is `~` and what follows the drive (`C:\x\y` is `~\x\y`, the root itself `~`), and a folder
/// on another drive is shown in full.
///
/// Any other `home` is matched as Oh My Posh 31.5.0 matches it, quirk included (measured the
/// same way): both paths are lowered as Go's `strings.ToLower` lowers them, so `home` matches
/// in any case and the rest keeps its own, and the folder is then cut at the lowered home's
/// length in bytes ([`strip_lowered_prefix`]). Where lowering changes a letter's length (`İ`
/// 2 bytes to `i` 1, the Kelvin sign U+212A 3 bytes to `k` 1), the cut misses the end of the
/// home, and the folder is then mostly shown in full, even the home folder itself.
pub(crate) fn prompt_path(home: &str, cwd: &str) -> String {
    let cwd = normalize(cwd);
    if let Some(letter) = drive_alone(home) {
        return match after_drive(&cwd, letter) {
            Some("\\") => "~".to_string(),
            Some(rest) if rest.starts_with('\\') => format!("~{rest}"),
            _ => cwd,
        };
    }
    let home = normalize(home);
    if home.is_empty() {
        return cwd;
    }
    match strip_lowered_prefix(&cwd, &home) {
        Some("") => "~".to_string(),
        Some(rest) if rest.starts_with('\\') => format!("~{rest}"),
        _ => cwd,
    }
}

/// The drive letter of `home` when it is a drive and nothing more, separators aside: `C:`,
/// `C:\`, `d:/`, `C:\\`.
fn drive_alone(home: &str) -> Option<char> {
    let mut chars = home.chars();
    match (chars.next(), chars.next()) {
        (Some(letter), Some(':'))
            if letter.is_ascii_alphabetic() && chars.all(|c| c == '\\' || c == '/') =>
        {
            Some(letter)
        }
        _ => None,
    }
}

/// What follows the drive of `cwd` when that drive is `letter`, in any case (lowered as
/// [`go_lower`] lowers it).
fn after_drive(cwd: &str, letter: char) -> Option<&str> {
    let mut chars = cwd.chars();
    match (chars.next(), chars.next()) {
        (Some(drive), Some(':')) if go_lower_char(drive) == go_lower_char(letter) => {
            Some(chars.as_str())
        }
        _ => None,
    }
}

/// `text` after `prefix`, the way Oh My Posh cuts it: when [`go_lower`] of `text` starts with
/// [`go_lower`] of `prefix`, `text` itself is cut at the lowered prefix's length in bytes.
/// `None` when the lowered text does not start so, or when the cut falls inside a character
/// (Go would leave part of one there, which is never a `\`).
fn strip_lowered_prefix<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let prefix = go_lower(prefix);
    if !go_lower(text).starts_with(&prefix) {
        return None;
    }
    text.get(prefix.len()..)
}

/// `text` lowered as Go's `strings.ToLower` lowers it: each character by itself with the simple
/// mapping, so `İ` (U+0130) becomes `i`, and a character whose lower case is more than one
/// character stays as it is.
fn go_lower(text: &str) -> String {
    text.chars().map(go_lower_char).collect()
}

/// One character lowered as [`go_lower`] lowers it.
fn go_lower_char(c: char) -> char {
    if c == '\u{130}' {
        return 'i';
    }
    let mut lower = c.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(one), None) => one,
        _ => c,
    }
}

/// A Windows path the way the path segment starts from it: `\` only, an upper case drive
/// letter, no trailing `\` except after a drive (`C:\`).
fn normalize(path: &str) -> String {
    let slashes = path.replace('/', "\\");
    let mut chars = slashes.chars();
    let lettered = match (chars.next(), chars.next()) {
        (Some(letter), Some(':')) if letter.is_ascii_alphabetic() => {
            format!("{}:{}", letter.to_ascii_uppercase(), chars.as_str())
        }
        _ => slashes.clone(),
    };
    let trimmed = lettered.trim_end_matches('\\');
    let mut rest = trimmed.chars();
    let is_drive = matches!(
        (rest.next(), rest.next(), rest.next()),
        (Some(letter), Some(':'), None) if letter.is_ascii_alphabetic()
    );
    if is_drive {
        format!("{trimmed}\\")
    } else if trimmed.is_empty() && !lettered.is_empty() {
        "\\".to_string()
    } else {
        trimmed.to_string()
    }
}

/// `.FormattedMs` in Oh My Posh's `austin` style: `2s`, `2.5s`, `1m 0s`, `1h 1m 1.001s`,
/// `1d 0h 0m 0s`. The seconds keep up to three decimals with no trailing zeros. Measured on
/// Oh My Posh 31.5.0 from 2000 ms to 2 days (`tests/parity.rs`); the segment shows nothing
/// under 2000 ms, so shorter durations never reach this.
pub(crate) fn austin(ms: u64) -> String {
    const SECOND: u64 = 1000;
    const MINUTE: u64 = 60 * SECOND;
    const HOUR: u64 = 60 * MINUTE;
    const DAY: u64 = 24 * HOUR;
    let within_minute = ms % MINUTE;
    let (whole, thousandths) = (within_minute / SECOND, within_minute % SECOND);
    let seconds = if thousandths == 0 {
        format!("{whole}s")
    } else {
        let digits = format!("{thousandths:03}");
        format!("{whole}.{}s", digits.trim_end_matches('0'))
    };
    if ms >= DAY {
        format!(
            "{}d {}h {}m {seconds}",
            ms / DAY,
            ms % DAY / HOUR,
            ms % HOUR / MINUTE
        )
    } else if ms >= HOUR {
        format!("{}h {}m {seconds}", ms / HOUR, ms % HOUR / MINUTE)
    } else if ms >= MINUTE {
        format!("{}m {seconds}", ms / MINUTE)
    } else {
        seconds
    }
}

/// `HH:MM`: the time segment (Go's `15:04`) and banner.lua's `os.date("%H:%M")` alike.
pub(crate) fn hh_mm(clock: &Clock) -> String {
    format!("{:02}:{:02}", clock.hour, clock.minute)
}

/// The day of the week, 1 for Sunday as `os.date("*t").wday` counts, by Tomohiko Sakamoto's
/// method for the Gregorian calendar. It is total, because the app sends the clock and nothing
/// checks it: a month out of range takes January's offset (month 0 counts in the year before,
/// as January does, and 13 and up in the year itself), any day is added as it is, year 0 is
/// not moved back to a year before it, and the answer is always 1 to 7.
pub(crate) fn weekday(clock: &Clock) -> u8 {
    const OFFSETS: [u32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let month = usize::from(clock.month);
    let offset = month
        .checked_sub(1)
        .and_then(|i| OFFSETS.get(i))
        .copied()
        .unwrap_or(0);
    let year = u32::from(clock.year) - u32::from(clock.month < 3 && clock.year > 0);
    let day = (year + year / 4 - year / 100 + year / 400 + offset + u32::from(clock.day)) % 7;
    // `day` is 0 to 6, 0 for Sunday
    u8::try_from(day).map_or(1, |d| d + 1)
}

/// What banner.lua's `clean` leaves of a user name, a computer name or a folder: it drops the
/// C0 controls and DEL, the C1 controls (U+0080 to U+009F), U+2028 to U+202E and U+2066 to
/// U+2069, each on its own (the rest of an escape sequence stays as text).
pub(crate) fn lua_clean(text: &str) -> String {
    text.chars()
        .filter(|c| {
            !matches!(
                u32::from(*c),
                0x00..=0x1F | 0x7F..=0x9F | 0x2028..=0x202E | 0x2066..=0x2069
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_path_is_shown_as_oh_my_posh_shows_it() {
        // each pair measured on Oh My Posh 31.5.0 with home C:\Users\someone
        let home = r"C:\Users\someone";
        for (cwd, shown) in [
            (r"C:\Users\someone", "~"),
            (r"C:\Users\someone\", "~"),
            (r"C:\Users\someone\projects\x", r"~\projects\x"),
            (r"c:\users\SOMEONE\projects", r"~\projects"),
            (r"C:\Users\someoneelse\x", r"C:\Users\someoneelse\x"),
            (r"D:\work\x", r"D:\work\x"),
            (r"D:\work\", r"D:\work"),
            ("D:", r"D:\"),
            (r"C:\", r"C:\"),
            (r"c:\x", r"C:\x"),
            ("C:/Users/someone/fwd", r"~\fwd"),
            (r"C:\Users\someone\a\..\b", r"~\a\..\b"),
            (r"C:\Users\someone\a b\c", r"~\a b\c"),
        ] {
            assert_eq!(prompt_path(home, cwd), shown, "{cwd}");
        }
        // a home with a trailing separator, and one in another case
        assert_eq!(
            prompt_path(r"C:\Users\someone\", r"C:\Users\someone\x"),
            r"~\x"
        );
        assert_eq!(
            prompt_path(r"c:\users\someone", r"C:\Users\Someone\X"),
            r"~\X"
        );
        // Thai letters in the user name and the folder
        assert_eq!(
            prompt_path(r"C:\Users\ผู้ทดสอบ ไทย", r"C:\Users\ผู้ทดสอบ ไทย\เอกสาร"),
            r"~\เอกสาร"
        );
        // no home: the folder as it is
        assert_eq!(prompt_path("", r"C:\x\"), r"C:\x");
    }

    #[test]
    fn the_home_is_matched_as_oh_my_posh_matches_it_quirk_and_all() {
        // (the home, --pwd, what Oh My Posh 31.5.0 printed), measured by tests/parity.rs.
        // Both are lowered as Go lowers them and the folder is cut at the lowered home's length
        // in bytes, so a letter whose lower case is shorter (`İ` 2 bytes to `i` 1, the Kelvin
        // sign U+212A 3 bytes to `k` 1) moves the cut.
        let rows = [
            (r"C:\Users\İsa", r"c:\users\isa\x", r"~\x"),
            (r"C:\Users\isa", r"c:\users\İsa\x", r"C:\users\İsa\x"),
            // the very folder of the home, and still no `~`
            (r"C:\Users\İsa", r"C:\Users\İsa\x", r"C:\Users\İsa\x"),
            (
                r"C:\Users\İbrahim",
                r"C:\Users\İbrahim\Documents",
                r"C:\Users\İbrahim\Documents",
            ),
            ("C:\\Users\\\u{212a}", r"c:\users\k\x", r"~\x"),
            // the cut falls inside the Kelvin sign
            (
                r"C:\Users\k",
                "c:\\users\\\u{212a}\\x",
                "C:\\users\\\u{212a}\\x",
            ),
            (r"C:\Users\Ünal", r"c:\users\ünal\x", r"~\x"),
        ];
        let wrong: Vec<String> = rows
            .iter()
            .filter_map(|(home, cwd, shown)| {
                let got = prompt_path(home, cwd);
                (got != *shown).then(|| format!("{home} | {cwd}: {got}, not {shown}"))
            })
            .collect();
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    #[test]
    fn a_home_at_a_drives_root_holds_every_folder_on_that_drive() {
        // (the home, --pwd, what Oh My Posh 31.5.0 printed), measured on the pinned binary by
        // tests/parity.rs (the `D:\` row by the final review's probe). A home that is a drive
        // and nothing more holds every folder on that drive, in any case, a bare `C:` as much
        // as `C:\`; a folder on another drive is shown in full.
        let rows = [
            (r"C:\", r"C:\x\y", r"~\x\y"),
            (r"D:\", r"D:\x\y", r"~\x\y"),
            ("d:/", r"D:\x\y", r"~\x\y"),
            (r"C:\\", r"C:\x\y", r"~\x\y"),
            (r"C:\", r"C:\", "~"),
            (r"C:\", "C:", "~"),
            (r"C:\", "c:/x", r"~\x"),
            ("C:", r"C:\x\y", r"~\x\y"),
            ("C:", r"C:\Users", r"~\Users"),
            ("C:", r"C:\", "~"),
            ("C:", "C:", "~"),
            // a trailing separator after the folder
            ("d:/", r"d:\x\", r"~\x"),
            // another drive
            (r"C:\", r"D:\x", r"D:\x"),
            ("C:", r"D:\x\y", r"D:\x\y"),
        ];
        let wrong: Vec<String> = rows
            .iter()
            .filter_map(|(home, cwd, shown)| {
                let got = prompt_path(home, cwd);
                (got != *shown).then(|| format!("{home} | {cwd}: {got}, not {shown}"))
            })
            .collect();
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    #[test]
    fn letters_are_lowered_one_by_one_as_go_lowers_them() {
        // Go's simple mapping makes `İ` a plain `i`; Rust's `to_lowercase` adds a combining dot
        assert_eq!("\u{130}".to_lowercase(), "i\u{307}");
        assert_eq!(go_lower(r"C:\Users\İsa"), r"c:\users\isa");
        assert_eq!(go_lower("\u{212a}"), "k");
        assert_eq!(go_lower("\u{dc}NAL"), "\u{fc}nal");
        // letters with no case, Thai among them, stay as they are
        assert_eq!(go_lower(r"\ผู้ทดสอบ 1"), r"\ผู้ทดสอบ 1");
    }

    #[test]
    fn durations_are_written_in_the_austin_style() {
        // each pair measured on Oh My Posh 31.5.0
        for (ms, shown) in [
            (2000, "2s"),
            (2001, "2.001s"),
            (2010, "2.01s"),
            (2049, "2.049s"),
            (2100, "2.1s"),
            (3500, "3.5s"),
            (59_999, "59.999s"),
            (60_000, "1m 0s"),
            (60_001, "1m 0.001s"),
            (61_500, "1m 1.5s"),
            (119_999, "1m 59.999s"),
            (3_599_999, "59m 59.999s"),
            (3_600_000, "1h 0m 0s"),
            (3_661_001, "1h 1m 1.001s"),
            (86_399_999, "23h 59m 59.999s"),
            (86_400_000, "1d 0h 0m 0s"),
            (90_061_001, "1d 1h 1m 1.001s"),
            (172_800_000, "2d 0h 0m 0s"),
        ] {
            assert_eq!(austin(ms), shown, "{ms}");
        }
    }

    #[test]
    fn the_time_has_two_digits_for_hours_and_minutes() {
        let clock = |hour, minute| Clock {
            year: 2026,
            month: 1,
            day: 1,
            hour,
            minute,
        };
        assert_eq!(hh_mm(&clock(7, 5)), "07:05");
        assert_eq!(hh_mm(&clock(16, 47)), "16:47");
        assert_eq!(hh_mm(&clock(0, 0)), "00:00");
    }

    #[test]
    fn weekdays_follow_the_calendar() {
        let on = |year, month, day| {
            weekday(&Clock {
                year,
                month,
                day,
                hour: 0,
                minute: 0,
            })
        };
        assert_eq!(on(2026, 10, 9), 6, "spec §3.6: Friday 9 October 2026");
        assert_eq!(on(2026, 10, 11), 1, "a Sunday");
        assert_eq!(on(2000, 1, 1), 7, "a Saturday in January");
        assert_eq!(on(2000, 2, 29), 3, "a leap day, Tuesday");
        assert_eq!(on(1900, 3, 1), 5, "1900 was no leap year: Thursday");
        assert_eq!(on(2024, 12, 31), 3, "a Tuesday");
        // out of range never panics
        assert!((1..=7).contains(&on(2026, 0, 0)));
        assert!((1..=7).contains(&on(2026, 13, 40)));
        assert!((1..=7).contains(&on(0, 1, 1)));
    }

    #[test]
    fn the_weekday_is_one_to_seven_for_every_clock_the_app_can_send() {
        // The app sends the clock and nothing checks its fields (Task 2's review), so every
        // month and day a `u8` holds, and the years at the edges of a `u16`, give a day of the
        // week and never a panic or an overflow. Out of range, the day is what the formula
        // gives (see `weekday`), pinned below.
        for year in [0, 1, 2, 3, 4, 99, 100, 400, 2026, u16::MAX - 1, u16::MAX] {
            for month in 0..=u8::MAX {
                for day in 0..=u8::MAX {
                    let clock = Clock {
                        year,
                        month,
                        day,
                        hour: u8::MAX,
                        minute: u8::MAX,
                    };
                    assert!((1..=7).contains(&weekday(&clock)), "{clock:?}");
                }
            }
        }
        // the fallbacks, pinned
        let at = |year, month, day| {
            weekday(&Clock {
                year,
                month,
                day,
                hour: 0,
                minute: 0,
            })
        };
        assert_eq!(at(0, 0, 0), 1);
        assert_eq!(at(2026, 13, 32), 2);
        assert_eq!(at(u16::MAX, u8::MAX, u8::MAX), 6);
    }

    #[test]
    fn lua_clean_drops_exactly_what_banner_lua_drops() {
        let dropped: Vec<char> = (0x00..=0x1F)
            .chain(0x7F..=0x9F)
            .chain(0x2028..=0x202E)
            .chain(0x2066..=0x2069)
            .filter_map(char::from_u32)
            .collect();
        for c in dropped {
            assert_eq!(lua_clean(&format!("a{c}b")), "ab", "U+{:04X}", u32::from(c));
        }
        for c in [
            ' ', '~', '\u{a0}', '\u{2027}', '\u{202f}', '\u{2065}', '\u{206a}', 'ก', '👋',
        ] {
            let s = format!("a{c}b");
            assert_eq!(lua_clean(&s), s, "U+{:04X}", u32::from(c));
        }
        // the escape character goes, the rest of its sequence stays
        assert_eq!(lua_clean("x\u{1b}[31my"), "x[31my");
    }
}
