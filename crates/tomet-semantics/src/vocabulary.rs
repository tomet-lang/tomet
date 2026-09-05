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
}

impl Vocabulary {
    /// Reads a parsed vocabulary document.
    ///
    /// Returns `None` when the document has no `@vocabulary(ns)` header:
    /// a vocabulary is one because it says so, never because it happens
    /// to contain `@element`. That is the same rule `@blueprint` needed
    /// after `extract_blueprint_schema` had counted any `@kind` at all.
    pub fn from_document(doc: &Document) -> Option<Self> {
        let mut vocab = Vocabulary {
            namespace: header_namespace(doc)?,
            elements: BTreeMap::new(),
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
    /// Resolves an element name against `std` and everything in scope.
    ///
    /// A bare name goes to `std` first and to the document's kind second.
    /// The order cannot matter, because a vocabulary shadowing a builtin
    /// is rejected when it is read -- but it is written this way round so
    /// that the code says which one wins.
    pub fn classify(&self, name: &Name) -> Result<ElementKind, UnknownName> {
        let unknown = || UnknownName {
            name: name.name.clone(),
        };

        let Some(namespace) = name.namespace.as_deref() else {
            if let Some(kind) = builtin(&name.name) {
                return Ok(kind);
            }
            return match self.kind.as_ref() {
                Some(vocab) if vocab.elements.contains_key(&name.name) => Ok(ElementKind::Custom(
                    format!("{}.{}", vocab.namespace, name.name),
                )),
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
            Some(vocab) if vocab.elements.contains_key(&name.name) => {
                Ok(ElementKind::Custom(name.to_string()))
            }
            _ => Err(unknown()),
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

fn builtin(name: &str) -> Option<ElementKind> {
    BUILTIN_KINDS
        .iter()
        .find(|(builtin, _)| *builtin == name)
        .map(|(_, kind)| kind.clone())
}

/// The namespace a `@vocabulary(ns)` header names.
fn header_namespace(doc: &Document) -> Option<String> {
    doc.blocks.iter().find_map(|block| {
        let Block::Element(el) = block else {
            return None;
        };
        if classify_lenient(el) != ElementKind::Vocabulary {
            return None;
        }
        positional_name(el)
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
