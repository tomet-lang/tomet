//! What a `@vocabulary(ns)` document declares, and how a name resolves
//! against the namespaces a document has in scope.
//!
//! Extraction is pure: it takes a parsed [`Document`] and reads it. The
//! file that document came from is somebody else's problem --
//! `tomet-semantics-resolver` finds `.tomet/vocabularies/<ns>.vocabulary.tmt`
//! and reads it, because this layer may not do I/O.
//!
//! See `docs/spec/vocabulary.tmt` for the normative description. Two
//! rules from it are enforced here:
//!
//! - Exactly two namespaces may be written bare -- `std`, because the
//!   language carries it, and the document's own `@kind`, because it is
//!   declared on the first line. Anything brought in with `@use` is
//!   always written out.
//! - `std` wins every collision, and cannot be shadowed: a vocabulary
//!   that declares a name already in [`BUILTIN_KINDS`] is rejected at the
//!   declaration rather than at each use site, which is one error instead
//!   of many.

use std::collections::BTreeMap;

use tomet_ast::{Block, Document, Name};
use tomet_tree::ValueExt;

use crate::kind::{BUILTIN_KINDS, ElementKind, Shape, UnknownName, classify_lenient};
use crate::positional::normalized_element_args;

/// Where an element is allowed to sit in a document.
///
/// Distinct from an element's *shape*, which is whether it stands as a
/// block or belongs to running text. `@use` has `Region::Preamble` and is
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

/// One element, as a vocabulary declares it.
///
/// Only the three axes are read so far. `@args`, `@data` and `@content`
/// parse and are ignored here; they describe the three slots and are the
/// next thing to land.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ElementDecl {
    /// The shape this element may take, or `None` for either.
    pub display: Option<Shape>,
    pub region: Region,
    pub singleton: bool,
}

/// The elements one namespace declares.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Vocabulary {
    pub namespace: String,
    pub elements: BTreeMap<String, ElementDecl>,
    /// `@vocabulary(ns){ open: true }` -- any name resolves in this
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

impl Vocabulary {
    /// Reads a parsed vocabulary document.
    ///
    /// Returns `None` when the document has no `@vocabulary(ns)` header:
    /// a vocabulary is one because it says so, never because it happens
    /// to contain `@element`. That is the same rule `@blueprint` needed
    /// after `extract_blueprint_schema` had counted any `@kind` at all.
    pub fn from_document(doc: &Document) -> Option<Self> {
        let (namespace, open) = header(doc)?;
        let mut vocab = Vocabulary {
            namespace,
            elements: BTreeMap::new(),
            open,
        };

        for block in &doc.blocks {
            let Block::Element(el) = block else { continue };
            if classify_lenient(el) != ElementKind::Element {
                continue;
            }
            let Some(name) = positional_name(el) else {
                continue;
            };
            vocab.elements.insert(name, decl_from_element(el));
        }

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
            .filter(|name| BUILTIN_KINDS.iter().any(|(builtin, _)| builtin == *name))
            .map(String::as_str)
            .collect()
    }
}

/// The namespaces in scope for one document.
#[derive(Debug, Clone, Default)]
pub struct Bindings {
    /// The vocabulary named by the document's `@kind(X)`. Its names may
    /// be written bare.
    pub kind: Option<Vocabulary>,
    /// Vocabularies brought in with `@use`, by the name each one calls
    /// itself. Never written bare.
    pub used: BTreeMap<String, Vocabulary>,
}

impl Bindings {
    /// What `doc` has in scope, given every vocabulary available.
    ///
    /// Pure: the vocabularies arrive already parsed, so this is the half
    /// a caller that cannot open a file can still run.
    /// `tomet-resolver::bindings_for` is this plus reading them off disk,
    /// and a wasm host that has the sources can pass them straight in.
    ///
    /// A document's `@kind(X)` binds the vocabulary calling itself `X`.
    /// Everything else has to be asked for with `@use`.
    pub fn for_document<I>(doc: &Document, available: I) -> Self
    where
        I: IntoIterator<Item = Vocabulary>,
    {
        let by_namespace: BTreeMap<String, Vocabulary> = available
            .into_iter()
            .map(|v| (v.namespace.clone(), v))
            .collect();

        let kind = crate::meta::document_kind(doc)
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
        };
        let unbound = |namespace: &str| UnknownName {
            name: name.to_string(),
            unbound_namespace: Some(namespace.to_string()),
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
            Some(namespace) => match self.kind.as_ref() {
                Some(kind) if kind.namespace == namespace => kind,
                _ => self.used.get(namespace)?,
            },
        };
        vocab.elements.get(&name.name)
    }
}

/// The namespaces a document asks for with `@use`.
///
/// The argument is the namespace, not a path. The vault already says
/// where each vocabulary lives, so naming the file here would write the
/// path twice -- and a vocabulary names itself, so the path was never
/// the thing being identified anyway.
///
/// It also makes the two ways into scope use one currency: `@kind(writ)`
/// binds by name, and so does `@use(deck)`. The earlier path form forced
/// a step between them (path -> file -> namespace) that nothing could
/// take without carrying paths through this layer, which is why it had
/// been trimming a filename stem and hoping.
fn used_namespaces(doc: &Document) -> Vec<String> {
    let mut names = Vec::new();
    for block in &doc.blocks {
        let Block::Element(el) = block else { continue };
        if classify_lenient(el) != ElementKind::Use {
            continue;
        }
        let Some(args) = normalized_element_args(el) else {
            continue;
        };
        let namespace = args.as_str().map(str::to_string).or_else(|| {
            args.get("target")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        });
        if let Some(namespace) = namespace {
            names.push(namespace);
        }
    }
    names
}

