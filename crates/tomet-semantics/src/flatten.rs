//! Projecting an element's data onto a flat, string-valued attribute map.
//!
//! Every output format Tomet writes into carries element data as string
//! pairs and nothing more -- HTML's `data-*`, Pandoc's `Attr`. A Tomet
//! value is a tree, so something has to give.
//!
//! The rule is @strong[a readable projection plus an exact copy] -- scalars and
//! sequences of scalars keep their own flat entry, and whenever that
//! projection would lose something -- a map anywhere, or a sequence
//! holding one -- the whole group is additionally serialized as JSON
//! under [`EXACT_DATA_KEY`]. A reader that finds that key should prefer
//! it and ignore the projection.
//!
//! Dotted keys (`m.k` for a nested `k` inside `m`) were considered and
//! rejected: a Tomet map key may itself contain `.` (the repository's own
//! `default.config.tmt` has `"url.wiki"`), so `m.k` would be ambiguous
//! between a nested lookup and a flat key of that name. Element @em[names]
//! use `.` as a namespace separator, but map keys deliberately do not.

use tomet_ast::{ElementView, Value};
use tomet_tree::scalar_string;

/// The attribute a consumer should read instead of the flat projection
/// when it is present.
pub const EXACT_DATA_KEY: &str = "tomet-data";

/// The key [`FlatData::into_pairs`] gives a positional group's value.
pub const POSITIONAL_KEY: &str = "value";

/// An element's data, ready for a string-valued attribute map.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FlatData {
    /// The readable projection: one entry per key whose value survives as
    /// a string. Callers add their own prefix (`data-`) or skip list.
    pub pairs: Vec<(String, String)>,
    /// A positional group's projection -- `@icon(star)` has a value but
    /// no key to hang it on, so it cannot join `pairs`. Callers name it
    /// themselves (the HTML writer calls it `data-value`).
    pub positional: Option<String>,
    /// JSON of the whole of `(args)` and `{value}`, present only when
    /// `pairs` and `positional` together would lose something.
    pub exact: Option<String>,
}

impl FlatData {
    /// The projection plus, when needed, the exact copy under
    /// [`EXACT_DATA_KEY`] -- i.e. everything a caller should emit.
    pub fn into_pairs(self) -> Vec<(String, String)> {
        let mut pairs = self.pairs;
        if let Some(positional) = self.positional {
            pairs.push((POSITIONAL_KEY.to_string(), positional));
        }
        if let Some(exact) = self.exact {
            pairs.push((EXACT_DATA_KEY.to_string(), exact));
        }
        pairs
    }
}

/// Whether `key` can be spliced into an attribute name (`data-{key}`).
///
/// Exactly the keys cotomet writes bare (letters, digits, `_`, `-`, `.`). A key
/// that needs quotes -- a path, a key with a space, `"` or `>` -- cannot be
/// part of an attribute name, and spliced in it would add attributes of its own.
/// The empty key is the positional marker and is not judged here.
pub fn is_attribute_key(key: &str) -> bool {
    key.chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.')
}

/// Flattens `el`'s `(args)` and `{value}` for an attribute map.
///
/// The element children a `{...}` group may hold are skipped: they belong
/// in the output's body rather than its attributes.
pub fn flatten_element_data(el: ElementView<'_>) -> FlatData {
    let value = el.value().map(|v| v.as_data());
    flatten_data(el.args(), value.as_ref())
}

