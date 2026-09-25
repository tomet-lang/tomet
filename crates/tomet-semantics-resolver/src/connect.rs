//! Resolution for remote attribute connections declared inside `@references[...]` blocks.

use tomet_ast::{Block, Document, Element, ElementValue, Inline, Value};
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
        if let Block::Element(el) = &blocks[i]
            && is_references_element(el)
        {
            let extracted = extract_connections_from_references(el);
            connections.extend(extracted);
            blocks.remove(i);
            continue;
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
            if let Inline::Element(child_el) = inline
                && let Some(conn) = parse_remote_connection_element(child_el)
            {
                connections.push(conn);
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

/// Reads a remote connection.
///
/// **This has no surface syntax right now.** It used to be written
/// `<id:taskA>:{...}`, with the target smuggled through `<T>`'s
/// anything-goes name charset and string-split back out here. `<T>` is
/// gone, and no replacement spelling has been decided. Nothing produces a
/// `RemoteConnection` until one is, so this returns `None` rather than
/// inventing a spelling -- a spelling was invented here once (`@id(...)`)
/// and withdrawn, because nobody had asked for it.
///
/// The surrounding logic is intact: `resolve_connect_targets` still walks
/// `@references[...]` and hands each target element its attributes. Only
/// the way in is missing. Decide a spelling and rewrite this, or drop the
/// feature; `docs/roadmap.tmt` carries the question.
fn parse_remote_connection_element(_el: &Element) -> Option<RemoteConnection> {
    None
}

/// Reads the target list a remote connection carried: one id, or
/// `[a, b]`.
///
/// Dead, and deliberately so. Its only caller was
/// `parse_remote_connection_element`, which returns `None` until a
/// spelling is decided -- so this is the half of the feature that still
/// works, kept beside the half that does not. Deleting it would make
/// "decide a spelling" mean "and rewrite the parsing too", which is not
/// what the roadmap entry is asking for.
#[allow(dead_code)]
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

    // The three remote-connection tests that lived here are gone with
    // the syntax they exercised. They were written against
    // `<id:taskA>:{...}`, which is unspellable now that `<T>` is
    // removed, and no replacement has been decided. They are not
    // rewritten against an invented spelling -- see this module's
    // `parse_remote_connection_element`.

    #[test]
    fn ignores_top_level_uncontained_remote_connection() {
        let src = r#"
@task(id: taskA)[ Clean room ]

@id(taskA):{ priority: high }
"#;
        let doc = parse_document(src).unwrap();
        let resolved = resolve_connect_targets(doc);

        // Top-level uncontained `<id:taskA>` is NOT resolved and stays as block
        assert_eq!(resolved.blocks.len(), 2);
    }
}
