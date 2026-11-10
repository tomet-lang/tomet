//! The layout of the fields of `::meta`, according to `meta.always_newline`.

use tomet_config::PrinterConfig;
use tomet_cst::{SyntaxElement, SyntaxKind as K, SyntaxNode};
use tomet_cst_parser::parse_cst;
use tomet_style::GroupLayout;

use crate::edit::{Edit, apply_keeping_the_document, leading_indent};

/// Lays the fields of each `::meta(...)` out the way `meta.always_newline` asks: one to a line, or
/// all on one line. Unset, `src` is returned as written; so is a `::meta` that has no field,
/// carries a comment, has a `{value}` group, or has a field that runs over several lines.
///
/// The text of each field is kept as it is written: the formatter changes where the fields break,
/// never what they say, so the per-field rules for a type, a format or a prefix are not its
/// business. Only white space changes, and that is checked, not assumed.
pub fn format_meta_layout(src: &str, config: &PrinterConfig) -> String {
    let Some(layout) = config.meta.field_layout else {
        return src.to_string();
    };
    let root = parse_cst(src);
    let edits: Vec<Edit> = root
        .descendants()
        .filter(|node| node.kind() == K::BLOCK_FORM && is_meta(node))
        .filter_map(|meta| edit(src, &meta, layout))
        .collect();
    if edits.is_empty() {
        return src.to_string();
    }
    apply_keeping_the_document(src, edits).unwrap_or_else(|| src.to_string())
}

/// Whether `form` is the declaration `::meta`, not the element `@meta`.
fn is_meta(form: &SyntaxNode) -> bool {
    let Some(sigil) = form.children().find(|child| child.kind() == K::SIGIL) else {
        return false;
    };
    sigil
        .first_token()
        .is_some_and(|token| token.kind() == K::COLON)
        && sigil
            .children()
            .find(|child| child.kind() == K::NAME)
            .is_some_and(|name| name.text() == "meta")
}

/// A field as `key: value`: its text as written, with one space after the colon. `None` for a field
/// that runs over several lines.
fn field_text(entry: &SyntaxNode) -> Option<String> {
    let text = entry.text().to_string();
    if text.contains('\n') {
        return None;
    }
    let colon = entry
        .children_with_tokens()
        .find(|element| element.kind() == K::COLON)?;
    let key_end = usize::from(colon.text_range().end()) - usize::from(entry.text_range().start());
    let (key, value) = text.split_at(key_end);
    Some(format!("{key} {}", value.trim_start()))
}

/// The change that lays the form's args out as `layout` says, if it is not already.
fn edit(src: &str, form: &SyntaxNode, layout: GroupLayout) -> Option<Edit> {
    let mut args = form.children().filter(|child| child.kind() == K::ARGS);
    let (args, None) = (args.next()?, args.next()) else {
        return None;
    };
    // Anything but the sigil and the args -- a `{value}`, a comment -- keeps the form as written.
    for element in form.children_with_tokens() {
        let plain = match &element {
            SyntaxElement::Node(node) => matches!(node.kind(), K::SIGIL | K::ARGS),
            SyntaxElement::Token(token) => token.kind() == K::WHITESPACE,
        };
        if !plain {
            return None;
        }
    }

    let mut fields = Vec::new();
    for element in args.children_with_tokens() {
        match element.kind() {
            K::L_PAREN | K::R_PAREN | K::COMMA | K::WHITESPACE | K::NEWLINE => {}
            K::MAP_ENTRY => fields.push(field_text(element.as_node()?)?),
            _ => return None,
        }
    }
    if fields.is_empty() {
        return None;
    }

    let start = usize::from(form.text_range().start());
    let indent = leading_indent(src, start);
    let text = layout.render(&fields, indent);
    let range = args.text_range();
    (src[usize::from(range.start())..usize::from(range.end())] != text)
        .then(|| (usize::from(range.start()), usize::from(range.end()), text))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn laid_out(layout: GroupLayout, src: &str) -> String {
        let mut config = PrinterConfig::default();
        config.meta.field_layout = Some(layout);
        format_meta_layout(src, &config)
    }

    fn one_per_line(src: &str) -> String {
        laid_out(GroupLayout::OnePerLine, src)
    }

    #[test]
    fn unset_the_source_is_returned_as_written() {
        let src = "::meta(a: 1, b: 2)\n";
        assert_eq!(format_meta_layout(src, &PrinterConfig::default()), src);
    }

    #[test]
    fn the_fields_of_a_meta_on_one_line_go_one_to_a_line() {
        assert_eq!(
            one_per_line("::meta(a: 1, b: \"x y\")\n\nbody\n"),
            "::meta(\n  a: 1\n  b: \"x y\"\n)\n\nbody\n"
        );
        assert_eq!(
            one_per_line("::meta(a: 1, tags: (x, y))\n"),
            "::meta(\n  a: 1\n  tags: (x, y)\n)\n"
        );
    }

    #[test]
    fn the_fields_of_a_meta_over_lines_go_on_one_line() {
        assert_eq!(
            laid_out(GroupLayout::OneLine, "::meta(\n  a: 1\n  b: 2\n)\n"),
            "::meta(a: 1, b: 2)\n"
        );
    }

    #[test]
    fn a_meta_that_is_not_plain_is_left_alone() {
        for src in [
            "::meta()\n",
            "::meta(a: 1) // note\n",
            "::meta(a: 1){k: 2}\n",
            "::meta(\n  a: 1 // note\n  b: 2\n)\n",
            "::kind(a, b)\n",
            "@meta(a: 1, b: 2)\n",
        ] {
            assert_eq!(one_per_line(src), src, "{src:?}");
            assert_eq!(laid_out(GroupLayout::OneLine, src), src, "{src:?}");
        }
    }

    #[test]
    fn a_field_is_written_with_one_space_after_its_colon() {
        assert_eq!(
            laid_out(
                GroupLayout::OneLine,
                "::meta(\n  title:  Hello\n  id:x\n)\n"
            ),
            "::meta(title: Hello, id: x)\n"
        );
    }

    #[test]
    fn what_is_written_is_stable() {
        for layout in [GroupLayout::OneLine, GroupLayout::OnePerLine] {
            let once = laid_out(layout, "::meta(a: 1, b: 2)\n");
            assert_eq!(laid_out(layout, &once), once);
        }
    }
}
