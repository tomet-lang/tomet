//! How a map key and a string are spelled so that the parser reads back what was written.
//!
//! A key or a string has more than one spelling: bare, or inside `"..."`. A writer picks the bare
//! one only where the parser reads it as the same text and the quoted one everywhere else, so what
//! the parser reads is the truth and this crate states it once for every writer.
//!
//! - [`is_name`]: a name the parser reads as one name where a name is expected: an element's
//!   `@name(`, an interpolation call's `$name(`.
//! - [`write_key`] / [`key_text`]: a map key, bare when [`is_bare_key`] says the parser reads it
//!   as itself.
//! - [`write_scalar_string`] / [`scalar_text`]: a string value, bare when no other value could be
//!   read from the same text.
//! - [`write_quoted`] / [`quoted_text`]: a string inside `"..."`, whatever it holds.
use tomet_type::Type;

/// Whether `key` is read back as itself when written without quotes.
///
/// A bare key is one of:
///
/// - `true`, `false` or `null`;
/// - a number made of digits, or digits, a `.` and digits (`2024`, `1.0`);
/// - a [name](is_name).
///
/// A key with any other character, a non-ASCII letter included, is quoted.
pub fn is_bare_key(key: &str) -> bool {
    matches!(key, "true" | "false" | "null") || is_number_key(key) || is_name(key)
}

fn is_number_key(key: &str) -> bool {
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    match key.split_once('.') {
        Some((whole, fraction)) => digits(whole) && digits(fraction),
        None => digits(key),
    }
}

/// Whether `name` is read as one name: an ASCII letter or `_`, then ASCII letters, digits, `_`, `-`
/// and `.`, where each `.` has a name part right after it, and no run of digits that opens a token
/// (after a `_`, `-` or `.`) is followed by `.` and a digit, which reads as a float. A lone `true`,
/// `false` or `null` is a value and not a name.
pub fn is_name(name: &str) -> bool {
    if matches!(name, "true" | "false" | "null") {
        return false;
    }
    let bytes = name.as_bytes();
    let Some(&first) = bytes.first() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return false;
    }
    if !bytes
        .iter()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
    {
        return false;
    }
    let is_part =
        |b: Option<&u8>| b.is_some_and(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'));
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'.' if !is_part(bytes.get(i + 1)) => return false,
            b if b.is_ascii_digit() => {
                // A run of digits that is not inside a word (`a1`) is a number of its own.
                let opens = !bytes[i - 1].is_ascii_alphanumeric();
                let mut end = i;
                while bytes.get(end).is_some_and(u8::is_ascii_digit) {
                    end += 1;
                }
                if opens
                    && bytes.get(end) == Some(&b'.')
                    && bytes.get(end + 1).is_some_and(u8::is_ascii_digit)
                {
                    return false;
                }
                i = end;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    true
}

/// Writes a map key: bare when [`is_bare_key`], inside `"..."` otherwise.
pub fn write_key(key: &str, out: &mut String) {
    if is_bare_key(key) {
        out.push_str(key);
    } else {
        write_quoted(key, out);
    }
}

/// [`write_key`] as an owned string.
pub fn key_text(key: &str) -> String {
    let mut out = String::new();
    write_key(key, &mut out);
    out
}

/// Writes `s` as a `"..."` string, with `\"`, `\\`, `\n`, `\r`, `\t` escapes.
pub fn write_quoted(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
}

/// [`write_quoted`] as an owned string: `s` inside `"..."` whatever it holds.
pub fn quoted_text(s: &str) -> String {
    let mut out = String::new();
    write_quoted(s, &mut out);
    out
}

/// Writes a string value: bare where the same text could be read as nothing else, inside `"..."`
/// where it could be a number, a keyword, an empty or padded text, or holds a delimiter, a quote or
/// a control character, or opens with a character that starts something.
pub fn write_scalar_string(s: &str, out: &mut String) {
    if Type::Datetime.holds_text(s) == Some(true) {
        out.push_str(s);
        return;
    }

    let starts_with_special = matches!(
        s.chars().next(),
        Some('&' | '*' | '!' | '%' | '@' | '`' | '|' | '>' | '?' | '-' | '~')
    );

    let is_keyword_or_number = matches!(s, "null" | "true" | "false") || is_number(s);

    let needs_quotes = s.is_empty()
        || is_keyword_or_number
        || s.trim() != s
        || starts_with_special
        || s.contains([
            '"', '\'', ':', ',', '(', ')', '[', ']', '{', '}', '\n', '\r', '\t', ' ',
        ]);

    if needs_quotes {
        write_quoted(s, out);
    } else {
        out.push_str(s);
    }
}

/// [`write_scalar_string`] as an owned string.
pub fn scalar_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    write_scalar_string(s, &mut out);
    out
}

