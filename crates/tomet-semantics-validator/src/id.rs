use tomet_ast::{Block, Document, Element, Inline, Span};

/// Walks every node in `doc` and returns `(id, span)` for each
/// [`Element::id`]/[`Section::id`] found, in document order.
pub(crate) fn collect_ids(doc: &Document) -> Vec<(String, Span)> {
    let mut ids = Vec::new();
    for block in &doc.blocks {
        collect_block_ids(block, &mut ids);
    }
    ids
}

fn collect_block_ids(block: &Block, ids: &mut Vec<(String, Span)>) {
    match block {
        Block::Section(sec) => {
            if let Some(id) = &sec.id {
                ids.push((id.0.clone(), sec.span));
            }
            for inline in &sec.title {
                collect_inline_ids(inline, ids);
            }
            for conn in &sec.connects {
                collect_element_ids(conn, ids);
            }
            for child in &sec.blocks {
                collect_block_ids(child, ids);
            }
        }
        Block::Element(el) => collect_element_ids(el, ids),
        Block::Paragraph(p) => {
            for inline in &p.content {
                collect_inline_ids(inline, ids);
            }
        }
    }
}

fn collect_element_ids(el: &Element, ids: &mut Vec<(String, Span)>) {
    if let Some(id) = &el.id {
        ids.push((id.0.clone(), el.span));
    }
    if let Some(content) = &el.content {
        for inline in content {
            collect_inline_ids(inline, ids);
        }
    }
    if let Some(children) = &el.children {
        for child in children {
            collect_block_ids(child, ids);
        }
    }
    for conn in &el.connects {
        collect_element_ids(conn, ids);
    }
}

fn collect_inline_ids(inline: &Inline, ids: &mut Vec<(String, Span)>) {
    if let Inline::Element(el) = inline {
        collect_element_ids(el, ids);
    }
}

use tomet_cst::{SyntaxKind, SyntaxNode, TextRange};

/// Scans `root` for every `#(id)` slot and returns the exact [`TextRange`]
/// of each id's value, in document order.
///
/// Mirrors [`collect_ids`]. No owner-kind allowlist or `@link` exclusion
/// is needed the way the old `MAP_ENTRY`-scanning version needed both:
/// an `ID_GROUP` node can only ever appear where `parse_groups`/
/// `parse_connect` put one, so there is nothing else it could be mistaken
/// for, and no attribute-key collision with `@link`'s own `id` argument
/// to guard against (that argument is not this slot).
pub(crate) fn collect_ids_cst(root: &SyntaxNode) -> Vec<(String, TextRange)> {
    use SyntaxKind as K;

    root.descendants()
        .filter(|node| node.kind() == K::ID_GROUP)
        .filter_map(|group| {
            let value = group.children_with_tokens().filter_map(|e| e.into_token()).find(
                |t| !t.kind().is_trivia() && !matches!(t.kind(), K::HASH | K::L_PAREN | K::R_PAREN),
            )?;
            let text = value.text().trim_matches('"').to_string();
            Some((text, value.text_range()))
        })
        .collect()
}
