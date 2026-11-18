//! What a `::vocabulary(ns)` document declares, and how a name resolves
//! against the namespaces a document has in scope.
//!
//! Extraction is pure: it takes a parsed [`File`] and reads it. The
//! file that document came from is somebody else's problem --
//! `tomet-load` finds `.tomet/vocabularies/<ns>.vocabulary.tmt`
//! and reads it, because this layer may not do I/O.
//!
//! See `docs/spec/vocabulary.tmt` for the normative description. These
//! rules are enforced here:
//!
//! - Exactly two namespaces may be written bare -- `std`, because the
//!   language carries it, and the document's own `::kind`, because it is
//!   declared on the first line. Anything brought in with `::use` is
//!   always written out, in the namespace the document brings it in under:
//!   its own (`::use(node-graph)`, `@node-graph.node`), or the alias `as:`
//!   gives it (`::use(node-graph, as: ng)`, `@ng.node`). An alias replaces
//!   the namespace in that document. What a name resolves to is always in
//!   the vocabulary's own namespace, never in the alias, so `@ng.node` here
//!   and `@node-graph.node` elsewhere are one element. The rules for a
//!   `::use` are [`use_problems`].
//! - A vocabulary declares its names in three tables: `::element` (the
//!   `@name`s), `::attribute` (the `::name`s) and `::constructor` (the
//!   `ns.name(...)` values). The sigil says which table a name is looked up
//!   in, so the same name may be in more than one.
//! - `std` wins every collision, and cannot be shadowed: a vocabulary
//!   that declares an element `std` has as a body element, or an attribute
//!   `std` has as a standard declaration, is rejected at the declaration
//!   rather than at each use site, which is one error instead of many.
//! - `doc` (`RESERVED_NAMESPACES`) is a second namespace tomet carries
//!   itself, resolved the same unconditional way `std` is in
//!   [`Bindings::classify`]/[`Bindings::classify_attribute`] -- but, unlike
//!   `std`, never written bare: `@doc.icon(...)` needs its full name, just
//!   never an `::use(doc)` in front of it.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use tomet_ast::{File, Name, Value};
use tomet_tree::ValueExt;
use tomet_type::{Type, TypeError};

use tomet_semantics_core::{TupleView, admits, is_placeholder, parse_type_spec};

use tomet_declaration::{DeclKind, Scope};

use crate::kind::{ElementKind, Shape, UnknownName, body_kind, classify_declaration_lenient};
use crate::positional::declaration_args_view;

/// One parameter, as an entry of an element's `args:` map declares it.
///
/// `param` and not `arg`: parameter is the declaration side, argument is
/// the call side, so `(args)` needs no rename.
///
/// `type:` is a [`Type`] (`tomet-type`), and an argument written for the
/// parameter is held to it (`tomet check`). A `type:` that names no type is
/// kept as the reason, not dropped: the parameter is then not checked, and
/// [`Vocabulary::param_problems`] says so once, where the vocabulary is read.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParamDecl {
    pub name: String,
    /// The declared type: `None` when `type:` is not written, `Some(Err(_))`
    /// when it is written and is not a type (an unknown name, or a call the
    /// type does not take, such as `list(string, string)`).
    pub ty: Option<Result<Type, TypeError>>,
    pub required: bool,
    /// Whether this parameter may be filled by position rather than by
    /// name. The *order* comes from where the entry sits in `args:`, which is why
    /// there is no separate `positional: [ ... ]` list -- that would state
    /// an order the declarations already carry.
    pub positional: bool,
    pub default: Option<Value>,
}

/// One element, as a vocabulary declares it with `::element`: an `@name` of the document body.
///
/// `args:` and `content:` are read; `@data` is not yet. `about:`, the
/// element's prose, is not read either. Its fields are [`ELEMENT_KEYS`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ElementDecl {
    /// The shape this element may take, or `None` for either.
    pub display: Option<Shape>,
    /// `singleton: true`: at most one in a document.
    pub singleton: bool,
    /// `(args)`, in declaration order.
    pub params: Vec<ParamDecl>,
    /// `content: ( allow: )`, or `None` when the element declared no
    /// `content:` at all -- distinct from `Some(ContentAllow::Any)`
    /// (`content: ( allow: any )`, written out), the same "absent says
    /// nothing, not none" reasoning `@data`'s `open:` already has.
    pub content_allow: Option<ContentAllow>,
}

/// One attribute, as a vocabulary declares it with `::attribute`: a `::name` a document writes
/// about itself in the vocabulary's terms (`::upstream(commonfiles)`).
///
/// An attribute and an element are different things, kept in different tables: the sigil a
/// document writes says which table a name is looked up in, so a vocabulary may have an
/// attribute and an element of the same name. `about:`, the attribute's prose, is not read. Its
/// fields are [`ATTRIBUTE_KEYS`].
#[derive(Debug, Clone, PartialEq)]
pub struct AttributeDecl {
    /// `singleton: true`: at most one in a document.
    pub singleton: bool,
    /// `host:`: which files may write it -- `tmt`, `ctmt`, or `both`, the default.
    pub host: Scope,
    /// `(args)`, in declaration order.
    pub params: Vec<ParamDecl>,
}

impl Default for AttributeDecl {
    fn default() -> Self {
        AttributeDecl {
            singleton: false,
            host: Scope::Both,
            params: Vec::new(),
        }
    }
}

/// One constructor, as a vocabulary declares it with `::constructor`.
///
/// A constructor builds a data value (`ns.name(...)`) from a tuple of arguments. Its fields are
/// [`CONSTRUCTOR_KEYS`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConstructorDecl {
    /// `(args)`, in declaration order.
    pub params: Vec<ParamDecl>,
}

/// The keys an `::element` entry may have.
pub const ELEMENT_KEYS: [&str; 5] = ["about", "display", "singleton", "args", "content"];

/// The keys an `::attribute` entry may have.
pub const ATTRIBUTE_KEYS: [&str; 4] = ["about", "args", "singleton", "host"];

/// The keys a `::constructor` entry may have.
pub const CONSTRUCTOR_KEYS: [&str; 2] = ["about", "args"];

/// The keys an entry of `args:` may have.
pub const PARAM_KEYS: [&str; 5] = ["type", "positional", "required", "default", "about"];

/// The values `host:` of an `::attribute` entry may take.
pub const HOST_VALUES: [&str; 3] = ["tmt", "ctmt", "both"];

/// The tables a vocabulary declares its names in: [`DeclKind::Element`], [`DeclKind::Attribute`]
/// and [`DeclKind::Constructor`].
pub const TABLES: [DeclKind; 3] = [
    DeclKind::Element,
    DeclKind::Attribute,
    DeclKind::Constructor,
];

/// Something wrong with how an entry of one of a vocabulary's [`TABLES`] is declared: a field the
/// table does not have, a value a field cannot take, or a parameter declared in a way that cannot
/// be held to (a `type:` that names no type, a `default:` that is not of its `type:`).
///
/// Not an invalid vocabulary: the entry is read as far as it can be. Reported once, where the
/// vocabulary is read and where its file is checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryProblem {
    /// The table the entry is in.
    pub table: DeclKind,
    /// The entry, or `entry.param` for one of its parameters.
    pub name: String,
    pub reason: String,
}

/// The keys an entry of `table` may have.
fn table_keys(table: DeclKind) -> &'static [&'static str] {
    match table {
        DeclKind::Element => &ELEMENT_KEYS,
        DeclKind::Attribute => &ATTRIBUTE_KEYS,
        _ => &CONSTRUCTOR_KEYS,
    }
}

/// A constructor call (`namespace.name(...)`) read against its vocabulary definition.
#[derive(Debug, Clone, PartialEq)]
pub struct Constructed {
    /// The constructor's name (`"upstream"` for `catasto.upstream(...)`).
    pub name: String,
    /// The arguments by parameter name, with positional values mapped to their names and
    /// `default`s applied.
    pub args: BTreeMap<String, Value>,
}

impl Constructed {
    /// The value of argument `name`, if supplied or defaulted.
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.args.get(name)
    }
}

/// Why a value could not be read as a constructor call of a vocabulary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallError {
    /// The value is not a namespaced `Value::Call` (`namespace.name(...)`).
    NotACall,
    /// The call's namespace is not the vocabulary's namespace.
    WrongNamespace {
        call: String,
        expected: String,
        found: String,
    },
    /// The call's namespace is not in scope in the document ([`Bindings::read_call`]).
    UnboundNamespace { call: String, namespace: String },
    /// The vocabulary declares no constructor of this name.
    UnknownConstructor {
        call: String,
        namespace: String,
        constructor: String,
        known: Vec<String>,
    },
    /// An argument was supplied both by position and by name.
    DuplicateArgument { call: String, argument: String },
    /// More positional values were supplied than the constructor has positional slots.
    TooManyPositionalArguments {
        call: String,
        slots: Vec<String>,
        given: usize,
    },
    /// A named argument is not declared by the constructor.
    UnknownArgument { call: String, argument: String },
    /// A required argument was not supplied.
    MissingRequiredArgument { call: String, argument: String },
    /// An argument's value does not fit the `type:` its parameter declares.
    InvalidArgument {
        call: String,
        argument: String,
        ty: String,
        reason: String,
    },
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallError::NotACall => write!(f, "not a namespaced constructor call"),
            CallError::WrongNamespace {
                call,
                expected,
                found,
            } => write!(
                f,
                "`{call}` is in the namespace `{found}`, not `{expected}`"
            ),
            CallError::UnboundNamespace { call, namespace } => write!(
                f,
                "`{call}`: the namespace `{namespace}` is not in scope -- write `::use({namespace})`"
            ),
            CallError::UnknownConstructor {
                call,
                namespace,
                constructor,
                known,
            } => {
                if known.is_empty() {
                    write!(
                        f,
                        "`{call}`: the namespace `{namespace}` declares no constructor `{constructor}`"
                    )
                } else {
                    let list: Vec<String> = known.iter().map(|k| format!("`{k}`")).collect();
                    write!(
                        f,
                        "`{call}`: the namespace `{namespace}` declares no constructor `{constructor}`; \
                         its constructors are {}",
                        list.join(", ")
                    )
                }
            }
            CallError::DuplicateArgument { call, argument } => write!(
                f,
                "`{call}`: the argument `{argument}` is written both by position and by name"
            ),
            CallError::TooManyPositionalArguments { call, slots, given } => {
                if slots.is_empty() {
                    write!(f, "`{call}` takes no positional arguments, found {given}")
                } else if slots.len() == 1 {
                    write!(
                        f,
                        "`{call}` takes at most 1 positional argument (`{}`), found {given}",
                        slots[0]
                    )
                } else {
                    let list: Vec<String> = slots.iter().map(|s| format!("`{s}`")).collect();
                    write!(
                        f,
                        "`{call}` takes at most {} positional arguments ({}), found {given}",
                        slots.len(),
                        list.join(", ")
                    )
                }
            }
            CallError::UnknownArgument { call, argument } => write!(
                f,
                "`{call}` has no argument `{argument}`; its vocabulary \
                 declares the ones it takes under `args:`"
            ),
            CallError::MissingRequiredArgument { call, argument } => {
                write!(f, "`{call}` requires the argument `{argument}`")
            }
            CallError::InvalidArgument {
                call,
                argument,
                ty,
                reason,
            } => write!(
                f,
                "`{call}`'s argument `{argument}` is declared `type: {ty}`, but {reason}"
            ),
        }
    }
}

impl std::error::Error for CallError {}

/// `content: ( allow: )`'s value -- what a vocabulary's own element may hold
/// in `[content]`/`|content` (`Vec<Block>`, see `tmtroot/docs/spec/
/// feature/content-shape.tmt`).
#[derive(Debug, Clone, PartialEq)]
pub enum ContentAllow {
    /// `allow: (a, b)` -- only these names, at any depth.
    Names(Vec<Name>),
    /// `allow: inline` -- at most one plain paragraph, no nested blocks.
    Inline,
    /// `allow: any` -- any blocks, any names.
    Any,
}

impl ParamDecl {
    /// The declared type, when `type:` names one.
    pub fn declared_type(&self) -> Option<Type> {
        self.ty.as_ref().and_then(|ty| ty.as_ref().ok().cloned())
    }
}

