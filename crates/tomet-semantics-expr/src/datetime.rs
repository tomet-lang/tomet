//! Datetimes: reading them from text and writing them with a format string.

use time::format_description::well_known::Rfc3339;
use time::{Date, Month, OffsetDateTime, PrimitiveDateTime, Time, UtcOffset};

/// Reads RFC 3339 (`2026-10-17T09:00:00+09:00`) or a bare date
/// (`2026-10-17`, taken as midnight UTC). `None` for anything else.
///
/// What the vault uses to turn a `datetime` field into a value.
pub fn parse_datetime(text: &str) -> Option<OffsetDateTime> {
    let text = text.trim();
    if let Ok(parsed) = OffsetDateTime::parse(text, &Rfc3339) {
        return Some(parsed);
    }
    let mut parts = text.split('-');
    let (year, month, day) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() || year.len() != 4 || month.len() != 2 || day.len() != 2 {
        return None;
    }
    let date = Date::from_calendar_date(
        year.parse().ok()?,
        Month::try_from(month.parse::<u8>().ok()?).ok()?,
        day.parse().ok()?,
    )
    .ok()?;
    Some(PrimitiveDateTime::new(date, Time::MIDNIGHT).assume_utc())
}

/// Reads a UTC offset written `+09:00`, `-05:30` or `Z`. `None` for anything else.
pub fn parse_offset(text: &str) -> Option<UtcOffset> {
    let text = text.trim();
    if text == "Z" {
        return Some(UtcOffset::UTC);
    }
    let (sign, rest) = match text.as_bytes().first()? {
        b'+' => (1i8, &text[1..]),
        b'-' => (-1i8, &text[1..]),
        _ => return None,
    };
    let (hours, minutes) = rest.split_once(':')?;
    if hours.len() != 2 || minutes.len() != 2 {
        return None;
    }
    let (hours, minutes) = (hours.parse::<i8>().ok()?, minutes.parse::<i8>().ok()?);
    // No offset in use is further than 18 hours from UTC.
    if !(0..=18).contains(&hours) || !(0..60).contains(&minutes) {
        return None;
    }
    UtcOffset::from_hms(sign * hours, sign * minutes, 0).ok()
}

/// Writes `value` with `format`. The symbols are `YYYY` `YY` `MM` (month)
/// `DD` `HH` `mm` (minute) and `ss`; every other character is copied as it
/// is, so `YYYY年MM月DD日` is a format. The value's own offset is used and
/// never converted.
pub fn format_datetime(value: &OffsetDateTime, format: &str) -> String {
    let symbols: [(&str, String); 7] = [
        ("YYYY", format!("{:04}", value.year())),
        ("YY", format!("{:02}", value.year().rem_euclid(100))),
        ("MM", format!("{:02}", u8::from(value.month()))),
        ("DD", format!("{:02}", value.day())),
        ("HH", format!("{:02}", value.hour())),
        ("mm", format!("{:02}", value.minute())),
        ("ss", format!("{:02}", value.second())),
    ];
    let mut out = String::new();
    let mut rest = format;
    'next: while !rest.is_empty() {
        for (symbol, text) in &symbols {
            if let Some(after) = rest.strip_prefix(symbol) {
                out.push_str(text);
                rest = after;
                continue 'next;
            }
        }
        let mut chars = rest.chars();
        if let Some(c) = chars.next() {
            out.push(c);
        }
        rest = chars.as_str();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> OffsetDateTime {
        parse_datetime(text).unwrap_or_else(|| panic!("`{text}` should parse"))
    }

    #[test]
    fn rfc_3339_keeps_its_offset() {
        let dt = at("2026-10-17T09:05:07+09:00");
        assert_eq!(dt.offset().whole_hours(), 9);
        assert_eq!(format_datetime(&dt, "HH:mm:ss"), "09:05:07");
    }

    #[test]
    fn an_offset_is_read_with_its_sign() {
        assert_eq!(parse_offset("+09:00").unwrap().whole_hours(), 9);
        assert_eq!(parse_offset("-05:30").unwrap().whole_minutes(), -330);
        assert_eq!(parse_offset("Z"), Some(UtcOffset::UTC));
        for bad in ["", "09:00", "+9:00", "+09", "+25:00", "x"] {
            assert_eq!(parse_offset(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_bare_date_is_midnight_utc() {
        let dt = at("2026-01-05");
        assert_eq!(format_datetime(&dt, "YYYY-MM-DD HH:mm"), "2026-01-05 00:00");
        assert_eq!(dt.offset().whole_hours(), 0);
    }

    #[test]
    fn text_that_is_not_a_datetime_is_none() {
        for text in [
            "",
            "hello",
            "2026-13-01",
            "2026-1-1",
            "2026-01-01-01",
            "26-01-01",
        ] {
            assert_eq!(parse_datetime(text), None, "{text}");
        }
    }

    #[test]
    fn month_and_minute_are_told_apart_by_case() {
        let dt = at("2026-03-04T05:06:07Z");
        assert_eq!(format_datetime(&dt, "MM/mm"), "03/06");
        assert_eq!(format_datetime(&dt, "YY"), "26");
        assert_eq!(format_datetime(&dt, "YYYY"), "2026");
    }

    #[test]
    fn other_characters_pass_through() {
        let dt = at("2026-03-04T05:06:07Z");
        assert_eq!(format_datetime(&dt, "YYYY年MM月DD日"), "2026年03月04日");
        assert_eq!(format_datetime(&dt, "[DD] at HH"), "[04] at 05");
        assert_eq!(format_datetime(&dt, ""), "");
    }
}
