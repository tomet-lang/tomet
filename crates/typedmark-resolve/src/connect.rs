//! Resolution for remote attribute connections declared inside `@references[...]` blocks.

use typedmark_ast::{Block, Document, Element, ElementValue, Heading, Inline, ListItem, Sigil, Value};
use typedmark_semantics::merge_connected_values;

/// A remote attribute connection definition targeting one or more element IDs.
#[derive(Debug, Clone)]
pub struct RemoteConnection {
    pub target_ids: Vec<String>,
    pub args: Option<Value>,
    pub value: Option<Value>,
}

/// Resolves and applies remote attribute connections declared within `@references[...]` blocks across a [`Document`].
///
/// Modifies target elements in-place by merging connected attributes
/// (preserving direct element attributes over connected defaults).
pub fn resolve_connect_targets(mut doc: Document) -> Document {
    let connections = extract_and_remove_references_connections(&mut doc.blocks);

    if connections.is_empty() {
        return doc;
    }

    for conn in &connections {
        for target_id in &conn.target_ids {
            apply_connection_to_blocks(&mut doc.blocks, target_id, conn);
        }
    }

    doc
}

fn extract_and_remove_references_connections(blocks: &mut Vec<Block>) -> Vec<RemoteConnection> {
    let mut connections = Vec::new();
    let mut i = 0;
    while i < blocks.len() {
        if let Block::Element(el) = &blocks[i] {
            if is_references_element(el) {
                let extracted = extract_connections_from_references(el);
                connections.extend(extracted);
                blocks.remove(i);
                continue;
            }
        }
        i += 1;
    }
    connections
}

fn is_references_element(el: &Element) -> bool {
    matches!(&el.sigil, Sigil::At(Some(name)) | Sigil::Type(name) if name == "references")
}

fn extract_connections_from_references(el: &Element) -> Vec<RemoteConnection> {
    let mut connections = Vec::new();

    // Check `el.content` (e.g. `@references[ <id:taskA>:{...} ]`)
    if let Some(content) = &el.content {
        for inline in content {
            if let Inline::Element(child_el) = inline {
                if let Some(conn) = parse_remote_connection_element(child_el) {
                    connections.push(conn);
                }
            }
        }
    }

    // Check `el.value` if children elements are placed inside `{ ... }`
    if let Some(ElementValue::Children(children)) = &el.value {
        for child_el in children {
            if let Some(conn) = parse_remote_connection_element(child_el) {
                connections.push(conn);
            }
        }
    }

    connections
}

fn parse_remote_connection_element(el: &Element) -> Option<RemoteConnection> {
    if let Sigil::Type(name) = &el.sigil {
        if name.starts_with("id:") {
            let target_str = &name[3..];
            let target_ids = parse_target_ids_from_str(target_str);
            let conn_val = match &el.value {
                Some(ElementValue::Data(v)) => Some(v.clone()),
                _ => None,
            };
            return Some(RemoteConnection {
                target_ids,
                args: el.args.clone(),
                value: conn_val,
            });
        }
    }
    None
}

fn parse_target_ids_from_str(s: &str) -> Vec<String> {
    let s = s.trim();
    if s.starts_with('[') && s.ends_with(']') {
        s[1..s.len() - 1]
            .split(',')
            .map(|part| part.trim().trim_matches('"').trim_matches('\'').to_string())
            .filter(|part| !part.is_empty())
            .collect()
    } else {
        vec![s.trim_matches('"').trim_matches('\'').to_string()]
    }
}

fn apply_connection_to_blocks(blocks: &mut [Block], target_id: &str, conn: &RemoteConnection) {
    for block in blocks {
        match block {
            Block::Element(el) => {
                apply_connection_to_element(el, target_id, conn);
            }
            Block::Heading(h) => {
                apply_connection_to_heading(h, target_id, conn);
            }
            Block::List(l) => {
                for item in &mut l.items {
                    apply_connection_to_list_item(item, target_id, conn);
                }
            }
            Block::Paragraph(p) => {
                for inline in &mut p.content {
                    if let Inline::Element(el) = inline {
                        apply_connection_to_element(el, target_id, conn);
                    }
                }
            }
        }
    }
}

fn element_has_id(el: &Element, target_id: &str) -> bool {
    if let Some(Value::Map(entries)) = &el.args {
        if entries.iter().any(|(k, v)| k == "id" && matches!(v, Value::String(s) if s == target_id)) {
            return true;
        }
    }
    if let Some(ElementValue::Data(Value::Map(entries))) = &el.value {
        if entries.iter().any(|(k, v)| k == "id" && matches!(v, Value::String(s) if s == target_id)) {
            return true;
        }
    }
    false
}

