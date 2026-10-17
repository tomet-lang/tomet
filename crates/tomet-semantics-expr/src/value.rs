//! The values an expression evaluates to.
//!
//! This is not [`tomet_ast::Value`]. That type is the `.tmt` data model: what
//! `@meta{...}` and `(args)` hold, and what serde reads and writes. An
//! expression also produces things that only exist while evaluating -- a
//! reference to a document, a directory, a function -- and putting those in
//! the data model would make it carry values that can never be saved.
//! [`EvalValue::from_ast`] and [`EvalValue::to_ast`] cross between the two.
//!
//! =[ References and fragments ]
//!
//! A [`DocRef`], [`DirRef`] and [`EvalValue::Node`] point at something in the
//! source. Putting one where output goes is a type mismatch, because it does
//! not say whether the author wants a link or the content. A
//! [`EvalValue::Fragment`] is a tree built by evaluation (`include`, `link`)
//! and can be placed.

use std::rc::Rc;

use time::OffsetDateTime;
use tomet_ast::{Block, Element, InterpExpr, Section, Value};

/// A document, by vault-relative path with `/` separators. A reference: the
/// content is fetched through the [`super::Resolver`] when it is needed.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DocRef(pub String);

/// A directory, by vault-relative path with `/` separators.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DirRef(pub String);

/// A node that can carry a unique id: an element or a section.
///
/// Both are needed because a heading's id attaches to the whole section
/// (heading and everything under it), not to a lone element.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Element(Box<Element>),
    Section(Box<Section>),
}

impl Node {
    /// The node's unique id, if it has one.
    pub fn id(&self) -> Option<&str> {
        match self {
            Node::Element(el) => el.id.as_ref().map(|id| id.0.as_str()),
            Node::Section(sec) => sec.id.as_ref().map(|id| id.0.as_str()),
        }
    }
}

/// A name that holds members and is not itself a value you can print.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Namespace {
    /// `macro`: the macros defined in settings.
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
    String(String),
    Seq(Vec<EvalValue>),
    /// Insertion-ordered, like [`Value::Map`].
    Map(Vec<(String, EvalValue)>),
    /// A call literal other than `list(...)`, kept as written
    /// (`enum(a, b)`). `list(...)` becomes a [`EvalValue::Seq`].
    Call(String, Vec<EvalValue>),
    /// An element or a section, with the document it came from.
    Node {
        doc: DocRef,
        node: Box<Node>,
    },
    Document(DocRef),
    Directory(DirRef),
    /// A tree built by evaluation. The only kind of value besides scalars
    /// that can be placed where output goes.
    Fragment(Vec<Block>),
    DateTime(OffsetDateTime),
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
            EvalValue::String(_) => "string",
            EvalValue::Seq(_) => "list",
            EvalValue::Map(_) => "map",
            EvalValue::Call(..) => "call",
            EvalValue::Node { .. } => "element",
            EvalValue::Document(_) => "document",
            EvalValue::Directory(_) => "directory",
            EvalValue::Fragment(_) => "fragment",
            EvalValue::DateTime(_) => "datetime",
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
                if name == "list" {
                    EvalValue::Seq(args)
                } else {
                    EvalValue::Call(name.clone(), args)
                }
            }
            Value::Element(el) => EvalValue::Node {
                doc: doc.clone(),
                node: Box::new(Node::Element(el.clone())),
            },
            Value::Blocks(blocks) => EvalValue::Fragment(blocks.clone()),
        }
    }

    /// Writes the value back as `.tmt` data. `None` for the values that only
    /// exist while evaluating: documents, directories, functions, datetimes
    /// and namespaces, and a section (which is not an element).
    pub fn to_ast(&self) -> Option<Value> {
        Some(match self {
            EvalValue::Null => Value::Null,
            EvalValue::Bool(b) => Value::Bool(*b),
            EvalValue::Int(i) => Value::Int(*i),
            EvalValue::Float(f) => Value::Float(*f),
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
            EvalValue::Node { node, .. } => match &**node {
                Node::Element(el) => Value::Element(el.clone()),
                Node::Section(_) => return None,
            },
            EvalValue::Fragment(blocks) => Value::Blocks(blocks.clone()),
            EvalValue::Document(_)
            | EvalValue::Directory(_)
            | EvalValue::DateTime(_)
            | EvalValue::Function(_)
            | EvalValue::Namespace(_) => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> DocRef {
        DocRef("a.tmt".to_string())
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
    fn a_list_literal_is_a_seq_and_other_calls_are_kept() {
        let list = Value::Call("list".into(), vec![Value::Int(1), Value::Int(2)]);
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
}
