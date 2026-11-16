//! The values an expression evaluates to.
//!
//! This is not [`tomet_ast::Value`]. That type is the `.tmt` data model: what
//! `::meta(...)` and `(args)` hold, and what serde reads and writes. An
//! expression also produces things that only exist while evaluating -- a
//! reference to a document, a directory, a function -- and putting those in
//! the data model would make it carry values that can never be saved.
//! [`EvalValue::from_ast`] and [`EvalValue::to_ast`] cross between the two.
//!
//! =[ Types ]
//!
//! A [`Type`] is a value too ([`EvalValue::Type`]): `string` and
//! `list(string, length: 6)` are written in the expression language and
//! evaluate to one. A type is not data, so it has no `.tmt` form to cross
//! to, and it is not text, so it is not placed in output.
//!
//! =[ References and fragments ]
//!
//! A [`DocRef`], [`DirRef`] and [`EvalValue::Part`] point at something in the
//! source. Putting one where output goes is a type mismatch, because it does
//! not say whether the author wants a link or the content. A
//! [`EvalValue::Fragment`] is a tree built by evaluation (`include`, `link`)
//! and can be placed.

use std::rc::Rc;

use time::OffsetDateTime;
use tomet_ast::{
    Block, Connect, Declaration, Element, InterpExpr, List, ListItem, Section, Tuple, Value,
};
use tomet_scalar::Scalar;
use tomet_type::{Type, Version};

/// A document, by vault-relative path with `/` separators. A reference: the
/// content is fetched through the [`super::Resolver`] when it is needed.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DocRef(pub String);

/// A directory, by vault-relative path with `/` separators.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DirRef(pub String);

/// A structured part of a document: an element, a section, a list, a connect,
/// a declaration, or a list item.
///
/// Unlike a whole document ([`EvalValue::Document`]) or detached block output
/// ([`EvalValue::Fragment`]), a [`Part`] belongs to a source document and can
/// be inspected via `::id`, `::name`, `::connects`, `::content`, the connect of a
/// given name (`::meta`) and `::doc`; read for a key of its data (`.key`); and
/// asked for `.item(n)` and `.link()`.
///
/// A heading's id attaches to the whole section (heading and everything under
/// it), not to a lone element, and a list's to the whole list, not to one of
/// its items -- which is why each is a part of its own.
///
/// A list has an `id` in the tree, but no syntax writes one yet (a trailing
/// `#(id)` belongs to the last item). It is addressed here by the same rules
/// as the others, so that when a spelling is chosen nothing in the evaluator
/// has to change.
#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    Element(Box<Element>),
    Section(Box<Section>),
    List(Box<List>),
    /// A `::name(...)` stacked after one of the others. It has no id, so no
    /// `#id` finds it; it is reached through its owner's `.connects`.
    Connect(Box<Connect>),
    /// A `::name(...)` declaration, when it was given an id.
    Declaration(Box<Declaration>),
    /// An item of a list: a part of its own, since it can carry an id.
    Item(Box<ListItem>),
}

impl Part {
    /// The part's unique id, if it has one.
    pub fn id(&self) -> Option<&str> {
        match self {
            Part::Element(el) => el.id.as_ref().map(|id| id.0.as_str()),
            Part::Section(sec) => sec.id.as_ref().map(|id| id.0.as_str()),
            Part::List(list) => list.id.as_ref().map(|id| id.0.as_str()),
            Part::Connect(_) => None,
            Part::Declaration(d) => d.id.as_ref().map(|id| id.0.as_str()),
            Part::Item(item) => item.id.as_ref().map(|id| id.0.as_str()),
        }
    }
}

/// A name that holds members and is not itself a value you can print.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Namespace {
    /// `macro`: the macros defined by `::macro`.
    Macro,
}

/// A lambda together with the names it closed over.
#[derive(Clone)]
pub struct Closure {
    pub params: Vec<String>,
    pub body: Rc<InterpExpr>,
    pub env: Env,
}

impl std::fmt::Debug for Closure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Closure({} => {})", self.params.join(", "), self.body)
    }
}

/// Two functions are equal only when they are the same lambda expression.
impl PartialEq for Closure {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.body, &other.body)
    }
}

/// The names a lambda body can see besides the reserved ones: its own
/// parameters, and the parameters of the lambdas around it.
///
/// A linked list shared by reference counting, so binding a name never
/// copies the names already bound and a closure can keep its env cheaply.
#[derive(Clone, Default)]
pub struct Env(Option<Rc<EnvNode>>);

