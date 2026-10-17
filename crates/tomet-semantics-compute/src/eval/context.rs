//! What an evaluation can see: the phase, the documents around it, and the
//! clock and the random source.
//!
//! The evaluator performs no I/O. Everything it cannot know by itself comes
//! through the traits here, and the layers above implement them: `tomet-vault`
//! over in-memory sources and `tomet-load` over the disk.

use std::collections::HashMap;
use std::sync::Arc;

use time::OffsetDateTime;
use tomet_ast::Document;

use super::value::{DirRef, DocRef, EvalValue};

/// When the expression is being evaluated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// A document is being rendered. Evaluating twice gives the same result,
    /// so only functions whose result depends on their arguments alone run.
    Output,
    /// A document is being created from a blueprint, and the result is
    /// written into its source. `generate.*` and `vars` exist only here.
    Create,
}

/// Looks things up in the vault on the evaluator's behalf.
///
/// Errors are the text to show; the evaluator wraps them.
pub trait Resolver {
    /// The parsed document at `doc`.
    fn document(&self, doc: &DocRef) -> Result<Arc<Document>, String>;
    /// The document a note name or identifier stands for (`ref("name")`).
    fn note(&self, name: &str) -> Result<DocRef, String>;
    /// Every document under `dir`, subdirectories included.
    fn dir_docs(&self, dir: &DirRef) -> Result<Vec<DocRef>, String>;
    /// The document's `@meta`, typed by the settings' `type:` declarations:
    /// a `list` field is a list (empty when the document lacks it), a
    /// `datetime` field is a datetime, and the other declared fields are
    /// `Null` when the document lacks them. A key that is not declared and
    /// not present is absent from the map.
    fn meta(&self, doc: &DocRef) -> Result<EvalValue, String>;
}

/// A resolver for contexts that have no vault. Every lookup fails.
pub struct NoResolver;

impl Resolver for NoResolver {
    fn document(&self, doc: &DocRef) -> Result<Arc<Document>, String> {
        Err(format!("no vault to find the document `{}` in", doc.0))
    }
    fn note(&self, name: &str) -> Result<DocRef, String> {
        Err(format!("no vault to find the note `{name}` in"))
    }
    fn dir_docs(&self, dir: &DirRef) -> Result<Vec<DocRef>, String> {
        Err(format!("no vault to list the directory `{}` in", dir.0))
    }
    fn meta(&self, doc: &DocRef) -> Result<EvalValue, String> {
        Err(format!("no vault to read the metadata of `{}` from", doc.0))
    }
}

/// The clock and the random source behind `generate.*`.
///
/// A trait so a caller can fix them: the output of a blueprint is then the
/// same every time, which is what a test needs.
pub trait Generate {
    fn now(&self) -> OffsetDateTime;
    fn uuid(&self) -> String;
}

/// The real clock (UTC) and real random UUIDs.
pub struct SystemGenerate;

impl Generate for SystemGenerate {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }
    fn uuid(&self) -> String {
        uuid::Uuid::new_v4().to_string()
    }
}

/// Everything an evaluation can see besides the expression itself.
pub struct EvalContext<'a> {
    pub phase: Phase,
    pub resolver: &'a dyn Resolver,
    pub generate: &'a dyn Generate,
    /// The document the expression was written in: what `self` is.
    pub self_doc: DocRef,
    /// Macro name to template, already merged (the document's own win).
    pub macros: &'a HashMap<String, String>,
    /// `vars`, the values passed to `tomet new`. Only read in [`Phase::Create`].
    pub vars: &'a EvalValue,
}

/// The real clock written in a fixed offset, and real random UUIDs.
///
/// `generate.now()` is a datetime, and a datetime carries its offset. The
/// vault's settings say which one a field wants (`offset: "+09:00"`), so a
/// document created in Tokyo says `+09:00` and not UTC.
pub struct OffsetGenerate {
    pub offset: time::UtcOffset,
}

impl Generate for OffsetGenerate {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc().to_offset(self.offset)
    }
    fn uuid(&self) -> String {
        uuid::Uuid::new_v4().to_string()
    }
}

impl OffsetGenerate {
    /// The real clock in UTC.
    pub fn utc() -> Self {
        Self {
            offset: time::UtcOffset::UTC,
        }
    }

    /// The real clock in the offset written as `+09:00`, `-05:30` or `Z`.
    /// `None` when the text is not an offset.
    pub fn from_offset_text(text: &str) -> Option<Self> {
        super::datetime::parse_offset(text).map(|offset| Self { offset })
    }
}