/// Flattens the two groups directly.
///
/// Callers pass `None` for a group the surrounding mapping has already
/// consumed -- a heading's level, a link's target -- so it is not emitted
/// a second time as an attribute.
pub fn flatten_data(args: Option<&Value>, value: Option<&Value>) -> FlatData {
    let mut pairs = Vec::new();
    let mut positional = None;
    let mut lossy = false;
    for group in [args, value].into_iter().flatten() {
        match group {
            Value::Map(entries) => {
                for (k, v) in entries {
                    match scalar_string(v) {
                        // A key that cannot be spliced into `data-{key}` has no
                        // readable projection; the exact copy carries it.
                        Some(s) if is_attribute_key(k) => pairs.push((k.clone(), s)),
                        _ => lossy = true,
                    }
                }
            }
            // A positional group (`@icon(star)`) has no key. Its value
            // still projects if it is scalar; only a positional *map*
            // needs the exact copy.
            other => match scalar_string(other) {
                Some(s) => positional = Some(s),
                None => lossy = true,
            },
        }
    }

    let exact = lossy.then(|| {
        let mut obj = serde_json::Map::new();
        if let Some(args) = args {
            obj.insert("args".to_string(), value_to_json(args));
        }
        if let Some(value) = value {
            obj.insert("value".to_string(), value_to_json(value));
        }
        serde_json::Value::Object(obj).to_string()
    });

    FlatData {
        pairs,
        positional,
        exact,
    }
}