struct EnvNode {
    name: String,
    value: EvalValue,
    parent: Env,
}

impl Env {
    pub fn new() -> Self {
        Self::default()
    }

    /// A new env with `name` bound to `value`, in front of this one.
    pub fn bind(&self, name: impl Into<String>, value: EvalValue) -> Env {
        Env(Some(Rc::new(EnvNode {
            name: name.into(),
            value,
            parent: self.clone(),
        })))
    }

    /// The innermost binding of `name`.
    pub fn get(&self, name: &str) -> Option<&EvalValue> {
        let mut cur = self;
        while let Some(node) = &cur.0 {
            if node.name == name {
                return Some(&node.value);
            }
            cur = &node.parent;
        }
        None
    }
}

impl std::fmt::Debug for Env {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Env(..)")
    }
}

/// What an expression evaluates to.
#[derive(Debug, Clone, PartialEq)]
pub enum EvalValue {
    /// A scalar value: `none`, `auto`, a boolean, an integer, a float,
    /// a number kept as written, or text. See [`Scalar`].
    Scalar(Scalar),
    /// A tuple of values: the one data container of the evaluator, matching [`Tuple`](tomet_ast::Tuple).
    Tuple(Tuple<EvalValue>),
    /// A call literal, kept as written (`list(string)`, `enum(a, b)`).
    Call(String, Tuple<EvalValue>),
    /// A structured part of a document: an element, a section, a list, a connect,
    /// a declaration, or a list item, with the document it came from.
    Part {
        doc: DocRef,
        part: Box<Part>,
    },
    Document(DocRef),
    Directory(DirRef),
    /// A tree built by evaluation. The only kind of value besides scalars
    /// that can be placed where output goes.
    Fragment(Vec<Block>),
    DateTime(OffsetDateTime),
    Version(Version),
    Function(Closure),
    Namespace(Namespace),
    /// A type: `string`, `list(string, length: 6)`. It is a value of the expression language
    /// and not of the data model, so it has no data form.
    Type(Type),
}

impl EvalValue {
    /// The name used in error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            EvalValue::Scalar(s) => match s {
                Scalar::None => "none",
                Scalar::Auto => "auto",
                Scalar::Bool(_) => "bool",
                Scalar::Int(_) => "int",
                Scalar::Float(_) => "float",
                Scalar::Number(_) => "number",
                Scalar::Text(_) => "string",
            },
            EvalValue::Tuple(_) => "tuple",
            EvalValue::Call(..) => "call",
            EvalValue::Part { .. } => "element",
            EvalValue::Document(_) => "document",
            EvalValue::Directory(_) => "directory",
            EvalValue::Fragment(_) => "fragment",
            EvalValue::DateTime(_) => "datetime",
            EvalValue::Version(_) => "version",
            EvalValue::Function(_) => "function",
            EvalValue::Namespace(_) => "namespace",
            EvalValue::Type(_) => "type",
        }
    }

    /// Reads a `.tmt` data value. `doc` is the document the value was read
    /// from, for the elements it may contain.
    pub fn from_ast(value: &Value, doc: &DocRef) -> EvalValue {
        match value {
            Value::Scalar(s) => EvalValue::Scalar(s.clone()),
            Value::Tuple(tuple) => {
                EvalValue::Tuple(tuple.clone().map_values(|v| Self::from_ast(&v, doc)))
            }
            Value::Call(name, args) => EvalValue::Call(
                name.clone(),
                args.clone().map_values(|v| Self::from_ast(&v, doc)),
            ),
            Value::Element(el) => EvalValue::Part {
                doc: doc.clone(),
                part: Box::new(Part::Element(el.clone())),
            },
            Value::Blocks(blocks) => EvalValue::Fragment(blocks.clone()),
            // Not evaluated yet, so it stays what it is: a block that holds it.
            Value::Interp(interp) => EvalValue::Fragment(vec![Block::Interp(interp.clone())]),
            // A hole exists only in a pattern; kept as the block that holds it.
            Value::Hole(hole) => EvalValue::Fragment(vec![Block::Hole((**hole).clone())]),
            // A type seat exists only in a blueprint, and is inert like a hole until a later
            // step fills it; kept as the block that holds it.
            Value::TypeSeat(seat) => EvalValue::Fragment(vec![Block::TypeSeat(seat.clone())]),
        }
    }

    /// Writes the value back as `.tmt` data. `None` for the values that only
    /// exist while evaluating: documents, directories, functions, datetimes,
    /// namespaces and types, and a section or a list (which are not elements).
    pub fn to_ast(&self) -> Option<Value> {
        Some(match self {
            EvalValue::Scalar(s) => Value::Scalar(s.clone()),
            EvalValue::Tuple(tuple) => Value::Tuple(
                tuple
                    .clone()
                    .try_map_values(|v| Self::to_ast(&v).ok_or(()))
                    .ok()?,
            ),
            EvalValue::Call(name, args) => Value::Call(
                name.clone(),
                args.clone()
                    .try_map_values(|v| Self::to_ast(&v).ok_or(()))
                    .ok()?,
            ),
            EvalValue::Part { part, .. } => match &**part {
                Part::Element(el) => Value::Element(el.clone()),
                Part::Section(_)
                | Part::List(_)
                | Part::Connect(_)
                | Part::Declaration(_)
                | Part::Item(_) => {
                    return None;
                }
            },
            EvalValue::Fragment(blocks) => Value::Blocks(blocks.clone()),
            EvalValue::Version(v) => Value::Scalar(Scalar::Text(v.to_string())),
            EvalValue::Document(_)
            | EvalValue::Directory(_)
            | EvalValue::DateTime(_)
            | EvalValue::Function(_)
            | EvalValue::Namespace(_)
            | EvalValue::Type(_) => return None,
        })
    }

    /// The text of a bare scalar, if that is what the value is.
    ///
    /// Following `tmtroot/spec/expressions/overview.expr.tmt`, scalars are
    /// `None`, booleans, numbers, strings, and versions. A `None` evaluates to
    /// empty text (`""`).
    ///
    /// Returns `None` for non-scalar values (such as sequences, maps, parts,
    /// documents, directories, fragments, functions, namespaces, or types).
    pub fn scalar_text(&self) -> Option<String> {
        match self {
            EvalValue::Scalar(Scalar::None) => Some(String::new()),
            EvalValue::Scalar(s) => s.text().map(|c| c.into_owned()),
            EvalValue::Version(v) => Some(v.to_string()),
            _ => None,
        }
    }

    /// Returns a reference to the inner [`Scalar`], if this value is one.
    pub fn as_scalar(&self) -> Option<&Scalar> {
        match self {
            EvalValue::Scalar(s) => Some(s),
            _ => None,
        }
    }
}