fn apply_connection_to_element(el: &mut Element, target_id: &str, conn: &RemoteConnection) {
    if element_has_id(el, target_id) {
        if let Some(conn_args) = &conn.args {
            el.args = merge_connected_values(el.args.as_ref(), Some(conn_args));
        }
        if let Some(conn_val) = &conn.value {
            let direct_v = match &el.value {
                Some(ElementValue::Data(v)) => Some(v),
                _ => None,
            };
            if let Some(merged) = merge_connected_values(direct_v, Some(conn_val)) {
                el.value = Some(ElementValue::Data(merged));
            }
        }
    }

    if let Some(ElementValue::Children(children)) = &mut el.value {
        for child in children {
            apply_connection_to_element(child, target_id, conn);
        }
    }
}

fn apply_connection_to_heading(h: &mut Heading, target_id: &str, conn: &RemoteConnection) {
    let has_id = match &h.attrs {
        Some(Value::Map(entries)) => entries.iter().any(|(k, v)| k == "id" && matches!(v, Value::String(s) if s == target_id)),
        _ => false,
    };
    if has_id {
        let conn_data = conn.value.as_ref().or(conn.args.as_ref());
        if let Some(conn_val) = conn_data {
            h.attrs = merge_connected_values(h.attrs.as_ref(), Some(conn_val));
        }
    }
}

fn apply_connection_to_list_item(item: &mut ListItem, target_id: &str, conn: &RemoteConnection) {
    let has_id = match &item.attrs {
        Some(Value::Map(entries)) => entries.iter().any(|(k, v)| k == "id" && matches!(v, Value::String(s) if s == target_id)),
        _ => false,
    };
    if has_id {
        let conn_data = conn.value.as_ref().or(conn.args.as_ref());
        if let Some(conn_val) = conn_data {
            item.attrs = merge_connected_values(item.attrs.as_ref(), Some(conn_val));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use typedmark_parser::parse_document;

    #[test]
    fn resolves_single_remote_id_connection_in_references() {
        let src = r#"
<task>(id: taskA)[ Clean room ]

@references[
  <id:taskA>:{ priority: high, status: todo }
]
"#;
        let doc = parse_document(src).unwrap();
        let resolved = resolve_connect_targets(doc);

        assert_eq!(resolved.blocks.len(), 1);
        match &resolved.blocks[0] {
            Block::Element(el) => {
                assert_eq!(el.sigil, Sigil::Type("task".into()));
                assert_eq!(
                    el.value,
                    Some(ElementValue::Data(Value::Map(vec![
                        ("priority".into(), Value::String("high".into())),
                        ("status".into(), Value::String("todo".into())),
                    ])))
                );
            }
            other => panic!("expected element, got {other:?}"),
        }
    }

    #[test]
    fn respects_direct_overrides_over_references_connection() {
        let src = r#"
<task>(id: taskA):{ priority: low }[ Clean room ]

@references[
  <id:taskA>:{ priority: high, status: todo }
]
"#;
        let doc = parse_document(src).unwrap();
        let resolved = resolve_connect_targets(doc);

        assert_eq!(resolved.blocks.len(), 1);
        match &resolved.blocks[0] {
            Block::Element(el) => {
                // Direct `priority: low` MUST win over connected `priority: high`
                assert_eq!(
                    el.value,
                    Some(ElementValue::Data(Value::Map(vec![
                        ("priority".into(), Value::String("low".into())),
                        ("status".into(), Value::String("todo".into())),
                    ])))
                );
            }
            other => panic!("expected element, got {other:?}"),
        }
    }

    #[test]
    fn resolves_multi_id_remote_connection_in_references() {
        let src = r#"
<task>(id: taskA)[ Clean room ]
<task>(id: taskB)[ Wash dishes ]

@references[
  <id:[taskA, taskB]>:{ tag: house }
]
"#;
        let doc = parse_document(src).unwrap();
        let resolved = resolve_connect_targets(doc);

        assert_eq!(resolved.blocks.len(), 2);
        for block in &resolved.blocks {
            match block {
                Block::Element(el) => {
                    assert_eq!(
                        el.value,
                        Some(ElementValue::Data(Value::Map(vec![(
                            "tag".into(),
                            Value::String("house".into())
                        )])))
                    );
                }
                other => panic!("expected element, got {other:?}"),
            }
        }
    }

    #[test]
    fn ignores_top_level_uncontained_remote_connection() {
        let src = r#"
<task>(id: taskA)[ Clean room ]

<id:taskA>:{ priority: high }
"#;
        let doc = parse_document(src).unwrap();
        let resolved = resolve_connect_targets(doc);

        // Top-level uncontained `<id:taskA>` is NOT resolved and stays as block
        assert_eq!(resolved.blocks.len(), 2);
    }
}
