//! Paragraph separator formatting according to `paragraph.separator`.

use std::collections::BTreeMap;

use tomet_ast::{Block, Value};
use tomet_ast_parser::parse_file;
use tomet_config::PrinterConfig;
use tomet_style::{ParagraphSeparator, stands_like_a_paragraph};
use tomet_tree::{Node, Visit, walk};

/// Sets the paragraphs of `src` off from each other the way `paragraph.separator` asks: by a blank
/// line, or by a `;` opening the next paragraph's line. Unset, `src` is returned as written.
///
/// Only the text between two neighbouring blocks that stand like paragraphs is rewritten, and only
/// when that text is nothing but line ends, blank lines, indentation and the `;` itself -- a
/// comment between the two, a `|` run, or anything else in the gap is left alone. A heading, a
/// list or a code block keeps the separator it has.
///
/// Both spellings read back as the same document. This is checked, not assumed: if the rewritten
/// text does not parse to the `File` it started from, `src` is returned unchanged.
pub fn format_paragraph_separators(src: &str, config: &PrinterConfig) -> String {
    let Some(separator) = config.format.paragraph_separator else {
        return src.to_string();
    };
    let Ok(doc) = parse_file(src) else {
        return src.to_string();
    };

    let mut edits = BTreeMap::new();
    collect(&doc.body, src, separator, &mut edits);
    let _ = walk(&doc, &mut |node, _| {
        for blocks in block_runs(node) {
            collect(blocks, src, separator, &mut edits);
        }
        Visit::<()>::Descend
    });
    if edits.is_empty() {
        return src.to_string();
    }

    let mut out = String::with_capacity(src.len());
    let mut at = 0;
    for (start, (end, text)) in &edits {
        out.push_str(&src[at..*start]);
        out.push_str(text);
        at = *end;
    }
    out.push_str(&src[at..]);

    match parse_file(&out) {
        Ok(after) if after == doc => out,
        _ => src.to_string(),
    }
}

/// The runs of sibling blocks that `node` itself holds.
fn block_runs<'a>(node: Node<'a>) -> Vec<&'a [Block]> {
    let mut runs: Vec<&'a [Block]> = Vec::new();
    match node {
        Node::Block(Block::Section(section)) => runs.push(&section.blocks),
        Node::Item(item) => runs.push(&item.content),
        Node::Declaration(declaration) => runs.extend(declaration.content.as_deref()),
        Node::Connect(connect) => runs.extend(connect.content.as_deref()),
        Node::Value(Value::Blocks(blocks)) => runs.push(blocks),
        Node::Block(_) | Node::Inline(_) | Node::Value(_) | Node::Entry(_) | Node::List(_) => {}
    }
    if let Some(element) = node.element_view() {
        runs.extend(element.content);
    }
    if let Some(hole) = node.hole() {
        runs.extend(hole.content.as_deref());
    }
    runs
}

/// Records, keyed by where it starts, the rewrite of the gap between each two neighbours of
/// `blocks` that both [`stands_like_a_paragraph`].
fn collect(
    blocks: &[Block],
    src: &str,
    separator: ParagraphSeparator,
    edits: &mut BTreeMap<usize, (usize, String)>,
) {
    // A `;` opening the first block's line separates it from nothing: a blank line has none.
    if separator == ParagraphSeparator::Blank
        && let Some(first) = blocks.first()
        && stands_like_a_paragraph(first)
        && !first.span().is_dummy()
        && let Some((from, to)) = leading_semicolon(src, first.span().start)
    {
        edits.insert(from, (to, String::new()));
    }
    for pair in blocks.windows(2) {
        let (before, after) = (&pair[0], &pair[1]);
        if !stands_like_a_paragraph(before) || !stands_like_a_paragraph(after) {
            continue;
        }
        let (before, after) = (before.span(), after.span());
        if before.is_dummy() || after.is_dummy() || before.end > after.start {
            continue;
        }
        let Some(gap) = src.get(before.end..after.start) else {
            continue;
        };
        if let Some(text) = rewrite_gap(gap, separator) {
            edits.insert(before.end, (after.start, text));
        }
    }
}

/// The byte range, from the `;` to `start`, of a `;` that opens the line the block starting at
/// `start` is on.
fn leading_semicolon(src: &str, start: usize) -> Option<(usize, usize)> {
    let head = src.get(..start)?.trim_end_matches([' ', '\t']);
    let before = head.strip_suffix(';')?;
    let line = before.rsplit('\n').next()?;
    line.chars()
        .all(|c| matches!(c, ' ' | '\t'))
        .then_some((before.len(), start))
}

fn is_blank(line: &str) -> bool {
    line.chars().all(|c| matches!(c, ' ' | '\t' | '\r'))
}

/// `gap` -- the text between the end of one block and the start of the next -- as `separator`
/// spells it, or `None` when it already is, or is not only a separator.
///
/// A separator is the rest of the first block's last line, any blank lines, then the next line's
/// indentation and its optional `;`.
fn rewrite_gap(gap: &str, separator: ParagraphSeparator) -> Option<String> {
    let mut lines = gap.split('\n');
    if !is_blank(lines.next()?) {
        return None;
    }
    let lines: Vec<&str> = lines.collect();
    let (last, blank_lines) = lines.split_last()?;
    if !blank_lines.iter().all(|line| is_blank(line)) {
        return None;
    }
    let indent_len = last
        .find(|c| !matches!(c, ' ' | '\t'))
        .unwrap_or(last.len());
    let (indent, rest) = last.split_at(indent_len);
    let semicolon = match rest.strip_prefix(';') {
        Some(after) if is_blank(after) => true,
        None if rest.is_empty() => false,
        _ => return None,
    };
    // `a` and `b` on neighbouring lines with nothing between them are not two paragraphs.
    if blank_lines.is_empty() && !semicolon {
        return None;
    }
    let text = format!("\n{}", separator.after_line_end(indent));
    (text != gap).then_some(text)
}
