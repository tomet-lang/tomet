//! Resolves a `${...}` interpolation's `Identifier`/`Member` chain
//! (`typedmark-parser`'s `InterpExpr`) against a `Document`'s own
//! `{id:...}`/`(id:...)`-tagged nodes. Same-document only for v1 -- no
//! `@settings(file:...)`-style cross-file lookup yet.
//!
//! `InterpExprKind::Literal`/`Call` aren't references at all, so
//! [`resolve_reference`] rejects them; evaluating a `Call` (calling a
//! named function) is `typedmark-compute`'s job, which is expected to
//! call back into this function for its `Identifier`/`Member`
//! sub-expressions rather than reimplementing id lookup itself.

use std::ops::ControlFlow;
use typedmark_ast::{Document, Element, ElementValue, InterpExpr, InterpExprKind, Value};
use typedmark_walker::{Node, Visitor, walk_document};

use crate::ResolveError;

/// Resolves an `Identifier`/`Member` chain to the `Value` found at that
/// path. A bare `${id}` resolves to the id'd node's "value view" (see
/// [`node_value`]); each `.member` step afterward is a plain
/// `Value::Map` key lookup on the previous step's result.
pub fn resolve_reference(doc: &Document, expr: &InterpExpr) -> Result<Value, ResolveError> {
    match &expr.kind {
        InterpExprKind::Identifier(id) => {
            find_by_id(doc, id).ok_or_else(|| ResolveError::UnknownId { id: id.clone() })
        }
        InterpExprKind::Member { object, member } => {
            let base = resolve_reference(doc, object)?;
            value_member(&base, member).ok_or_else(|| ResolveError::NoSuchMember {
                member: member.clone(),
            })
        }
        InterpExprKind::Literal(_) | InterpExprKind::Call { .. } => {
            Err(ResolveError::NotAReference)
        }
    }
}

/// Depth-first search (via `typedmark-walker`) for the first node anywhere
/// in `doc` tagged `{id:...}`/`(id:...)` with `id`, returning its
/// [`node_value`]. Not shared with `typedmark-validator/src/id.rs`'s use
/// of the same walker: that one collects *every* id for
/// duplicate-detection (never stopping early, no need to know each
/// node's own data beyond its `Span`), while this one *stops* at the
/// first match to one specific id and needs the matched node's actual
/// data -- the shared part is only the tree-walk itself
/// (`typedmark_walker::walk_document`), not this search's own logic.
fn find_by_id(doc: &Document, id: &str) -> Option<Value> {
    struct FindById<'a> {
        id: &'a str,
    }
    impl Visitor<Value> for FindById<'_> {
        fn visit(&mut self, node: Node<'_>) -> ControlFlow<Value> {
            if id_matches(node.attrs(), self.id) {
                let value = match &node {
                    Node::Element(el) => node_value(el),
                    Node::Heading(_) | Node::ListItem(_) => {
                        node.attrs().cloned().unwrap_or(Value::Null)
                    }
                };
                return ControlFlow::Break(value);
            }
            ControlFlow::Continue(())
        }
    }
    match walk_document(doc, &mut FindById { id }) {
        ControlFlow::Break(value) => Some(value),
        ControlFlow::Continue(()) => None,
    }
}

fn id_matches(value: Option<&Value>, target: &str) -> bool {
    let Some(Value::Map(entries)) = value else {
        return false;
    };
    entries.iter().any(|(key, value)| {
        key == "id"
            && match value {
                Value::String(s) => s == target,
                Value::Int(i) => i.to_string() == target,
                _ => false,
            }
    })
}

/// An `Element`'s "value view" for `${id}`/`${id.member}` purposes:
/// `{value}` and `(args)` merged into one `Value::Map` when both are
/// maps (`{value}`'s keys win on conflict -- it's the more common home
/// for an element's real data, e.g. `@meta{...}`, while `args` tends
/// toward attributes/flags), or whichever one is a map if only one is,
/// or the raw `{value}`/`(args)` `Value` as-is if neither is a map (a
/// bare scalar group).
fn node_value(el: &Element) -> Value {
    let args_map = match &el.args {
        Some(Value::Map(entries)) => Some(entries.clone()),
        _ => None,
    };
    let value_map = match &el.value {
        Some(ElementValue::Data(Value::Map(entries))) => Some(entries.clone()),
        _ => None,
    };
    match (args_map, value_map) {
        (Some(mut merged), Some(value_entries)) => {
            for (key, value) in value_entries {
                match merged.iter_mut().find(|(k, _)| *k == key) {
                    Some(existing) => existing.1 = value,
                    None => merged.push((key, value)),
                }
            }
            Value::Map(merged)
        }
        (Some(args), None) => Value::Map(args),
        (None, Some(value)) => Value::Map(value),
        (None, None) => match &el.value {
            Some(ElementValue::Data(v)) => v.clone(),
            _ => el.args.clone().unwrap_or(Value::Null),
        },
    }
}

