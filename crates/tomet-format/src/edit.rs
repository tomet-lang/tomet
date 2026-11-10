//! Changes to the text of a document that must leave the document as it is.

use tomet_ast_parser::parse_file;

/// A change to the text: the range it replaces, and what it puts there.
pub(crate) type Edit = (usize, usize, String);

/// `text` with the non-overlapping `edits`, which are in the order of the text, made. `None` when
/// the result does not parse to `File` the text did: only white space is meant to change, and that
/// is checked, not assumed.
pub(crate) fn apply_keeping_the_document(text: &str, edits: Vec<Edit>) -> Option<String> {
    let doc = parse_file(text).ok()?;
    let mut next = text.to_string();
    for (start, end, replacement) in edits.into_iter().rev() {
        next.replace_range(start..end, &replacement);
    }
    (parse_file(&next).ok()? == doc).then_some(next)
}

/// The leading white space of the line that `at` is on.
pub(crate) fn leading_indent(src: &str, at: usize) -> &str {
    let line = &src[src[..at].rfind('\n').map_or(0, |i| i + 1)..];
    &line[..line.len() - line.trim_start_matches([' ', '\t']).len()]
}

/// The text of the line that `at` is on, from the start of the line up to `at`.
pub(crate) fn line_prefix(src: &str, at: usize) -> &str {
    let start = src[..at].rfind('\n').map_or(0, |i| i + 1);
    &src[start..at]
}
