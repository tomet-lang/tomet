//! What a `::vocabulary(ns)` document declares, and how a name resolves
//! against the namespaces a document has in scope.
//!
//! Extraction is pure: it takes a parsed [`Document`] and reads it. The
//! file that document came from is somebody else's problem --
//! `tomet-load` finds `.tomet/vocabularies/<ns>.vocabulary.tmt`
//! and reads it, because this layer may not do I/O.
//!
//! See `docs/spec/vocabulary.tmt` for the normative description. Three
//! rules from it are enforced here:
//!
//! - Exactly two namespaces may be written bare -- `std`, because the
//!   language carries it, and the document's own `::kind`, because it is
//!   declared on the first line. Anything brought in with `::use` is
//!   always written out.
//! - `std` wins every collision, and cannot be shadowed: a vocabulary
//!   that declares a name already in [`builtin_kinds`] is rejected at the
//!   declaration rather than at each use site, which is one error instead
//!   of many.
//! - `doc` (`RESERVED_NAMESPACES`) is a second namespace tomet carries
//!   itself, resolved the same unconditional way `std` is in
//!   [`Bindings::classify`]/[`Bindings::declaration`] -- but, unlike
//!   `std`, never written bare: `@doc.icon(...)` needs its full name, just
//!   never an `::use(doc)` in front of it.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use tomet_ast::{Document, Name, Value};
use tomet_tree::ValueExt;
use tomet_type::{Type, TypeError};

use tomet_declaration::DeclKind;

use crate::kind::{ElementKind, Shape, UnknownName, builtin_kinds, classify_std_lenient};
use crate::positional::normalized_element_args;

/// Where an element is allowed to sit in a document.
///
/// Distinct from an element's *shape*, which is whether it stands as a
/// block or belongs to running text. `::use` has `Region::Preamble` and is
/// not a singleton -- you bring in several vocabularies -- which is the
/// proof the two axes are independent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Region {
    /// Only before any body content.
    Preamble,
    /// Anywhere.
    #[default]
    Body,
}

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
    /// when it is written and is not a type (an unknown name, or a spelling
    /// such as `list(string)` that has no meaning yet).
    pub ty: Option<Result<Type, TypeError>>,
    pub required: bool,
    /// Whether this parameter may be filled by position rather than by
    /// name. The *order* comes from where the entry sits in `args:`, which is why
    /// there is no separate `positional: [ ... ]` list -- that would state
    /// an order the declarations already carry.
    pub positional: bool,
    pub default: Option<Value>,
}

/// One element, as a vocabulary declares it.
///
/// `args:` and `content:` are read; `@data` is not yet. `about:`, the
/// element's prose, is not read either.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ElementDecl {
    /// The shape this element may take, or `None` for either.
    pub display: Option<Shape>,
    pub region: Region,
    pub singleton: bool,
    /// `(args)`, in declaration order.
    pub params: Vec<ParamDecl>,
    /// `content: { allow: }`, or `None` when the element declared no
    /// `content:` at all -- distinct from `Some(ContentAllow::Any)`
    /// (`content: { allow: any }`, written out), the same "absent says
    /// nothing, not none" reasoning `@data`'s `open:` already has.
    pub content_allow: Option<ContentAllow>,
}

/// `content: { allow: }`'s value -- what a vocabulary's own element may hold
/// in `[content]`/`|content` (`Vec<Block>`, see `tmtroot/docs/spec/
/// feature/content-shape.tmt`).
#[derive(Debug, Clone, PartialEq)]
pub enum ContentAllow {
    /// `allow: list(a, b)` -- only these names, at any depth.
    Names(Vec<Name>),
    /// `allow: inline` -- at most one plain paragraph, no nested blocks.
    Inline,
    /// `allow: any` -- any blocks, any names.
    Any,
}

/// Something wrong with how a parameter is declared, found where the
/// vocabulary is read rather than at every use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamProblem {
    pub element: String,
    pub param: String,
    pub reason: String,
}

impl ParamDecl {
    /// The declared type, when `type:` names one.
    pub fn declared_type(&self) -> Option<Type> {
        self.ty.as_ref().and_then(|ty| ty.as_ref().ok().copied())
    }
}

impl ElementDecl {
    /// The positional slots, in declaration order.
    pub fn positional_keys(&self) -> Vec<String> {
        self.params
            .iter()
            .filter(|p| p.positional)
            .map(|p| p.name.clone())
            .collect()
    }

