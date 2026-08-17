use std::ops::ControlFlow;
use typedmark_ast::{Document, Span, Value};
use typedmark_walk::{Node, Visitor, walk_document};

/// Walks every node in `doc` and returns `(id, span)` for each `id` key
/// found in an attached `Value::Map` (see `typedmark-walk::Node::attrs`),
/// in document order.
pub(crate) fn collect_ids(doc: &Document) -> Vec<(String, Span)> {
    let mut collector = Collector { ids: Vec::new() };
    let _ = walk_document(doc, &mut collector);
    collector.ids
}

struct Collector {
    ids: Vec<(String, Span)>,
}

impl Visitor<()> for Collector {
    fn visit(&mut self, node: Node<'_>) -> ControlFlow<()> {
        if let Some(id) = id_from_value(node.attrs()) {
            self.ids.push((id, node.span()));
        }
        ControlFlow::Continue(())
    }
}

/// Looks up an `id` key in a `Value::Map` and renders it to a comparable
/// string (`Value::String` as-is, `Value::Int` via `to_string`; any other
/// shape isn't treated as an id).
fn id_from_value(value: Option<&Value>) -> Option<String> {
    let Value::Map(entries) = value? else {
        return None;
    };
    entries.iter().find_map(|(key, value)| {
        if key != "id" {
            return None;
        }
        match value {
            Value::String(s) => Some(s.clone()),
            Value::Int(i) => Some(i.to_string()),
            _ => None,
        }
    })
}
