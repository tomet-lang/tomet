//! Datetimes: reading them from text and writing them with a format string.

use time::{Date, Month, OffsetDateTime, PrimitiveDateTime, Time, UtcOffset};
use tomet_type::datetime::{parse_datetime_parts, parse_offset_seconds};

use super::error::EvalError;
use super::value::EvalValue;

/// Reads a date, or a date and a time, as a datetime.
///
/// What text reads as one is `tomet_type::datetime`'s to say -- the same
/// reading `tomet check` and `tomet format` use. This only gives the parts a
/// clock type; a value with no offset is UTC.
///
/// What the vault uses to turn a `datetime` field into a value.
pub fn parse_datetime(text: &str) -> Option<OffsetDateTime> {
    let parts = parse_datetime_parts(text)?;
    let date = Date::from_calendar_date(
        i32::from(parts.year),
        Month::try_from(parts.month).ok()?,
        parts.day,
    )
    .ok()?;
    let time =
        Time::from_hms_nano(parts.hour, parts.minute, parts.second, parts.nanosecond).ok()?;
    let offset = UtcOffset::from_whole_seconds(parts.offset_seconds).ok()?;
    Some(PrimitiveDateTime::new(date, time).assume_offset(offset))
}

/// Reads a UTC offset written `+09:00`, `-05:30` or `Z`. `None` for anything else.
pub fn parse_offset(text: &str) -> Option<UtcOffset> {
    UtcOffset::from_whole_seconds(parse_offset_seconds(text)?).ok()
}

/// `value.member` for a datetime, without a call: the parts of the value as
/// it is written, in its own offset. `.year` and the rest are integers;
/// `.offset` is text, `+09:00`, and `+00:00` for UTC.
pub(crate) fn member(value: &OffsetDateTime, member: &str) -> Result<EvalValue, EvalError> {
    Ok(match member {
        "year" => EvalValue::Int(i64::from(value.year())),
        "month" => EvalValue::Int(i64::from(u8::from(value.month()))),
        "day" => EvalValue::Int(i64::from(value.day())),
        "hour" => EvalValue::Int(i64::from(value.hour())),
        "minute" => EvalValue::Int(i64::from(value.minute())),
        "second" => EvalValue::Int(i64::from(value.second())),
        "offset" => EvalValue::String(offset_text(value.offset())),
        _ => {
            return Err(EvalError::UnknownMember {
                on: "datetime",
                member: member.to_string(),
            });
        }
    })
}

/// An offset as `+09:00`; the sign is always written.
fn offset_text(offset: UtcOffset) -> String {
    let minutes = offset.whole_minutes();
    let sign = if minutes < 0 { '-' } else { '+' };
    format!("{sign}{:02}:{:02}", minutes.abs() / 60, minutes.abs() % 60)
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

    // What text reads as a datetime is tested where it is defined
    // (`tomet_type::datetime`); these are the conversion into a clock.

    #[test]
    fn rfc_3339_keeps_its_offset() {
        let dt = at("2026-10-17T09:05:07+09:00");
        assert_eq!(dt.offset().whole_hours(), 9);
        assert_eq!(format_datetime(&dt, "HH:mm:ss"), "09:05:07");
    }

    #[test]
    fn a_bare_date_is_midnight_utc() {
        let dt = at("2026-01-05");
        assert_eq!(format_datetime(&dt, "YYYY-MM-DD HH:mm"), "2026-01-05 00:00");
        assert_eq!(dt.offset().whole_hours(), 0);
    }

    #[test]
    fn the_looser_iso_8601_forms_are_read() {
        for (text, rendered) in [
            ("2026-10-17T09:00", "2026-10-17 09:00:00"),
            ("2026-10-17 09:00", "2026-10-17 09:00:00"),
            ("2026-10-17 09:05:07", "2026-10-17 09:05:07"),
            ("2026-10-17T09:05:07Z", "2026-10-17 09:05:07"),
            ("2026-10-17T09:05+09:00", "2026-10-17 09:05:00"),
            ("2026-10-17T09:05:07.5", "2026-10-17 09:05:07"),
        ] {
            let dt = at(text);
            assert_eq!(
                format_datetime(&dt, "YYYY-MM-DD HH:mm:ss"),
                rendered,
                "{text}"
            );
        }
    }

    #[test]
    fn a_missing_offset_is_utc_and_a_given_one_is_kept() {
        assert_eq!(at("2026-10-17T09:00").offset().whole_hours(), 0);
        assert_eq!(at("2026-10-17T09:00+09:00").offset().whole_hours(), 9);
        assert_eq!(
            at("2026-10-17 09:00:00-05:30").offset().whole_minutes(),
            -330
        );
        assert_eq!(at("2026-10-17T09:00:00.25").nanosecond(), 250_000_000);
    }

    #[test]
    fn text_that_is_not_a_datetime_is_none() {
        for text in ["", "hello", "2026-13-01", "2026-02-30", "2026-01-01T24:00"] {
            assert_eq!(parse_datetime(text), None, "{text}");
        }
    }

    #[test]
    fn an_offset_is_a_utc_offset() {
        assert_eq!(parse_offset("+09:00").unwrap().whole_hours(), 9);
        assert_eq!(parse_offset("-05:30").unwrap().whole_minutes(), -330);
        assert_eq!(parse_offset("Z"), Some(UtcOffset::UTC));
        assert_eq!(parse_offset("x"), None);
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
