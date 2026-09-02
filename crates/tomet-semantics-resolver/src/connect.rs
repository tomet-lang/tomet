//! Resolution for remote attribute connections declared inside `@references[...]` blocks.

use tomet_ast::{Block, Document, Element, ElementValue, Inline, Sigil, Value};
use tomet_semantics::merge_connected_values;
use tomet_tree::{ElementExt, for_each_element_mut};

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
            for_each_element_mut(&mut doc, |el| {
                apply_connection_to_element(el, target_id, conn);
            });
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
    el.sigil.is_bare_named("references")
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
    if let Some(children) = el.value.as_ref().map(|v| v.as_children()) {
        for child_el in children {
            if let Some(conn) = parse_remote_connection_element(child_el) {
                connections.push(conn);
            }
        }
    }

    connections
}

/// Reads a remote connection: `#id(taskA):{ priority: high }`.
///
/// This used to be spelled `<id:taskA>:{...}`, with the target smuggled
/// through `<T>`'s anything-goes name charset -- the parser stored the
/// whole `"id:taskA"` as the element's name and this function string-split
/// it back apart. Element names are now proper ASCII identifiers, so the
/// target lives in a positional argument where it belongs, and nothing
/// here has to know about a special charset.
fn parse_remote_connection_element(el: &Element) -> Option<RemoteConnection> {
    if !el.sigil.is_bare_named("id") {
        return None;
    }
    let target_str = el.args.as_ref().and_then(target_arg_str)?;
    Some(RemoteConnection {
        target_ids: parse_target_ids_from_str(&target_str),
        args: el.args.clone(),
        value: el.value.as_ref().and_then(|v| v.as_data()),
    })
}

/// The target of a `#id(...)`, whether written positionally (`#id(taskA)`,
/// which the value grammar records under the empty positional key) or
/// explicitly (`#id(target:taskA)`).
fn target_arg_str(args: &Value) -> Option<String> {
    match args {
        Value::String(s) => Some(s.clone()),
        Value::Map(entries) => entries
            .iter()
            .find(|(k, _)| k.is_empty() || k == "target" || k == "id")
            .and_then(|(_, v)| match v {
                Value::String(s) => Some(s.clone()),
                _ => None,
            }),
        _ => None,
    }
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

/// Walks `doc` via `tomet-tree`'s generic mutable tree walk and
/// merges `conn`'s attrs/value into every id-bearing `Element` (a list
/// item is just `Element{ sigil: Bare, .. }`, so no separate case is
/// needed here anymore) whose id matches `target_id` -- see that crate's
/// module doc for why the traversal itself lives there rather than being
/// hand-rolled here.
fn element_has_id(el: &Element, target_id: &str) -> bool {
    let Some(id_val) = el.get_attr("id") else {
        return false;
    };
    match id_val {
        Value::String(s) => s == target_id,
        Value::Int(i) => i.to_string() == target_id,
        _ => false,
    }
}

/// The `Element` half of [`ConnectionApplier`]'s merge -- recursion into
/// nested content/children is `tomet_walker::walk_document_mut`'s job
/// now, not this function's.
fn apply_connection_to_element(el: &mut Element, target_id: &str, conn: &RemoteConnection) {
    if element_has_id(el, target_id) {
        if let Some(conn_args) = &conn.args {
            el.args = merge_connected_values(el.args.as_ref(), Some(conn_args));
        }
        if let Some(conn_val) = &conn.value {
            let direct_v = match &el.value {
                Some(v) => v.as_data(),
                _ => None,
            };
            if let Some(merged) = merge_connected_values(direct_v.as_ref(), Some(conn_val)) {
                el.value = Some(ElementValue::from_map(merged));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_parser::parse_document;

    #[test]
    fn resolves_single_remote_id_connection_in_references() {
        let src = r#"
#task(id: taskA)[ Clean room ]

#references[
  #id(taskA):{ priority: high, status: todo }
]
"#;
        let doc = parse_document(src).unwrap();
        let resolved = resolve_connect_targets(doc);

        assert_eq!(resolved.blocks.len(), 1);
        match &resolved.blocks[0] {
            Block::Element(el) => {
                assert_eq!(el.sigil, Sigil::block("task"));
                assert_eq!(
                    el.value,
                    Some(ElementValue::from_map(Value::Map(vec![
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
#task(id: taskA):{ priority: low }[ Clean room ]

#references[
  #id(taskA):{ priority: high, status: todo }
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
                    Some(ElementValue::from_map(Value::Map(vec![
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
#task(id: taskA)[ Clean room ]
#task(id: taskB)[ Wash dishes ]

#references[
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
                        Some(ElementValue::from_map(Value::Map(vec![(
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
#task(id: taskA)[ Clean room ]

#id(taskA):{ priority: high }
"#;
        let doc = parse_document(src).unwrap();
        let resolved = resolve_connect_targets(doc);

        // Top-level uncontained `<id:taskA>` is NOT resolved and stays as block
        assert_eq!(resolved.blocks.len(), 2);
    }
}