impl ElementDecl {
    /// The positional slots, in declaration order.
    pub fn positional_keys(&self) -> Vec<String> {
        positional_keys_of(&self.params)
    }

    pub fn param(&self, name: &str) -> Option<&ParamDecl> {
        param_of(&self.params, name)
    }
}

impl AttributeDecl {
    /// The positional slots, in declaration order.
    pub fn positional_keys(&self) -> Vec<String> {
        positional_keys_of(&self.params)
    }

    pub fn param(&self, name: &str) -> Option<&ParamDecl> {
        param_of(&self.params, name)
    }
}

impl ConstructorDecl {
    /// The positional slots, in declaration order.
    pub fn positional_keys(&self) -> Vec<String> {
        positional_keys_of(&self.params)
    }

    pub fn param(&self, name: &str) -> Option<&ParamDecl> {
        param_of(&self.params, name)
    }
}

fn positional_keys_of(params: &[ParamDecl]) -> Vec<String> {
    params
        .iter()
        .filter(|p| p.positional)
        .map(|p| p.name.clone())
        .collect()
}

fn param_of<'a>(params: &'a [ParamDecl], name: &str) -> Option<&'a ParamDecl> {
    params.iter().find(|p| p.name == name)
}

/// The elements, attributes and constructors one namespace declares, each in a table of its own.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Vocabulary {
    pub namespace: String,
    /// `::element`: the `@name`s.
    pub elements: BTreeMap<String, ElementDecl>,
    /// `::attribute`: the `::name`s.
    pub attributes: BTreeMap<String, AttributeDecl>,
    /// `::constructor`: the `ns.name(...)` values.
    pub constructors: BTreeMap<String, ConstructorDecl>,
    /// `::vocabulary(ns, open: true)` -- any name resolves in this
    /// namespace, declared or not, as an element and as an attribute.
    ///
    /// For a sketchpad. `docs/design/ideas/` writes elements that do not
    /// exist yet, which is what a sketch is for, so asking it to obey a
    /// vocabulary asks it to stop being a sketch. Making that a property
    /// of a *vocabulary* rather than a flag on the checker keeps the
    /// question where it belongs, and keeps the exemption visible: an
    /// open vocabulary has to be declared in the vault's config like any
    /// other, so which kinds are open is a thing you can read.
    ///
    /// `std` still wins. An open vocabulary claims every undeclared name,
    /// but bare names are resolved against the builtins first, so `@link`
    /// in a sketch is still `@std.link` and still has to be shaped like
    /// one.
    pub open: bool,
}

/// Namespaces no vault vocabulary may claim, because tomet already gives
/// them a hardcoded meaning -- `std` conceptually, and `doc`.
///
/// `std` is not actually checked here today (nothing ever compared a
/// vocabulary's own namespace string against `"std"`; only individual
/// element names were checked, via `shadowed_builtins`). `doc` is the first
/// namespace this list actually enforces.
pub const RESERVED_NAMESPACES: [&str; 1] = ["doc"];

/// Element names no vocabulary may declare, because tomet reserves them
/// for syntax and structural rules (e.g. `item` for plain list items in
/// `::rule(allow: (item, ...))`).
pub const RESERVED_ELEMENT_NAMES: [&str; 1] = ["item"];

/// Whether `name` is a reserved element name that no vocabulary may declare.
pub fn is_reserved_element_name(name: &str) -> bool {
    RESERVED_ELEMENT_NAMES.contains(&name)
}

/// The vocabularies tomet ships hardcoded in Rust rather than as a vault's
/// `::vocabulary(ns)` document -- the same reason `std`'s elements are a
/// Rust table (`builtin_kinds`) rather than a `::vocabulary(std)` document,
/// which is a limit of the vocabulary format, not a permanent design.
///
/// `doc.index` declares no elements: an index document's body is `std`
/// (`ul`/`ol`/`@link`) plus `$filter(...)`/`$by(...)`, which are neither
/// elements nor registered functions -- `tomet-transform::index_query`
/// pattern-matches them directly, with no dependency on `Bindings` at all.
/// This vocabulary exists purely so `::kind(doc.index)` resolves instead of
/// being rejected as unknown.
///
/// `doc` (bare namespace, not `doc.index`) is the opposite case: it
/// declares exactly one element, `icon`, and exists so `@doc.icon(...)`
/// resolves as running text. A recognized `icon` in `std` that never drew
/// anything would be worse than not having it, the same way
/// `::config(style:)` is worse than unimplemented, because a bare
/// `@icon` would look like the real spelling while doing nothing. `doc.icon`
/// sits under a namespace that says the opposite by construction:
/// Tomet resolves and draws nothing here, on purpose, forever -- an
/// external renderer implements this notation to make it into a picture.
/// `params` exists so `check_arguments` still catches a typo'd key; it is
/// not a step toward Tomet ever reading `name`/`pkg` itself.
pub fn builtin_doc_vocabularies() -> Vec<Vocabulary> {
    vec![
        Vocabulary {
            namespace: "doc.index".to_string(),
            elements: BTreeMap::new(),
            attributes: BTreeMap::new(),
            constructors: BTreeMap::new(),
            open: false,
        },
        Vocabulary {
            namespace: "doc".to_string(),
            elements: BTreeMap::from([(
                "icon".to_string(),
                ElementDecl {
                    display: Some(Shape::Inline),
                    singleton: false,
                    content_allow: None,
                    params: vec![
                        ParamDecl {
                            name: "name".to_string(),
                            ty: None,
                            required: true,
                            positional: true,
                            default: None,
                        },
                        ParamDecl {
                            name: "pkg".to_string(),
                            ty: None,
                            required: false,
                            positional: false,
                            default: None,
                        },
                    ],
                },
            )]),
            attributes: BTreeMap::new(),
            constructors: BTreeMap::new(),
            open: false,
        },
    ]
}

/// The `doc` vocabulary alone, cached -- `Bindings::classify`/`declaration`
/// need it unconditionally, the same way they call `builtin()` for `std`
/// unconditionally, so it can't wait for `Bindings::for_document`'s
/// `available` (a caller using `Bindings::default()`, which most of this
/// crate's own tests do, never populates that at all). `std` gets this for
/// free from the table being a `const`; `doc` needs a static instead
/// because it is real [`Vocabulary`] data, built once by
/// [`builtin_doc_vocabularies`] and reused rather than reallocated on every
/// lookup.
static DOC_VOCAB: LazyLock<Vocabulary> = LazyLock::new(|| {
    builtin_doc_vocabularies()
        .into_iter()
        .find(|v| v.namespace == "doc")
        .expect("doc is among the builtin vocabularies")
});

impl Vocabulary {
    /// Reads a parsed vocabulary document.
    ///
    /// Returns `None` when the document has no `::vocabulary(ns)` header:
    /// a vocabulary is one because it says so, never because it happens
    /// to contain `::element`. That is the same rule `::blueprint` needed
    /// after `extract_blueprint_schema` had counted any `::kind` at all.
    pub fn from_file<B>(doc: &File<B>) -> Option<Self> {
        let (namespace, open) = header(doc)?;
        let mut vocab = Vocabulary {
            namespace,
            elements: BTreeMap::new(),
            attributes: BTreeMap::new(),
            constructors: BTreeMap::new(),
            open,
        };

        for declaration in &doc.header {
            match classify_declaration_lenient(declaration) {
                ElementKind::Declaration(DeclKind::Element) => {
                    let Some(entries) = crate::data::declaration_entries(declaration) else {
                        continue;
                    };
                    if let Some(tuple) = entries.as_tuple() {
                        for (name, decl) in tuple.named() {
                            vocab
                                .elements
                                .insert(name.to_string(), decl_from_value(decl));
                        }
                    }
                }
                ElementKind::Declaration(DeclKind::Attribute) => {
                    let Some(entries) = crate::data::declaration_entries(declaration) else {
                        continue;
                    };
                    if let Some(tuple) = entries.as_tuple() {
                        for (name, decl) in tuple.named() {
                            vocab
                                .attributes
                                .insert(name.to_string(), attribute_from_value(decl));
                        }
                    }
                }
                ElementKind::Declaration(DeclKind::Constructor) => {
                    let Some(entries) = crate::data::declaration_entries(declaration) else {
                        continue;
                    };
                    if let Some(tuple) = entries.as_tuple() {
                        for (name, decl) in tuple.named() {
                            vocab
                                .constructors
                                .insert(name.to_string(), constructor_from_value(decl));
                        }
                    }
                }
                _ => {}
            }
        }

        Some(vocab)
    }

    /// Whether this vocabulary answers for the element `name` (`@name`) -- because it declares
    /// it, or because it is open.
    ///
    /// Reserved element names like `item` are never claimed by any vocabulary,
    /// even if `open: true`.
    pub fn has(&self, name: &str) -> bool {
        if is_reserved_element_name(name) {
            return false;
        }
        self.open || self.elements.contains_key(name)
    }

    /// Whether this vocabulary answers for the attribute `name` (`::name`) -- because it
    /// declares it, or because it is open.
    pub fn has_attribute(&self, name: &str) -> bool {
        self.open || self.attributes.contains_key(name)
    }

    /// Names this vocabulary declares that `std` already has in the same table, or that tomet
    /// reserves as structural names, with the table each is in.
    ///
    /// The rule is per table: an element may not take the name of a `std` body element (`@link`),
    /// and an attribute may not take the name of a standard declaration (`::kind`). Non-empty
    /// means the vocabulary is invalid. Reported here, once, rather than at every use site: `std`
    /// winning is what keeps a bare name unambiguous, so the shadowing itself is the error.
    pub fn shadowed_builtins(&self) -> Vec<(DeclKind, &str)> {
        let elements = self
            .elements
            .keys()
            .filter(|name| is_body_kind_name(name) || is_reserved_element_name(name))
            .map(|name| (DeclKind::Element, name.as_str()));
        let attributes = self
            .attributes
            .keys()
            .filter(|name| DeclKind::from_name(name).is_some())
            .map(|name| (DeclKind::Attribute, name.as_str()));
        elements.chain(attributes).collect()
    }

    /// Reads a constructor call (`Value::Call` with head `namespace.name`) against this
    /// vocabulary, returning the constructor's name and its arguments by parameter name
    /// (with positional arguments mapped through [`TupleView`] and `default`s applied).
    ///
    /// ```
    /// use tomet_ast::Value;
    /// use tomet_ast_parser::{parse_file, parse_value};
    /// use tomet_scalar::Scalar;
    /// use tomet_semantics::Vocabulary;
    ///
    /// let vocab_doc = parse_file(
    ///     r#"::kind(vocabulary)
    /// ::vocabulary(catasto, version: "1.0.0")
    /// ::constructor(
    ///   upstream: (
    ///     args: (
    ///       name: (type: string, positional: true, required: true)
    ///       path: (type: string, required: true)
    ///     )
    ///   )
    /// )
    /// "#,
    /// )
    /// .unwrap();
    /// let vocab = Vocabulary::from_file(&vocab_doc).unwrap();
    /// let call = parse_value(r#"catasto.upstream(commonfiles, path: ".editorconfig")"#).unwrap();
    ///
    /// let constructed = vocab.read_call(&call).unwrap();
    /// assert_eq!(constructed.name, "upstream");
    /// assert_eq!(
    ///     constructed.get("name"),
    ///     Some(&Value::Scalar(Scalar::Text("commonfiles".into())))
    /// );
    /// assert_eq!(
    ///     constructed.get("path"),
    ///     Some(&Value::Scalar(Scalar::Text(".editorconfig".into())))
    /// );
    /// ```
    pub fn read_call(&self, value: &Value) -> Result<Constructed, CallError> {
        read_call(value, self)
    }
}

/// Reads a constructor call (`Value::Call` with head `namespace.name`) against `vocabulary`,
/// returning the constructor's name and its arguments by parameter name (with positional arguments
/// mapped through [`TupleView`] and `default`s applied).
///
/// See [`Vocabulary::read_call`] for an example.
pub fn read_call(value: &Value, vocabulary: &Vocabulary) -> Result<Constructed, CallError> {
    let Value::Call(head, args) = value else {
        return Err(CallError::NotACall);
    };
    let Some((namespace, constructor)) = head.split_once('.') else {
        return Err(CallError::NotACall);
    };
    if namespace != vocabulary.namespace {
        return Err(CallError::WrongNamespace {
            call: head.clone(),
            expected: vocabulary.namespace.clone(),
            found: namespace.to_string(),
        });
    }
    construct(head, namespace, constructor, args, vocabulary)
}