/// The text of a bare scalar, if that is what the value is.
///
/// See [`EvalValue::scalar_text`].
pub fn scalar_text(value: &EvalValue) -> Option<String> {
    value.scalar_text()
}

impl From<tomet_scalar::Scalar> for EvalValue {
    fn from(s: tomet_scalar::Scalar) -> Self {
        EvalValue::Scalar(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_type::Kind;

    fn doc() -> DocRef {
        DocRef("a.tmt".to_string())
    }

    #[test]
    fn number_preserved_to_eval_and_back() {
        let ast = Value::Scalar(Scalar::Number("1.10".into()));
        let eval = EvalValue::from_ast(&ast, &doc());
        assert_eq!(eval, EvalValue::Scalar(Scalar::Number("1.10".into())));
        assert_eq!(eval.type_name(), "number");
        assert_eq!(eval.to_ast(), Some(ast));
    }

    #[test]
    fn data_values_cross_to_eval_values_and_back() {
        let ast = Value::Tuple(Tuple::from_named([
            ("n".into(), Value::Scalar(Scalar::Int(1))),
            (
                "tags".into(),
                Value::Tuple(Tuple::from_unnamed([Value::Scalar(Scalar::Text(
                    "a".into(),
                ))])),
            ),
            ("ok".into(), Value::Scalar(Scalar::Bool(true))),
            ("nothing".into(), Value::Scalar(Scalar::None)),
        ]));
        let eval = EvalValue::from_ast(&ast, &doc());
        assert_eq!(eval.to_ast(), Some(ast));
    }

    #[test]
    fn a_tuple_and_a_call_are_kept() {
        let list = Value::Tuple(Tuple::from_unnamed([
            Value::Scalar(Scalar::Int(1)),
            Value::Scalar(Scalar::Int(2)),
        ]));
        assert_eq!(
            EvalValue::from_ast(&list, &doc()),
            EvalValue::Tuple(Tuple::from_unnamed([
                EvalValue::Scalar(Scalar::Int(1)),
                EvalValue::Scalar(Scalar::Int(2))
            ]))
        );
        let other = Value::Call(
            "enum".into(),
            Tuple::from_unnamed([Value::Scalar(Scalar::Text("a".into()))]),
        );
        assert!(matches!(
            EvalValue::from_ast(&other, &doc()),
            EvalValue::Call(name, _) if name == "enum"
        ));
    }

    #[test]
    fn empty_tuple_and_mixed_tuple_cross_to_eval_values() {
        let empty = Value::Tuple(Tuple::new());
        assert_eq!(
            EvalValue::from_ast(&empty, &doc()),
            EvalValue::Tuple(Tuple::new())
        );

        let mut mixed = Tuple::new();
        mixed
            .try_push_unnamed(Value::Scalar(Scalar::Int(10)))
            .unwrap();
        mixed
            .try_push_named("k", Value::Scalar(Scalar::Int(20)))
            .unwrap();
        mixed
            .try_push_unnamed(Value::Scalar(Scalar::Int(30)))
            .unwrap();
        let eval = EvalValue::from_ast(&Value::Tuple(mixed.clone()), &doc());
        let back = eval.to_ast().expect("converts back to AST value");
        assert_eq!(back, Value::Tuple(mixed));
    }

    #[test]
    fn mixed_tuple_read_from_source_through_eval_bridge() {
        let ast = tomet_ast_parser::parse_value(r#"(a, sep: ",")"#).unwrap();
        let eval = EvalValue::from_ast(&ast, &doc());
        let back = eval.to_ast().expect("converts back to AST value");
        assert_eq!(back, ast);
    }

    #[test]
    fn values_that_only_exist_while_evaluating_do_not_become_data() {
        assert_eq!(EvalValue::Document(doc()).to_ast(), None);
        assert_eq!(EvalValue::Directory(DirRef("d".into())).to_ast(), None);
        assert_eq!(EvalValue::Namespace(Namespace::Macro).to_ast(), None);
        assert_eq!(EvalValue::Type(Type::bare(Kind::String)).to_ast(), None);
        // One of them anywhere inside makes the whole thing not data.
        assert_eq!(
            EvalValue::Tuple(Tuple::from_unnamed([
                EvalValue::Scalar(Scalar::Int(1)),
                EvalValue::Document(doc())
            ]))
            .to_ast(),
            None
        );
    }

    #[test]
    fn an_env_finds_the_innermost_binding_and_does_not_disturb_the_outer() {
        let outer = Env::new().bind("x", EvalValue::Scalar(Scalar::Int(1)));
        let inner = outer
            .bind("x", EvalValue::Scalar(Scalar::Int(2)))
            .bind("y", EvalValue::Scalar(Scalar::Int(3)));
        assert_eq!(inner.get("x"), Some(&EvalValue::Scalar(Scalar::Int(2))));
        assert_eq!(inner.get("y"), Some(&EvalValue::Scalar(Scalar::Int(3))));
        assert_eq!(outer.get("x"), Some(&EvalValue::Scalar(Scalar::Int(1))));
        assert_eq!(outer.get("y"), None);
    }

    #[test]
    fn test_eval_value_scalar_text() {
        assert_eq!(
            EvalValue::Scalar(Scalar::None).scalar_text().as_deref(),
            Some("")
        );
        assert_eq!(
            EvalValue::Scalar(Scalar::Auto).scalar_text().as_deref(),
            Some("auto")
        );
        assert_eq!(
            EvalValue::Scalar(Scalar::Bool(true))
                .scalar_text()
                .as_deref(),
            Some("true")
        );
        assert_eq!(
            EvalValue::Scalar(Scalar::Int(42)).scalar_text().as_deref(),
            Some("42")
        );
        assert_eq!(
            EvalValue::Scalar(Scalar::Float(3.14))
                .scalar_text()
                .as_deref(),
            Some("3.14")
        );
        assert_eq!(
            EvalValue::Scalar(Scalar::Number("1.10".into()))
                .scalar_text()
                .as_deref(),
            Some("1.10")
        );
        assert_eq!(
            EvalValue::Scalar(Scalar::Text("hello".into()))
                .scalar_text()
                .as_deref(),
            Some("hello")
        );
        assert_eq!(
            EvalValue::Version(Version::parse("1.2.3").unwrap())
                .scalar_text()
                .as_deref(),
            Some("1.2.3")
        );
        assert_eq!(EvalValue::Tuple(Tuple::new()).scalar_text(), None);
        assert_eq!(EvalValue::Document(doc()).scalar_text(), None);
        assert_eq!(
            EvalValue::Type(Type::bare(Kind::String)).scalar_text(),
            None
        );
    }
}