/// Converts a Tomet value to JSON, preserving structure exactly.
///
/// `tomet-convert-pandoc`'s own `json_to_value` is this function's
/// inverse, checked against it in `assert_json_round_trip`.
pub fn value_to_json(v: &Value) -> serde_json::Value {
    match v {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Int(i) => serde_json::Value::from(*i),
        Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            // JSON has no NaN or infinity; a string keeps the value
            // visible rather than turning it into null.
            .unwrap_or_else(|| serde_json::Value::String(f.to_string())),
        // JSON here has no arbitrary-precision number, so a kept-as-written number
        // is the `f64` it spells; one beyond an `f64` is its text.
        Value::Number(text) => match v.as_number().and_then(serde_json::Number::from_f64) {
            Some(n) => serde_json::Value::Number(n),
            None => serde_json::Value::String(text.clone()),
        },
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Seq(items) => serde_json::Value::Array(items.iter().map(value_to_json).collect()),
        Value::Map(entries) => serde_json::Value::Object(
            entries
                .iter()
                .map(|(k, v)| (k.clone(), value_to_json(v)))
                .collect(),
        ),
        Value::Call(name, args) => {
            let mut obj = serde_json::Map::new();
            obj.insert("call".to_string(), serde_json::Value::String(name.clone()));
            obj.insert(
                "args".to_string(),
                serde_json::Value::Array(args.iter().map(value_to_json).collect()),
            );
            serde_json::Value::Object(obj)
        }
        // Same shape as `Call` above (a tagged object), since JSON has no
        // element concept either. Only `sigil`/`args` carry over -- a
        // value-embedded element's own `content`/`value`/`children` are
        // rare enough in practice (`@doc.icon("x")` uses none of them)
        // that this isn't worth widening `serde_json` conversions for
        // `Inline`/`Group`, which don't have one anywhere else.
        // A hole exists only in a pattern, which is not data: only its matcher is kept.
        Value::Hole(hole) => {
            let mut obj = serde_json::Map::new();
            obj.insert(
                "hole".to_string(),
                serde_json::Value::String(
                    hole.matcher
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_default(),
                ),
            );
            serde_json::Value::Object(obj)
        }
        Value::Interp(interp) => {
            let mut obj = serde_json::Map::new();
            obj.insert(
                "interp".to_string(),
                serde_json::Value::String(interp.expr.to_string()),
            );
            serde_json::Value::Object(obj)
        }
        // A type seat exists only in a blueprint, which is not data: only the type, as written,
        // is kept.
        Value::TypeSeat(seat) => {
            let mut obj = serde_json::Map::new();
            obj.insert(
                "type_seat".to_string(),
                serde_json::Value::String(seat.expr.to_string()),
            );
            serde_json::Value::Object(obj)
        }
        Value::Element(el) => {
            let mut obj = serde_json::Map::new();
            let name = el.name.to_string();
            obj.insert("element".to_string(), serde_json::Value::String(name));
            if let Some(args) = &el.data.args {
                obj.insert("args".to_string(), value_to_json(args));
            }
            serde_json::Value::Object(obj)
        }
        // Same reasoning as `Element` above: block content has no stable
        // JSON shape anywhere else in the codebase, and an element like
        // `@conflict` that carries it is never flattened through this
        // generic path in practice -- a caller that knows about it reads
        // `a`/`b` directly. A lossy placeholder keeps this exhaustive
        // without inventing a new serialization nothing else uses.
        Value::Blocks(blocks) => serde_json::Value::String(format!("<{} block(s)>", blocks.len())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_ast::{Element, Group, Name};
    use tomet_tree::element_new;

    fn el_with(args: Option<Value>, value: Option<Value>) -> Element {
        let mut el = element_new(Name::bare("x"));
        el.data.args = args;
        el.data.value = value.map(Group::from_map);
        el
    }

    fn map(entries: &[(&str, Value)]) -> Value {
        Value::Map(
            entries
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        )
    }

    #[test]
    fn flat_data_projects_without_an_exact_copy() {
        let el = el_with(
            Some(map(&[
                ("a", Value::Int(1)),
                (
                    "tags",
                    Value::Seq(vec![Value::String("p".into()), Value::String("q".into())]),
                ),
            ])),
            None,
        );
        let data = flatten_element_data(el.as_view());
        assert_eq!(
            data.pairs,
            vec![
                ("a".to_string(), "1".to_string()),
                ("tags".to_string(), "p, q".to_string()),
            ]
        );
        assert_eq!(data.exact, None);
    }

    #[test]
    fn a_nested_map_adds_an_exact_copy_and_is_left_out_of_the_projection() {
        // The bug this exists for: the HTML writer must not emit
        // `data-m=""` and lose `{ k: v }` entirely.
        let el = el_with(
            Some(map(&[
                ("a", Value::Int(1)),
                ("m", map(&[("k", Value::String("v".into()))])),
            ])),
            None,
        );
        let data = flatten_element_data(el.as_view());
        assert_eq!(data.pairs, vec![("a".to_string(), "1".to_string())]);
        assert_eq!(
            data.exact.as_deref(),
            Some(r#"{"args":{"a":1,"m":{"k":"v"}}}"#)
        );
    }

    #[test]
    fn args_and_value_both_reach_the_exact_copy() {
        let el = el_with(
            Some(map(&[("url", Value::String("https://e.com".into()))])),
            Some(map(&[
                ("id", Value::String("b1".into())),
                ("meta", map(&[("author", Value::String("alice".into()))])),
            ])),
        );
        let data = flatten_element_data(el.as_view());
        assert_eq!(
            data.pairs,
            vec![
                ("url".to_string(), "https://e.com".to_string()),
                ("id".to_string(), "b1".to_string()),
            ]
        );
        assert_eq!(
            data.exact.as_deref(),
            Some(
                r#"{"args":{"url":"https://e.com"},"value":{"id":"b1","meta":{"author":"alice"}}}"#
            )
        );
    }

    #[test]
    fn into_pairs_appends_the_exact_copy() {
        let el = el_with(Some(map(&[("m", map(&[("k", Value::Int(1))]))])), None);
        let pairs = flatten_element_data(el.as_view()).into_pairs();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, EXACT_DATA_KEY);
    }

    #[test]
    fn a_sequence_of_maps_does_not_project() {
        assert_eq!(
            scalar_string(&Value::Seq(vec![map(&[("k", Value::Int(1))])])),
            None
        );
    }

    #[test]
    fn a_list_projects_like_a_sequence() {
        let list = Value::Seq(vec![Value::String("a".into()), Value::String("b".into())]);
        assert_eq!(scalar_string(&list), Some("a, b".to_string()));

        let json = value_to_json(&list);
        assert_eq!(json, serde_json::json!(["a", "b"]));
    }
}