/// The call `head` (written `namespace.constructor(args)`) read as the constructor `constructor`
/// of `vocabulary`, whatever namespace it was written in.
fn construct(
    head: &str,
    namespace: &str,
    constructor: &str,
    args: &tomet_ast::Tuple,
    vocabulary: &Vocabulary,
) -> Result<Constructed, CallError> {
    let Some(decl) = vocabulary.constructors.get(constructor) else {
        let known = vocabulary.constructors.keys().cloned().collect();
        return Err(CallError::UnknownConstructor {
            call: head.to_string(),
            namespace: namespace.to_string(),
            constructor: constructor.to_string(),
            known,
        });
    };
    let view = crate::positional::constructor_args_view(args, decl);
    if let Some(err) = check_call_params(head, &decl.params, Some(&view))
        .into_iter()
        .next()
    {
        return Err(err);
    }

    let mut by_name = BTreeMap::new();
    for param in &decl.params {
        if let Some(val) = view
            .get(&param.name)
            .cloned()
            .or_else(|| param.default.clone())
        {
            if let Some(ty) = param.declared_type()
                && !is_placeholder(&val)
                && let Err(mismatch) = admits(&ty, &val)
            {
                return Err(CallError::InvalidArgument {
                    call: head.to_string(),
                    argument: param.name.clone(),
                    ty: ty.to_string(),
                    reason: mismatch.to_string(),
                });
            }
            by_name.insert(param.name.clone(), val);
        }
    }

    Ok(Constructed {
        name: constructor.to_string(),
        args: by_name,
    })
}

/// Checks `view` against `params`, returning every argument problem found.
///
/// Shared between element/declaration argument validation and constructor call checking/reading.
pub fn check_call_params(
    call: &str,
    params: &[ParamDecl],
    view: Option<&TupleView<'_, '_>>,
) -> Vec<CallError> {
    let mut errors = Vec::new();
    let Some(view) = view else {
        for param in params.iter().filter(|p| p.required) {
            errors.push(CallError::MissingRequiredArgument {
                call: call.to_string(),
                argument: param.name.clone(),
            });
        }
        return errors;
    };

    let tuple = view.tuple();
    let unnamed_count = tuple.entries().filter(|e| e.name().is_none()).count();
    let slots = view.slots();
    let mut has_duplicate_positional = false;
    for i in 0..unnamed_count.min(slots.len()) {
        if let Some(slot) = slots.get(i)
            && tuple.entries().any(|e| e.name() == Some(slot))
        {
            has_duplicate_positional = true;
            errors.push(CallError::DuplicateArgument {
                call: call.to_string(),
                argument: slot.to_string(),
            });
        }
    }
    if unnamed_count > slots.len() {
        let slot_names = (0..slots.len())
            .filter_map(|i| slots.get(i).map(str::to_string))
            .collect();
        errors.push(CallError::TooManyPositionalArguments {
            call: call.to_string(),
            slots: slot_names,
            given: unnamed_count,
        });
    }

    for (key, _) in tuple.named() {
        if !key.is_empty() && !params.iter().any(|p| p.name == key) {
            errors.push(CallError::UnknownArgument {
                call: call.to_string(),
                argument: key.to_string(),
            });
        }
    }

    for param in params {
        let value = if has_duplicate_positional {
            let pos_idx = (0..slots.len()).find(|&i| slots.get(i) == Some(param.name.as_str()));
            tuple
                .entries()
                .find(|e| e.name() == Some(param.name.as_str()))
                .map(|e| e.value())
                .or_else(|| {
                    pos_idx
                        .filter(|&i| i < unnamed_count)
                        .and_then(|i| tuple.nth_unnamed(i))
                })
        } else {
            view.get(&param.name)
        };
        if let Some(value) = value {
            if let Some(ty) = param.declared_type()
                && !is_placeholder(value)
                && let Err(mismatch) = admits(&ty, value)
            {
                errors.push(CallError::InvalidArgument {
                    call: call.to_string(),
                    argument: param.name.clone(),
                    ty: ty.to_string(),
                    reason: mismatch.to_string(),
                });
            }
        } else if param.required {
            errors.push(CallError::MissingRequiredArgument {
                call: call.to_string(),
                argument: param.name.clone(),
            });
        }
    }

    errors
}

/// Every vocabulary a vault declares, with whatever went wrong.
///
/// The rules for accepting one live in [`LoadedVocabularies::add`], and
/// they are pure: a caller hands over a parsed document and the label it
/// wants errors reported under. Where the document came from -- a file
/// `tomet-load` read, or source text a wasm host was given -- is not
/// this type's business, so both get the same verdicts.
#[derive(Debug)]
pub struct LoadedVocabularies {
    /// By the namespace each one names for itself.
    pub by_namespace: BTreeMap<String, Vocabulary>,
    /// Declared vocabularies that are missing, unreadable, unparseable,
    /// headerless, claim a namespace twice, claim a reserved namespace,
    /// or shadow a builtin name.
    pub errors: Vec<String>,
    /// The label each accepted namespace was declared under, so a
    /// duplicate can name both sides.
    declared_as: BTreeMap<String, String>,
}

impl Default for LoadedVocabularies {
    /// Seeded with the hardcoded vocabularies, so [`add`](Self::add)
    /// reports a vault vocabulary colliding with one of them the same way
    /// it reports two vault vocabularies colliding with each other.
    fn default() -> Self {
        let mut loaded = LoadedVocabularies {
            by_namespace: BTreeMap::new(),
            errors: Vec::new(),
            declared_as: BTreeMap::new(),
        };
        for builtin in builtin_doc_vocabularies() {
            loaded
                .declared_as
                .insert(builtin.namespace.clone(), "<builtin>".to_string());
            loaded
                .by_namespace
                .insert(builtin.namespace.clone(), builtin);
        }
        loaded
    }
}

impl LoadedVocabularies {
    /// Accepts the vocabulary `doc` declares, or records why not.
    ///
    /// `label` is how the declaration is named in an error -- a path, for
    /// the vault that read it off disk.
    pub fn add<B>(&mut self, label: &str, doc: &File<B>) {
        let Some(vocab) = Vocabulary::from_file(doc) else {
            self.errors.push(format!(
                "declared vocabulary names no namespace -- it needs `::vocabulary(<ns>)`: {label}"
            ));
            return;
        };

        if let Some(reserved) = RESERVED_NAMESPACES
            .iter()
            .find(|ns| vocab.namespace == **ns || vocab.namespace.starts_with(&format!("{ns}.")))
        {
            self.errors.push(format!(
                "{label} declares itself `{}`, which is under the reserved `{reserved}` \
                 namespace tomet already gives a hardcoded meaning",
                vocab.namespace
            ));
            return;
        }

        // Reported here, once, rather than at every document that writes
        // the shadowed name.
        for (table, shadowed) in vocab.shadowed_builtins() {
            self.errors.push(if is_reserved_element_name(shadowed) {
                format!(
                    "{label} declares `{shadowed}`, which is a reserved element name; \
                     a vocabulary may not declare it"
                )
            } else {
                format!(
                    "{label} declares the {table} `{shadowed}`, which is already a built-in \
                     {table}; std wins, so a vocabulary may not take one"
                )
            });
        }
        for (_, problem) in entry_problems_in(doc) {
            self.errors.push(format!(
                "{label}: `{}` in `::{}`: {}",
                problem.name, problem.table, problem.reason
            ));
        }

        if let Some(first) = self.declared_as.get(&vocab.namespace) {
            self.errors.push(format!(
                "two vocabularies both call themselves `{}`: {first} and {label}",
                vocab.namespace
            ));
            return;
        }
        self.declared_as
            .insert(vocab.namespace.clone(), label.to_string());
        self.by_namespace.insert(vocab.namespace.clone(), vocab);
    }

    /// The namespaces `doc` has in scope out of what was accepted.
    pub fn bindings_for<B>(&self, doc: &File<B>) -> Bindings {
        Bindings::for_document(doc, self.by_namespace.values().cloned())
    }
}

/// What counts as a vocabulary having a name: whatever it answers for, an open vocabulary
/// answering for every name, or only what it declares.
#[derive(Debug, Clone, Copy)]
enum Claim {
    Open,
    Declared,
}

/// The namespaces in scope for one document.
#[derive(Debug, Clone, Default)]
pub struct Bindings {
    /// The vocabulary named by the document's `::kind(X)`. Its names may
    /// be written bare.
    pub kind: Option<Vocabulary>,
    /// Vocabularies brought in with `::use`, by the namespace this document writes each one
    /// in: its `as:` alias, or the namespace it calls itself. Never written bare.
    pub used: BTreeMap<String, Vocabulary>,
    /// The aliases the document gives with `::use(ns, as: alias)`, alias to the namespace it
    /// stands for. Read from the document alone, so it holds whether or not the vocabulary is at
    /// hand ([`classify_lenient`](Bindings::classify_lenient)).
    pub aliases: BTreeMap<String, String>,
}

impl Bindings {
    /// What `doc` has in scope, given every vocabulary available.
    ///
    /// Pure: the vocabularies arrive already parsed, so this is the half
    /// a caller that cannot open a file can still run. A caller that can
    /// reads them through `tomet-load`, which finds and parses each
    /// declared vocabulary and then calls
    /// [`LoadedVocabularies::bindings_for`] directly, which is this plus
    /// reading them off disk; a wasm host that has the sources instead
    /// can pass them straight in.
    ///
    /// A document's `::kind(X)` binds the vocabulary calling itself `X`.
    /// Everything else has to be asked for with `::use`, and is bound under the namespace the
    /// `::use` gives it ([`Use::local`]). When two `::use` give one namespace, the first holds it
    /// ([`use_problems`] reports the second).
    pub fn for_document<I, B>(doc: &File<B>, available: I) -> Self
    where
        I: IntoIterator<Item = Vocabulary>,
    {
        let by_namespace: BTreeMap<String, Vocabulary> = available
            .into_iter()
            .map(|v| (v.namespace.clone(), v))
            .collect();

        let kind = tomet_declaration::document_kind(doc)
            .and_then(|kind| by_namespace.get(&kind))
            .cloned();

        let mut used = BTreeMap::new();
        let mut aliases = BTreeMap::new();
        for found in uses(doc) {
            if let Some(alias) = &found.alias {
                aliases
                    .entry(alias.clone())
                    .or_insert_with(|| found.namespace.clone());
            }
            if let Some(vocab) = by_namespace.get(&found.namespace) {
                used.entry(found.local().to_string())
                    .or_insert_with(|| vocab.clone());
            }
        }

        Bindings {
            kind,
            used,
            aliases,
        }
    }

    /// [`classify`](Bindings::classify), falling back to `Custom` for a name it does not resolve:
    /// the counterpart of [`classify_std_lenient`](crate::classify_std_lenient) for a reader that
    /// renders whatever it is given, such as a converter.
    ///
    /// The fallback is named in the vocabulary's own namespace, as a resolved name is: under
    /// `::use(node-graph, as: ng)`, `@ng.node` is `Custom("node-graph.node")` even with no
    /// vocabulary at hand, because the alias is the document's ([`aliases`](Bindings::aliases)).
    pub fn classify_lenient(&self, name: &Name) -> ElementKind {
        self.classify(name)
            .unwrap_or_else(|_| self.unresolved(name))
    }

    /// [`classify_attribute`](Bindings::classify_attribute), falling back to `Custom` for a name
    /// it does not resolve, named in the vocabulary's own namespace: the counterpart of
    /// [`classify_lenient`](Bindings::classify_lenient) for a `::name`.
    pub fn classify_attribute_lenient(&self, name: &Name) -> ElementKind {
        self.classify_attribute(name)
            .unwrap_or_else(|_| self.unresolved(name))
    }

    /// What a name no table resolves stands for: itself, with an alias the document gives
    /// replaced by the namespace it stands for.
    fn unresolved(&self, name: &Name) -> ElementKind {
        ElementKind::Custom(match name.namespace.as_deref() {
            Some(written) => {
                let namespace = self.aliases.get(written).map_or(written, String::as_str);
                format!("{namespace}.{}", name.name)
            }
            None => name.name.clone(),
        })
    }

