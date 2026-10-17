//! What text reads as a date, or a date and a time.
//!
//! The one definition of what a `datetime` field holds. The evaluator reads a
//! field with it, `tomet check` checks a field with it, and `tomet format`
//! decides with it what to rewrite as RFC 3339. It gives the parts and
//! nothing else -- no clock type -- so a crate that only needs to know
//! whether text reads as a datetime does not carry one.

/// A date, or a date and a time, as written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateTimeParts {
    pub year: u16,
    /// 1 to 12.
    pub month: u8,
    /// 1 to the length of the month.
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub nanosecond: u32,
    /// Seconds east of UTC. Zero when the text gave no offset.
    pub offset_seconds: i32,
}

/// Reads a date, or a date and a time: `2026-10-17`, `2026-10-17T09:00`,
/// `2026-10-17 09:00:00`, `2026-10-17T09:00:00.5+09:00`.
///
/// - The date is `YYYY-MM-DD`; with nothing after it the time is midnight.
/// - The time follows after a `T` or a space, as `HH:MM`, or `HH:MM:SS`
///   with an optional fraction.
/// - An offset (`Z`, `+09:00`) may close it. Without one the value is read
///   as UTC, which is what a bare date has always been.
///
/// `None` for anything else, including a day the month does not have.
pub fn parse_datetime_parts(text: &str) -> Option<DateTimeParts> {
    let text = text.trim();
    let (year, month, day) = parse_date(text.get(..10)?)?;
    let rest = &text[10..];
    let mut parts = DateTimeParts {
        year,
        month,
        day,
        hour: 0,
        minute: 0,
        second: 0,
        nanosecond: 0,
        offset_seconds: 0,
    };
    if rest.is_empty() {
        return Some(parts);
    }
    let time_text = rest.strip_prefix('T').or_else(|| rest.strip_prefix(' '))?;
    let (clock, offset_text) =
        time_text.split_at(time_text.find(['Z', '+', '-']).unwrap_or(time_text.len()));
    if !offset_text.is_empty() {
        parts.offset_seconds = parse_offset_seconds(offset_text)?;
    }
    (parts.hour, parts.minute, parts.second, parts.nanosecond) = parse_clock(clock)?;
    Some(parts)
}

/// Reads a UTC offset written `+09:00`, `-05:30` or `Z`, as seconds east of
/// UTC. `None` for anything else.
pub fn parse_offset_seconds(text: &str) -> Option<i32> {
    let text = text.trim();
    if text == "Z" {
        return Some(0);
    }
    let (sign, rest) = match text.as_bytes().first()? {
        b'+' => (1, &text[1..]),
        b'-' => (-1, &text[1..]),
        _ => return None,
    };
    let (hours, minutes) = rest.split_once(':')?;
    let (hours, minutes) = (two_digits(hours)?, two_digits(minutes)?);
    // No offset in use is further than 18 hours from UTC.
    if hours > 18 || minutes >= 60 {
        return None;
    }
    Some(sign * (i32::from(hours) * 3600 + i32::from(minutes) * 60))
}

/// `YYYY-MM-DD`.
fn parse_date(text: &str) -> Option<(u16, u8, u8)> {
    let mut parts = text.split('-');
    let (year, month, day) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() || year.len() != 4 || !year.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let (year, month, day) = (
        year.parse::<u16>().ok()?,
        two_digits(month)?,
        two_digits(day)?,
    );
    if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
        return None;
    }
    Some((year, month, day))
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// `HH:MM` or `HH:MM:SS`, the seconds with an optional `.fraction`: the hour,
/// minute, second and nanosecond.
fn parse_clock(text: &str) -> Option<(u8, u8, u8, u32)> {
    let mut parts = text.split(':');
    let (hours, minutes) = (parts.next()?, parts.next()?);
    let seconds = parts.next();
    if parts.next().is_some() {
        return None;
    }
    let (seconds, fraction) = match seconds {
        None => (0, ""),
        Some(s) => match s.split_once('.') {
            Some((_, "")) => return None,
            Some((whole, fraction)) => (two_digits(whole)?, fraction),
            None => (two_digits(s)?, ""),
        },
    };
    if fraction.len() > 9 || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let nanos = if fraction.is_empty() {
        0
    } else {
        format!("{fraction:0<9}").parse::<u32>().ok()?
    };
    let (hours, minutes) = (two_digits(hours)?, two_digits(minutes)?);
    if hours > 23 || minutes > 59 || seconds > 59 {
        return None;
    }
    Some((hours, minutes, seconds, nanos))
}

