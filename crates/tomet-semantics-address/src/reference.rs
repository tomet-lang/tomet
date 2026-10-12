//! Resolves a `${...}` interpolation's address path
//! (`tomet-parser`'s `InterpExpr`) against a `Document`'s nodes.
//!
//! Supports:
//! - Root element lookups: `${#intro}` or bare `${intro}`
//! - Element property access: `${#intro.prop}` or `${#intro.prop.tags}`
//! - Step indexing: `${#intro.prop.tags.item(0)}` or `${#intro.item(0)}`
//! - Method-style property lookup: `${#intro.prop("tags")}` or `${#intro.prop(tags)}`
//! - Element metadata projection: `${#intro.id}`, `${#intro.sigil}`, `${#intro.connects}`
//! - Root document indexing: `${item(0)}`

use tomet_ast::{
    Block, Document, Element, ElementValue, Entry, Inline, InterpExpr, InterpExprKind, List,
    Literal, Value,
};
use tomet_tree::ValueExt;

use crate::ResolveError;

/// An intermediate or final target encountered during path traversal.
enum Target<'a> {
    Element(&'a Element),
    Value(Value),
}

/// Resolves an interpolation reference expression against `doc`.
pub fn resolve_reference(doc: &Document, expr: &InterpExpr) -> Result<Value, ResolveError> {
    let target = resolve_target(doc, expr)?;
    match target {
        Target::Element(el) => Ok(node_value(el)),
        Target::Value(val) => Ok(val),
    }
}

fn resolve_target<'a>(doc: &'a Document, expr: &InterpExpr) -> Result<Target<'a>, ResolveError> {
    match &expr.kind {
        InterpExprKind::Id(id) | InterpExprKind::Identifier(id) => {
            let el = find_element_by_id(doc, id)
                .ok_or_else(|| ResolveError::UnknownId { id: id.clone() })?;
            Ok(Target::Element(el))
        }
        InterpExprKind::Member { object, member } => {
            let base = resolve_target(doc, object)?;
            match base {
                Target::Element(el) => match member.as_str() {
                    "prop" => Ok(Target::Value(node_value(el))),
                    "id" => Ok(Target::Value(
                        el.id
                            .as_ref()
                            .map(|id| Value::String(id.0.clone()))
                            .unwrap_or(Value::Null),
                    )),
                    "sigil" => Ok(Target::Value(Value::String(
                        el.sigil.name().map(|n| n.to_string()).unwrap_or_default(),
                    ))),
                    "connects" => Ok(Target::Value(Value::Seq(
                        el.connects
                            .iter()
                            .map(|c| Value::Element(Box::new(c.clone())))
                            .collect(),
                    ))),
                    "content" => {
                        let blocks = el.content.clone().unwrap_or_default();
                        Ok(Target::Value(Value::Blocks(blocks)))
                    }
                    _ => {
                        // Direct property access fallback (e.g. #intro.tags)
                        let val = node_value(el);
                        if let Some(sub) = value_member(&val, member) {
                            Ok(Target::Value(sub))
                        } else {
                            Err(ResolveError::NoSuchMember {
                                member: member.clone(),
                            })
                        }
                    }
                },
                Target::Value(val) => {
                    value_member(&val, member)
                        .map(Target::Value)
                        .ok_or_else(|| ResolveError::NoSuchMember {
                            member: member.clone(),
                        })
                }
            }
        }
        InterpExprKind::Call { callee, args } => match &callee.kind {
            InterpExprKind::Member { object, member } => {
                let base = resolve_target(doc, object)?;
                match member.as_str() {
                    "item" => {
                        let idx = eval_int_index(args)?;
                        match base {
                            Target::Value(val) => {
                                let items = match &val {
                                    Value::Seq(items) => Some(items.as_slice()),
                                    Value::Call(name, items) if name == "list" => {
                                        Some(items.as_slice())
                                    }
                                    _ => None,
                                };
                                if let Some(items) = items {
                                    if idx >= 0 && (idx as usize) < items.len() {
                                        Ok(Target::Value(items[idx as usize].clone()))
                                    } else {
                                        Err(ResolveError::IndexOutOfBounds { index: idx })
                                    }
                                } else {
                                    Err(ResolveError::InvalidStep {
                                        message: format!(
                                            "cannot index with item({idx}) on non-sequence"
                                        ),
                                    })
                                }
                            }
                            Target::Element(el) => {
                                if let Some(content) = &el.content {
                                    if idx >= 0 && (idx as usize) < content.len() {
                                        match &content[idx as usize] {
                                            Block::Element(child_el) => {
                                                Ok(Target::Element(child_el))
                                            }
                                            other => Ok(Target::Value(Value::Blocks(vec![
                                                other.clone(),
                                            ]))),
                                        }
                                    } else {
                                        Err(ResolveError::IndexOutOfBounds { index: idx })
                                    }
                                } else {
                                    Err(ResolveError::IndexOutOfBounds { index: idx })
                                }
                            }
                        }
                    }
                    "prop" => {
                        let key = eval_string_key(args)?;
                        match base {
                            Target::Element(el) => {
                                let val = node_value(el);
                                value_member(&val, &key)
                                    .map(Target::Value)
                                    .ok_or_else(|| ResolveError::NoSuchMember { member: key })
                            }
                            Target::Value(val) => value_member(&val, &key)
                                .map(Target::Value)
                                .ok_or_else(|| ResolveError::NoSuchMember { member: key }),
                        }
                    }
                    _ => Err(ResolveError::NotAReference),
                }
            }
            InterpExprKind::Identifier(name) if name == "item" => {
                let idx = eval_int_index(args)?;
                if idx >= 0 && (idx as usize) < doc.blocks.len() {
                    match &doc.blocks[idx as usize] {
                        Block::Element(el) => Ok(Target::Element(el)),
                        other => Ok(Target::Value(Value::Blocks(vec![other.clone()]))),
                    }
                } else {
                    Err(ResolveError::IndexOutOfBounds { index: idx })
                }
            }
            _ => Err(ResolveError::NotAReference),
        },
        _ => Err(ResolveError::NotAReference),
    }
}