    /// Resolves an element name (`@name`) against `std`'s body elements and the `::element`
    /// tables in scope.
    ///
    /// A bare name goes to `std` first and to the document's kind second.
    /// The order cannot matter, because a vocabulary shadowing a builtin
    /// is rejected when it is read -- but it is written this way round so
    /// that the code says which one wins.
    ///
    /// A name that is no element but is an attribute in scope is not unknown: it is written with
    /// the wrong sigil, and the [`UnknownName`] says so ([`UnknownName::wrong_sigil`]). Only a
    /// name the other table has decides that: an open vocabulary claims every name in both
    /// tables, which says nothing about which one a name belongs to.
    pub fn classify(&self, name: &Name) -> Result<ElementKind, UnknownName> {
        self.resolve_element(name, Claim::Open)
            .map_err(|mut unknown| {
                unknown.wrong_sigil = self.resolve_attribute(name, Claim::Declared).is_ok();
                unknown
            })
    }

    /// Resolves an attribute name (`::name`) against `std`'s declarations and the `::attribute`
    /// tables in scope: the counterpart of [`classify`](Bindings::classify).
    ///
    /// Bare names follow the same rule as elements: only `std` and the document's own `::kind`
    /// may be written bare. A name that is no attribute but is an element in scope is written
    /// with the wrong sigil, and the [`UnknownName`] says so.
    pub fn classify_attribute(&self, name: &Name) -> Result<ElementKind, UnknownName> {
        self.resolve_attribute(name, Claim::Open)
            .map_err(|mut unknown| {
                unknown.wrong_sigil = self.resolve_element(name, Claim::Declared).is_ok();
                unknown
            })
    }

    fn resolve_attribute(&self, name: &Name, claim: Claim) -> Result<ElementKind, UnknownName> {
        let has = |vocab: &Vocabulary| match claim {
            Claim::Open => vocab.has_attribute(&name.name),
            Claim::Declared => vocab.attributes.contains_key(&name.name),
        };
        let unknown = || UnknownName::attribute(name);
        let unbound = |namespace: &str| UnknownName {
            unbound_namespace: Some(namespace.to_string()),
            ..UnknownName::attribute(name)
        };

        let Some(namespace) = name.namespace.as_deref() else {
            if let Some(kind) = DeclKind::from_name(&name.name) {
                return Ok(ElementKind::Declaration(kind));
            }
            return match self.kind.as_ref() {
                Some(vocab) if has(vocab) => Ok(ElementKind::Custom(format!(
                    "{}.{}",
                    vocab.namespace, name.name
                ))),
                _ => Err(unknown()),
            };
        };

        if namespace == "std" {
            return DeclKind::from_name(&name.name)
                .map(ElementKind::Declaration)
                .ok_or_else(unknown);
        }
        if namespace == "doc" {
            return if has(&DOC_VOCAB) {
                Ok(ElementKind::Custom(name.to_string()))
            } else {
                Err(unknown())
            };
        }
        match self.vocabulary_in_scope(namespace) {
            Some(vocab) if has(vocab) => Ok(ElementKind::Custom(format!(
                "{}.{}",
                vocab.namespace, name.name
            ))),
            Some(_) => Err(unknown()),
            None => Err(unbound(namespace)),
        }
    }

    fn resolve_element(&self, name: &Name, claim: Claim) -> Result<ElementKind, UnknownName> {
        let has = |vocab: &Vocabulary| match claim {
            Claim::Open => vocab.has(&name.name),
            Claim::Declared => {
                !is_reserved_element_name(&name.name) && vocab.elements.contains_key(&name.name)
            }
        };
        let unknown = || UnknownName::element(name);
        let unbound = |namespace: &str| UnknownName {
            unbound_namespace: Some(namespace.to_string()),
            ..UnknownName::element(name)
        };

        let Some(namespace) = name.namespace.as_deref() else {
            if let Some(kind) = body_kind(&name.name) {
                return Ok(kind);
            }
            if is_reserved_element_name(&name.name) {
                return Err(unknown());
            }
            return match self.kind.as_ref() {
                Some(vocab) if has(vocab) => Ok(ElementKind::Custom(format!(
                    "{}.{}",
                    vocab.namespace, name.name
                ))),
                _ => Err(unknown()),
            };
        };

        if namespace == "std" {
            return body_kind(&name.name).ok_or_else(unknown);
        }

        // `doc` is tomet's own second reserved namespace, resolved the
        // same unconditional way as `std` just above -- no `::use`, no
        // `::vocabulary(doc)` document, works even with `Bindings::default()`.
        // Still a real, closed vocabulary: `doc.glyph` (undeclared) is
        // `unknown()`, not silently accepted.
        if namespace == "doc" {
            return if has(&DOC_VOCAB) {
                Ok(ElementKind::Custom(name.to_string()))
            } else {
                Err(unknown())
            };
        }

        match self.vocabulary_in_scope(namespace) {
            Some(vocab) if has(vocab) => Ok(ElementKind::Custom(format!(
                "{}.{}",
                vocab.namespace, name.name
            ))),
            Some(_) => Err(unknown()),
            None => Err(unbound(namespace)),
        }
    }

    /// The vocabulary a name written in `namespace` (or bare, for `None`) is looked up in:
    /// the document's own kind for a bare name, `doc` always, and otherwise what is in scope.
    fn vocabulary_for(&self, namespace: Option<&str>) -> Option<&Vocabulary> {
        match namespace {
            None | Some("std") => self.kind.as_ref(),
            // Unconditional, like `classify`'s `"doc"` branch -- `doc.icon`
            // has a real declaration (`name`/`pkg`) regardless of what, if
            // anything, `self.kind`/`self.used` hold.
            Some("doc") => Some(&DOC_VOCAB),
            Some(namespace) => self.vocabulary_in_scope(namespace),
        }
    }

    /// The `::element` entry behind an element name (`@name`), when one is in scope.
    pub fn declaration(&self, name: &Name) -> Option<&ElementDecl> {
        if is_reserved_element_name(&name.name) {
            return None;
        }
        self.vocabulary_for(name.namespace.as_deref())?
            .elements
            .get(&name.name)
    }

    /// The `::attribute` entry behind an attribute name (`::name`), when one is in scope.
    pub fn attribute(&self, name: &Name) -> Option<&AttributeDecl> {
        self.vocabulary_for(name.namespace.as_deref())?
            .attributes
            .get(&name.name)
    }

    /// Reads a constructor call (`ns.name(...)`) against the vocabulary in scope for the namespace
    /// it is written in: under `::use(catasto, as: c)`, `c.git(...)` is `catasto.git(...)`. The
    /// counterpart of [`read_call`] for a call written in a document.
    pub fn read_call(&self, value: &Value) -> Result<Constructed, CallError> {
        let Value::Call(head, args) = value else {
            return Err(CallError::NotACall);
        };
        let Some((namespace, constructor)) = head.split_once('.') else {
            return Err(CallError::NotACall);
        };
        let Some(vocabulary) = self.vocabulary_in_scope(namespace) else {
            return Err(CallError::UnboundNamespace {
                call: head.clone(),
                namespace: namespace.to_string(),
            });
        };
        construct(head, namespace, constructor, args, vocabulary)
    }

    /// The vocabulary in scope for `namespace` as this document writes it: the namespace of its
    /// own `::kind`, or the one a `::use` brings a vocabulary in under (an alias, when it gives
    /// one). A vocabulary brought in under an alias is not in scope under its own namespace.
    pub fn vocabulary_in_scope(&self, namespace: &str) -> Option<&Vocabulary> {
        match self.kind.as_ref() {
            Some(kind) if kind.namespace == namespace => Some(kind),
            _ => self.used.get(namespace),
        }
    }
}

/// The arguments `::use` takes: the namespace (`target`, its first slot), the `version` wanted
/// (its second slot), and `as`, the alias it is written under.
pub const USE_ARGS: [&str; 3] = ["target", "version", "as"];

/// What one `::use(ns, version, as: alias)` brings in: the vocabulary calling itself `ns`, and the
/// alias this document writes its names under, if it gives one.
///
/// The argument is the namespace, not a path. The vault already says
/// where each vocabulary lives, so naming the file here would write the
/// path twice -- and a vocabulary names itself, so the path was never
/// the thing being identified anyway.
///
/// It also makes the two ways into scope use one currency: `::kind(writ)`
/// binds by name, and so does `::use(deck)`. A path form would force
/// a step between them (path -> file -> namespace) that nothing could
/// take without carrying paths through this layer, which would mean
/// trimming a filename stem and hoping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Use {
    /// The namespace the vocabulary calls itself.
    pub namespace: String,
    /// `as:`, when it is written and is a bare name ([`is_alias`]).
    pub alias: Option<String>,
}

impl Use {
    /// The namespace this document writes the vocabulary's names in: `ng` for
    /// `::use(node-graph, as: ng)`, `node-graph` for `::use(node-graph)`.
    pub fn local(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.namespace)
    }
}

/// Whether `text` may be an alias: one bare name part, an ASCII letter or `_` then ASCII letters,
/// digits, `_` and `-`, as a namespace is written. A dotted name is not one: the dot separates the
/// namespace from the name.
pub fn is_alias(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
}

/// What a declaration brings in when it is a `::use` naming a namespace. An `as:` that is no
/// bare name is no alias: the vocabulary is brought in under its own namespace, and
/// [`use_problems`] reports the `as:`.
fn use_of(declaration: &tomet_ast::Declaration) -> Option<Use> {
    if classify_declaration_lenient(declaration) != ElementKind::Declaration(DeclKind::Use) {
        return None;
    }
    let args = declaration_args_view(declaration)?;
    let namespace = args.get("target").and_then(|v| v.as_str())?.to_string();
    let alias = args
        .get("as")
        .and_then(|v| v.as_str())
        .filter(|text| is_alias(text))
        .map(str::to_string);
    Some(Use { namespace, alias })
}

/// Every `::use` a document writes, in order.
pub fn uses<B>(doc: &File<B>) -> Vec<Use> {
    doc.header.iter().filter_map(use_of).collect()
}

/// Something wrong with a `::use`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UseProblem {
    /// An argument `::use` does not take ([`USE_ARGS`]): `::use(catasto, from: x)`.
    UnknownArgument(String),
    /// An `as:` that is not a bare name: `as: "a b"`, `as: ng.x`, `as: 3`.
    NotAnAlias,
    /// An alias that is a namespace tomet carries itself, `std` or `doc`.
    ReservedAlias(String),
    /// An alias that is the namespace of the document's own `::kind`.
    KindAlias(String),
    /// An alias that is already the namespace another `::use` brings a vocabulary in under.
    AliasTaken { alias: String, by: String },
    /// The vocabulary is brought in by an earlier `::use` already; `first` is the index of that
    /// one in the header.
    UsedTwice { namespace: String, first: usize },
    /// The vocabulary is the document's own `::kind`, which brings it in already.
    UsedKind(String),
}

impl std::fmt::Display for UseProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UseProblem::UnknownArgument(argument) => write!(
                f,
                "`::use` has no argument `{argument}`; it takes the namespace, the version and \
                 `as:`, as in `::use(node-graph, \"1.0.0\", as: ng)`"
            ),
            UseProblem::NotAnAlias => write!(
                f,
                "`as:` of `::use` takes a bare name, the namespace to write the vocabulary in: \
                 `::use(node-graph, as: ng)`"
            ),
            UseProblem::ReservedAlias(alias) => write!(
                f,
                "`as: {alias}`: `{alias}` is a namespace tomet carries itself, and names no \
                 other vocabulary"
            ),
            UseProblem::KindAlias(alias) => write!(
                f,
                "`as: {alias}`: `{alias}` is this document's own `::kind`, and names no other \
                 vocabulary"
            ),
            UseProblem::AliasTaken { alias, by } => write!(
                f,
                "`as: {alias}`: `{alias}` already names the vocabulary `{by}` in this document"
            ),
            UseProblem::UsedTwice { namespace, .. } => write!(
                f,
                "`{namespace}` is brought in by an earlier `::use` already"
            ),
            UseProblem::UsedKind(namespace) => write!(
                f,
                "`{namespace}` is this document's own `::kind`, which brings it in already"
            ),
        }
    }
}

