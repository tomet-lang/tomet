//! =[ tomet-validator-profile ]
//!
//! Checks that a document keeps the promise of its `::profile(...)`.
//!
//! =[ Architecture & Responsibilities ]
//!
//! - (Standalone)
//!   | A document that promises `standalone` means what its own bytes say. [`standalone_findings`]
//!   | finds everything it takes from outside, by trying to resolve each thing in a world with
//!   | nothing outside the file: an expression is held to the names the file defines
//!   | ([`tomet_expr::reaches_outside`]), an element name to no vocabulary at all, and an `@term` to
//!   | no dictionary but the file itself. Nothing is evaluated and nothing is read from disk, so the
//!   | check needs no vault.
//! - (Findings, then diagnostics)
//!   | A [`Finding`] says what is outside and where, which is what a converter that makes a file
//!   | standalone works from. [`validate_profile`] is the same list as [`Diagnostic`]s, for
//!   | `tomet check`, and is empty unless the document declares the profile.
//!
//! =[ Not here ]
//!
//! - `oneline`: it needs the lines of the source, which a parsed [`Document`] does not carry.
//! - A vocabulary the document holds itself. No reader for one exists yet, so any element that is
//!   not built in counts as taken from outside.

use tomet_ast::{Document, ElementView};
use tomet_span::Span;
use tomet_declaration::{Profile, document_profiles};
use tomet_expr::{OwnNames, Reach, reaches_outside};
use tomet_validator_core::Diagnostic;

/// What a document takes from outside itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dirt {
    /// Part of a `${...}` that is answered by something other than the file.
    Expression(Reach),
    /// An element that only a vocabulary gives a meaning to. `name` is as written, with its
    /// namespace when it has one.
    Vocabulary { name: String },
    /// A `@term` mention that no dictionary inside the file defines.
    Term { target: String },
}

impl std::fmt::Display for Dirt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Dirt::Expression(reach) => write!(f, "{reach}"),
            Dirt::Vocabulary { name } => write!(
                f,
                "the element `{name}`, whose vocabulary the file does not hold"
            ),
            Dirt::Term { target } => write!(
                f,
                "the term `{target}`, which no dictionary in the file defines"
            ),
        }
    }
}

/// One thing a document takes from outside, and where it is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub dirt: Dirt,
    /// The `${...}` or the element or mention that takes it.
    pub span: Span,
}

/// Everything `doc` takes from outside itself, in the order it is written, whether or not it
/// promises `standalone`.
pub fn standalone_findings(doc: &Document) -> Vec<Finding> {
    let mut findings = Vec::new();
    expression_findings(doc, &mut findings);
    vocabulary_findings(doc, &mut findings);
    term_findings(doc, &mut findings);
    findings.sort_by_key(|finding| finding.span.start);
    findings
}

/// What `doc` breaks of the profiles it declares. Empty when it declares none that this checks.
///
/// Only `standalone` is checked so far; `oneline` is not, see the module doc.
pub fn validate_profile(doc: &Document) -> Vec<Diagnostic> {
    if !document_profiles(doc).contains(&Profile::Standalone) {
        return Vec::new();
    }
    standalone_findings(doc)
        .into_iter()
        .map(|finding| Diagnostic::NotStandalone {
            what: finding.dirt.to_string(),
            span: finding.span,
        })
        .collect()
}

fn expression_findings(doc: &Document, findings: &mut Vec<Finding>) {
    let own = OwnNames::of_document(doc);
    tomet_tree::for_each_interpolation(doc, |interpolation| {
        for reaching in reaches_outside(&interpolation.expr, &own) {
            findings.push(Finding {
                dirt: Dirt::Expression(reaching.reach),
                span: interpolation.span,
            });
        }
    });
}

/// An element name no vocabulary is in scope for: the same classification `tomet check` makes,
/// with nothing bound but what is built in.
fn vocabulary_findings(doc: &Document, findings: &mut Vec<Finding>) {
    let mut diagnostics = Vec::new();
    tomet_validator_core::validate_core(
        doc,
        &tomet_semantics::Bindings::default(),
        &mut diagnostics,
    );
    for diagnostic in diagnostics {
        if let Diagnostic::UnknownElement { name, span, .. } = diagnostic {
            findings.push(Finding {
                dirt: Dirt::Vocabulary { name },
                span,
            });
        }
    }
}

/// A `@term` mention that does not resolve when the only dictionary there is, is the document
/// itself (if it is one).
fn term_findings(doc: &Document, findings: &mut Vec<Finding>) {
    use tomet_semantics::{ElementKind, LoadedDictionaries, TermRef, ValueExt};

    let own = tomet_declaration::document_dictionary(doc);
    let mut dictionaries = LoadedDictionaries::default();
    if own.is_some() {
        dictionaries.add("", doc);
    }
    let mut mention = |el: ElementView<'_>| {
        if tomet_semantics::classify_std_lenient(el) != ElementKind::Term {
            return;
        }
        // A `@term` with no argument is a definition, not a mention.
        let Some(target) = tomet_semantics::normalized_element_args(el).and_then(|args| {
            args.get("target")
                .and_then(|value| value.as_str())
                .map(String::from)
        }) else {
            return;
        };
        if dictionaries
            .resolve(&TermRef::parse(&target), own.as_deref())
            .is_err()
        {
            findings.push(Finding {
                dirt: Dirt::Term { target },
                span: el.span,
            });
        }
    };
    tomet_tree::for_each_element(doc, |el| mention(el.as_view()));
    // A list item written `-@term(...)` is a `ListItem` with a name, not an `Element`.
    tomet_tree::for_each_list_item(doc, |item| {
        if let Some(el) = item.as_element_view() {
            mention(el);
        }
    });
}

#[cfg(test)]
mod tests;
