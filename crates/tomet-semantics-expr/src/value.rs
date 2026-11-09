//! The values an expression evaluates to.
//!
//! This is not [`tomet_ast::Value`]. That type is the `.tmt` data model: what
//! `::meta(...)` and `(args)` hold, and what serde reads and writes. An
//! expression also produces things that only exist while evaluating -- a
//! reference to a document, a directory, a function -- and putting those in
//! the data model would make it carry values that can never be saved.
//! [`EvalValue::from_ast`] and [`EvalValue::to_ast`] cross between the two.
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
use tomet_ast::{Block, Connect, Declaration, Element, InterpExpr, List, ListItem, Section, Value};
use tomet_type::Version;

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
/// be inspected via `.name`, `.prop`, `.id`, `.connects`, `.content`, and
/// `.item(n)`.
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
    /// `generate`: the functions whose result changes from call to call.
    /// Only available when a document is being created.
    Generate,
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
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    /// A number kept as it was written, because reading it as an `i64` or an
    /// `f64` would change it: `1.10` (an `f64` prints `1.1`), `1e3`, `-0`, or an
    /// integer too large for an `i64`.
    Number(String),
    String(String),
    Seq(Vec<EvalValue>),
    /// Insertion-ordered, like [`Value::Map`].
    Map(Vec<(String, EvalValue)>),
    /// A call literal, kept as written (`list(string)`, `enum(a, b)`).
    /// A sequence `(a, b)` is an [`EvalValue::Seq`], not a call.
    Call(String, Vec<EvalValue>),
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
}

impl EvalValue {
    /// The name used in error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            EvalValue::Null => "null",
            EvalValue::Bool(_) => "bool",
            EvalValue::Int(_) => "int",
            EvalValue::Float(_) => "float",
            EvalValue::Number(_) => "number",
            EvalValue::String(_) => "string",
            EvalValue::Seq(_) => "list",
            EvalValue::Map(_) => "map",
            EvalValue::Call(..) => "call",
            EvalValue::Part { .. } => "element",
            EvalValue::Document(_) => "document",
            EvalValue::Directory(_) => "directory",
            EvalValue::Fragment(_) => "fragment",
            EvalValue::DateTime(_) => "datetime",
            EvalValue::Version(_) => "version",
            EvalValue::Function(_) => "function",
            EvalValue::Namespace(_) => "namespace",
        }
    }

    /// Reads a `.tmt` data value. `doc` is the document the value was read
    /// from, for the elements it may contain.
    pub fn from_ast(value: &Value, doc: &DocRef) -> EvalValue {
        match value {
            Value::Null => EvalValue::Null,
            Value::Bool(b) => EvalValue::Bool(*b),
            Value::Int(i) => EvalValue::Int(*i),
            Value::Float(f) => EvalValue::Float(*f),
            Value::Number(text) => EvalValue::Number(text.clone()),
            Value::String(s) => EvalValue::String(s.clone()),
            Value::Seq(items) => {
                EvalValue::Seq(items.iter().map(|v| Self::from_ast(v, doc)).collect())
            }
            Value::Map(entries) => EvalValue::Map(
                entries
                    .iter()
                    .map(|(k, v)| (k.clone(), Self::from_ast(v, doc)))
                    .collect(),
            ),
            Value::Call(name, args) => {
                let args = args.iter().map(|v| Self::from_ast(v, doc)).collect();
                EvalValue::Call(name.clone(), args)
            }
            Value::Element(el) => EvalValue::Part {
                doc: doc.clone(),
                part: Box::new(Part::Element(el.clone())),
            },
            Value::Blocks(blocks) => EvalValue::Fragment(blocks.clone()),
            // Not evaluated yet, so it stays what it is: a block that holds it.
            Value::Interp(interp) => EvalValue::Fragment(vec![Block::Interp(interp.clone())]),
            // A hole exists only in a pattern; kept as the block that holds it.
            Value::Hole(hole) => EvalValue::Fragment(vec![Block::Hole((**hole).clone())]),
        }
    }

    /// Writes the value back as `.tmt` data. `None` for the values that only
    /// exist while evaluating: documents, directories, functions, datetimes
    /// and namespaces, and a section or a list (which are not elements).
    pub fn to_ast(&self) -> Option<Value> {
        Some(match self {
            EvalValue::Null => Value::Null,
            EvalValue::Bool(b) => Value::Bool(*b),
            EvalValue::Int(i) => Value::Int(*i),
            EvalValue::Float(f) => Value::Float(*f),
            EvalValue::Number(text) => Value::Number(text.clone()),
            EvalValue::String(s) => Value::String(s.clone()),
            EvalValue::Seq(items) => {
                Value::Seq(items.iter().map(Self::to_ast).collect::<Option<_>>()?)
            }
            EvalValue::Map(entries) => Value::Map(
                entries
                    .iter()
                    .map(|(k, v)| v.to_ast().map(|v| (k.clone(), v)))
                    .collect::<Option<_>>()?,
            ),
            EvalValue::Call(name, args) => Value::Call(
                name.clone(),
                args.iter().map(Self::to_ast).collect::<Option<_>>()?,
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
            EvalValue::Version(v) => Value::String(v.to_string()),
            EvalValue::Document(_)
            | EvalValue::Directory(_)
            | EvalValue::DateTime(_)
            | EvalValue::Function(_)
            | EvalValue::Namespace(_) => return None,
        })
    }

    /// The text of a bare scalar, if that is what the value is.
    ///
    /// Following `tmtroot/spec/expressions/overview.expr.tmt`, scalars are
    /// `Null`, booleans, numbers, strings, and versions. A `Null` evaluates to
    /// empty text (`""`).
    ///
    /// Returns `None` for non-scalar values (such as sequences, maps, parts,
    /// documents, directories, fragments, functions, or namespaces).
    pub fn scalar_text(&self) -> Option<String> {
        Some(match self {
            EvalValue::Null => String::new(),
            EvalValue::Bool(b) => b.to_string(),
            EvalValue::Int(i) => i.to_string(),
            EvalValue::Float(f) => f.to_string(),
            EvalValue::Number(text) => text.clone(),
            EvalValue::String(s) => s.clone(),
            EvalValue::Version(v) => v.to_string(),
            _ => return None,
        })
    }
}

