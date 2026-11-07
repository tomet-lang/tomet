//! =[ comment ]
//!
//! HTML's comment syntax, `<!-- ... -->`, written and read in one place.
//!
//! A comment is HTML syntax wherever HTML is written: a page, but also the raw HTML that CommonMark
//! passes through and the `"html"` raw blocks of Pandoc's AST. So the converters for those
//! formats use this instead of each spelling the delimiters, and escaping what may not appear
//! inside them, for themselves. What a `@comment` says is `tomet_semantics::comment_text`.

/// `text` as an HTML comment.
///
/// What cannot appear inside a comment (`-->`, `--!>` and `<!--`) is broken with a space, so the
/// comment ends where it is meant to and nowhere earlier. A line of text is padded by a space on
/// each side; an empty text is the empty comment `<!---->`.
pub fn render(text: &str) -> String {
    if text.is_empty() {
        return "<!---->".to_string();
    }
    let safe = text
        .replace("-->", "-- >")
        .replace("--!>", "-- !>")
        .replace("<!--", "<! --");
    format!("<!-- {safe} -->")
}

/// The text of `source` if it is exactly one HTML comment, and `None` if it is anything else:
/// text around it, or more than one comment.
///
/// Whitespace around the comment, and between its delimiters and its text, is not part of the text.
pub fn parse(source: &str) -> Option<String> {
    let inner = source
        .trim()
        .strip_prefix("<!--")?
        .strip_suffix("-->")?;
    if inner.contains("-->") {
        return None;
    }
    Some(inner.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_comment_is_padded_by_a_space() {
        assert_eq!(render("note"), "<!-- note -->");
    }

    #[test]
    fn an_empty_comment_has_nothing_between_the_delimiters() {
        assert_eq!(render(""), "<!---->");
    }

    #[test]
    fn what_would_end_or_open_a_comment_is_broken() {
        assert_eq!(render("a --> b"), "<!-- a -- > b -->");
        assert_eq!(render("a --!> b"), "<!-- a -- !> b -->");
        assert_eq!(render("a <!-- b"), "<!-- a <! -- b -->");
        assert_eq!(render("a --->"), "<!-- a --- > -->");
    }

    #[test]
    fn a_rendered_comment_parses_back_to_its_text() {
        for text in ["note", "one\n\ntwo", "a --> b"] {
            let back = parse(&render(text)).unwrap();
            assert_eq!(back, text.replace("-->", "-- >"));
        }
        assert_eq!(parse(&render("")), Some(String::new()));
    }

    #[test]
    fn surrounding_whitespace_is_not_text() {
        assert_eq!(parse("  <!--   note \n-->\n"), Some("note".to_string()));
        assert_eq!(parse("<!--\n  one\n  two\n-->"), Some("one\n  two".to_string()));
    }

    #[test]
    fn anything_but_exactly_one_comment_is_not_one() {
        assert_eq!(parse("text"), None);
        assert_eq!(parse("a <!-- b -->"), None);
        assert_eq!(parse("<!-- a --> b"), None);
        assert_eq!(parse("<!-- a --> <!-- b -->"), None);
        assert_eq!(parse("<!-- unterminated"), None);
        assert_eq!(parse("<div><!-- a --></div>"), None);
    }
}
