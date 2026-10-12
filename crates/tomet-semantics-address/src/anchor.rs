//! In-memory anchor (`#(id)`) resolution and lookup across a [`Document`].
//!
//! Searches for matching [`Element::id`], [`Section::id`], or [`List::id`]
//! definitions anywhere in a parsed AST. Pure in-memory (no I/O).

use tomet_ast::{Block, Document, Element, ElementValue, Entry, Inline, List, Span, Value};

/// Returns whether an anchor with the given ID exists anywhere in `doc`.
pub fn has_anchor(doc: &Document, anchor: &str) -> bool {
    find_anchor(doc, anchor).is_some()
}

/// Finds the [`Span`] of the node defining the given anchor ID in `doc`, if any.
pub fn find_anchor(doc: &Document, anchor: &str) -> Option<Span> {
    for block in &doc.blocks {
        if let Some(span) = find_in_block(block, anchor) {
            return Some(span);
        }
    }
    None
}

/// Collects all anchor IDs defined in `doc` along with their [`Span`]s, in document order.
pub fn collect_anchors(doc: &Document) -> Vec<(String, Span)> {
    let mut anchors = Vec::new();
    for block in &doc.blocks {
        collect_in_block(block, &mut anchors);
    }
    anchors
}

fn find_in_block(block: &Block, anchor: &str) -> Option<Span> {
    match block {
        Block::Section(sec) => {
            if let Some(id) = &sec.id
                && id.0 == anchor
            {
                return Some(sec.span);
            }
            for inline in &sec.title {
                if let Some(span) = find_in_inline(inline, anchor) {
                    return Some(span);
                }
            }
            for conn in &sec.connects {
                if let Some(span) = find_in_element(conn, anchor) {
                    return Some(span);
                }
            }
            for child in &sec.blocks {
                if let Some(span) = find_in_block(child, anchor) {
                    return Some(span);
                }
            }
            None
        }
        Block::Element(el) => find_in_element(el, anchor),
        Block::Paragraph(p) => {
            for inline in &p.content {
                if let Some(span) = find_in_inline(inline, anchor) {
                    return Some(span);
                }
            }
            None
        }
        Block::List(list) => find_in_list(list, anchor),
    }
}

fn find_in_list(list: &List, anchor: &str) -> Option<Span> {
    if let Some(id) = &list.id
        && id.0 == anchor
    {
        return Some(list.span);
    }
    for conn in &list.connects {
        if let Some(span) = find_in_element(conn, anchor) {
            return Some(span);
        }
    }
    for item in &list.items {
        if let Some(span) = find_in_element(&item.element, anchor) {
            return Some(span);
        }
        if let Some(sub) = &item.sublist
            && let Some(span) = find_in_list(sub, anchor)
        {
            return Some(span);
        }
    }
    None
}

fn find_in_element(el: &Element, anchor: &str) -> Option<Span> {
    if let Some(id) = &el.id
        && id.0 == anchor
    {
        return Some(el.span);
    }
    if let Some(content) = &el.content {
        for block in content {
            if let Some(span) = find_in_block(block, anchor) {
                return Some(span);
            }
        }
    }
    for conn in &el.connects {
        if let Some(span) = find_in_element(conn, anchor) {
            return Some(span);
        }
    }
    if let Some(args) = &el.args
        && let Some(span) = find_in_value(args, anchor)
    {
        return Some(span);
    }
    if let Some(ElementValue::Group(entries)) = &el.value {
        for entry in entries {
            match entry {
                Entry::Element(child) => {
                    if let Some(span) = find_in_element(child, anchor) {
                        return Some(span);
                    }
                }
                Entry::Pair(_, v) => {
                    if let Some(span) = find_in_value(v, anchor) {
                        return Some(span);
                    }
                }
            }
        }
    }
    None
}

fn find_in_inline(inline: &Inline, anchor: &str) -> Option<Span> {
    match inline {
        Inline::Element(el) => find_in_element(el, anchor),
        _ => None,
    }
}

