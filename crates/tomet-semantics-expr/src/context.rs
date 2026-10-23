//! What an evaluation can see: the phase, the documents around it, and the
//! clock and the random source.
//!
//! The evaluator performs no I/O. Everything it cannot know by itself comes
//! through the traits here, and the layers above implement them: `tomet-vault`
//! over in-memory sources and `tomet-load` over the disk.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use time::OffsetDateTime;
use tomet_ast::{Block, Document};

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
    /// The document's `::meta`, typed by the settings' `type:` declarations:
    /// a `list` field is a list (empty when the document lacks it), a
    /// `datetime` field is a datetime, and the other declared fields are
    /// `Null` when the document lacks them. A key that is not declared and
    /// not present is absent from the map.
    fn meta(&self, doc: &DocRef) -> Result<EvalValue, String>;
    /// Prepares `blocks`, which were read from `doc`, as `doc` itself would
    /// be prepared: its own expressions evaluated in its own context, its
    /// macros, its `self`. This is what makes an included file mean what it
    /// means in its own place, before it is put into another.
    ///
    /// `includes` is the chain of documents being included at this moment,
    /// outermost first, ending with `doc`; the preparation must pass it on so
    /// that an include inside can see it is going in a circle.
    fn prepare_blocks(
        &self,
        doc: &DocRef,
        blocks: Vec<Block>,
        includes: &[DocRef],
    ) -> Result<PreparedBlocks, String>;
}

/// Blocks prepared in their own file's context.
#[derive(Debug, Clone, PartialEq)]
pub struct PreparedBlocks {
    pub blocks: Vec<Block>,
    /// How many things in them could not be resolved. They are left in the
    /// blocks as written, and reported when their own file is checked; this
    /// is only the count, so the file that includes them can say it did.
    pub unresolved: usize,
}

impl PreparedBlocks {
    /// Blocks in which nothing was left unresolved.
    pub fn clean(blocks: Vec<Block>) -> Self {
        Self {
            blocks,
            unresolved: 0,
        }
    }
}

/// Something an evaluation noticed that is not part of its result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Note {
    /// An `include` put in content from `from` that has `unresolved` things
    /// left as written.
    Included { from: DocRef, unresolved: usize },
}

/// Where an evaluation leaves its [`Note`]s.
///
/// Clones share one list, so a note written under a context made from this one
/// (`at_heading_level`) is found by whoever holds the original.
#[derive(Debug, Clone, Default)]
pub struct Notes(Arc<Mutex<Vec<Note>>>);

impl Notes {
    pub fn push(&self, note: Note) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).push(note);
    }

    /// Everything noted since the last call.
    pub fn take(&self) -> Vec<Note> {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(|e| e.into_inner()))
    }
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
    fn prepare_blocks(
        &self,
        doc: &DocRef,
        _blocks: Vec<Block>,
        _includes: &[DocRef],
    ) -> Result<PreparedBlocks, String> {
        Err(format!("no vault to prepare the content of `{}` in", doc.0))
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
    /// The documents being included right now, outermost first, when this
    /// expression sits in an included file. Empty for a document that is
    /// being prepared on its own.
    pub includes: &'a [DocRef],
    /// The level of the heading this expression sits under: 0 at the top of
    /// the document, 1 under a `=[` heading, 2 under `==[`, and so on. An
    /// included heading goes one level below this.
    pub heading_level: usize,
    /// Where the evaluation leaves what it noticed besides its result.
    pub notes: Notes,
}

impl<'a> EvalContext<'a> {
    /// The same context for an expression under a heading of `level`.
    pub fn at_heading_level(&self, level: usize) -> EvalContext<'a> {
        EvalContext {
            phase: self.phase,
            resolver: self.resolver,
            generate: self.generate,
            self_doc: self.self_doc.clone(),
            macros: self.macros,
            vars: self.vars,
            includes: self.includes,
            heading_level: level,
            notes: self.notes.clone(),
        }
    }
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
