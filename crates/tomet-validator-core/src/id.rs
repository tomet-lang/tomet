use tomet_ast::{Block, Document, Element, ElementValue, Entry, Inline, List, Span, Value};

/// Walks every node in `doc` and returns `(id, span)` for each
/// [`Element::id`]/[`Section::id`] found, in document order.
pub fn collect_ids(doc: &Document) -> Vec<(String, Span)> {
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
        Block::List(list) => collect_list_ids(list, ids),
    }
}

fn collect_list_ids(list: &List, ids: &mut Vec<(String, Span)>) {
    if let Some(id) = &list.id {
        ids.push((id.0.clone(), list.span));
    }
    for conn in &list.connects {
        collect_element_ids(conn, ids);
    }
    for item in &list.items {
        collect_element_ids(&item.element, ids);
        if let Some(sub) = &item.sublist {
            collect_list_ids(sub, ids);
        }
    }
}

fn collect_element_ids(el: &Element, ids: &mut Vec<(String, Span)>) {
    if let Some(id) = &el.id {
        ids.push((id.0.clone(), el.span));
    }
    if let Some(args) = &el.args {
        collect_value_ids(args, ids);
    }
    if let Some(ElementValue::Group(entries)) = &el.value {
        for entry in entries {
            match entry {
                Entry::Pair(_, value) => collect_value_ids(value, ids),
                Entry::Element(child) => collect_element_ids(child, ids),
            }
        }
    }
    if let Some(content) = &el.content {
        for block in content {
            collect_block_ids(block, ids);
        }
    }
    for conn in &el.connects {
        collect_element_ids(conn, ids);
    }
}

/// The ids of the elements that sit in a value slot (`key: @file(a)#(entry)`), at any depth. They are
/// anchors like any other (`@link(#entry)` resolves), so they count for uniqueness too.
fn collect_value_ids(value: &Value, ids: &mut Vec<(String, Span)>) {
    match value {
        Value::Element(el) => collect_element_ids(el, ids),
        Value::Map(entries) => {
            for (_, value) in entries {
                collect_value_ids(value, ids);
            }
        }
        Value::Seq(values) | Value::Call(_, values) => {
            for value in values {
                collect_value_ids(value, ids);
            }
        }
        Value::Blocks(blocks) => {
            for block in blocks {
                collect_block_ids(block, ids);
            }
        }
        _ => {}
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
pub fn collect_ids_cst(root: &SyntaxNode) -> Vec<(String, TextRange)> {
    use SyntaxKind as K;

    root.descendants()
        .filter(|node| node.kind() == K::ID_GROUP)
        .filter_map(|group| {
            let value = group
                .children_with_tokens()
                .filter_map(|e| e.into_token())
                .find(|t| {
                    !t.kind().is_trivia() && !matches!(t.kind(), K::HASH | K::L_PAREN | K::R_PAREN)
                })?;
            let text = value.text().trim_matches('"').to_string();
            Some((text, value.text_range()))
        })
        .collect()
}
