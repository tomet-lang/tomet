use tomet_ast::{Block, Document, Element, Inline, Section, Span, Value};
use tomet_tree::{ElementExt, ValueExt};

/// Walks every node in `doc` and returns `(id, span)` for each `id` key
/// found in an attached `Value::Map` (see `ElementExt::attrs_view`), in document order.
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
            if let Some(id) = id_from_value(section_attrs(sec)) {
                ids.push((id, sec.span));
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
    if !el.sigil.is_bare_named("link") {
        if let Some(id) = id_from_value(el.attrs_view()) {
            ids.push((id, el.span));
        }
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

fn section_attrs(sec: &Section) -> Option<Value> {
    match (&sec.args, &sec.value) {
        (Some(args), Some(val)) => match (args, val.as_data()) {
            (Value::Map(m1), Some(Value::Map(m2))) => {
                let mut merged = m1.clone();
                merged.extend(m2);
                Some(Value::Map(merged))
            }
            (_, Some(val)) => Some(val),
            (args, None) => Some(args.clone()),
        },
        (Some(args), None) => Some(args.clone()),
        (None, Some(val)) => val.as_data(),
        (None, None) => None,
    }
}

/// Looks up an `id` key in a `Value::Map` and renders it to a comparable
/// string (`Value::String` as-is, `Value::Int` via `to_string`; any other
/// shape isn't treated as an id).
fn id_from_value(value: Option<Value>) -> Option<String> {
    let value = value?;
    let id_val = value.get("id")?;
    match id_val {
        Value::String(s) => Some(s.clone()),
        Value::Int(i) => Some(i.to_string()),
        _ => None,
    }
}

use tomet_cst::{SyntaxKind, SyntaxNode, TextRange};

/// Scans `root` for all `id: <value>` property occurrences and returns their exact [`TextRange`]s.
pub(crate) fn collect_ids_cst(root: &SyntaxNode) -> Vec<(String, TextRange)> {
    let mut ids = Vec::new();
    let tokens: Vec<_> = root
        .descendants_with_tokens()
        .filter_map(|el| el.into_token())
        .collect();

    let mut i = 0;
    while i < tokens.len() {
        if tokens[i].kind() == SyntaxKind::IDENT && tokens[i].text() == "id" {
            let mut j = i + 1;
            while j < tokens.len() && tokens[j].kind().is_trivia() {
                j += 1;
            }
            if j < tokens.len() && tokens[j].kind() == SyntaxKind::COLON {
                let mut k = j + 1;
                while k < tokens.len() && tokens[k].kind().is_trivia() {
                    k += 1;
                }
                if k < tokens.len() {
                    let val_token = &tokens[k];
                    let kind = val_token.kind();
                    if kind == SyntaxKind::IDENT
                        || kind == SyntaxKind::INT_NUMBER
                        || kind == SyntaxKind::STRING_LITERAL
                    {
                        let text = val_token
                            .text()
                            .trim_matches('"')
                            .trim_matches('\'')
                            .to_string();
                        ids.push((text, val_token.text_range()));
                    }
                }
            }
        }
        i += 1;
    }
    ids
}
