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
//! - [`write_scalar`] / [`scalar_spelling`]: any scalar. What is bare is [`Scalar::bare`]'s; this
//!   adds the quotes of a text and the call of a float with no finite value.
//! - [`write_scalar_string`] / [`string_spelling`]: a string value, bare when no other value could be
//!   read from the same text.
//! - [`write_quoted`] / [`quoted_text`]: a string inside `"..."`, whatever it holds.
//! - [`fence_line_requires_marker`]: whether a line inside a fenced code block requires a line
//!   marker (` `` `) to avoid being read as a line marker or closing fence.
use tomet_scalar::Scalar;
use tomet_type::{Kind, Type};

/// Whether `key` is read back as itself when written without quotes.
///
/// A bare key is one of:
///
/// - `true`, `false`, `none` or `auto`;
/// - a number made of digits, or digits, a `.` and digits (`2024`, `1.0`);
/// - a [name](is_name).
///
/// A key with any other character, a non-ASCII letter included, is quoted.
pub fn is_bare_key(key: &str) -> bool {
    Scalar::is_value_keyword(key) || is_number_key(key) || is_name(key)
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
/// `false`, `none` or `auto` is a value and not a name.
pub fn is_name(name: &str) -> bool {
    if Scalar::is_value_keyword(name) {
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
/// a control character, or opens with a character that starts something, or holds a `<$`, which
/// begins a type seat wherever it stands.
///
/// A datetime (`2026-06-17T05:52:44Z`) holds a `:`, which would have it quoted; it is written bare,
/// because the parser reads it bare as the same text. This is the one place this crate asks the
/// type crate a question.
pub fn write_scalar_string(s: &str, out: &mut String) {
    if Type::bare(Kind::Datetime).holds_text(s) == Some(true) {
        out.push_str(s);
        return;
    }

    let starts_with_special = matches!(
        s.chars().next(),
        Some('&' | '*' | '!' | '%' | '@' | '`' | '|' | '>' | '?' | '-' | '~')
    );

    let is_non_text_scalar = !matches!(Scalar::from_bare(s), Ok(Scalar::Text(_)));

    let needs_quotes = s.is_empty()
        || is_non_text_scalar
        || s.trim() != s
        || starts_with_special
        || s.contains("<$")
        || s.contains([
            '"', '\'', ':', ',', '(', ')', '[', ']', '{', '}', '\n', '\r', '\t', ' ',
        ]);

    if needs_quotes {
        write_quoted(s, out);
    } else {
        out.push_str(s);
    }
}

/// Writes a scalar as source the parser reads back as the same scalar: the bare word of
/// [`Scalar::bare`] where it has one, a text by [`write_scalar_string`], and a float with no finite
/// value as the call that makes one, `float(nan)`, `float(inf)`, `float(-inf)`.
pub fn write_scalar(scalar: &Scalar, out: &mut String) {
    match scalar {
        Scalar::Text(s) => write_scalar_string(s, out),
        Scalar::Float(f) if f.is_nan() => out.push_str("float(nan)"),
        Scalar::Float(f) if f.is_infinite() => {
            out.push_str(if *f > 0.0 {
                "float(inf)"
            } else {
                "float(-inf)"
            });
        }
        _ => out.push_str(
            &scalar
                .bare()
                .expect("a text and a non-finite float are handled above"),
        ),
    }
}

/// [`write_scalar`] as an owned string.
pub fn scalar_spelling(scalar: &Scalar) -> String {
    let mut out = String::new();
    write_scalar(scalar, &mut out);
    out
}

/// [`write_scalar_string`] as an owned string.
pub fn string_spelling(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    write_scalar_string(s, &mut out);
    out
}

/// Whether `line` requires a line marker (` `` `) when written inside a fenced code block
/// with an opening fence of length `fence_len`.
///
/// Written bare, a line whose leading run of backticks is:
/// - exactly 2 (`run == 2`) would be read as a line marker, stripping the two backticks;
/// - at least `fence_len` and followed only by whitespace (`run >= fence_len && line[run..].trim().is_empty()`)
///   would close the fence early.
///
/// Every other line can be written bare.
pub fn fence_line_requires_marker(line: &str, fence_len: usize) -> bool {
    let run = line.chars().take_while(|c| *c == '`').count();
    run == 2 || (run >= fence_len && line[run..].trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scalar_is_written_as_the_parser_reads_it() {
        for (scalar, want) in [
            (Scalar::None, "none"),
            (Scalar::Auto, "auto"),
            (Scalar::Bool(false), "false"),
            (Scalar::Int(-3), "-3"),
            (Scalar::Float(1.0), "1.0"),
            (Scalar::Number("1.10".into()), "1.10"),
            (Scalar::Text("plain".into()), "plain"),
            (Scalar::Text("has space".into()), "\"has space\""),
            (Scalar::Text("none".into()), "\"none\""),
            (Scalar::Text("1.5".into()), "\"1.5\""),
            (Scalar::Float(f64::NAN), "float(nan)"),
            (Scalar::Float(f64::INFINITY), "float(inf)"),
            (Scalar::Float(f64::NEG_INFINITY), "float(-inf)"),
        ] {
            assert_eq!(scalar_spelling(&scalar), want, "{scalar:?}");
        }
    }

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
        for bare in ["true", "false", "none", "auto", "2024", "1.0", "01"] {
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
        assert_eq!(string_spelling("plain"), "plain");
        assert_eq!(
            string_spelling("2026-06-17T05:52:44Z"),
            "2026-06-17T05:52:44Z"
        );
        for quoted in [
            "true", "false", "none", "auto", "null", "123", "45.67", "", " a", "a b",
        ] {
            assert_eq!(string_spelling(quoted), format!("\"{quoted}\""));
        }
        // Text shaped like an interpolation is a string like any other: quoted, never read as the
        // interpolation.
        assert_eq!(string_spelling("$(x)"), "\"$(x)\"");
        assert_eq!(string_spelling("$f(x)"), "\"$f(x)\"");
        // Likewise text shaped like a type seat, wherever in the string it is; a name tag is text.
        assert_eq!(string_spelling("<$date>"), "\"<$date>\"");
        assert_eq!(string_spelling("a<$list(int)>b"), "\"a<$list(int)>b\"");
        assert_eq!(string_spelling("<name>"), "<name>");
        assert_eq!(
            string_spelling("hello \"world\""),
            "\"hello \\\"world\\\"\""
        );
        assert_eq!(string_spelling("line1\nline2"), "\"line1\\nline2\"");
    }

    #[test]
    fn string_spelling_and_from_bare_agreement() {
        let sample = [
            // Keywords
            "none",
            "auto",
            "true",
            "false",
            "null",
            // Numbers
            "42",
            "-42",
            "0",
            "-0",
            "1.10",
            "1e3",
            "1.5",
            "-1.5",
            // Empty and padded text
            "",
            " ",
            " padded ",
            "   ",
            // Datetime (bare exception)
            "2026-06-17T05:52:44Z",
            "2026-10-25",
            // Plain text
            "plain",
            "hello",
        ];

        for s in sample {
            let written = string_spelling(s);
            if !written.starts_with('"') {
                // When written bare, from_bare must read it back as Text(s)
                // (or datetime exception)
                assert_eq!(
                    Scalar::from_bare(&written),
                    if Type::bare(Kind::Datetime).holds_text(s) == Some(true) {
                        // A bare datetime is either a JSON number shape (date like 2026-10-25 is Text)
                        // or Text.
                        Ok(Scalar::Text(s.to_string()))
                    } else {
                        Ok(Scalar::Text(s.to_string()))
                    },
                    "for bare {s}"
                );
            } else {
                // When written quoted, from_bare on bare `s` must NOT be Text(s)
                // (or holds special characters / whitespace)
                let reads_as_other = !matches!(Scalar::from_bare(s), Ok(Scalar::Text(_)));
                let has_special = s.is_empty()
                    || s.trim() != s
                    || s.contains([
                        '"', '\'', ':', ',', '(', ')', '[', ']', '{', '}', '\n', '\r', '\t', ' ',
                    ])
                    || matches!(
                        s.chars().next(),
                        Some('&' | '*' | '!' | '%' | '@' | '`' | '|' | '>' | '?' | '-' | '~')
                    )
                    || s.contains("<$");
                assert!(
                    reads_as_other || has_special,
                    "quoted string {s:?} should have required quotes"
                );
            }
        }
    }

    #[test]
    fn test_fence_line_requires_marker() {
        // Line starting with exactly two backticks needs marker
        assert!(fence_line_requires_marker("``", 3));
        assert!(fence_line_requires_marker("`` code", 3));
        assert!(!fence_line_requires_marker("` code", 3));
        assert!(!fence_line_requires_marker("code", 3));

        // Line that would close a fence of length 3 needs marker
        assert!(fence_line_requires_marker("```", 3));
        assert!(fence_line_requires_marker("```  ", 3));
        assert!(fence_line_requires_marker("````", 3));

        // Line starting with ``` but having text after does not close fence, so bare is safe
        assert!(!fence_line_requires_marker("```rust", 3));
        assert!(!fence_line_requires_marker("``` rust", 3));
    }
}