/// The problems of the `::use` declarations in `header`, each with the index of the declaration
/// it is about.
///
/// A `::use` takes [`USE_ARGS`] and nothing else. Its alias is a bare name ([`is_alias`]), and a
/// namespace the document does not write anything else in: not `std` or `doc`, which tomet
/// carries itself, not the document's own `::kind`, and not one another `::use` brings a
/// vocabulary in under. A vocabulary is brought in once: by one `::use`, or by the `::kind`. Too many positional arguments, and a
/// `version` that is not a version, are reported with the other standard declarations.
pub fn use_problems(header: &[tomet_ast::Declaration]) -> Vec<(usize, UseProblem)> {
    let kind = tomet_declaration::declarations_from_header(header)
        .into_iter()
        .find(|found| found.kind == DeclKind::Kind)
        .and_then(|found| tomet_declaration::kind_name(&found.declaration));
    let written: Vec<(usize, Use)> = header
        .iter()
        .enumerate()
        .filter_map(|(idx, declaration)| use_of(declaration).map(|found| (idx, found)))
        .collect();

    let mut problems = Vec::new();
    for (idx, declaration) in header.iter().enumerate() {
        if classify_declaration_lenient(declaration) != ElementKind::Declaration(DeclKind::Use) {
            continue;
        }
        let Some(args) = declaration.data.args.as_ref() else {
            continue;
        };
        for (name, value) in args.named() {
            if !USE_ARGS.contains(&name) {
                problems.push((idx, UseProblem::UnknownArgument(name.to_string())));
            } else if name == "as" && !value.as_str().is_some_and(is_alias) {
                problems.push((idx, UseProblem::NotAnAlias));
            }
        }
    }
    for (at, (idx, found)) in written.iter().enumerate() {
        if let Some((first, _)) = written[..at]
            .iter()
            .find(|(_, earlier)| earlier.namespace == found.namespace)
        {
            problems.push((
                *idx,
                UseProblem::UsedTwice {
                    namespace: found.namespace.clone(),
                    first: *first,
                },
            ));
            continue;
        }
        if kind.as_deref() == Some(found.namespace.as_str()) {
            problems.push((*idx, UseProblem::UsedKind(found.namespace.clone())));
            continue;
        }
        let Some(alias) = &found.alias else {
            continue;
        };
        let taken_by = written
            .iter()
            .enumerate()
            .find_map(|(other_at, (_, other))| {
                let clash = other_at != at
                    && other.namespace != found.namespace
                    && other.local() == alias
                    // Two aliases that clash are reported once, at the later one.
                    && (other.alias.is_none() || other_at < at);
                clash.then(|| other.namespace.clone())
            });
        let problem = if alias == "std" || RESERVED_NAMESPACES.contains(&alias.as_str()) {
            UseProblem::ReservedAlias(alias.clone())
        } else if kind.as_deref() == Some(alias.as_str()) {
            UseProblem::KindAlias(alias.clone())
        } else if let Some(by) = taken_by {
            UseProblem::AliasTaken {
                alias: alias.clone(),
                by,
            }
        } else {
            continue;
        };
        problems.push((*idx, problem));
    }
    problems.sort_by_key(|(idx, _)| *idx);
    problems
}

fn is_body_kind_name(name: &str) -> bool {
    body_kind(name).is_some()
}

/// The namespace a vocabulary's own header names, and whether it is open: `::vocabulary(ns, version: "1.0.0")`
/// names `ns`, and `open: true` marks it open.
///
/// `::vocabulary(ns: @file(...))` is a registration of the file in a vault's config,
/// which is not a vocabulary's header: it has no positional namespace argument.
fn header<B>(doc: &File<B>) -> Option<(String, bool)> {
    for declaration in &doc.header {
        if classify_declaration_lenient(declaration)
            != ElementKind::Declaration(DeclKind::Vocabulary)
        {
            continue;
        }
        let Some(args) = declaration_args_view(declaration) else {
            continue;
        };
        let Some(namespace) = args.get("namespace").and_then(|v| v.as_str()) else {
            continue;
        };
        let open = args.get("open").and_then(|v| v.as_bool()).unwrap_or(false);
        return Some((namespace.to_string(), open));
    }
    None
}

/// One entry of `::element()`: the map of fields an element declares.
///
/// A value that is not a map declares nothing, the same as an empty one.
/// `about` is the element's prose and is not read: nothing in the checker
/// needs it.
fn decl_from_value(decl: &Value) -> ElementDecl {
    ElementDecl {
        display: match decl.get("display").and_then(|v| v.as_str()) {
            Some("block") => Some(Shape::Block),
            Some("inline") => Some(Shape::Inline),
            _ => None,
        },
        singleton: decl
            .get("singleton")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        params: decl.get("args").map(params_from_value).unwrap_or_default(),
        content_allow: decl.get("content").and_then(content_allow_from_value),
    }
}

/// One entry of `::attribute()`: the map of fields an attribute declares.
///
/// A `host:` that is none of [`HOST_VALUES`] is read as the default, `both`;
/// [`entry_problems_in`] reports it.
fn attribute_from_value(decl: &Value) -> AttributeDecl {
    AttributeDecl {
        singleton: decl
            .get("singleton")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        host: match decl.get("host").and_then(|v| v.as_str()) {
            Some("tmt") => Scope::Tmt,
            Some("ctmt") => Scope::Ctmt,
            _ => Scope::Both,
        },
        params: decl.get("args").map(params_from_value).unwrap_or_default(),
    }
}

/// One entry of `::constructor()`: the map of fields a constructor declares.
fn constructor_from_value(decl: &Value) -> ConstructorDecl {
    ConstructorDecl {
        params: decl.get("args").map(params_from_value).unwrap_or_default(),
    }
}