/// Exactly two ASCII digits, as a number.
fn two_digits(text: &str) -> Option<u8> {
    if text.len() == 2 && text.bytes().all(|b| b.is_ascii_digit()) {
        text.parse().ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str) -> DateTimeParts {
        parse_datetime_parts(text).unwrap_or_else(|| panic!("`{text}` should read"))
    }

    #[test]
    fn a_bare_date_is_midnight_utc() {
        let parts = read("2026-01-05");
        assert_eq!((parts.year, parts.month, parts.day), (2026, 1, 5));
        assert_eq!((parts.hour, parts.minute, parts.second), (0, 0, 0));
        assert_eq!(parts.offset_seconds, 0);
    }

    #[test]
    fn the_time_may_follow_after_t_or_a_space_and_drop_its_seconds() {
        for text in [
            "2026-10-17T09:05",
            "2026-10-17 09:05",
            "2026-10-17T09:05:00",
        ] {
            let parts = read(text);
            assert_eq!(
                (parts.hour, parts.minute, parts.second),
                (9, 5, 0),
                "{text}"
            );
        }
        assert_eq!(read("2026-10-17 09:05:07").second, 7);
    }

    #[test]
    fn a_missing_offset_is_utc_and_a_given_one_is_kept() {
        assert_eq!(read("2026-10-17T09:00").offset_seconds, 0);
        assert_eq!(read("2026-10-17T09:00Z").offset_seconds, 0);
        assert_eq!(read("2026-10-17T09:00+09:00").offset_seconds, 9 * 3600);
        assert_eq!(
            read("2026-10-17 09:00:00-05:30").offset_seconds,
            -(5 * 3600 + 1800)
        );
    }

    #[test]
    fn a_fraction_is_nanoseconds() {
        assert_eq!(read("2026-10-17T09:00:00.25").nanosecond, 250_000_000);
        assert_eq!(read("2026-10-17T09:00:00.000000001Z").nanosecond, 1);
    }

    #[test]
    fn a_day_the_month_does_not_have_is_not_read() {
        assert!(parse_datetime_parts("2026-02-28").is_some());
        assert!(parse_datetime_parts("2026-02-29").is_none());
        assert!(parse_datetime_parts("2028-02-29").is_some());
        assert!(parse_datetime_parts("2100-02-29").is_none());
        assert!(parse_datetime_parts("2000-02-29").is_some());
        assert!(parse_datetime_parts("2026-04-31").is_none());
        assert!(parse_datetime_parts("2026-04-30").is_some());
    }

    #[test]
    fn text_that_is_not_a_datetime_is_none() {
        for text in [
            "",
            "hello",
            "2026-13-01",
            "2026-00-10",
            "2026-01-00",
            "2026-1-1",
            "2026-01-01-01",
            "26-01-01",
            "+026-01-01",
            "2026-01-01T",
            "2026-01-01T9:00",
            "2026-01-01T09",
            "2026-01-01T09:00:00:00",
            "2026-01-01T24:00",
            "2026-01-01T09:60",
            "2026-01-01T09:00:60",
            "2026-01-01T09:00:00.",
            "2026-01-01T09:00:00.1234567890",
            "2026-01-01T09:00garbage",
            "2026-01-01T09:00+9:00",
            "2026-01-01T09:00+19:00",
            "2026-01-01t09:00",
            "2026-01-01x09:00",
        ] {
            assert_eq!(parse_datetime_parts(text), None, "{text}");
        }
    }

    #[test]
    fn an_offset_is_read_with_its_sign() {
        assert_eq!(parse_offset_seconds("+09:00"), Some(9 * 3600));
        assert_eq!(parse_offset_seconds("-05:30"), Some(-(5 * 3600 + 1800)));
        assert_eq!(parse_offset_seconds("Z"), Some(0));
        for bad in ["", "09:00", "+9:00", "+09", "+25:00", "+09:60", "x"] {
            assert_eq!(parse_offset_seconds(bad), None, "{bad}");
        }
    }
}
