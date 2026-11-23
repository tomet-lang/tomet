//! The plain-data projection of a [`Value`]: [`PlainValue`], and what refuses to become one, [`NotPlain`].
//!
//! A [`Value`] has two serde forms, for two readers. Its derived form (see [`Value`]) is the AST
//! wire: lossless, tagged, and read back as the same value. [`PlainValue`] is the other one, for a
//! reader who wants *data* -- `null`, booleans, numbers, strings, arrays and objects, what every
//! language already has -- and does not want to learn the AST to get it: a document's `::meta`, a
//! `.ctmt` value, the value a script parses out of a string. The rules of the projection are
//! those of [`PlainValue`].

use crate::{EntryKey, Tuple, Value};
use serde::{Serialize, ser};
use std::fmt;
use tomet_scalar::Scalar;

/// Plain data: what a [`Value`] says when it is only data, as the JSON every language has. It is
/// what `parseValue`, `loads`, and the `meta` of a processed file hand out.
///
/// =[ Rules ]
///
/// The projection is explicitly lossy, and it does not round-trip: a [`PlainValue`] is not a
/// [`Value`], and nothing reads one back into one (it is written, never read).
///
/// - `none` is `null`, a boolean is a boolean, an integer is an integer and a text is a string.
/// - A float, and a number kept as written, is a float: `1.10` is `1.1`, `1e3` is `1000.0` and an
///   integer beyond an `i64` is the nearest `f64`. One with no finite `f64` (`1e999`, `NaN`) has no
///   plain form and is an error: a string that stood for it would be indistinguishable from a text.
/// - A tuple with at least one entry, and no entry with a key, is an array. Any other tuple is an
///   object, in written order: an entry's key is its name, or its position in decimal if it has
///   none, so `(a, k: b)` is `{"0": a, "k": b}`. An empty tuple is an empty object, not an empty
///   array: it has no entry to say it is a list, and an object is the container that can hold what
///   a tuple can. A reader that must tell `(a, b)` from `("0": a, "1": b)` cannot: they are one tuple.
/// - Everything else has no plain form and is refused ([`NotPlain`]), never lowered to a stand-in. `auto` is
///   not `null` (that is `none`) and not the text `"auto"`; a [`Value::Call`], [`Value::Interp`],
///   [`Value::Hole`], [`Value::TypeSeat`], [`Value::Element`], [`Value::Blocks`] and a type-seat key
///   are nodes, and an object that stood for one would be exactly the ambiguity the wire exists
///   to remove. A reader that wants those reads the AST wire.
///
/// An object keeps its entries in order and holds no key twice. Serialized, it is the natural JSON
/// of its shape (`null`, a boolean, a number, a string, an array, an object) with no tag of its own.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema", schemars(untagged))]
pub enum PlainValue {
    /// `none`.
    Null,
    /// A boolean.
    Bool(bool),
    /// An integer.
    Int(i64),
    /// A float, or a number kept as written.
    Float(f64),
    /// A text.
    Text(String),
    /// A tuple with entries, none of them keyed.
    Array(Vec<PlainValue>),
    /// Any other tuple, in written order: an entry's key is its name, or its position among the
    /// unnamed entries if it has none. An empty tuple is the empty object.
    Object(
        #[cfg_attr(
            feature = "schema",
            schemars(with = "std::collections::BTreeMap<String, PlainValue>")
        )]
        Vec<(String, PlainValue)>,
    ),
}

/// What a [`Value`] holds that is not plain data, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotPlain {
    /// What it is that has no plain form.
    pub reason: NotPlainReason,
    /// The keys from the root to the tuple or value at fault: a name, or a position in decimal. Empty at the root.
    pub path: Vec<String>,
}

/// Why a [`Value`] has no plain form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotPlainReason {
    /// `auto`: leaves the value to the system, which says nothing as data.
    Auto,
    /// A float, or a number kept as written, that no finite `f64` holds.
    NotFinite,
    /// A [`Value::Call`].
    Call,
    /// A [`Value::Interp`].
    Interp,
    /// A [`Value::Hole`].
    Hole,
    /// A [`Value::TypeSeat`].
    TypeSeat,
    /// A tuple entry keyed by a type seat. It is `path` that names the tuple.
    TypeSeatKey,
    /// A [`Value::Element`].
    Element,
    /// A [`Value::Blocks`].
    Blocks,
}

impl NotPlainReason {
    fn describe(self) -> &'static str {
        match self {
            NotPlainReason::Auto => "`auto`",
            NotPlainReason::NotFinite => "a number with no finite value",
            NotPlainReason::Call => "a call",
            NotPlainReason::Interp => "an interpolation",
            NotPlainReason::Hole => "a hole",
            NotPlainReason::TypeSeat => "a type seat",
            NotPlainReason::TypeSeatKey => "a type seat key",
            NotPlainReason::Element => "an element",
            NotPlainReason::Blocks => "block content",
        }
    }
}

