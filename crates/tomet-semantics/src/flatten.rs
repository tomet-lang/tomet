//! Projecting an element's data onto a flat, string-valued attribute map.
//!
//! Every output format Tomet writes into carries element data as string
//! pairs and nothing more -- HTML's `data-*`, Pandoc's `Attr`. A Tomet
//! value is a tree, so something has to give.
//!
//! The rule is **a readable projection plus an exact copy**: scalars and
//! sequences of scalars keep their own flat entry, and whenever that
//! projection would lose something -- a map anywhere, or a sequence
//! holding one -- the whole group is additionally serialized as JSON
//! under [`EXACT_DATA_KEY`]. A reader that finds that key should prefer
//! it and ignore the projection.
//!
//! Dotted keys (`m.k` for a nested `k` inside `m`) were considered and
//! rejected: a Tomet map key may itself contain `.` (the repository's own
//! `default.config.tmt` has `"url.wiki"`), so `m.k` would be ambiguous
//! between a nested lookup and a flat key of that name. Element *names*
//! use `.` as a namespace separator, but map keys deliberately do not.

use tomet_ast::{Element, Value};

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

/// Flattens `el`'s `(args)` and `{value}` for an attribute map.
///
/// A `+++` raw body and a `${...}` interpolation are not data and are
/// skipped; so are the element children a `{...}` group may hold, which
/// belong in the output's body rather than its attributes.
pub fn flatten_element_data(el: &Element) -> FlatData {
    let value = el.value.as_ref().and_then(|v| v.as_data());
    flatten_data(el.args.as_ref(), value.as_ref())
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
                        Some(s) => pairs.push((k.clone(), s)),
                        None => lossy = true,
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

/// The string a value projects to, or `None` when it cannot project.
///
/// A sequence joins with `", "`, which is what the HTML writer has always
/// done. That is not reversible -- `[p, q]` and the literal `"p, q"`
/// project identically -- but it stays readable, and the exact copy is
/// what a reader uses when it needs the difference.
pub fn scalar_string(v: &Value) -> Option<String> {
    match v {
        Value::Null => Some(String::new()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Int(i) => Some(i.to_string()),
        Value::Float(f) => Some(f.to_string()),
        Value::String(s) => Some(s.clone()),
        Value::Seq(items) => {
            let parts: Option<Vec<String>> = items.iter().map(scalar_string).collect();
            Some(parts?.join(", "))
        }
        Value::Map(_) => None,
        // A call is a structured literal like a map, not a scalar --
        // there is no single string it collapses to.
        Value::Call(..) => None,
        // An embedded element is a whole element, not a scalar -- same
        // bucket as `Map`/`Call`.
        Value::Element(_) => None,
    }
}

/// Converts a Tomet value to JSON, preserving structure exactly.
///
/// The inverse of `embedded::json_to_value`, which reads a `+++` fence
/// body declared `format: json` back into a [`Value`].
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
        // `Inline`/`ElementValue`, which don't have one anywhere else.
        Value::Element(el) => {
            let mut obj = serde_json::Map::new();
            let name = el
                .sigil
                .name()
                .map(|n| n.to_string())
                .unwrap_or_default();
            obj.insert("element".to_string(), serde_json::Value::String(name));
            if let Some(args) = &el.args {
                obj.insert("args".to_string(), value_to_json(args));
            }
            serde_json::Value::Object(obj)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_ast::{ElementValue, Sigil};
    use tomet_tree::element_new;

    fn el_with(args: Option<Value>, value: Option<Value>) -> Element {
        let mut el = element_new(Sigil::named("x"));
        el.args = args;
        el.value = value.map(ElementValue::from_map);
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
        let data = flatten_element_data(&el);
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
        // The bug this exists for: the HTML writer used to emit
        // `data-m=""` and lose `{ k: v }` entirely.
        let el = el_with(
            Some(map(&[
                ("a", Value::Int(1)),
                ("m", map(&[("k", Value::String("v".into()))])),
            ])),
            None,
        );
        let data = flatten_element_data(&el);
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
        let data = flatten_element_data(&el);
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
        let pairs = flatten_element_data(&el).into_pairs();
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
}