/// Validates every entry of the vocabulary [`TABLES`] (`::element`, `::attribute`,
/// `::constructor`) in `doc`, returning each problem paired with the span of the declaration
/// where it was found.
///
/// An entry is a map of the fields its table has ([`ELEMENT_KEYS`], [`ATTRIBUTE_KEYS`],
/// [`CONSTRUCTOR_KEYS`]); any other field is reported, and so is an entry declared twice in one
/// table. `args:` is a map of parameters of [`PARAM_KEYS`], each with a `type:` that names a
/// type and a `default:` of that type; `singleton:` is a bool and `host:` one of
/// [`HOST_VALUES`].
pub fn entry_problems_in<B>(doc: &File<B>) -> Vec<(tomet_span::Span, EntryProblem)> {
    let mut problems = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for declaration in &doc.header {
        let ElementKind::Declaration(table) = classify_declaration_lenient(declaration) else {
            continue;
        };
        if !TABLES.contains(&table) {
            continue;
        }
        let span = declaration.span;
        let Some(entries) = crate::data::declaration_entries(declaration) else {
            continue;
        };
        let Some(tuple) = entries.as_tuple() else {
            continue;
        };
        if !tuple.has_no_unnamed() {
            continue;
        }
        let mut report = |name: String, reason: String| {
            problems.push((
                span,
                EntryProblem {
                    table,
                    name,
                    reason,
                },
            ));
        };
        for (entry, decl) in tuple.named() {
            if !seen.insert((table, entry.to_string())) {
                report(entry.to_string(), format!("duplicate {table} `{entry}`"));
            }
            let Some(fields) = decl.as_tuple().filter(|t| t.has_no_unnamed()) else {
                report(
                    entry.to_string(),
                    format!(
                        "{} is declared as a map: `(about: ..., args: ...)`",
                        article(table)
                    ),
                );
                continue;
            };
            for (field, value) in fields.named() {
                if !table_keys(table).contains(&field) {
                    report(
                        entry.to_string(),
                        format!(
                            "unknown field `{field}`; {} has {}",
                            article(table),
                            table_keys(table)
                                .iter()
                                .map(|k| format!("`{k}`"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    );
                    continue;
                }
                match field {
                    "args" => param_problems(entry, value, &mut report),
                    "singleton" => {
                        if let Err(mismatch) = admits(&Type::bare(tomet_type::Kind::Bool), value) {
                            report(entry.to_string(), format!("`singleton`: {mismatch}"));
                        }
                    }
                    "host" if !value.as_str().is_some_and(|v| HOST_VALUES.contains(&v)) => {
                        report(
                            entry.to_string(),
                            format!(
                                "`host` is one of {}",
                                HOST_VALUES.map(|k| format!("`{k}`")).join(", ")
                            ),
                        );
                    }
                    _ => {}
                }
            }
        }
    }

    problems
}

/// `an element`, `an attribute`, `a constructor`: how a problem names an entry of `table`.
fn article(table: DeclKind) -> String {
    match table {
        DeclKind::Constructor => format!("a {table}"),
        _ => format!("an {table}"),
    }
}

/// The problems in an entry's `args:` map, reported as `entry.param`.
fn param_problems(entry: &str, args: &Value, report: &mut impl FnMut(String, String)) {
    let Some(args) = args.as_tuple().filter(|t| t.has_no_unnamed()) else {
        report(
            format!("{entry}.args"),
            "`args` is declared as a map: `(name: (type: ..., ...))`".to_string(),
        );
        return;
    };
    for (param, spec) in args.named() {
        let target = format!("{entry}.{param}");
        let Some(fields) = spec.as_tuple().filter(|t| t.has_no_unnamed()) else {
            report(
                target,
                "an argument is declared as a map: `(type: ..., positional: ..., required: ..., \
                 default: ..., about: ...)`"
                    .to_string(),
            );
            continue;
        };
        let mut parsed_ty: Option<Result<Type, TypeError>> = None;
        for (key, value) in fields.named() {
            match key {
                "type" => {
                    let res = parse_type_spec(value);
                    if let Err(err) = &res {
                        report(target.clone(), err.to_string());
                    }
                    parsed_ty = Some(res);
                }
                "positional" | "required" => {
                    if let Err(mismatch) = admits(&Type::bare(tomet_type::Kind::Bool), value) {
                        report(target.clone(), format!("`{key}`: {mismatch}"));
                    }
                }
                "default" | "about" => {}
                other => {
                    report(
                        target.clone(),
                        format!(
                            "unknown field `{other}`; an argument has {}",
                            PARAM_KEYS.map(|k| format!("`{k}`")).join(", ")
                        ),
                    );
                }
            }
        }
        if let (Some(Ok(ty)), Some(default)) = (parsed_ty.as_ref(), fields.get("default"))
            && let Err(mismatch) = admits(ty, default)
        {
            report(
                target,
                format!("`default` is not of `type: {ty}`: {mismatch}"),
            );
        }
    }
}

/// The `content: ( allow: )` an element declares, if any.
fn content_allow_from_value(content: &Value) -> Option<ContentAllow> {
    let allow = content.get("allow")?;
    if let Some(s) = allow.as_str() {
        match s {
            "inline" => Some(ContentAllow::Inline),
            "any" => Some(ContentAllow::Any),
            _ => None,
        }
    } else if let Value::Tuple(tuple) = allow {
        if tuple.has_no_named() {
            let mut names = Vec::with_capacity(tuple.len());
            for item in tuple.unnamed() {
                names.push(name_from_dotted(item.as_str()?));
            }
            Some(ContentAllow::Names(names))
        } else {
            None
        }
    } else {
        None
    }
}

/// `"ns.mycard"` -> a namespaced [`Name`]; `"card"` -> a bare one. Same
/// split `tomet-semantics-validator::rule`'s own copy uses for
/// `::rule(allow:(...))` -- duplicated rather than shared, since that
/// crate depends on this one, not the other way around.
fn name_from_dotted(s: &str) -> Name {
    match s.split_once('.') {
        Some((namespace, name)) => Name::namespaced(namespace, name),
        None => Name::bare(s),
    }
}

/// The parameters in an element's `args:` map, in the order written.
///
/// Each key is a parameter's name and its value the map of fields that
/// describes it. An element with no `args:` declares no parameters, which
/// is not the same as declaring that it takes none -- that distinction
/// belongs to `@data`'s `open:` and is not built yet.
fn params_from_value(args: &Value) -> Vec<ParamDecl> {
    let Some(tuple) = args.as_tuple() else {
        return Vec::new();
    };
    tuple
        .named()
        .map(|(name, spec)| ParamDecl {
            name: name.to_string(),
            ty: spec.get("type").map(parse_type_spec),
            required: spec
                .get("required")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            positional: spec
                .get("positional")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            default: spec.get("default").cloned(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_type::Kind;

    fn vocab(src: &str) -> Vocabulary {
        let doc = tomet_ast_parser::parse_file(src).expect("vocabulary parses");
        Vocabulary::from_file(&doc).expect("has a ::vocabulary header")
    }

    fn name(s: &str) -> Name {
        match s.split_once('.') {
            Some((ns, n)) => Name::namespaced(ns, n),
            None => Name::bare(s),
        }
    }

    const WRIT: &str = r#"::kind(vocabulary)
::vocabulary(writ, version: "1.0.0")

::element(
  layers: (
    display: block
    singleton: true
    about: | Layers.
  )
  pure: (
    display: block
    about: | Purity.
  )
)
"#;

    #[test]
    fn reads_the_namespace_and_its_elements() {
        let v = vocab(WRIT);
        assert_eq!(v.namespace, "writ");
        assert_eq!(v.elements.keys().collect::<Vec<_>>(), ["layers", "pure"]);

        let layers = &v.elements["layers"];
        assert_eq!(layers.display, Some(Shape::Block));
        assert!(layers.singleton);
        assert!(!v.elements["pure"].singleton);
    }

    /// `args:`' entries are read, in the order they are written -- which is
    /// what makes a separate `positional: [ ... ]` list a duplicate rather
    /// than a shorthand.
    #[test]
    fn reads_the_parameters_an_element_declares() {
        let v = vocab(
            r#"::kind(vocabulary)
::vocabulary(deck, version: "1.0.0")

::element(
  card: (
    display: block
    args: (
      id: (
        type: uint
        positional: true
        required: true
        about: | 通し番号。
      )
      tags: (
        type: list(string)
        about: | タグ。
      )
    )
    about: | カード一枚。
  )
)
"#,
        );
        let card = &v.elements["card"];
        assert_eq!(
            card.params
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            ["id", "tags"]
        );
        assert_eq!(card.positional_keys(), ["id"]);

        let id = card.param("id").expect("declared");
        assert!(id.required);
        assert!(id.positional);
        assert_eq!(id.ty, Some(Ok(Type::bare(Kind::Uint))));
        assert_eq!(id.declared_type(), Some(Type::bare(Kind::Uint)));

        let tags = card.param("tags").expect("declared");
        assert!(!tags.required);
        assert!(!tags.positional);
        let list_of_strings: Type = "list(string)".parse().unwrap();
        assert_eq!(tags.ty, Some(Ok(list_of_strings.clone())));
        assert_eq!(tags.declared_type(), Some(list_of_strings));
    }

    #[test]
    fn reads_content_allow_as_a_name_list() {
        let v = vocab(
            r#"::kind(vocabulary)
::vocabulary(deck, version: "1.0.0")

::element(
  card: (
    display: block
    content: (allow: (link, em, ns.widget))
    about: | カード一枚。
  )
)
"#,
        );
        let card = &v.elements["card"];
        assert_eq!(
            card.content_allow,
            Some(ContentAllow::Names(vec![
                name("link"),
                name("em"),
                name("ns.widget"),
            ]))
        );
    }

    #[test]
    fn reads_content_allow_inline_and_any() {
        let v = vocab(
            r#"::kind(vocabulary)
::vocabulary(deck, version: "1.0.0")

::element(
  label: (
    content: (allow: inline)
    about: | 短い説明。
  )
  box: (
    content: (allow: any)
    about: | 何でも。
  )
)
"#,
        );
        assert_eq!(
            v.elements["label"].content_allow,
            Some(ContentAllow::Inline)
        );
        assert_eq!(v.elements["box"].content_allow, Some(ContentAllow::Any));
    }

    #[test]
    fn no_content_declaration_is_none() {
        let v = vocab(
            r#"::kind(vocabulary)
::vocabulary(deck, version: "1.0.0")

::element(
  plain: (
    display: block
    about: | 何も宣言しない。
  )
)
"#,
        );
        assert_eq!(v.elements["plain"].content_allow, None);
    }

    /// An element with no `args:` declares no parameters. That is not the
    /// same as declaring it takes none, which is `@data`'s `open:` and is
    /// not built.
    #[test]
    fn an_element_without_args_has_no_parameters() {
        assert!(vocab(WRIT).elements["layers"].params.is_empty());
    }

    /// A document is a vocabulary because it says so, not because it
    /// happens to hold `::element` -- the mistake `extract_blueprint_schema`
    /// made, where any `::kind` at all counted as a blueprint.
    #[test]
    fn a_document_without_the_header_is_not_a_vocabulary() {
        let doc = tomet_ast_parser::parse_file("::kind(note)\n\n::element(x: ())\n").unwrap();
        assert!(Vocabulary::from_file(&doc).is_none());
    }

    #[test]
    fn shadowing_a_builtin_is_reported_at_the_declaration() {
        let v = vocab(
            "::vocabulary(deck)\n\n::element(link: (), ref: (), item: (), kind: ())\n\
             ::attribute(meta: (), link: (), upstream: ())\n",
        );
        // per table: `@kind` and `::link` take no std name of their own table
        assert_eq!(
            v.shadowed_builtins(),
            [
                (DeclKind::Element, "item"),
                (DeclKind::Element, "link"),
                (DeclKind::Attribute, "meta")
            ]
        );
        assert!(vocab(WRIT).shadowed_builtins().is_empty());
    }

    const CATASTO: &str = r#"::kind(vocabulary)
::vocabulary(catasto, version: "1.0.0")
::attribute(
  upstream: (
    about: | The upstream repositories.
    host: ctmt
    singleton: true
    args: ( name: (type: string, positional: true, required: true) )
  )
  note: ()
)
::element(
  upstream: (display: block)
)
"#;

    #[test]
    fn reads_the_attributes_a_vocabulary_declares() {
        let v = vocab(CATASTO);
        let upstream = &v.attributes["upstream"];
        assert!(upstream.singleton);
        assert_eq!(upstream.host, Scope::Ctmt);
        assert_eq!(upstream.positional_keys(), ["name"]);
        let note = &v.attributes["note"];
        assert!(!note.singleton);
        assert_eq!(note.host, Scope::Both);
        // the same name in the other table is another thing
        assert_eq!(v.elements["upstream"].display, Some(Shape::Block));
    }

    #[test]
    fn the_sigil_says_which_table_a_name_is_looked_up_in() {
        let b = Bindings {
            kind: Some(vocab(CATASTO)),
            ..Default::default()
        };
        // both tables have `upstream`
        assert!(b.classify(&name("upstream")).is_ok());
        assert!(b.classify_attribute(&name("upstream")).is_ok());
        // `note` is an attribute only: `@note` is written with the wrong sigil
        assert!(b.classify_attribute(&name("note")).is_ok());
        let err = b.classify(&name("note")).unwrap_err();
        assert!(err.wrong_sigil && !err.attribute, "{err:?}");
        assert!(err.to_string().contains("write `::note`"), "{err}");
        // std's own tables hold the same way
        assert!(b.classify_attribute(&name("kind")).is_ok());
        let err = b.classify_attribute(&name("link")).unwrap_err();
        assert!(err.wrong_sigil && err.attribute, "{err:?}");
        // nothing in either table is unknown as what it was written as
        let err = b.classify_attribute(&name("nonesuch")).unwrap_err();
        assert!(!err.wrong_sigil);
        assert!(
            err.to_string().starts_with("unknown attribute `nonesuch`"),
            "{err}"
        );
        assert!(b.attribute(&name("upstream")).is_some());
        assert!(b.attribute(&name("catasto.upstream")).is_some());
        assert!(b.attribute(&name("kind")).is_none());
    }

    #[test]
    fn an_attribute_of_a_used_vocabulary_is_written_out_and_its_namespace_must_be_in_scope() {
        let mut b = Bindings::default();
        let err = b.classify_attribute(&name("catasto.upstream")).unwrap_err();
        assert_eq!(err.unbound_namespace.as_deref(), Some("catasto"));
        b.used.insert("catasto".to_string(), vocab(CATASTO));
        assert!(b.classify_attribute(&name("catasto.upstream")).is_ok());
        // a used vocabulary is never bare
        assert!(b.classify_attribute(&name("upstream")).is_err());
    }

    fn doc(src: &str) -> File {
        tomet_ast_parser::parse_file(src).expect("document parses")
    }

    #[test]
    fn an_alias_replaces_the_namespace_and_names_resolve_in_the_real_one() {
        let b = Bindings::for_document(&doc("::use(catasto, as: c)\n\nBody.\n"), [vocab(CATASTO)]);
        let real = Ok(ElementKind::Custom("catasto.upstream".to_string()));
        assert_eq!(b.classify_attribute(&name("c.upstream")), real);
        assert_eq!(b.classify(&name("c.upstream")), real);
        assert!(
            b.attribute(&name("c.upstream"))
                .is_some_and(|a| a.singleton)
        );
        // the alias replaces the namespace: written out, it is not in scope
        let err = b.classify_attribute(&name("catasto.upstream")).unwrap_err();
        assert_eq!(err.unbound_namespace.as_deref(), Some("catasto"));
        assert_eq!(err.name, "catasto.upstream");
        // a message about a written name shows it as written
        let err = b.classify_attribute(&name("c.nonesuch")).unwrap_err();
        assert_eq!(err.name, "c.nonesuch");
        // unaliased, the same name resolves to the same thing
        let plain = Bindings::for_document(&doc("::use(catasto)\n\nBody.\n"), [vocab(CATASTO)]);
        assert_eq!(plain.classify_attribute(&name("catasto.upstream")), real);
    }

    #[test]
    fn an_alias_holds_with_no_vocabulary_at_hand() {
        let b = Bindings::for_document(
            &doc("::use(node-graph, as: ng)\n\nBody.\n"),
            std::iter::empty(),
        );
        let real = ElementKind::Custom("node-graph.node".to_string());
        assert_eq!(b.classify_lenient(&name("ng.node")), real);
        assert_eq!(b.classify_lenient(&name("node-graph.node")), real);
        assert_eq!(b.classify_lenient(&name("link")), ElementKind::Link);
        assert_eq!(
            b.classify_lenient(&name("nonesuch")),
            ElementKind::Custom("nonesuch".to_string())
        );
        let attribute = ElementKind::Custom("node-graph.port".to_string());
        assert_eq!(b.classify_attribute_lenient(&name("ng.port")), attribute);
        assert_eq!(
            b.classify_attribute_lenient(&name("kind")),
            ElementKind::Declaration(DeclKind::Kind)
        );
    }

    #[test]
    fn an_aliased_attribute_resolves_leniently_in_the_real_namespace() {
        let b = Bindings::for_document(&doc("::use(catasto, as: c)\n\nBody.\n"), [vocab(CATASTO)]);
        assert_eq!(
            b.classify_attribute_lenient(&name("c.upstream")),
            ElementKind::Custom("catasto.upstream".to_string())
        );
    }

    #[test]
    fn std_written_out_is_std() {
        let b = Bindings::default();
        assert_eq!(
            b.classify_attribute(&name("std.kind")),
            Ok(ElementKind::Declaration(DeclKind::Kind))
        );
        let written = doc("::std.kind(config, 2)\n");
        let view = declaration_args_view(&written.header[0]).unwrap();
        assert_eq!(view.get("kind").and_then(|v| v.as_str()), Some("config"));
    }

    #[test]
    fn a_constructor_call_resolves_through_the_alias() {
        let b = Bindings::for_document(
            &doc("::use(catasto, as: c)\n\nBody.\n"),
            [vocab(CATASTO_VOCAB)],
        );
        let call = tomet_ast_parser::parse_value(r#"c.git("https://x/y.git")"#).unwrap();
        let read = b.read_call(&call).unwrap();
        assert_eq!(read.name, "git");
        assert_eq!(read.get("rev").and_then(|v| v.as_str()), Some("main"));
        let unaliased = tomet_ast_parser::parse_value(r#"catasto.git("https://x/y.git")"#).unwrap();
        assert_eq!(
            b.read_call(&unaliased).unwrap_err(),
            CallError::UnboundNamespace {
                call: "catasto.git".to_string(),
                namespace: "catasto".to_string(),
            }
        );
    }

    #[test]
    fn the_use_declarations_are_held_to_their_rules() {
        let problems = |src: &str| -> Vec<(usize, UseProblem)> { use_problems(&doc(src).header) };
        assert_eq!(
            problems("::use(catasto, \"1.0.0\", as: c)\n::use(deck)\n"),
            []
        );
        assert_eq!(
            problems("::use(catasto, from: x)\n"),
            [(0, UseProblem::UnknownArgument("from".to_string()))]
        );
        for alias in ["\"a b\"", "ng.x", "3", "(a)"] {
            assert_eq!(
                problems(&format!("::use(catasto, as: {alias})\n")),
                [(0, UseProblem::NotAnAlias)],
                "{alias}"
            );
        }
        for alias in ["std", "doc"] {
            assert_eq!(
                problems(&format!("::use(catasto, as: {alias})\n")),
                [(0, UseProblem::ReservedAlias(alias.to_string()))]
            );
        }
        assert_eq!(
            problems("::kind(writ)\n::use(catasto, as: writ)\n"),
            [(1, UseProblem::KindAlias("writ".to_string()))]
        );
        assert_eq!(
            problems("::kind(writ)\n::use(writ, as: w)\n"),
            [(1, UseProblem::UsedKind("writ".to_string()))]
        );
        let taken = |by: &str| UseProblem::AliasTaken {
            alias: "deck".to_string(),
            by: by.to_string(),
        };
        // the namespace of another `::use`, before or after it
        assert_eq!(
            problems("::use(catasto, as: deck)\n::use(deck)\n"),
            [(0, taken("deck"))]
        );
        // two aliases that clash: reported at the later one
        assert_eq!(
            problems("::use(writ, as: deck)\n::use(catasto, as: deck)\n"),
            [(1, taken("writ"))]
        );
        let twice = UseProblem::UsedTwice {
            namespace: "catasto".to_string(),
            first: 0,
        };
        assert_eq!(
            problems("::use(catasto)\n::use(catasto, as: c)\n"),
            [(1, twice)]
        );
        assert!(
            UseProblem::NotAnAlias
                .to_string()
                .contains("`::use(node-graph, as: ng)`")
        );
    }

    #[test]
    fn an_unknown_name_is_advised_with_its_own_sigil() {
        let attribute = UnknownName::attribute(&name("upstrem")).to_string();
        assert!(attribute.contains("(`::ns.upstrem`)"), "{attribute}");
        let element = UnknownName::element(&name("bookmrk")).to_string();
        assert!(element.contains("(`@ns.bookmrk`)"), "{element}");
    }

    #[test]
    fn an_entry_with_a_field_its_table_does_not_have_is_a_problem() {
        let doc = tomet_ast_parser::parse_file(
            "::kind(vocabulary)\n::vocabulary(deck)\n\
             ::element(card: (region: preamble, display: block))\n\
             ::attribute(character: (display: block, host: page, singleton: yes))\n\
             ::constructor(git: (singleton: true))\n",
        )
        .unwrap();
        let problems: Vec<_> = entry_problems_in(&doc)
            .into_iter()
            .map(|(_, p)| (p.table, p.name, p.reason))
            .collect();
        assert_eq!(problems.len(), 5, "{problems:?}");
        assert!(
            problems[0].0 == DeclKind::Element
                && problems[0]
                    .2
                    .starts_with("unknown field `region`; an element has `about`"),
            "{problems:?}"
        );
        assert!(
            problems[1].0 == DeclKind::Attribute
                && problems[1].2.starts_with(
                    "unknown field `display`; an attribute has `about`, `args`, `singleton`, `host`"
                ),
            "{problems:?}"
        );
        assert!(
            problems[2]
                .2
                .starts_with("`host` is one of `tmt`, `ctmt`, `both`"),
            "{problems:?}"
        );
        assert!(problems[3].2.starts_with("`singleton`:"), "{problems:?}");
        assert!(
            problems[4].0 == DeclKind::Constructor
                && problems[4]
                    .2
                    .starts_with("unknown field `singleton`; a constructor has"),
            "{problems:?}"
        );
    }

    #[test]
    fn reserved_element_name_item_is_rejected_even_in_open_vocabulary() {
        let open_v = vocab("::vocabulary(sketch, open: true)\n");
        assert!(!open_v.has("item"));
        assert!(open_v.has("card"));

        let b = Bindings {
            kind: Some(open_v),
            ..Default::default()
        };
        assert!(b.classify(&name("item")).is_err());
        assert!(b.classify(&name("sketch.item")).is_err());
        assert!(b.declaration(&name("item")).is_none());
    }

    #[test]
    fn loaded_vocabularies_rejects_reserved_element_name() {
        let doc = tomet_ast_parser::parse_file(
            "::kind(vocabulary)\n::vocabulary(deck)\n\n::element(item: ())\n",
        )
        .unwrap();
        let mut loaded = LoadedVocabularies::default();
        loaded.add("deck.vocabulary.tmt", &doc);
        assert_eq!(loaded.errors.len(), 1, "{:?}", loaded.errors);
        assert!(
            loaded.errors[0].contains("`item`, which is a reserved element name"),
            "{:?}",
            loaded.errors
        );
    }

    const DECK: &str = "::vocabulary(deck)

::element(
  entry: (
    args: (
      id: (type: uint, positional: true)
      tags: (type: list(string))
      unsupported: (type: set(string))
      owner: (type: person)
      stars: (type: uint, default: \"many\")
      note: ()
    )
  )
)
";

    #[test]
    fn a_type_that_names_no_type_or_a_default_that_does_not_fit_is_a_problem() {
        let doc = tomet_ast_parser::parse_file(DECK).unwrap();
        let problems: Vec<EntryProblem> = entry_problems_in(&doc)
            .into_iter()
            .map(|(_, p)| p)
            .collect();
        let by_param = |p: &str| {
            problems
                .iter()
                .find(|x| x.name == format!("entry.{p}"))
                .map(|x| x.reason.as_str())
        };
        // `set(string)` and `person` are no types
        assert!(
            by_param("unsupported")
                .unwrap()
                .contains("`set` is not a type"),
            "{problems:?}"
        );
        assert!(
            by_param("owner")
                .unwrap()
                .contains("`person` is not a type"),
            "{problems:?}"
        );
        assert!(
            by_param("stars")
                .unwrap()
                .contains("`default` is not of `type: uint`"),
            "{problems:?}"
        );
        // a good type, and no type at all, are not problems
        assert_eq!(by_param("id"), None);
        assert_eq!(by_param("tags"), None);
        assert_eq!(by_param("note"), None);
        assert_eq!(problems.len(), 3);
    }

    #[test]
    fn the_problem_is_reported_once_where_the_vocabulary_is_read() {
        let doc = tomet_ast_parser::parse_file(DECK).unwrap();
        let mut loaded = LoadedVocabularies::default();
        loaded.add("deck.vocabulary.tmt", &doc);
        assert_eq!(loaded.errors.len(), 3, "{:?}", loaded.errors);
        assert!(
            loaded.errors[0].starts_with("deck.vocabulary.tmt: `entry."),
            "{:?}",
            loaded.errors
        );
        // the vocabulary is still accepted: a parameter that cannot be checked is not checked
        assert!(loaded.by_namespace.contains_key("deck"));
    }

    #[test]
    fn the_documents_own_kind_may_be_written_bare() {
        let b = Bindings {
            kind: Some(vocab(WRIT)),
            ..Default::default()
        };
        assert_eq!(
            b.classify(&name("layers")),
            Ok(ElementKind::Custom("writ.layers".into()))
        );
        assert_eq!(
            b.classify(&name("writ.layers")),
            Ok(ElementKind::Custom("writ.layers".into()))
        );
        assert!(b.classify(&name("nope")).is_err());
    }

    /// Bare resolves to `std` first, and `::use`d namespaces are never
    /// written bare -- the two halves of "exactly two namespaces may be
    /// omitted".
    #[test]
    fn std_wins_and_used_namespaces_are_never_bare() {
        let deck = vocab("::vocabulary(deck)\n\n::element(ref: ())\n");
        let b = Bindings {
            kind: Some(vocab(WRIT)),
            used: [("deck".to_string(), deck)].into_iter().collect(),
            ..Default::default()
        };

        assert_eq!(b.classify(&name("link")), Ok(ElementKind::Link));
        assert_eq!(b.classify(&name("std.link")), Ok(ElementKind::Link));
        assert_eq!(
            b.classify(&name("deck.ref")),
            Ok(ElementKind::Custom("deck.ref".into()))
        );
        assert!(
            b.classify(&name("ref")).is_err(),
            "a `::use`d name written bare must not resolve"
        );
        assert!(
            b.classify(&name("std.layers")).is_err(),
            "std is not a fallback for everything in scope"
        );
    }

    /// The point of `for_document`: a caller that cannot open a file can
    /// still get the same answer by handing over the vocabularies.
    ///
    /// This is what the js/java/python bindings and `apps/web` needed --
    /// they call the validator with nothing in scope, so every element a
    /// vocabulary declares came back unknown. Correct, since nothing had
    /// said it existed, but useless in an editor whose host had the files
    /// all along.
    #[test]
    fn a_caller_that_cannot_read_files_can_pass_the_vocabularies_in() {
        let doc = tomet_ast_parser::parse_file("::kind(writ)\n\n@layers(){ \"1\": (\"a\") }\n")
            .expect("document parses");

        // Nothing in scope: `std` only, so the kind's element is unknown.
        assert!(Bindings::default().classify(&name("layers")).is_err());

        // The same document, with the vocabulary handed over.
        let bound = Bindings::for_document(&doc, [vocab(WRIT)]);
        assert_eq!(
            bound.classify(&name("layers")),
            Ok(ElementKind::Custom("writ.layers".into()))
        );
        assert_eq!(bound.classify(&name("link")), Ok(ElementKind::Link));
        assert!(bound.classify(&name("nonesuch")).is_err());
    }

    /// `::use` names the namespace, and that is what puts it in scope.
    ///
    /// Naming a file would mean guessing the namespace from a filename
    /// stem -- and the vault already declares that path, so the document
    /// would be repeating it.
    #[test]
    fn use_brings_a_namespace_into_scope_by_name() {
        let deck = vocab("::vocabulary(deck)\n\n::element(ref: ())\n");
        let doc = tomet_ast_parser::parse_file("::kind(writ)\n::use(deck)\n\n@deck.ref(id:1)\n")
            .expect("document parses");

        let bound = Bindings::for_document(&doc, [vocab(WRIT), deck]);
        assert_eq!(
            bound.classify(&name("deck.ref")),
            Ok(ElementKind::Custom("deck.ref".into()))
        );
        assert!(
            bound.classify(&name("ref")).is_err(),
            "a `::use`d namespace is never written bare"
        );
    }

    /// A version after the name is declared, not acted on: the namespace binds
    /// the same with or without it.
    #[test]
    fn a_version_after_the_name_does_not_change_what_binds() {
        let deck = vocab("::vocabulary(deck)\n\n::element(ref: ())\n");
        for src in [
            "::kind(writ, 1)\n::use(deck, 3)\n\n@deck.ref(id:1)\n",
            "::kind(writ)\n::use(deck)\n\n@deck.ref(id:1)\n",
        ] {
            let doc = tomet_ast_parser::parse_file(src).expect("document parses");
            let bound = Bindings::for_document(&doc, [vocab(WRIT), deck.clone()]);
            assert!(bound.classify(&name("layers")).is_ok(), "{src}");
            assert!(bound.classify(&name("deck.ref")).is_ok(), "{src}");
        }
    }

    /// A vocabulary that is available but not asked for stays out of
    /// scope. Only the document's own kind binds without being written.
    #[test]
    fn an_available_vocabulary_is_not_in_scope_unless_the_document_asks() {
        let deck = vocab("::vocabulary(deck)\n\n::element(ref: ())\n");
        let doc =
            tomet_ast_parser::parse_file("::kind(writ)\n\n@layers()\n").expect("document parses");

        let bound = Bindings::for_document(&doc, [vocab(WRIT), deck]);
        assert!(bound.classify(&name("layers")).is_ok());
        assert!(
            bound.classify(&name("deck.ref")).is_err(),
            "available is not the same as bound -- `::use` is what asks"
        );
    }

    /// With nothing in scope, only `std` resolves -- which is what every
    /// document that declares no kind gets.
    #[test]
    fn empty_bindings_resolve_std_and_nothing_else() {
        let b = Bindings::default();
        assert_eq!(
            b.classify_attribute(&name("meta")),
            Ok(ElementKind::Declaration(DeclKind::Meta))
        );
        assert_eq!(b.classify(&name("link")), Ok(ElementKind::Link));
        assert!(b.classify(&name("layers")).is_err());
        assert!(b.classify(&name("writ.layers")).is_err());
    }

    #[test]
    fn builtin_doc_vocabularies_names_doc_index_and_declares_nothing() {
        let vocabs = builtin_doc_vocabularies();
        let index = vocabs
            .iter()
            .find(|v| v.namespace == "doc.index")
            .expect("doc.index is among the builtin vocabularies");
        assert!(index.elements.is_empty());
        assert!(!index.open);
    }

    /// `::kind(doc.index)` resolves the same way `::kind(writ)` does once its
    /// vocabulary is in `available` -- the hardcoded and vault-declared
    /// paths meet at the same `Bindings::for_document`.
    #[test]
    fn kind_doc_index_binds_from_the_builtin_vocabulary() {
        let doc = tomet_ast_parser::parse_file("::kind(doc.index)\n\n#[ An index ]\n")
            .expect("document parses");
        let bound = Bindings::for_document(&doc, builtin_doc_vocabularies());
        assert!(bound.kind.is_some());
    }

    /// `Bindings::default()` -- what `tomet_validator::validate_document`
    /// (no vault, no vocabularies loaded at all) actually uses -- has to
    /// resolve `doc.icon` too, not just a `Bindings` built through
    /// `for_document` with `builtin_doc_vocabularies()` handed in. This is
    /// exactly why `classify`/`declaration` special-case `"doc"`
    /// unconditionally, the same as `"std"`, rather than `for_document`
    /// populating `used` with it: `used` only exists on a `Bindings` that
    /// went through `for_document`, and `default()` never does.
    #[test]
    fn doc_icon_resolves_even_with_bindings_default() {
        let bindings = Bindings::default();
        assert_eq!(
            bindings.classify(&name("doc.icon")),
            Ok(ElementKind::Custom("doc.icon".to_string()))
        );
        assert!(bindings.declaration(&name("doc.icon")).is_some());
    }

    /// Unlike `deck` in
    /// [`an_available_vocabulary_is_not_in_scope_unless_the_document_asks`],
    /// `doc` never needs `::use`: it is reserved, so no vault vocabulary may
    /// ever claim it, which is what makes binding it unconditionally safe.
    #[test]
    fn doc_icon_resolves_with_no_use_or_vocabulary_declaration() {
        let doc =
            tomet_ast_parser::parse_file("::kind(writ)\n\n@layers()\n").expect("document parses");
        let bound = Bindings::for_document(&doc, builtin_doc_vocabularies());
        assert_eq!(
            bound.classify(&name("doc.icon")),
            Ok(ElementKind::Custom("doc.icon".to_string()))
        );
    }

    /// `doc` declares only `icon` -- a typo'd or invented name under it is
    /// still an error, not a silent `Custom`. Tomet not resolving `icon`
    /// itself is a different question from tomet not checking the name at
    /// all.
    #[test]
    fn doc_rejects_a_name_it_does_not_declare() {
        let doc =
            tomet_ast_parser::parse_file("::kind(writ)\n\n@layers()\n").expect("document parses");
        let bound = Bindings::for_document(&doc, builtin_doc_vocabularies());
        assert!(bound.classify(&name("doc.glyph")).is_err());
    }

    /// `name` (positional, required) and `pkg` (named, optional) reach
    /// `check_arguments` through the same `declaration()` a vault
    /// vocabulary's `@param`s would, even though nothing declared a
    /// `::vocabulary(doc)` document anywhere.
    #[test]
    fn doc_icon_declares_name_and_pkg() {
        let doc =
            tomet_ast_parser::parse_file("::kind(writ)\n\n@layers()\n").expect("document parses");
        let bound = Bindings::for_document(&doc, builtin_doc_vocabularies());
        let decl = bound
            .declaration(&name("doc.icon"))
            .expect("doc.icon has a declaration");
        assert_eq!(decl.positional_keys(), vec!["name".to_string()]);
        assert!(decl.param("name").is_some_and(|p| p.required));
        assert!(decl.param("pkg").is_some_and(|p| !p.required));
    }

    fn parsed(src: &str) -> File {
        tomet_ast_parser::parse_file(src).expect("document parses")
    }

    fn vocab_src(ns: &str) -> String {
        format!("::kind(vocabulary)\n::vocabulary({ns}, version: \"1.0.0\")\n")
    }

    #[test]
    fn add_accepts_a_vocabulary_and_binds_it() {
        let mut loaded = LoadedVocabularies::default();
        loaded.add("a.tmt", &parsed(&vocab_src("acme")));
        assert!(loaded.errors.is_empty(), "{:?}", loaded.errors);
        assert!(loaded.by_namespace.contains_key("acme"));
        // The hardcoded ones are there without anything declared.
        assert!(loaded.by_namespace.contains_key("doc.index"));
    }

    #[test]
    fn add_rejects_a_reserved_namespace_and_keeps_the_builtin() {
        let mut loaded = LoadedVocabularies::default();
        loaded.add("doc.tmt", &parsed(&vocab_src("doc")));
        loaded.add("index.tmt", &parsed(&vocab_src("doc.index")));
        assert_eq!(loaded.errors.len(), 2, "{:?}", loaded.errors);
        assert!(loaded.errors.iter().all(|e| e.contains("reserved")));
        assert!(loaded.by_namespace.contains_key("doc.index"));
    }

    #[test]
    fn add_names_both_sides_of_a_duplicate_namespace() {
        let mut loaded = LoadedVocabularies::default();
        loaded.add("a.tmt", &parsed(&vocab_src("acme")));
        loaded.add("b.tmt", &parsed(&vocab_src("acme")));
        assert_eq!(loaded.errors.len(), 1, "{:?}", loaded.errors);
        assert!(loaded.errors[0].contains("a.tmt") && loaded.errors[0].contains("b.tmt"));
    }

    #[test]
    fn add_rejects_a_document_with_no_vocabulary_header() {
        let mut loaded = LoadedVocabularies::default();
        loaded.add("plain.tmt", &parsed("just prose\n"));
        assert!(
            loaded.errors[0].contains("plain.tmt"),
            "{:?}",
            loaded.errors
        );
    }

    const CATASTO_VOCAB: &str = r#"::kind(vocabulary)
::vocabulary(catasto, version: "1.0.0")
::constructor(
  git: (
    about: | A Git repository.
    args: (
      url: (
        type: string
        positional: true
        required: true
        about: | The repository URL.
      )
      rev: (
        type: string
        default: main
        about: | A tag, branch or commit.
      )
      root: (
        type: string
        about: | The directory inside the repository that paths are relative to.
      )
    )
  )
  upstream: (
    about: | A file of a named upstream.
    args: (
      name: (
        type: string
        positional: true
        required: true
        about: | The name given in `::upstreams`.
      )
      path: (
        type: string
        required: true
        about: | The file inside the upstream.
      )
    )
  )
)
"#;

    #[test]
    fn reads_constructors_and_their_parameters() {
        let v = vocab(CATASTO_VOCAB);
        assert_eq!(v.namespace, "catasto");
        assert_eq!(
            v.constructors.keys().collect::<Vec<_>>(),
            ["git", "upstream"]
        );

        let git = &v.constructors["git"];
        assert_eq!(git.positional_keys(), ["url"]);
        assert_eq!(
            git.params
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            ["url", "rev", "root"]
        );
        let url = git.param("url").unwrap();
        assert!(url.positional);
        assert!(url.required);
        assert_eq!(url.declared_type(), Some(Type::bare(Kind::String)));
        let rev = git.param("rev").unwrap();
        assert!(!rev.positional);
        assert!(!rev.required);
        assert_eq!(
            rev.default,
            Some(Value::Scalar(tomet_scalar::Scalar::Text("main".into())))
        );

        let upstream = &v.constructors["upstream"];
        assert_eq!(upstream.positional_keys(), ["name"]);
        assert!(upstream.param("name").unwrap().required);
        assert!(upstream.param("path").unwrap().required);
        assert!(!upstream.param("path").unwrap().positional);
    }

    #[test]
    fn reads_a_constructor_call_by_names_with_defaults_and_checks_types() {
        let v = vocab(CATASTO_VOCAB);
        let text = |s: &str| Value::Scalar(tomet_scalar::Scalar::Text(s.into()));

        // 1. catasto.upstream(commonfiles, path: ".editorconfig") -> name = commonfiles, path = ".editorconfig"
        let call = tomet_ast_parser::parse_value(
            r#"catasto.upstream(commonfiles, path: ".editorconfig")"#,
        )
        .unwrap();
        let read = v.read_call(&call).unwrap();
        assert_eq!(read.name, "upstream");
        assert_eq!(read.get("name"), Some(&text("commonfiles")));
        assert_eq!(read.get("path"), Some(&text(".editorconfig")));
        assert_eq!(
            read.args,
            BTreeMap::from([
                ("name".to_string(), text("commonfiles")),
                ("path".to_string(), text(".editorconfig")),
            ])
        );

        // 2. catasto.git("https://github.com/tomet-lang/commonfiles.git") applies default rev = main
        let git_call = tomet_ast_parser::parse_value(
            r#"catasto.git("https://github.com/tomet-lang/commonfiles.git")"#,
        )
        .unwrap();
        let git_read = read_call(&git_call, &v).unwrap();
        assert_eq!(git_read.name, "git");
        assert_eq!(
            git_read.get("url"),
            Some(&text("https://github.com/tomet-lang/commonfiles.git"))
        );
        assert_eq!(git_read.get("rev"), Some(&text("main")));
        assert_eq!(git_read.get("root"), None);

        // 3. Explicit rev overrides default, root is included when given
        let git_full = tomet_ast_parser::parse_value(
            r#"catasto.git("https://github.com/tomet-lang/commonfiles.git", rev: v1, root: base)"#,
        )
        .unwrap();
        let git_full_read = v.read_call(&git_full).unwrap();
        assert_eq!(git_full_read.get("rev"), Some(&text("v1")));
        assert_eq!(git_full_read.get("root"), Some(&text("base")));

        // 4. Errors: wrong namespace, unknown constructor, unknown argument, missing required,
        //    type mismatch, too many positional, positional + same name written twice
        for (src, expected) in [
            (
                r#"other.upstream(commonfiles, path: ".editorconfig")"#,
                "`other.upstream` is in the namespace `other`, not `catasto`",
            ),
            (
                r#"catasto.svn("https://example.com")"#,
                "`catasto.svn`: the namespace `catasto` declares no constructor `svn`; its constructors are `git`, `upstream`",
            ),
            (
                r#"catasto.upstream(commonfiles, path: ".editorconfig", extra: 1)"#,
                "`catasto.upstream` has no argument `extra`; its vocabulary declares the ones it takes under `args:`",
            ),
            (
                r#"catasto.upstream(commonfiles)"#,
                "`catasto.upstream` requires the argument `path`",
            ),
            (
                r#"catasto.upstream((1, 2), path: ".editorconfig")"#,
                "`catasto.upstream`'s argument `name` is declared `type: string`, but a list is not a string",
            ),
            (
                r#"catasto.upstream(commonfiles, ".editorconfig")"#,
                "`catasto.upstream` takes at most 1 positional argument (`name`), found 2",
            ),
            (
                r#"catasto.upstream(commonfiles, name: other, path: ".editorconfig")"#,
                "`catasto.upstream`: the argument `name` is written both by position and by name",
            ),
        ] {
            let val = tomet_ast_parser::parse_value(src).unwrap();
            let err = v.read_call(&val).unwrap_err();
            assert_eq!(err.to_string(), expected, "{src}");
        }
    }
}