fn find_in_value(value: &Value, anchor: &str) -> Option<Span> {
    match value {
        Value::Element(el) => find_in_element(el, anchor),
        Value::Seq(items) => {
            for item in items {
                if let Some(span) = find_in_value(item, anchor) {
                    return Some(span);
                }
            }
            None
        }
        Value::Map(entries) => {
            for (_, v) in entries {
                if let Some(span) = find_in_value(v, anchor) {
                    return Some(span);
                }
            }
            None
        }
        Value::Blocks(blocks) => {
            for b in blocks {
                if let Some(span) = find_in_block(b, anchor) {
                    return Some(span);
                }
            }
            None
        }
        _ => None,
    }
}

fn collect_in_block(block: &Block, anchors: &mut Vec<(String, Span)>) {
    match block {
        Block::Section(sec) => {
            if let Some(id) = &sec.id {
                anchors.push((id.0.clone(), sec.span));
            }
            for inline in &sec.title {
                collect_in_inline(inline, anchors);
            }
            for conn in &sec.connects {
                collect_in_element(conn, anchors);
            }
            for child in &sec.blocks {
                collect_in_block(child, anchors);
            }
        }
        Block::Element(el) => collect_in_element(el, anchors),
        Block::Paragraph(p) => {
            for inline in &p.content {
                collect_in_inline(inline, anchors);
            }
        }
        Block::List(list) => collect_in_list(list, anchors),
    }
}

fn collect_in_list(list: &List, anchors: &mut Vec<(String, Span)>) {
    if let Some(id) = &list.id {
        anchors.push((id.0.clone(), list.span));
    }
    for conn in &list.connects {
        collect_in_element(conn, anchors);
    }
    for item in &list.items {
        collect_in_element(&item.element, anchors);
        if let Some(sub) = &item.sublist {
            collect_in_list(sub, anchors);
        }
    }
}

fn collect_in_element(el: &Element, anchors: &mut Vec<(String, Span)>) {
    if let Some(id) = &el.id {
        anchors.push((id.0.clone(), el.span));
    }
    if let Some(content) = &el.content {
        for block in content {
            collect_in_block(block, anchors);
        }
    }
    for conn in &el.connects {
        collect_in_element(conn, anchors);
    }
    if let Some(args) = &el.args {
        collect_in_value(args, anchors);
    }
    if let Some(ElementValue::Group(entries)) = &el.value {
        for entry in entries {
            match entry {
                Entry::Element(child) => collect_in_element(child, anchors),
                Entry::Pair(_, v) => collect_in_value(v, anchors),
            }
        }
    }
}

fn collect_in_inline(inline: &Inline, anchors: &mut Vec<(String, Span)>) {
    if let Inline::Element(el) = inline {
        collect_in_element(el, anchors);
    }
}

fn collect_in_value(value: &Value, anchors: &mut Vec<(String, Span)>) {
    match value {
        Value::Element(el) => collect_in_element(el, anchors),
        Value::Seq(items) => {
            for item in items {
                collect_in_value(item, anchors);
            }
        }
        Value::Map(entries) => {
            for (_, v) in entries {
                collect_in_value(v, anchors);
            }
        }
        Value::Blocks(blocks) => {
            for b in blocks {
                collect_in_block(b, anchors);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_doc(src: &str) -> Document {
        tomet_parser::parse_document(src).unwrap()
    }

    #[test]
    fn finds_section_heading_anchor() {
        let doc = parse_doc("=[ Introduction ] #(intro)\n\nSome body text.\n");
        assert!(has_anchor(&doc, "intro"));
        assert!(!has_anchor(&doc, "outro"));
        assert!(find_anchor(&doc, "intro").is_some());
    }

    #[test]
    fn finds_element_anchor() {
        let doc = parse_doc("@card #(my-card)\n\nParagraph with @badge #(my-badge) inside.\n");
        assert!(has_anchor(&doc, "my-card"));
        assert!(has_anchor(&doc, "my-badge"));
        assert!(!has_anchor(&doc, "other"));
    }

    #[test]
    fn finds_list_anchor() {
        let doc = parse_doc("- item 1 #(my-list)\n- item 2\n");
        assert!(has_anchor(&doc, "my-list"));
    }

    #[test]
    fn collects_all_anchors_in_document_order() {
        let doc = parse_doc(
            "=[ Sec ] #(sec)\n\n@card #(c1)\n\nText with @tag #(t1) in it.\n\n- list item #(l1)\n",
        );
        let ids: Vec<String> = collect_anchors(&doc)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        assert_eq!(ids, vec!["sec", "c1", "t1", "l1"]);
    }
}