/// The text of a bare scalar, if that is what the value is.
///
/// See [`EvalValue::scalar_text`].
pub fn scalar_text(value: &EvalValue) -> Option<String> {
    value.scalar_text()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> DocRef {
        DocRef("a.tmt".to_string())
    }

    #[test]
    fn number_preserved_to_eval_and_back() {
        let ast = Value::Number("1.10".into());
        let eval = EvalValue::from_ast(&ast, &doc());
        assert_eq!(eval, EvalValue::Number("1.10".into()));
        assert_eq!(eval.type_name(), "number");
        assert_eq!(eval.to_ast(), Some(ast));
    }

    #[test]
    fn data_values_cross_to_eval_values_and_back() {
        let ast = Value::Map(vec![
            ("n".into(), Value::Int(1)),
            ("tags".into(), Value::Seq(vec![Value::String("a".into())])),
            ("ok".into(), Value::Bool(true)),
            ("nothing".into(), Value::Null),
        ]);
        let eval = EvalValue::from_ast(&ast, &doc());
        assert_eq!(eval.to_ast(), Some(ast));
    }

    #[test]
    fn a_list_is_a_seq_and_a_call_is_kept() {
        let list = Value::Seq(vec![Value::Int(1), Value::Int(2)]);
        assert_eq!(
            EvalValue::from_ast(&list, &doc()),
            EvalValue::Seq(vec![EvalValue::Int(1), EvalValue::Int(2)])
        );
        let other = Value::Call("enum".into(), vec![Value::String("a".into())]);
        assert!(matches!(
            EvalValue::from_ast(&other, &doc()),
            EvalValue::Call(name, _) if name == "enum"
        ));
    }

    #[test]
    fn values_that_only_exist_while_evaluating_do_not_become_data() {
        assert_eq!(EvalValue::Document(doc()).to_ast(), None);
        assert_eq!(EvalValue::Directory(DirRef("d".into())).to_ast(), None);
        assert_eq!(EvalValue::Namespace(Namespace::Macro).to_ast(), None);
        // One of them anywhere inside makes the whole thing not data.
        assert_eq!(
            EvalValue::Seq(vec![EvalValue::Int(1), EvalValue::Document(doc())]).to_ast(),
            None
        );
    }

    #[test]
    fn an_env_finds_the_innermost_binding_and_does_not_disturb_the_outer() {
        let outer = Env::new().bind("x", EvalValue::Int(1));
        let inner = outer
            .bind("x", EvalValue::Int(2))
            .bind("y", EvalValue::Int(3));
        assert_eq!(inner.get("x"), Some(&EvalValue::Int(2)));
        assert_eq!(inner.get("y"), Some(&EvalValue::Int(3)));
        assert_eq!(outer.get("x"), Some(&EvalValue::Int(1)));
        assert_eq!(outer.get("y"), None);
    }

    #[test]
    fn test_eval_value_scalar_text() {
        assert_eq!(EvalValue::Null.scalar_text().as_deref(), Some(""));
        assert_eq!(EvalValue::Bool(true).scalar_text().as_deref(), Some("true"));
        assert_eq!(EvalValue::Int(42).scalar_text().as_deref(), Some("42"));
        assert_eq!(
            EvalValue::Float(3.14).scalar_text().as_deref(),
            Some("3.14")
        );
        assert_eq!(
            EvalValue::Number("1.10".into()).scalar_text().as_deref(),
            Some("1.10")
        );
        assert_eq!(
            EvalValue::String("hello".into()).scalar_text().as_deref(),
            Some("hello")
        );
        assert_eq!(
            EvalValue::Version(Version::parse("1.2.3").unwrap())
                .scalar_text()
                .as_deref(),
            Some("1.2.3")
        );
        assert_eq!(EvalValue::Seq(vec![]).scalar_text(), None);
        assert_eq!(EvalValue::Map(vec![]).scalar_text(), None);
        assert_eq!(EvalValue::Document(doc()).scalar_text(), None);
    }
}