impl fmt::Display for NotPlain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} is not plain data", self.reason.describe())?;
        if !self.path.is_empty() {
            write!(f, " (at `{}`)", self.path.join("."))?;
        }
        Ok(())
    }
}

impl std::error::Error for NotPlain {}

impl Value {
    /// This value as plain data, or the first node in it that has none. See the [module](self).
    pub fn to_plain(&self) -> Result<PlainValue, NotPlain> {
        value(self, &mut Vec::new())
    }
}

impl Tuple {
    /// This tuple as plain data: an array or an object, or the first node in it that has no plain
    /// form. See the [module](self).
    pub fn to_plain(&self) -> Result<PlainValue, NotPlain> {
        tuple(self, &mut Vec::new())
    }

    /// The entries of this tuple that are plain data, each as its key and its plain value, in
    /// written order; the rest are left out.
    ///
    /// This is for a reader that takes what is data from a tuple that also holds what is not, such as the
    /// `::meta` of a document, which may write `icon: @doc.icon("star")` beside `title: "T"`. An entry is
    /// left out if its value is not plain (see [`Value::to_plain`]) or its key is a type seat; nothing
    /// is lowered in its place. Keys are as [`Tuple::to_plain`] writes them in an object: a name, or an
    /// unnamed entry's position among the unnamed entries as written, so a left-out unnamed entry leaves a
    /// gap in the positions. An object of the result is `PlainValue::Object(tuple.plain_entries().collect())`.
    pub fn plain_entries(&self) -> impl Iterator<Item = (String, PlainValue)> + '_ {
        let mut position = 0;
        self.entries().filter_map(move |entry| {
            let key = match entry.key() {
                Some(EntryKey::Name(name)) => name.clone(),
                Some(EntryKey::TypeSeat(_)) => return None,
                None => {
                    position += 1;
                    (position - 1).to_string()
                }
            };
            Some((key, entry.value().to_plain().ok()?))
        })
    }
}

impl TryFrom<&Value> for PlainValue {
    type Error = NotPlain;

    fn try_from(v: &Value) -> Result<Self, NotPlain> {
        v.to_plain()
    }
}

fn not_plain(reason: NotPlainReason, path: &[String]) -> NotPlain {
    NotPlain {
        reason,
        path: path.to_vec(),
    }
}

fn value(v: &Value, path: &mut Vec<String>) -> Result<PlainValue, NotPlain> {
    Ok(match v {
        Value::Scalar(scalar) => match scalar {
            Scalar::None => PlainValue::Null,
            Scalar::Auto => return Err(not_plain(NotPlainReason::Auto, path)),
            Scalar::Bool(b) => PlainValue::Bool(*b),
            Scalar::Int(i) => PlainValue::Int(*i),
            Scalar::Float(_) | Scalar::Number(_) => match scalar.as_number() {
                Some(f) if f.is_finite() => PlainValue::Float(f),
                _ => return Err(not_plain(NotPlainReason::NotFinite, path)),
            },
            Scalar::Text(text) => PlainValue::Text(text.clone()),
        },
        Value::Tuple(t) => tuple(t, path)?,
        Value::Call(..) => return Err(not_plain(NotPlainReason::Call, path)),
        Value::Interp(_) => return Err(not_plain(NotPlainReason::Interp, path)),
        Value::Hole(_) => return Err(not_plain(NotPlainReason::Hole, path)),
        Value::TypeSeat(_) => return Err(not_plain(NotPlainReason::TypeSeat, path)),
        Value::Element(_) => return Err(not_plain(NotPlainReason::Element, path)),
        Value::Blocks(_) => return Err(not_plain(NotPlainReason::Blocks, path)),
    })
}

fn tuple(t: &Tuple, path: &mut Vec<String>) -> Result<PlainValue, NotPlain> {
    if !t.is_empty() && t.has_no_named() {
        return t
            .values()
            .enumerate()
            .map(|(position, v)| {
                path.push(position.to_string());
                let plain = value(v, path);
                path.pop();
                plain
            })
            .collect::<Result<_, _>>()
            .map(PlainValue::Array);
    }
    let mut position = 0;
    let mut object = Vec::with_capacity(t.len());
    for entry in t {
        let key = match entry.key() {
            Some(EntryKey::Name(name)) => name.clone(),
            Some(EntryKey::TypeSeat(_)) => {
                return Err(not_plain(NotPlainReason::TypeSeatKey, path));
            }
            None => {
                position += 1;
                (position - 1).to_string()
            }
        };
        path.push(key.clone());
        let plain = value(entry.value(), path);
        path.pop();
        object.push((key, plain?));
    }
    Ok(PlainValue::Object(object))
}

