//! Whitespace-hygiene formatting for a standalone motem file (`.mtm`).

/// Normalizes the whitespace of a `.mtm` file: LF line endings, no trailing
/// whitespace, runs of blank lines collapsed to one, no leading blank lines,
/// exactly one final newline. Comments stay where they are.
///
/// Text inside a quoted string is data, so it is copied exactly: a newline,
/// trailing spaces or a CR inside `"..."` is part of the value.
///
/// It never changes what the file means. A file that does not parse is
/// returned as it came, since the scan below cannot know where a string
/// starts in text the parser does not accept; so is any result that would
/// read back as a different value.
pub fn format_data_source(src: &str) -> String {
    let Ok(before) = motem_syntax::parse_file(src) else {
        return src.to_string();
    };
    let formatted = scan(src);
    match motem_syntax::parse_file(&formatted) {
        Ok(after) if after == before => formatted,
        _ => src.to_string(),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum State {
    Normal,
    Comment,
    Quoted,
}

fn scan(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut state = State::Normal;
    // where the line being written starts in `out`, and whether the line
    // before it was blank (so a second blank one is dropped)
    let mut line_start = 0;
    let mut after_blank = false;
    let mut chars = src.chars().peekable();

    while let Some(c) = chars.next() {
        match state {
            State::Quoted => {
                out.push(c);
                match c {
                    '\\' => {
                        if let Some(escaped) = chars.next() {
                            out.push(escaped);
                            if escaped == '\n' {
                                line_start = out.len();
                            }
                        }
                    }
                    '"' => state = State::Normal,
                    '\n' => line_start = out.len(),
                    _ => {}
                }
            }
            State::Normal | State::Comment => match c {
                '\r' | '\n' => {
                    if c == '\r' && chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    state = State::Normal;
                    out.truncate(out.trim_end_matches([' ', '\t']).len().max(line_start));
                    if out.len() == line_start {
                        // a blank line: keep one, and none before the first text
                        if after_blank || out.is_empty() {
                            continue;
                        }
                        after_blank = true;
                    } else {
                        after_blank = false;
                    }
                    out.push('\n');
                    line_start = out.len();
                }
                '"' if state == State::Normal => {
                    out.push(c);
                    state = State::Quoted;
                }
                // `//` starts a comment, except in `scheme://`
                '/' if state == State::Normal
                    && chars.peek() == Some(&'/')
                    && !out.ends_with(':') =>
                {
                    out.push(c);
                    state = State::Comment;
                }
                _ => out.push(c),
            },
        }
    }

    let end = out.trim_end_matches([' ', '\t', '\n']).len();
    out.truncate(end);
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_line_endings_trailing_space_blank_runs_and_the_final_newline() {
        let src = "\n\na: 1  \r\n\t\r\n\r\n\r\nb: 2\t\n\n\n";
        assert_eq!(format_data_source(src), "a: 1\n\nb: 2\n");
    }

    #[test]
    fn leaves_a_formatted_file_alone() {
        let src = "// header\n\"tmtroot/a.tmt\": \"../a.tmt\"\n\nm: {\n  x: 1\n}\n";
        assert_eq!(format_data_source(src), src);
    }

    #[test]
    fn is_idempotent() {
        let src = "a: 1   \r\n\r\n\r\n// c  \r\nb: \"x  \r\n  y\"   \n\n";
        let once = format_data_source(src);
        assert_eq!(format_data_source(&once), once);
    }

    #[test]
    fn keeps_comments_including_quotes_and_slashes_in_them() {
        let src = "// don't \"quote this  \na: 1\nurl: https://example.com/a//b  \n";
        assert_eq!(
            format_data_source(src),
            "// don't \"quote this\na: 1\nurl: https://example.com/a//b\n"
        );
    }

    #[test]
    fn copies_a_multi_line_string_exactly() {
        // trailing spaces, a CRLF and a blank line are all inside the string
        let src = "a: \"one  \r\n\r\n\r\ntwo\"   \nb: 1\n";
        assert_eq!(
            format_data_source(src),
            "a: \"one  \r\n\r\n\r\ntwo\"\nb: 1\n"
        );
        // an escaped quote does not end the string
        let src = "a: \"x \\\"  \n  y\"  \n";
        assert_eq!(format_data_source(src), "a: \"x \\\"  \n  y\"\n");
    }

    #[test]
    fn never_changes_the_value() {
        for src in [
            "a: 1  \n\n\n  b: list(1,  2)   \n",
            "m: {\n  x: 1   \n\n\n  y: \"s  \n  t\"  \n}\n",
            "// only a comment   \n\n\n",
            "\"k  \": \"v \r\n w\"\r\n",
            "",
        ] {
            let before = motem_syntax::parse_file(src).unwrap();
            let after = motem_syntax::parse_file(&format_data_source(src)).unwrap();
            assert_eq!(before, after, "{src:?}");
        }
    }

    #[test]
    fn a_file_that_does_not_parse_is_returned_as_it_came() {
        for src in [
            "a: \"never closed  \n\n\n",
            "a: 1\na: 2  \n\n\n",
            "stray   \n\n\n",
        ] {
            assert_eq!(format_data_source(src), src);
        }
    }

    #[test]
    fn an_empty_or_comment_less_blank_file_is_empty() {
        assert_eq!(format_data_source(""), "");
        assert_eq!(format_data_source("  \n\n \t\n"), "");
        assert_eq!(format_data_source("// c"), "// c\n");
    }
}
