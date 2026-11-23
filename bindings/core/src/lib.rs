//! What the language bindings hand out and take.
//!
//! A binding is thin: it moves values across a language boundary. What the values are -- a vault to
//! read documents in, the document read for a page, and the diagnostics about it -- is here, once,
//! so that the JavaScript, Python and Java bindings do not each decide it again, and so that the
//! types of what crosses can be generated from these definitions (`tomet spec binding-schema`)
//! rather than written a second time in every language.
//!
//! =[ A vault, and a document read in it ]
//!
//! A [`Vault`] is made from [`VaultSources`]: the text of the config, the vocabularies and
//! dictionaries it registers, the names of its files and which of them have a page. It is
//! made once, owns all of its state, and reads many documents: [`Vault::process`] for a page
//! ([`ProcessedFile`], with the [`Diagnostic`]s about it), [`Vault::check`] for what `tomet check` says.
//!
//! =[ The diagnostics ]
//!
//! Everything found about a document comes as one [`Diagnostic`]: what the validator reports
//! (`tomet_validator::Diagnostic`), an expression that has no value, a link that has no target. Which
//! of them is fatal is the consumer's decision, not the document's: a page with an `@draft` is
//! rendered either way.

mod diagnostic;
mod highlight;
mod process;
mod vault;

pub use diagnostic::{Diagnostic, Position, Severity};
pub use highlight::{HighlightSpan, compute_highlight_spans};
pub use process::{ProcessOptions, ProcessedFile, TocItem};
pub use vault::{Input, Registry, SourceUrls, Vault, VaultSources, registry};

#[cfg(test)]
mod tests;
