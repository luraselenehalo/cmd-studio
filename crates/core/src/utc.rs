//! UTC times as text (recovery spec §4.3, §4.5): what Windows' event log writes in an event's
//! `TimeCreated SystemTime`, read into milliseconds since 1970, and the same written for people.
//! Howard Hinnant's civil-from-days and days-from-civil algorithms; nothing reads the clock.

/// `YYYY-MM-DDTHH:MM:SSZ` from milliseconds since 1970.
pub fn iso(ms: u64) -> String {
    let secs = ms / 1000;
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    format!("{year:04}-{month:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

/// Milliseconds since 1970 from `YYYY-MM-DDTHH:MM:SS[.fraction]Z`, the form of an event's
/// `SystemTime` (`2026-10-17T03:15:42.1234567Z`). Digits of the fraction past the third are
/// dropped. `None` for any other text, or a time before 1970.
pub fn parse(text: &str) -> Option<u64> {
    let text = text.trim().strip_suffix('Z')?;
    let (date, time) = text.split_once('T')?;
    let mut date = date.splitn(3, '-');
    let year: u64 = number(date.next()?)?;
    let month: u64 = number(date.next()?)?;
    let day: u64 = number(date.next()?)?;
    let (clock, fraction) = time.split_once('.').unwrap_or((time, ""));
    let mut clock = clock.splitn(3, ':');
    let hour: u64 = number(clock.next()?)?;
    let minute: u64 = number(clock.next()?)?;
    let second: u64 = number(clock.next()?)?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 {
        return None;
    }
    if second > 60 || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let millis: String = fraction
        .chars()
        .chain(std::iter::repeat('0'))
        .take(3)
        .collect();
    let millis: u64 = millis.parse().ok()?;
    let days = days_from_civil(year, month, day)?;
    // A year too far out for milliseconds in a u64 is no time, never an overflow.
    days.checked_mul(86_400)?
        .checked_add(hour * 3600 + minute * 60 + second)?
        .checked_mul(1000)?
        .checked_add(millis)
}

/// Digits only: `str::parse` alone would take a leading `+`.
fn number(text: &str) -> Option<u64> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// Days since 1970-01-01 of a date in the proleptic Gregorian calendar; `None` before 1970.
fn days_from_civil(year: u64, month: u64, day: u64) -> Option<u64> {
    let y = if month <= 2 {
        year.checked_sub(1)?
    } else {
        year
    };
    let era = y / 400;
    let yoe = y - era * 400;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era.checked_mul(146_097)?
        .checked_add(doe)?
        .checked_sub(719_468)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_reads_the_same_instant() {
        assert_eq!(iso(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso(1_791_694_709_000), "2026-10-11T04:58:29Z");
        assert_eq!(iso(951_782_400_000), "2000-02-29T00:00:00Z");
        for ms in [0, 951_782_400_000, 1_791_694_709_000, 4_102_444_799_000] {
            assert_eq!(parse(&iso(ms)), Some(ms), "{ms}");
        }
    }

    #[test]
    fn reads_the_event_logs_form_with_its_fraction() {
        assert_eq!(
            parse("2026-10-11T04:58:29.1234567Z"),
            Some(1_791_694_709_123)
        );
        assert_eq!(parse("2026-10-11T04:58:29.5Z"), Some(1_791_694_709_500));
        assert_eq!(
            parse("2026-10-11T04:58:29.000000000Z"),
            Some(1_791_694_709_000)
        );
        assert_eq!(parse(" 2026-10-11T04:58:29Z "), Some(1_791_694_709_000));
    }

    #[test]
    fn anything_else_is_none() {
        for text in [
            "",
            "2026-10-11T04:58:29",
            "2026-10-11 04:58:29Z",
            "2026-13-11T04:58:29Z",
            "2026-10-32T04:58:29Z",
            "2026-10-11T24:00:00Z",
            "2026-10-11T04:60:00Z",
            "2026-10-11T04:58:29.12aZ",
            "+2026-10-11T04:58:29Z",
            "1969-12-31T23:59:59Z",
            "2026-10-11T04:58Z",
            // too far out for milliseconds in a u64: None, never an overflow
            "18446744073709551615-10-11T04:58:29Z",
            "584556020-01-01T00:00:00Z",
        ] {
            assert_eq!(parse(text), None, "{text:?}");
        }
    }
}