fn eval_int_index(args: &[InterpExpr]) -> Result<i64, ResolveError> {
    match args {
        [expr] => match &expr.kind {
            InterpExprKind::Literal(Literal::Int(i)) => Ok(*i),
            _ => Err(ResolveError::InvalidStep {
                message: "item() requires an integer index".to_string(),
            }),
        },
        other => Err(ResolveError::InvalidStep {
            message: format!("item() expects 1 argument, got {}", other.len()),
        }),
    }
}

fn eval_string_key(args: &[InterpExpr]) -> Result<String, ResolveError> {
    match args {
        [expr] => match &expr.kind {
            InterpExprKind::Literal(Literal::String(s)) => Ok(s.clone()),
            InterpExprKind::Identifier(s) => Ok(s.clone()),
            _ => Err(ResolveError::InvalidStep {
                message: "prop() requires a string or identifier argument".to_string(),
            }),
        },
        other => Err(ResolveError::InvalidStep {
            message: format!("prop() expects 1 argument, got {}", other.len()),
        }),
    }
}

fn find_element_by_id<'a>(doc: &'a Document, id: &str) -> Option<&'a Element> {
    for block in &doc.blocks {
        if let Some(el) = find_element_in_block(block, id) {
            return Some(el);
        }
    }
    None
}

fn find_element_in_block<'a>(block: &'a Block, target: &str) -> Option<&'a Element> {
    match block {
        Block::Section(sec) => {
            for child in &sec.blocks {
                if let Some(el) = find_element_in_block(child, target) {
                    return Some(el);
                }
            }
            for conn in &sec.connects {
                if let Some(el) = find_element_in_element(conn, target) {
                    return Some(el);
                }
            }
            None
        }
        Block::Element(el) => find_element_in_element(el, target),
        Block::Paragraph(p) => {
            for inline in &p.content {
                if let Inline::Element(el) = inline {
                    if let Some(found) = find_element_in_element(el, target) {
                        return Some(found);
                    }
                }
            }
            None
        }
        Block::List(list) => find_element_in_list(list, target),
    }
}

fn find_element_in_list<'a>(list: &'a List, target: &str) -> Option<&'a Element> {
    for conn in &list.connects {
        if let Some(el) = find_element_in_element(conn, target) {
            return Some(el);
        }
    }
    for item in &list.items {
        if let Some(el) = find_element_in_element(&item.element, target) {
            return Some(el);
        }
        if let Some(sub) = &item.sublist {
            if let Some(el) = find_element_in_list(sub, target) {
                return Some(el);
            }
        }
    }
    None
}

