//! Laying out the content of a callout, or of a list item, that runs over several lines according
//! to `callout.style.content` and `list.multiline.style.content`.

use tomet_ast_parser::parse_file;
use tomet_config::PrinterConfig;
use tomet_cst::{SyntaxKind as K, SyntaxNode};
use tomet_cst_parser::parse_cst;
use tomet_style::ContentLayout;

use crate::edit::{Edit, apply_keeping_the_document};

/// The most times a document is gone over: a layout inside another one is reached on the next, and
/// forms nest no deeper than the parser reads.
const MAX_PASSES: usize = tomet_cst_parser::MAX_DEPTH;

/// Lays out, in `src`, the `[content]` of each `@callout` standing on its own lines and of each
/// list item, whose content already runs over several lines, the way the layout asked for. Unset,
/// `src` is returned as written.
///
/// Content of one line is left as it is. The lines of the content are taken out of the text, the
/// ones after the first without the indentation they have in common, and written again by
/// [`ContentLayout::render`], so the printer and the formatter lay a document out alike.
///
/// Only the white space inside and around the brackets changes, and that is checked, not assumed:
/// if a pass writes text that does not parse to the `File` it started from, it is not kept.
pub fn format_content_layouts(src: &str, config: &PrinterConfig) -> String {
    let callout = config.format.callout_content_style;
    let list = config.format.list_multiline_style_content;
    if callout.is_none() && list.is_none() {
        return src.to_string();
    }
    if parse_file(src).is_err() {
        return src.to_string();
    }

    let mut text = src.to_string();
    for _ in 0..MAX_PASSES {
        let edits = collect(&text, callout, list);
        if edits.is_empty() {
            break;
        }
        match apply_keeping_the_document(&text, edits) {
            Some(next) => text = next,
            None => break,
        }
    }
    text
}

/// The edits of one pass, in the order of the text. A form inside the content of one that is laid
/// out is left to the next pass, which finds the outer one already as it should be.
fn collect(src: &str, callout: Option<ContentLayout>, list: Option<ContentLayout>) -> Vec<Edit> {
    let root = parse_cst(src);
    let mut edits: Vec<Edit> = Vec::new();
    let mut taken_until = 0;
    for node in root.descendants() {
        if usize::from(node.text_range().start()) < taken_until {
            continue;
        }
        let edit = match node.kind() {
            K::BLOCK_FORM if is_callout(&node) => callout.and_then(|l| callout_edit(src, &node, l)),
            K::LIST_ITEM => list.and_then(|l| list_edit(src, &node, l)),
            _ => None,
        };
        if let Some(edit) = edit
            && src[edit.0..edit.1] != edit.2
        {
            taken_until = edit.1;
            edits.push(edit);
        }
    }
    edits
}

fn is_callout(form: &SyntaxNode) -> bool {
    form.children()
        .find(|child| child.kind() == K::SIGIL)
        .and_then(|sigil| sigil.children().find(|child| child.kind() == K::NAME))
        .is_some_and(|name| name.text() == "callout")
}

/// A callout's brackets stand under its line, indented as it is: `Expanded` keeps its opening one
/// on the element's own line and the others open it on the next.
fn callout_edit(src: &str, form: &SyntaxNode, layout: ContentLayout) -> Option<Edit> {
    let content = form.children().find(|child| child.kind() == K::CONTENT)?;
    let (start, end) = (
        usize::from(content.text_range().start()),
        usize::from(content.text_range().end()),
    );
    let lines = content_lines(&content)?;
    let base = leading_white_space(src, usize::from(form.text_range().start()));
    if base.contains('\t') {
        return None;
    }

    // The brackets are written whole, so the white space before them goes with them.
    let mut from = start;
    while from > 0 && matches!(src.as_bytes()[from - 1], b' ' | b'\t' | b'\n' | b'\r') {
        from -= 1;
    }
    let opening = if layout == ContentLayout::Expanded {
        String::new()
    } else {
        format!("\n{base}")
    };
    let text = format!(
        "{opening}{}",
        layout.render(&lines, &format!("{base}  "), base)
    );
    Some((from, end, text))
}

/// A list item's content stands two columns under its opening bracket, and so does the closing one.
fn list_edit(src: &str, item: &SyntaxNode, layout: ContentLayout) -> Option<Edit> {
    let content = item.children().find(|child| child.kind() == K::CONTENT)?;
    let (start, end) = (
        usize::from(content.text_range().start()),
        usize::from(content.text_range().end()),
    );
    let lines = content_lines(&content)?;
    let line_start = src[..start].rfind('\n').map_or(0, |i| i + 1);
    // A column counted in characters is a column only where the line is indented by spaces.
    if src[line_start..start].contains('\t') {
        return None;
    }
    let pad = " ".repeat(src[line_start..start].chars().count() + 2);
    Some((start, end, layout.render(&lines, &pad, &pad)))
}