impl Serialize for PlainValue {
    fn serialize<S: ser::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            PlainValue::Null => serializer.serialize_none(),
            PlainValue::Bool(b) => serializer.serialize_bool(*b),
            PlainValue::Int(i) => serializer.serialize_i64(*i),
            PlainValue::Float(f) => serializer.serialize_f64(*f),
            PlainValue::Text(text) => serializer.serialize_str(text),
            PlainValue::Array(items) => serializer.collect_seq(items),
            PlainValue::Object(entries) => {
                serializer.collect_map(entries.iter().map(|(k, v)| (k, v)))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Block, Element, Hole, InterpExpr, InterpExprKind, Interpolation, Name, TupleEntry, TypeSeat,
    };
    use tomet_span::Span;

    fn scalar(s: Scalar) -> Value {
        Value::Scalar(s)
    }

    fn int(n: i64) -> Value {
        scalar(Scalar::Int(n))
    }

    fn expr() -> InterpExpr {
        InterpExpr {
            kind: InterpExprKind::Identifier("x".into()),
            span: Span::dummy(),
        }
    }

    fn json(plain: &PlainValue) -> String {
        serde_json::to_string(plain).unwrap()
    }

    fn plain_json(v: &Value) -> String {
        json(&v.to_plain().unwrap())
    }

    fn mixed() -> Tuple {
        Tuple::try_from_entries([
            TupleEntry::unnamed(int(10)),
            TupleEntry::named("foo", int(20)),
            TupleEntry::unnamed(int(30)),
        ])
        .unwrap()
    }