fn find_element_in_element<'a>(el: &'a Element, target: &str) -> Option<&'a Element> {
    if el.id.as_ref().is_some_and(|id| id.0 == target) {
        return Some(el);
    }
    if let Some(content) = &el.content {
        for block in content {
            if let Some(found) = find_element_in_block(block, target) {
                return Some(found);
            }
        }
    }
    for conn in &el.connects {
        if let Some(found) = find_element_in_element(conn, target) {
            return Some(found);
        }
    }
    if let Some(ElementValue::Group(entries)) = &el.value {
        for entry in entries {
            if let Entry::Element(child) = entry {
                if let Some(found) = find_element_in_element(child, target) {
                    return Some(found);
                }
            }
        }
    }
    None
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
    use tomet_ast::Block;

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
    fn resolves_hash_id() {
        let doc = parse("@x#(greeting)(text:hi)\n");
        let value = resolve_reference(&doc, &interp("${#greeting}")).unwrap();
        assert_eq!(
            value,
            Value::Map(vec![("text".into(), Value::String("hi".into())),])
        );
    }

    #[test]
    fn resolves_element_metadata_fields() {
        let doc = parse("@card#(intro)(priority: high)\n");
        assert_eq!(
            resolve_reference(&doc, &interp("${#intro.id}")).unwrap(),
            Value::String("intro".into())
        );
        assert_eq!(
            resolve_reference(&doc, &interp("${#intro.sigil}")).unwrap(),
            Value::String("card".into())
        );
    }

    #[test]
    fn resolves_prop_dot_access() {
        let doc = parse("@card#(intro)(tags: list(alpha, beta), count: 2)\n");
        // .prop gets the whole map
        assert!(matches!(
            resolve_reference(&doc, &interp("${#intro.prop}")).unwrap(),
            Value::Map(_)
        ));
        // .prop.tags gets the list
        assert_eq!(
            resolve_reference(&doc, &interp("${#intro.prop.tags}")).unwrap(),
            Value::Call(
                "list".into(),
                vec![Value::String("alpha".into()), Value::String("beta".into())]
            )
        );
        // .prop.tags.item(0) gets the first item
        assert_eq!(
            resolve_reference(&doc, &interp("${#intro.prop.tags.item(0)}")).unwrap(),
            Value::String("alpha".into())
        );
        assert_eq!(
            resolve_reference(&doc, &interp("${#intro.prop.tags.item(1)}")).unwrap(),
            Value::String("beta".into())
        );
        // item out of bounds
        assert!(matches!(
            resolve_reference(&doc, &interp("${#intro.prop.tags.item(99)}")).unwrap_err(),
            ResolveError::IndexOutOfBounds { index: 99 }
        ));
    }

    #[test]
    fn resolves_prop_function_call() {
        let doc = parse("@card#(intro)(title: Hello)\n");
        assert_eq!(
            resolve_reference(&doc, &interp("${#intro.prop(\"title\")}")).unwrap(),
            Value::String("Hello".into())
        );
        assert_eq!(
            resolve_reference(&doc, &interp("${#intro.prop(title)}")).unwrap(),
            Value::String("Hello".into())
        );
    }

    #[test]
    fn resolves_root_item() {
        let doc = parse("@card#(first)(title: One)\n@card#(second)(title: Two)\n");
        assert_eq!(
            resolve_reference(&doc, &interp("${item(0).id}")).unwrap(),
            Value::String("first".into())
        );
        assert_eq!(
            resolve_reference(&doc, &interp("${item(1).prop.title}")).unwrap(),
            Value::String("Two".into())
        );
    }

    #[test]
    fn resolves_bare_id_from_args_and_value() {
        let doc1 = parse("@x#(greeting)(text:hi)\n");
        let value1 = resolve_reference(&doc1, &interp("${greeting}")).unwrap();
        assert_eq!(
            value1,
            Value::Map(vec![("text".into(), Value::String("hi".into())),])
        );

        let doc2 = parse("@meta#(greeting){text: hi}\n");
        let value2 = resolve_reference(&doc2, &interp("${greeting}")).unwrap();
        assert_eq!(
            value2,
            Value::Map(vec![("text".into(), Value::String("hi".into())),])
        );
    }

    #[test]
    fn member_prefers_value_over_args_on_conflict() {
        let doc = parse("@x#(greeting)(text:from_args){text: from_value}\n");
        let value = resolve_reference(&doc, &interp("${#greeting.text}")).unwrap();
        assert_eq!(value, Value::String("from_value".into()));
    }

    #[test]
    fn nested_member_chain_resolves() {
        let doc = parse("@x#(a){b: {c: deep}}\n");
        let value = resolve_reference(&doc, &interp("${#a.b.c}")).unwrap();
        assert_eq!(value, Value::String("deep".into()));
    }

    #[test]
    fn unknown_id_is_an_error() {
        let doc = parse("@x#(known)\n");
        let err = resolve_reference(&doc, &interp("${#missing}")).unwrap_err();
        assert!(matches!(err, ResolveError::UnknownId { id } if id == "missing"));
    }

    #[test]
    fn call_is_not_a_reference_when_not_a_step() {
        let doc = parse("\n");
        let err = resolve_reference(&doc, &interp("${sum(a, b)}")).unwrap_err();
        assert!(matches!(err, ResolveError::NotAReference));
    }
}
