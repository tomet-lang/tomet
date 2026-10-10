//! Resolves a `${...}` interpolation's `Identifier`/`Member` chain
//! (`tomet-parser`'s `InterpExpr`) against a `Document`'s own
//! `#(id)`-tagged nodes. Same-document only for v1 -- no
//! `@settings(file:...)`-style cross-file lookup yet.
//!
//! `InterpExprKind::Literal`/`Call` aren't references at all, so
//! [`resolve_reference`] rejects them; evaluating a `Call` (calling a
//! named function) is `tomet-compute`'s job, which is expected to
//! call back into this function for its `Identifier`/`Member`
//! sub-expressions rather than reimplementing id lookup itself.

use std::ops::ControlFlow;
use tomet_ast::{Document, Element, InterpExpr, InterpExprKind, Value};
use tomet_tree::{ValueExt, walk_document};

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
        InterpExprKind::Literal(_)
        | InterpExprKind::Call { .. }
        | InterpExprKind::NamedArg { .. } => Err(ResolveError::NotAReference),
    }
}

/// Depth-first search (via `tomet-tree`) for the first node anywhere
/// in `doc` with a matching `#(id)`. Returns a deep copy of that
/// node's value payload --
/// `ElementValue::from_map(v)` -> `v`, `ElementValue::from_children(c)` ->
/// `Value::Seq(...)` (wrapped as a synthetic list of the children's
/// values).
///
/// Note: this returns the *first* match in document order; duplicate-id
/// detection is a validator concern (`tomet-validator`), not a resolver
/// one. Traversal order is defined by the underlying walker
/// (`tomet_tree::walk_document`), not this search's own logic.
fn find_by_id(doc: &Document, id: &str) -> Option<Value> {
    let mut visitor = |el: &Element| {
        if id_matches(el, id) {
            ControlFlow::Break(node_value(el))
        } else {
            ControlFlow::Continue(())
        }
    };
    match walk_document(doc, &mut visitor) {
        ControlFlow::Break(value) => Some(value),
        ControlFlow::Continue(()) => None,
    }
}

fn id_matches(el: &Element, target: &str) -> bool {
    el.id.as_ref().is_some_and(|id| id.0 == target)
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
        Some(v) => match v.as_data() {
            Some(Value::Map(entries)) => Some(entries),
            _ => None,
        },
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
            Some(v) => v.as_data().unwrap_or(Value::Map(Vec::new())),
            _ => el.args.clone().unwrap_or(Value::Null),
        },
    }
}

fn value_member(base: &Value, member: &str) -> Option<Value> {
    base.get(member).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_ast::{Block, ElementValue};

    fn parse(src: &str) -> Document {
        tomet_parser::parse_document(src).expect("valid Tomet source")
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
        let doc = parse("@x#(greeting)(text:hi)\n");
        let value = resolve_reference(&doc, &interp("${greeting}")).unwrap();
        assert_eq!(
            value,
            Value::Map(vec![("text".into(), Value::String("hi".into())),])
        );
    }

    #[test]
    fn resolves_bare_id_from_value() {
        // The id lives in its own `#(...)` slot now, not in `args` or
        // `{value}`, so a node with no `(args)` at all but a `{value}`
        // resolves to just that value -- no "id" key shows up alongside
        // it the way it used to when `id` was merely an attribute key.
        let doc = parse("@meta#(greeting){text: hi}\n");
        let value = resolve_reference(&doc, &interp("${greeting}")).unwrap();
        assert_eq!(
            value,
            Value::Map(vec![("text".into(), Value::String("hi".into())),])
        );
    }

    #[test]
    fn member_prefers_value_over_args_on_conflict() {
        let doc = parse("@x#(greeting)(text:from_args){text: from_value}\n");
        let value = resolve_reference(&doc, &interp("${greeting.text}")).unwrap();
        assert_eq!(value, Value::String("from_value".into()));
    }

    #[test]
    fn member_falls_back_to_args_when_absent_from_value() {
        let doc = parse("@x#(greeting)(text:from_args){other: from_value}\n");
        let value = resolve_reference(&doc, &interp("${greeting.text}")).unwrap();
        assert_eq!(value, Value::String("from_args".into()));
    }

    #[test]
    fn nested_member_chain_resolves() {
        let doc = parse("@x#(a){b: {c: deep}}\n");
        let value = resolve_reference(&doc, &interp("${a.b.c}")).unwrap();
        assert_eq!(value, Value::String("deep".into()));
    }

    #[test]
    fn unknown_id_is_an_error() {
        let doc = parse("@x#(known)\n");
        let err = resolve_reference(&doc, &interp("${missing}")).unwrap_err();
        assert!(matches!(err, ResolveError::UnknownId { id } if id == "missing"));
    }

    #[test]
    fn member_on_non_map_is_an_error() {
        // `${a.text}` resolves to the scalar `Value::String("hi")` --
        // `a`'s own base is `Value::Map` here only because its `(args)`
        // happens to be one, so a truly non-map base only shows up one
        // level deeper, here via `.text` landing on a plain string
        // before `.sub` tries to go further.
        let doc = parse("@x#(a)(text:hi)\n");
        let err = resolve_reference(&doc, &interp("${a.text.sub}")).unwrap_err();
        assert!(matches!(err, ResolveError::NoSuchMember { member } if member == "sub"));
    }

    #[test]
    fn missing_member_key_is_an_error() {
        let doc = parse("@x#(a){present: yes}\n");
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