    #[test]
    fn a_scalar_is_the_data_it_says() {
        let cases = [
            (Scalar::None, "null"),
            (Scalar::Bool(true), "true"),
            (Scalar::Int(-3), "-3"),
            (Scalar::Float(0.5), "0.5"),
            (Scalar::Text("auto".into()), r#""auto""#),
            (Scalar::Text("1.10".into()), r#""1.10""#),
        ];
        for (s, expected) in cases {
            assert_eq!(plain_json(&scalar(s)), expected);
        }
    }

    #[test]
    fn a_number_kept_as_written_is_the_float_it_spells() {
        let float = |text: &str| scalar(Scalar::Number(text.into())).to_plain().unwrap();
        assert_eq!(float("1.10"), PlainValue::Float(1.1));
        assert_eq!(float("1e3"), PlainValue::Float(1000.0));
        assert_eq!(float("-0"), PlainValue::Float(-0.0));
        assert_eq!(
            float("123456789012345678901234567890"),
            PlainValue::Float(1.2345678901234568e29)
        );
    }

    #[test]
    fn a_number_no_float_holds_is_not_plain() {
        for s in [
            Scalar::Number("1e999".into()),
            Scalar::Float(f64::NAN),
            Scalar::Float(f64::INFINITY),
        ] {
            let err = scalar(s).to_plain().unwrap_err();
            assert_eq!(err.reason, NotPlainReason::NotFinite);
        }
    }

    #[test]
    fn auto_is_not_plain() {
        let err = scalar(Scalar::Auto).to_plain().unwrap_err();
        assert_eq!(
            err,
            NotPlain {
                reason: NotPlainReason::Auto,
                path: vec![]
            }
        );
        assert_eq!(err.to_string(), "`auto` is not plain data");
    }

    #[test]
    fn a_tuple_is_an_array_or_an_object_by_its_keys() {
        assert_eq!(plain_json(&Value::Tuple(Tuple::new())), "{}");
        assert_eq!(
            plain_json(&Value::Tuple(Tuple::from_unnamed([int(1), int(2)]))),
            "[1,2]"
        );
        assert_eq!(
            plain_json(&Value::Tuple(Tuple::from_named([
                ("a".into(), int(1)),
                ("b".into(), int(2))
            ]))),
            r#"{"a":1,"b":2}"#
        );
        // An unnamed entry among named ones is keyed by its position, in the order written.
        assert_eq!(
            plain_json(&Value::Tuple(mixed())),
            r#"{"0":10,"foo":20,"1":30}"#
        );
        // `(a, b)` and `("0": a, "1": b)` are one tuple, so one array.
        let written = Tuple::from_named([("0".into(), int(1)), ("1".into(), int(2))]);
        assert_eq!(plain_json(&Value::Tuple(written)), "[1,2]");
        // `("1": 2, "0": 1)` names its first entry, so it is an object, in order.
        let reordered = Tuple::try_from_entries([
            TupleEntry::named("1", int(2)),
            TupleEntry::named("0", int(1)),
        ])
        .unwrap();
        assert_eq!(plain_json(&Value::Tuple(reordered)), r#"{"1":2,"0":1}"#);
    }

    #[test]
    fn tuple_to_plain_is_the_tuple_value_to_plain() {
        let t = mixed();
        assert_eq!(t.to_plain(), Value::Tuple(t).to_plain());
    }

    #[test]
    fn a_node_that_is_not_data_is_named_and_placed() {
        let seat = TypeSeat {
            expr: expr(),
            span: Span::dummy(),
        };
        let hole = Hole {
            matcher: None,
            placement: crate::Placement::Inline,
            data: crate::Data::default(),
            content: None,
            span: Span::dummy(),
        };
        let element = Element {
            name: Name::bare("e"),
            placement: crate::Placement::Inline,
            data: crate::Data::default(),
            content: None,
            id: None,
            connects: vec![],
            span: Span::dummy(),
        };
        let cases = [
            (
                Value::Call("enum".into(), Tuple::new()),
                NotPlainReason::Call,
                "a call",
            ),
            (
                Value::Interp(Interpolation {
                    expr: expr(),
                    span: Span::dummy(),
                }),
                NotPlainReason::Interp,
                "an interpolation",
            ),
            (Value::Hole(Box::new(hole)), NotPlainReason::Hole, "a hole"),
            (
                Value::TypeSeat(seat),
                NotPlainReason::TypeSeat,
                "a type seat",
            ),
            (
                Value::Element(Box::new(element)),
                NotPlainReason::Element,
                "an element",
            ),
            (
                Value::Blocks(Vec::<Block>::new()),
                NotPlainReason::Blocks,
                "block content",
            ),
        ];
        for (node, reason, what) in cases {
            // At the root, and as the second entry of the value of `a`: `(a: (1, <node>))`.
            let at_root = node.to_plain().unwrap_err();
            assert_eq!(
                at_root,
                NotPlain {
                    reason,
                    path: vec![]
                }
            );
            assert_eq!(at_root.to_string(), format!("{what} is not plain data"));

            let nested = Value::Tuple(Tuple::from_named([(
                "a".into(),
                Value::Tuple(Tuple::from_unnamed([int(1), node])),
            )]));
            let nested = nested.to_plain().unwrap_err();
            assert_eq!(
                nested,
                NotPlain {
                    reason,
                    path: vec!["a".into(), "1".into()]
                }
            );
            assert_eq!(
                nested.to_string(),
                format!("{what} is not plain data (at `a.1`)")
            );
        }
    }

    #[test]
    fn a_type_seat_key_is_not_plain() {
        let seat = TypeSeat {
            expr: expr(),
            span: Span::dummy(),
        };
        let t = Tuple::from_entries([
            TupleEntry::unnamed(int(0)),
            TupleEntry::type_seat(seat, int(1)),
        ]);
        let wrapped = Value::Tuple(Tuple::from_named([("k".into(), Value::Tuple(t))]));
        let err = wrapped.to_plain().unwrap_err();
        assert_eq!(
            err,
            NotPlain {
                reason: NotPlainReason::TypeSeatKey,
                path: vec!["k".into()]
            }
        );
    }

    #[test]
    fn the_plain_entries_of_a_tuple_are_the_ones_that_are_data() {
        let seat = TypeSeat {
            expr: expr(),
            span: Span::dummy(),
        };
        let call = Value::Call("enum".into(), Tuple::new());
        let tuple = Tuple::try_from_entries([
            TupleEntry::unnamed(int(1)),
            TupleEntry::named("title", scalar(Scalar::Text("T".into()))),
            TupleEntry::named("icon", call.clone()),
            TupleEntry::named("mode", scalar(Scalar::Auto)),
            TupleEntry::type_seat(seat, int(7)),
            TupleEntry::unnamed(call),
            TupleEntry::unnamed(int(3)),
            TupleEntry::named("nested", Value::Tuple(mixed())),
        ])
        .unwrap();
        let entries: Vec<(String, String)> = tuple
            .plain_entries()
            .map(|(key, plain)| (key, json(&plain)))
            .collect();
        // A position is the entry's among the unnamed ones as written, so a left-out entry leaves a gap.
        assert_eq!(
            entries,
            [
                ("0".to_string(), "1".to_string()),
                ("title".into(), r#""T""#.into()),
                ("2".into(), "3".into()),
                ("nested".into(), r#"{"0":10,"foo":20,"1":30}"#.into()),
            ]
        );
        // Nothing is lowered: the whole tuple, which holds them, is not plain.
        assert!(tuple.to_plain().is_err());
        assert_eq!(Tuple::new().plain_entries().count(), 0);
    }
}