/// Whether the parser reads `s`, bare, as a number.
fn is_number(s: &str) -> bool {
    let b = s.as_bytes();
    let digits = |mut i: usize| {
        let start = i;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        (i > start).then_some(i)
    };
    let mut i = usize::from(b.first() == Some(&b'-'));
    match b.get(i) {
        Some(b'0') => i += 1,
        Some(b'1'..=b'9') => {
            let Some(next_i) = digits(i) else {
                return false;
            };
            i = next_i;
        }
        _ => return false,
    }
    if b.get(i) == Some(&b'.') {
        let Some(next_i) = digits(i + 1) else {
            return false;
        };
        i = next_i;
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(b.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        let Some(next_i) = digits(i) else {
            return false;
        };
        i = next_i;
    }
    i == b.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_bare_and_anything_else_is_quoted() {
        for bare in [
            "a",
            "_",
            "a-b",
            "a.b",
            "a1",
            "a.1",
            "_1a",
            "a_b.c-d",
            "title",
            "plain.key",
        ] {
            assert_eq!(key_text(bare), bare);
        }
        for quoted in [
            "",
            "-a",
            ".a",
            "a.",
            "a..b",
            "1a",
            "a b",
            "a:b",
            "tmtroot/a b.tmt",
            "見出し",
            "aé",
            "é",
        ] {
            assert_eq!(key_text(quoted), format!("\"{quoted}\""));
        }
    }

    #[test]
    fn a_keyword_or_a_number_is_a_bare_key() {
        for bare in ["true", "false", "null", "2024", "1.0", "01"] {
            assert!(is_bare_key(bare), "{bare}");
        }
        for quoted in ["1.", ".5", "1.2.3", "-1", "1e3"] {
            assert!(!is_bare_key(quoted), "{quoted}");
        }
    }

    #[test]
    fn digits_that_open_a_token_before_a_dot_and_a_digit_read_as_a_float() {
        // `_` ends the word, so `1.1` after it is a float and not a part of the name.
        assert!(!is_bare_key("_1.1"));
        assert!(!is_bare_key("a.1.1"));
        // Inside a word the digits belong to it.
        assert!(is_bare_key("a1.1"));
        assert!(is_bare_key("a.1"));
    }

    #[test]
    fn a_scalar_string_is_quoted_when_bare_it_would_be_something_else() {
        assert_eq!(scalar_text("plain"), "plain");
        assert_eq!(scalar_text("2026-06-17T05:52:44Z"), "2026-06-17T05:52:44Z");
        for quoted in ["true", "false", "null", "123", "45.67", "", " a", "a b"] {
            assert_eq!(scalar_text(quoted), format!("\"{quoted}\""));
        }
        // Text shaped like an interpolation is a string like any other: quoted, never read as the
        // interpolation.
        assert_eq!(scalar_text("${x}"), "\"${x}\"");
        assert_eq!(scalar_text("$f(x)"), "\"$f(x)\"");
        assert_eq!(scalar_text("hello \"world\""), "\"hello \\\"world\\\"\"");
        assert_eq!(scalar_text("line1\nline2"), "\"line1\\nline2\"");
    }
}