fn value_member(base: &Value, member: &str) -> Option<Value> {
    match base {
        Value::Map(entries) => entries
            .iter()
            .find(|(key, _)| key == member)
            .map(|(_, v)| v.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use typedmark_ast::Block;

    fn parse(src: &str) -> Document {
        typedmark_parser::parse_document(src).expect("valid TypedMark source")
    }

    fn interp(src: &str) -> InterpExpr {
        let doc = parse(&format!("{src}\n"));
        match &doc.blocks[0] {
            Block::Element(el) => match &el.value {
                Some(ElementValue::Interp(expr)) => expr.clone(),
                other => panic!("expected ElementValue::Interp, got {other:?}"),
            },
            other => panic!("expected a standalone ${{...}} element, got {other:?}"),
        }
    }

    #[test]
    fn resolves_bare_id_from_args() {
        let doc = parse("<x>(id:greeting, text:hi)\n");
        let value = resolve_reference(&doc, &interp("${greeting}")).unwrap();
        assert_eq!(
            value,
            Value::Map(vec![
                ("id".into(), Value::String("greeting".into())),
                ("text".into(), Value::String("hi".into())),
            ])
        );
    }

    #[test]
    fn resolves_bare_id_from_value() {
        // `(id:greeting)` means `args` is never truly absent here -- it's
        // always at least `{id: greeting}`, since that's where the
        // matched id itself lives. The merge is unfiltered (no special
        // "drop the id key" rule), so it shows up in the result
        // alongside `{value}`'s `text` key.
        let doc = parse("@meta(id:greeting){text: hi}\n");
        let value = resolve_reference(&doc, &interp("${greeting}")).unwrap();
        assert_eq!(
            value,
            Value::Map(vec![
                ("id".into(), Value::String("greeting".into())),
                ("text".into(), Value::String("hi".into())),
            ])
        );
    }

    #[test]
    fn member_prefers_value_over_args_on_conflict() {
        let doc = parse("<x>(id:greeting, text:from_args){text: from_value}\n");
        let value = resolve_reference(&doc, &interp("${greeting.text}")).unwrap();
        assert_eq!(value, Value::String("from_value".into()));
    }

    #[test]
    fn member_falls_back_to_args_when_absent_from_value() {
        let doc = parse("<x>(id:greeting, text:from_args){other: from_value}\n");
        let value = resolve_reference(&doc, &interp("${greeting.text}")).unwrap();
        assert_eq!(value, Value::String("from_args".into()));
    }

    #[test]
    fn nested_member_chain_resolves() {
        let doc = parse("<x>(id:a){b: {c: deep}}\n");
        let value = resolve_reference(&doc, &interp("${a.b.c}")).unwrap();
        assert_eq!(value, Value::String("deep".into()));
    }

    #[test]
    fn unknown_id_is_an_error() {
        let doc = parse("<x>(id:known)\n");
        let err = resolve_reference(&doc, &interp("${missing}")).unwrap_err();
        assert!(matches!(err, ResolveError::UnknownId { id } if id == "missing"));
    }

    #[test]
    fn member_on_non_map_is_an_error() {
        // `${a.text}` resolves to the scalar `Value::String("hi")` --
        // a bare id's own base is always at least `Value::Map` (its
        // `(args)` must be a map to hold the matched `id:` key at all),
        // so a truly non-map base only shows up one level deeper, here
        // via `.text` landing on a plain string before `.sub` tries to
        // go further.
        let doc = parse("<x>(id:a, text:hi)\n");
        let err = resolve_reference(&doc, &interp("${a.text.sub}")).unwrap_err();
        assert!(matches!(err, ResolveError::NoSuchMember { member } if member == "sub"));
    }

    #[test]
    fn missing_member_key_is_an_error() {
        let doc = parse("<x>(id:a){present: yes}\n");
        let err = resolve_reference(&doc, &interp("${a.absent}")).unwrap_err();
        assert!(matches!(err, ResolveError::NoSuchMember { member } if member == "absent"));
    }

    #[test]
    fn literal_is_not_a_reference() {
        let doc = parse("\n");
        let err = resolve_reference(&doc, &interp("${1}")).unwrap_err();
        assert!(matches!(err, ResolveError::NotAReference));
    }

    #[test]
    fn call_is_not_a_reference() {
        let doc = parse("\n");
        let err = resolve_reference(&doc, &interp("${sum(a, b)}")).unwrap_err();
        assert!(matches!(err, ResolveError::NotAReference));
    }
}
