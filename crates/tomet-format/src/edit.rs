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