/// The indentation of the line that `at` is on.
fn leading_white_space(src: &str, at: usize) -> &str {
    let line = &src[src[..at].rfind('\n').map_or(0, |i| i + 1)..];
    &line[..line.len() - line.trim_start_matches([' ', '\t']).len()]
}

/// The lines of a `[content]` that runs over several lines, without the brackets, the white space
/// that ends a line, the first line's that opens it, and the indentation the others share; `None`
/// for content of one line.
fn content_lines(content: &SyntaxNode) -> Option<Vec<String>> {
    let text = content.text().to_string();
    let inner = text.strip_prefix('[')?.strip_suffix(']')?;
    let raw: Vec<&str> = inner
        .split('\n')
        .map(|line| line.trim_end_matches([' ', '\t', '\r']))
        .collect();
    let indent = |line: &str| line.len() - line.trim_start_matches([' ', '\t']).len();
    let shared = raw[1..]
        .iter()
        .filter(|line| !line.is_empty())
        .map(|line| indent(line))
        .min()
        .unwrap_or(0);
    let mut lines: Vec<String> = std::iter::once(raw[0].trim_start().to_string())
        .chain(
            raw[1..]
                .iter()
                .map(|line| line.get(shared..).unwrap_or("").to_string()),
        )
        .collect();
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    if lines.first().is_some_and(|line| line.is_empty()) {
        lines.remove(0);
    }
    (lines.len() > 1).then_some(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(layout: ContentLayout) -> PrinterConfig {
        let mut config = PrinterConfig::default();
        config.format.callout_content_style = Some(layout);
        config.format.list_multiline_style_content = Some(layout);
        config
    }

    fn lay_out(src: &str, layout: ContentLayout) -> String {
        format_content_layouts(src, &config(layout))
    }

    #[test]
    fn unset_the_source_is_returned_as_written() {
        let src = "@callout(info)[ one\ntwo ]\n";
        assert_eq!(format_content_layouts(src, &PrinterConfig::default()), src);
    }

    #[test]
    fn content_of_one_line_is_left_alone() {
        for src in [
            "@callout(info)[ one ]\n",
            "- [ one ]\n",
            "- one\n",
            "@callout[]\n",
        ] {
            assert_eq!(lay_out(src, ContentLayout::Expanded), src, "{src:?}");
        }
    }

    #[test]
    fn a_callout_under_a_list_item_is_indented_with_its_line() {
        let src = "- a\n\n  @callout(info)[ one\n  two ]\n";
        let out = lay_out(src, ContentLayout::Block);
        assert_eq!(out, "- a\n\n  @callout(info)\n  [ one\n    two\n  ]\n");
        assert_eq!(lay_out(&out, ContentLayout::Block), out);
    }

    #[test]
    fn a_callout_inside_a_callout_is_reached_on_the_next_pass() {
        let src = "@callout(info)[ a\n\n@callout(note)[ b\nc ]\n\nd ]\n";
        let out = lay_out(src, ContentLayout::Expanded);
        // The blank lines of the content are padded like the others; whitespace hygiene, which
        // runs after this, takes the padding off.
        assert_eq!(
            out,
            "@callout(info)[\n  a\n  \n  @callout(note)[\n    b\n    c\n  ]\n  \n  d\n]\n"
        );
        assert_eq!(lay_out(&out, ContentLayout::Expanded), out);
    }

    #[test]
    fn an_indentation_with_tabs_is_left_as_written() {
        for src in [
            "\t@callout(info)[ one\ntwo ]\n",
            "-\t[ one\ntwo ]\n",
            "\t- [ one\ntwo ]\n",
        ] {
            assert_eq!(lay_out(src, ContentLayout::Block), src, "{src:?}");
        }
    }

    #[test]
    fn a_document_the_layout_would_change_is_left_as_written() {
        // The code line's indentation is part of the document; a layout that shifts it is refused.
        let src = "@callout(info)[ one\n``` \n  code\n```\n]\n";
        let out = lay_out(src, ContentLayout::Box);
        assert_eq!(
            parse_file(&out).map(|f| f == parse_file(src).unwrap()),
            Ok(true),
            "{out:?}"
        );
    }
}