    pub fn param(&self, name: &str) -> Option<&ParamDecl> {
        self.params.iter().find(|p| p.name == name)
    }
}

/// The elements one namespace declares.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Vocabulary {
    pub namespace: String,
    pub elements: BTreeMap<String, ElementDecl>,
    /// `::vocabulary(ns){ open: true }` -- any name resolves in this
    /// namespace, declared or not.
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
/// them a hardcoded meaning -- `std` conceptually, and now `doc`.
///
/// `std` is not actually checked here today (nothing ever compared a
/// vocabulary's own namespace string against `"std"`; only individual
/// element names were checked, via `shadowed_builtins`). `doc` is the first
/// namespace this list actually enforces.
pub const RESERVED_NAMESPACES: [&str; 1] = ["doc"];

/// The vocabularies tomet ships hardcoded in Rust rather than as a vault's
/// `::vocabulary(ns)` document -- the same reason `std`'s elements are a
/// Rust table (`builtin_kinds`) rather than a `::vocabulary(std)` document:
/// see `kind.rs`'s doc comment on why that is a limit of the vocabulary
/// format, not a permanent design.
///
/// `doc.index` declares no elements: an index document's body is `std`
/// (`ul`/`ol`/`@link`) plus `${filter(...)}`/`${by(...)}`, which are neither
/// elements nor registered functions -- `tomet-transform::index_query`
/// pattern-matches them directly, with no dependency on `Bindings` at all.
/// This vocabulary exists purely so `::kind(doc.index)` resolves instead of
/// being rejected as unknown.
///
/// `doc` (bare namespace, not `doc.index`) is the opposite case: it
/// declares exactly one element, `icon`, and exists so `@doc.icon(...)`
/// resolves as running text. `std` used to have its own `icon`, recognized
/// but never drawing anything (`docs/spec/builtin-elements.tmt`'s old
/// "recognized, does nothing" table) -- worse than not having it, the same
/// way `::config(style:)` is worse than unimplemented, because a bare
/// `@icon` looked like the real spelling while doing nothing. `doc.icon`
/// replaces it under a namespace that says the opposite by construction:
/// Tomet resolves and draws nothing here, on purpose, forever -- an
/// external renderer implements this notation to make it into a picture.
/// `params` exists so `check_arguments` still catches a typo'd key; it is
/// not a step toward Tomet ever reading `name`/`pkg` itself.
pub fn builtin_doc_vocabularies() -> Vec<Vocabulary> {
    vec![
        Vocabulary {
            namespace: "doc.index".to_string(),
            elements: BTreeMap::new(),
            open: false,
        },
        Vocabulary {
            namespace: "doc".to_string(),
            elements: BTreeMap::from([(
                "icon".to_string(),
                ElementDecl {
                    display: Some(Shape::Inline),
                    region: Region::Body,
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
    /// to contain `::elements`. That is the same rule `::blueprint` needed
    /// after `extract_blueprint_schema` had counted any `::kind` at all.
    pub fn from_document(doc: &Document) -> Option<Self> {
        let (namespace, open) = header(doc)?;
        let mut vocab = Vocabulary {
            namespace,
            elements: BTreeMap::new(),
            open,
        };

        tomet_tree::for_each_top_level_element(doc, |el| {
            if classify_std_lenient(el) != ElementKind::Declaration(DeclKind::Elements) {
                return;
            }
            let Some(data) = crate::data::element_data(el) else {
                return;
            };
            for (name, decl) in data.as_map().unwrap_or_default() {
                vocab.elements.insert(name.clone(), decl_from_value(decl));
            }
        });

        Some(vocab)
    }

    /// Whether this vocabulary answers for `name` -- because it declares
    /// it, or because it is open.
    pub fn has(&self, name: &str) -> bool {
        self.open || self.elements.contains_key(name)
    }

    /// Names this vocabulary declares that `std` already has, in
    /// declaration order.
    ///
    /// Non-empty means the vocabulary is invalid. Reported here, once,
    /// rather than at every use site: `std` winning is what keeps a bare
    /// name unambiguous, so the shadowing itself is the error.
    pub fn shadowed_builtins(&self) -> Vec<&str> {
        self.elements
            .keys()
            .filter(|name| builtin_kinds().any(|(builtin, _)| builtin == *name))
            .map(String::as_str)
            .collect()
    }

    /// Parameters declared in a way that cannot be held to: a `type:` that
    /// names no type, and a `default:` that is not of its `type:`. In
    /// declaration order.
    ///
    /// Not an invalid vocabulary: such a parameter is accepted and simply not
    /// checked. Reported once, where the vocabulary is read.
    pub fn param_problems(&self) -> Vec<ParamProblem> {
        let mut problems = Vec::new();
        for (element, decl) in &self.elements {
            for param in &decl.params {
                let reason = match &param.ty {
                    Some(Err(err)) => Some(err.to_string()),
                    Some(Ok(ty)) => param
                        .default
                        .as_ref()
                        .and_then(|default| ty.admits(default).err())
                        .map(|mismatch| format!("`default` is not of `type: {ty}`: {mismatch}")),
                    None => None,
                };
                if let Some(reason) = reason {
                    problems.push(ParamProblem {
                        element: element.clone(),
                        param: param.name.clone(),
                        reason,
                    });
                }
            }
        }
        problems
    }
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
    pub fn add(&mut self, label: &str, doc: &Document) {
        let Some(vocab) = Vocabulary::from_document(doc) else {
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
        for shadowed in vocab.shadowed_builtins() {
            self.errors.push(format!(
                "{label} declares `{shadowed}`, which is already a built-in name; \
                 std wins, so a vocabulary may not take one"
            ));
        }
        for problem in vocab.param_problems() {
            self.errors.push(format!(
                "{label}: `{}`'s parameter `{}`: {}",
                problem.element, problem.param, problem.reason
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
    pub fn bindings_for(&self, doc: &Document) -> Bindings {
        Bindings::for_document(doc, self.by_namespace.values().cloned())
    }
}

/// The namespaces in scope for one document.
#[derive(Debug, Clone, Default)]
pub struct Bindings {
    /// The vocabulary named by the document's `::kind(X)`. Its names may
    /// be written bare.
    pub kind: Option<Vocabulary>,
    /// Vocabularies brought in with `::use`, by the name each one calls
    /// itself. Never written bare.
    pub used: BTreeMap<String, Vocabulary>,
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
    /// Everything else has to be asked for with `::use`.
    pub fn for_document<I>(doc: &Document, available: I) -> Self
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
        for namespace in used_namespaces(doc) {
            if let Some(vocab) = by_namespace.get(&namespace) {
                used.insert(namespace, vocab.clone());
            }
        }

        Bindings { kind, used }
    }

    /// Resolves an element name against `std` and everything in scope.
    ///
    /// A bare name goes to `std` first and to the document's kind second.
    /// The order cannot matter, because a vocabulary shadowing a builtin
    /// is rejected when it is read -- but it is written this way round so
    /// that the code says which one wins.
    pub fn classify(&self, name: &Name) -> Result<ElementKind, UnknownName> {
        let unknown = || UnknownName {
            name: name.to_string(),
            unbound_namespace: None,
            declaration: false,
        };
        let unbound = |namespace: &str| UnknownName {
            name: name.to_string(),
            unbound_namespace: Some(namespace.to_string()),
            declaration: false,
        };

        let Some(namespace) = name.namespace.as_deref() else {
            if let Some(kind) = builtin(&name.name) {
                return Ok(kind);
            }
            return match self.kind.as_ref() {
                Some(vocab) if vocab.has(&name.name) => Ok(ElementKind::Custom(format!(
                    "{}.{}",
                    vocab.namespace, name.name
                ))),
                _ => Err(unknown()),
            };
        };

        if namespace == "std" {
            return builtin(&name.name).ok_or_else(unknown);
        }

        // `doc` is tomet's own second reserved namespace, resolved the
        // same unconditional way as `std` just above -- no `::use`, no
        // `::vocabulary(doc)` document, works even with `Bindings::default()`.
        // Still a real, closed vocabulary: `doc.glyph` (undeclared) is
        // `unknown()`, not silently accepted.
        if namespace == "doc" {
            return if DOC_VOCAB.has(&name.name) {
                Ok(ElementKind::Custom(name.to_string()))
            } else {
                Err(unknown())
            };
        }

        let vocab = match self.kind.as_ref() {
            Some(kind) if kind.namespace == namespace => Some(kind),
            _ => self.used.get(namespace),
        };

        match vocab {
            Some(vocab) if vocab.has(&name.name) => Ok(ElementKind::Custom(name.to_string())),
            Some(_) => Err(unknown()),
            None => Err(unbound(namespace)),
        }
    }

    /// The declaration behind a name, when one is in scope.
    pub fn declaration(&self, name: &Name) -> Option<&ElementDecl> {
        let vocab = match name.namespace.as_deref() {
            None | Some("std") => self.kind.as_ref()?,
            // Unconditional, like `classify`'s `"doc"` branch -- `doc.icon`
            // has a real declaration (`name`/`pkg`) regardless of what, if
            // anything, `self.kind`/`self.used` hold.
            Some("doc") => &DOC_VOCAB,
            Some(namespace) => match self.kind.as_ref() {
                Some(kind) if kind.namespace == namespace => kind,
                _ => self.used.get(namespace)?,
            },
        };
        vocab.elements.get(&name.name)
    }
}

/// The namespaces a document asks for with `::use`.
///
/// The argument is the namespace, not a path. The vault already says
/// where each vocabulary lives, so naming the file here would write the
/// path twice -- and a vocabulary names itself, so the path was never
/// the thing being identified anyway.
///
/// It also makes the two ways into scope use one currency: `::kind(writ)`
/// binds by name, and so does `::use(deck)`. The earlier path form forced
/// a step between them (path -> file -> namespace) that nothing could
/// take without carrying paths through this layer, which is why it had
/// been trimming a filename stem and hoping.
fn used_namespaces(doc: &Document) -> Vec<String> {
    let mut names = Vec::new();
    tomet_tree::for_each_top_level_element(doc, |el| {
        if classify_std_lenient(el) != ElementKind::Declaration(DeclKind::Use) {
            return;
        }
        let Some(args) = normalized_element_args(el) else {
            return;
        };
        let namespace = args.as_str().map(str::to_string).or_else(|| {
            args.get("target")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        });
        if let Some(namespace) = namespace {
            names.push(namespace);
        }
    });
    names
}

fn builtin(name: &str) -> Option<ElementKind> {
    builtin_kinds()
        .find(|(builtin, _)| *builtin == name)
        .map(|(_, kind)| kind)
}

/// The namespace a `::vocabulary(ns)` header names, and whether it is open.
fn header(doc: &Document) -> Option<(String, bool)> {
    let mut found = None;
    tomet_tree::for_each_top_level_element(doc, |el| {
        if found.is_some()
            || classify_std_lenient(el) != ElementKind::Declaration(DeclKind::Vocabulary)
        {
            return;
        }
        let Some(name) = positional_name(el) else {
            return;
        };
        let open = crate::data::element_data(el)
            .and_then(|data| data.get("open").and_then(|v| v.as_bool()))
            .unwrap_or(false);
        found = Some((name, open));
    });
    found
}

/// The positional argument of `::vocabulary(x)`, however it normalized.
fn positional_name(el: &tomet_ast::Element) -> Option<String> {
    let args = normalized_element_args(el)?;
    if let Some(s) = args.as_str() {
        return Some(s.to_string());
    }
    for key in ["name", "target", "kind"] {
        if let Some(s) = args.get(key).and_then(|v| v.as_str()) {
            return Some(s.to_string());
        }
    }
    None
}

/// One entry of `::elements{}`: the map of fields an element declares.
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
        region: match decl.get("region").and_then(|v| v.as_str()) {
            Some("preamble") => Region::Preamble,
            _ => Region::Body,
        },
        singleton: decl
            .get("singleton")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        params: decl.get("args").map(params_from_value).unwrap_or_default(),
        content_allow: decl.get("content").and_then(content_allow_from_value),
    }
}

/// The `content: { allow: }` an element declares, if any.
fn content_allow_from_value(content: &Value) -> Option<ContentAllow> {
    match content.get("allow")? {
        Value::String(s) if s == "inline" => Some(ContentAllow::Inline),
        Value::String(s) if s == "any" => Some(ContentAllow::Any),
        // `list(...)` -- the parser resolves it to a `Value::Seq`.
        Value::Seq(items) => {
            let mut names = Vec::with_capacity(items.len());
            for item in items {
                names.push(name_from_dotted(item.as_str()?));
            }
            Some(ContentAllow::Names(names))
        }
        _ => None,
    }
}

/// `"ns.mycard"` -> a namespaced [`Name`]; `"card"` -> a bare one. Same
/// split `tomet-semantics-validator::rule`'s own copy uses for
/// `:rule(allow:list(...))` -- duplicated rather than shared, since that
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
    args.as_map()
        .unwrap_or_default()
        .iter()
        .map(|(name, spec)| ParamDecl {
            name: name.clone(),
            ty: spec.get("type").map(Type::parse),
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

    fn vocab(src: &str) -> Vocabulary {
        let doc = tomet_parser::parse_document(src).expect("vocabulary parses");
        Vocabulary::from_document(&doc).expect("has a ::vocabulary header")
    }

    fn name(s: &str) -> Name {
        match s.split_once('.') {
            Some((ns, n)) => Name::namespaced(ns, n),
            None => Name::bare(s),
        }
    }

    const WRIT: &str = r#"::kind(vocabulary)
::vocabulary(writ){ version: "1.0.0" }

::elements{
  layers: {
    display: block
    singleton: true
    about: | Layers.
  }
  pure: {
    display: block
    about: | Purity.
  }
}
"#;

    #[test]
    fn reads_the_namespace_and_its_elements() {
        let v = vocab(WRIT);
        assert_eq!(v.namespace, "writ");
        assert_eq!(v.elements.keys().collect::<Vec<_>>(), ["layers", "pure"]);

        let layers = &v.elements["layers"];
        assert_eq!(layers.display, Some(Shape::Block));
        assert!(layers.singleton);
        assert_eq!(layers.region, Region::Body);
        assert!(!v.elements["pure"].singleton);
    }

    /// `args:`' entries are read, in the order they are written -- which is
    /// what makes a separate `positional: [ ... ]` list a duplicate rather
    /// than a shorthand.
    #[test]
    fn reads_the_parameters_an_element_declares() {
        let v = vocab(
            r#"::kind(vocabulary)
::vocabulary(deck){ version: "1.0.0" }

::elements{
  card: {
    display: block
    args: {
      id: {
        type: uint
        positional: true
        required: true
        about: | 通し番号。
      }
      tags: {
        type: list(string)
        about: | タグ。
      }
    }
    about: | カード一枚。
  }
}
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
        assert_eq!(id.ty, Some(Ok(Type::Uint)));
        assert_eq!(id.declared_type(), Some(Type::Uint));

        let tags = card.param("tags").expect("declared");
        assert!(!tags.required);
        assert!(!tags.positional);
        // `list(string)` has no spelling as a type yet: kept as the reason.
        assert_eq!(tags.ty, Some(Err(TypeError::NotAName)));
        assert_eq!(tags.declared_type(), None);
    }

    #[test]
    fn reads_content_allow_as_a_name_list() {
        let v = vocab(
            r#"::kind(vocabulary)
::vocabulary(deck){ version: "1.0.0" }

::elements{
  card: {
    display: block
    content: { allow: list(link, em, ns.widget) }
    about: | カード一枚。
  }
}
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
::vocabulary(deck){ version: "1.0.0" }

::elements{
  label: {
    content: { allow: inline }
    about: | 短い説明。
  }
  box: {
    content: { allow: any }
    about: | 何でも。
  }
}
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
::vocabulary(deck){ version: "1.0.0" }

::elements{
  plain: {
    display: block
    about: | 何も宣言しない。
  }
}
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
    /// happens to hold `::elements` -- the mistake `extract_blueprint_schema`
    /// made, where any `::kind` at all counted as a blueprint.
    #[test]
    fn a_document_without_the_header_is_not_a_vocabulary() {
        let doc = tomet_parser::parse_document("::kind(note)\n\n::elements{ x: {} }\n").unwrap();
        assert!(Vocabulary::from_document(&doc).is_none());
    }

    #[test]
    fn shadowing_a_builtin_is_reported_at_the_declaration() {
        let v = vocab("::vocabulary(deck)\n\n::elements{ link: {}, ref: {} }\n");
        assert_eq!(v.shadowed_builtins(), ["link"]);
        assert!(vocab(WRIT).shadowed_builtins().is_empty());
    }

    const DECK: &str = "::vocabulary(deck)

::elements{
  item: {
    args: {
      id: { type: uint, positional: true }
      tags: { type: list(string) }
      owner: { type: person }
      stars: { type: uint, default: \"many\" }
      note: {}
    }
  }
}
";

    #[test]
    fn a_type_that_names_no_type_or_a_default_that_does_not_fit_is_a_problem() {
        let problems = vocab(DECK).param_problems();
        let by_param = |p: &str| {
            problems
                .iter()
                .find(|x| x.param == p)
                .map(|x| x.reason.as_str())
        };
        // `list(string)` has no spelling yet, `person` is no type
        assert!(
            by_param("tags").unwrap().contains("no spelling yet"),
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
        assert_eq!(by_param("note"), None);
        assert_eq!(problems.len(), 3);
    }

    #[test]
    fn the_problem_is_reported_once_where_the_vocabulary_is_read() {
        let doc = tomet_parser::parse_document(DECK).unwrap();
        let mut loaded = LoadedVocabularies::default();
        loaded.add("deck.vocabulary.tmt", &doc);
        assert_eq!(loaded.errors.len(), 3, "{:?}", loaded.errors);
        assert!(
            loaded.errors[0].starts_with("deck.vocabulary.tmt: `item`'s parameter `"),
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
        let deck = vocab("::vocabulary(deck)\n\n::elements{ ref: {} }\n");
        let b = Bindings {
            kind: Some(vocab(WRIT)),
            used: [("deck".to_string(), deck)].into_iter().collect(),
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
        let doc = tomet_parser::parse_document("::kind(writ)\n\n@layers{ 1: list(\"a\") }\n")
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
    /// It used to name a file, which meant this had to guess the
    /// namespace from a filename stem -- and the vault had already
    /// declared that path, so the document was repeating it.
    #[test]
    fn use_brings_a_namespace_into_scope_by_name() {
        let deck = vocab("::vocabulary(deck)\n\n::elements{ ref: {} }\n");
        let doc = tomet_parser::parse_document("::kind(writ)\n::use(deck)\n\n@deck.ref(id:1)\n")
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
        let deck = vocab("::vocabulary(deck)\n\n::elements{ ref: {} }\n");
        for src in [
            "::kind(writ, 1)\n::use(deck, 3)\n\n@deck.ref(id:1)\n",
            "::kind(writ)\n::use(deck)\n\n@deck.ref(id:1)\n",
        ] {
            let doc = tomet_parser::parse_document(src).expect("document parses");
            let bound = Bindings::for_document(&doc, [vocab(WRIT), deck.clone()]);
            assert!(bound.classify(&name("layers")).is_ok(), "{src}");
            assert!(bound.classify(&name("deck.ref")).is_ok(), "{src}");
        }
    }

    /// A vocabulary that is available but not asked for stays out of
    /// scope. Only the document's own kind binds without being written.
    #[test]
    fn an_available_vocabulary_is_not_in_scope_unless_the_document_asks() {
        let deck = vocab("::vocabulary(deck)\n\n::elements{ ref: {} }\n");
        let doc =
            tomet_parser::parse_document("::kind(writ)\n\n@layers{}\n").expect("document parses");

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
            b.classify(&name("meta")),
            Ok(ElementKind::Declaration(DeclKind::Meta))
        );
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
        let doc = tomet_parser::parse_document("::kind(doc.index)\n\n#[ An index ]\n")
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
            tomet_parser::parse_document("::kind(writ)\n\n@layers{}\n").expect("document parses");
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
            tomet_parser::parse_document("::kind(writ)\n\n@layers{}\n").expect("document parses");
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
            tomet_parser::parse_document("::kind(writ)\n\n@layers{}\n").expect("document parses");
        let bound = Bindings::for_document(&doc, builtin_doc_vocabularies());
        let decl = bound
            .declaration(&name("doc.icon"))
            .expect("doc.icon has a declaration");
        assert_eq!(decl.positional_keys(), vec!["name".to_string()]);
        assert!(decl.param("name").is_some_and(|p| p.required));
        assert!(decl.param("pkg").is_some_and(|p| !p.required));
    }

    fn parsed(src: &str) -> Document {
        tomet_parser::parse_document(src).expect("document parses")
    }

    fn vocab_src(ns: &str) -> String {
        format!("::kind(vocabulary)\n::vocabulary({ns}){{ version: \"1.0.0\" }}\n")
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
}