fn builtin(name: &str) -> Option<ElementKind> {
    BUILTIN_KINDS
        .iter()
        .find(|(builtin, _)| *builtin == name)
        .map(|(_, kind)| kind.clone())
}

/// The namespace a `@vocabulary(ns)` header names, and whether it is open.
fn header(doc: &Document) -> Option<(String, bool)> {
    doc.blocks.iter().find_map(|block| {
        let Block::Element(el) = block else {
            return None;
        };
        if classify_lenient(el) != ElementKind::Vocabulary {
            return None;
        }
        let name = positional_name(el)?;
        let open = crate::embedded::element_data(el)
            .and_then(|data| data.get("open").and_then(|v| v.as_bool()))
            .unwrap_or(false);
        Some((name, open))
    })
}

/// The positional argument of `@vocabulary(x)` / `@element(x)`, however
/// it normalized.
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

fn decl_from_element(el: &tomet_ast::Element) -> ElementDecl {
    let Some(data) = crate::embedded::element_data(el) else {
        return ElementDecl::default();
    };

    ElementDecl {
        display: match data.get("display").and_then(|v| v.as_str()) {
            Some("block") => Some(Shape::Block),
            Some("inline") => Some(Shape::Inline),
            _ => None,
        },
        region: match data.get("region").and_then(|v| v.as_str()) {
            Some("preamble") => Region::Preamble,
            _ => Region::Body,
        },
        singleton: data
            .get("singleton")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vocab(src: &str) -> Vocabulary {
        let doc = tomet_parser::parse_document(src).expect("vocabulary parses");
        Vocabulary::from_document(&doc).expect("has a @vocabulary header")
    }

    fn name(s: &str) -> Name {
        match s.split_once('.') {
            Some((ns, n)) => Name::namespaced(ns, n),
            None => Name::bare(s),
        }
    }

    const WRIT: &str = r#"@kind(vocabulary)
@vocabulary(writ){ version: "1.0.0" }

@element(layers){ display: block, singleton: true }[ Layers. ]
@element(pure){ display: block }[ Purity. ]
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

    /// A document is a vocabulary because it says so, not because it
    /// happens to hold `@element` -- the mistake `extract_blueprint_schema`
    /// made, where any `@kind` at all counted as a blueprint.
    #[test]
    fn a_document_without_the_header_is_not_a_vocabulary() {
        let doc = tomet_parser::parse_document("@kind(note)\n\n@element(x){}[ y ]\n").unwrap();
        assert!(Vocabulary::from_document(&doc).is_none());
    }

    #[test]
    fn shadowing_a_builtin_is_reported_at_the_declaration() {
        let v = vocab("@vocabulary(deck)\n\n@element(link){}[ Mine. ]\n@element(ref){}[ Ok. ]\n");
        assert_eq!(v.shadowed_builtins(), ["link"]);
        assert!(vocab(WRIT).shadowed_builtins().is_empty());
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

    /// Bare resolves to `std` first, and `@use`d namespaces are never
    /// written bare -- the two halves of "exactly two namespaces may be
    /// omitted".
    #[test]
    fn std_wins_and_used_namespaces_are_never_bare() {
        let deck = vocab("@vocabulary(deck)\n\n@element(ref){}[ A card ref. ]\n");
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
            "a @use'd name written bare must not resolve"
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
        let doc = tomet_parser::parse_document("@kind(writ)\n\n@layers{ 1: [ \"a\" ] }\n")
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

    /// `@use` names the namespace, and that is what puts it in scope.
    ///
    /// It used to name a file, which meant this had to guess the
    /// namespace from a filename stem -- and the vault had already
    /// declared that path, so the document was repeating it.
    #[test]
    fn use_brings_a_namespace_into_scope_by_name() {
        let deck = vocab("@vocabulary(deck)\n\n@element(ref){}[ A card ref. ]\n");
        let doc = tomet_parser::parse_document("@kind(writ)\n@use(deck)\n\n@deck.ref(id:1)\n")
            .expect("document parses");

        let bound = Bindings::for_document(&doc, [vocab(WRIT), deck]);
        assert_eq!(
            bound.classify(&name("deck.ref")),
            Ok(ElementKind::Custom("deck.ref".into()))
        );
        assert!(
            bound.classify(&name("ref")).is_err(),
            "a @use'd namespace is never written bare"
        );
    }

    /// A vocabulary that is available but not asked for stays out of
    /// scope. Only the document's own kind binds without being written.
    #[test]
    fn an_available_vocabulary_is_not_in_scope_unless_the_document_asks() {
        let deck = vocab("@vocabulary(deck)\n\n@element(ref){}[ A card ref. ]\n");
        let doc =
            tomet_parser::parse_document("@kind(writ)\n\n@layers{}\n").expect("document parses");

        let bound = Bindings::for_document(&doc, [vocab(WRIT), deck]);
        assert!(bound.classify(&name("layers")).is_ok());
        assert!(
            bound.classify(&name("deck.ref")).is_err(),
            "available is not the same as bound -- `@use` is what asks"
        );
    }

    /// With nothing in scope, only `std` resolves -- which is what every
    /// document that declares no kind gets.
    #[test]
    fn empty_bindings_resolve_std_and_nothing_else() {
        let b = Bindings::default();
        assert_eq!(b.classify(&name("meta")), Ok(ElementKind::Meta));
        assert!(b.classify(&name("layers")).is_err());
        assert!(b.classify(&name("writ.layers")).is_err());
    }
}
