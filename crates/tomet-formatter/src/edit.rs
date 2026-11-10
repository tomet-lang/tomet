//! Changes to the text of a document that must leave the document as it is.

use tomet_ast_parser::parse_file;
use tomet_cst::{SyntaxKind as K, SyntaxNode};

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

/// Whether `form` is an element form (with `@`) named `name`.
pub(crate) fn is_element_named(form: &SyntaxNode, name: &str) -> bool {
    let Some(sigil) = form.children().find(|child| child.kind() == K::SIGIL) else {
        return false;
    };
    sigil.first_token().is_some_and(|tok| tok.kind() == K::AT)
        && sigil
            .children()
            .find(|child| child.kind() == K::NAME)
            .is_some_and(|n| n.text() == name)
}

/// Whether `form` is an element form (with `@`).
pub(crate) fn is_element(form: &SyntaxNode) -> bool {
    form.children()
        .find(|child| child.kind() == K::SIGIL)
        .and_then(|sigil| sigil.first_token())
        .is_some_and(|tok| tok.kind() == K::AT)
}

/// Whether `form` is a declaration form (with `::`) named `name`.
pub(crate) fn is_declaration_named(form: &SyntaxNode, name: &str) -> bool {
    let Some(sigil) = form.children().find(|child| child.kind() == K::SIGIL) else {
        return false;
    };
    sigil.first_token().is_some_and(|tok| tok.kind() == K::COLON)
        && sigil
            .children()
            .find(|child| child.kind() == K::NAME)
            .is_some_and(|n| n.text() == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_cst_parser::parse_cst;

    #[test]
    fn form_predicates_distinguish_elements_and_declarations() {
        let src = "@callout[text]\n::meta(a: 1)\n";
        let root = parse_cst(src);
        let forms: Vec<_> = root
            .descendants()
            .filter(|n| n.kind() == K::BLOCK_FORM)
            .collect();
        assert_eq!(forms.len(), 2);

        assert!(is_element(&forms[0]));
        assert!(is_element_named(&forms[0], "callout"));
        assert!(!is_element_named(&forms[0], "meta"));
        assert!(!is_declaration_named(&forms[0], "callout"));

        assert!(!is_element(&forms[1]));
        assert!(!is_element_named(&forms[1], "meta"));
        assert!(is_declaration_named(&forms[1], "meta"));
        assert!(!is_declaration_named(&forms[1], "callout"));
    }
}
