//! Language server library for Tomet (`tomet-lsp`).
//! Provides diagnostics (parser & validator), document formatting,
//! hover, document symbols, goto definition, and completions.

pub mod completion;
pub mod definition;
pub mod diagnostic;
pub mod format;
pub mod hover;
pub mod position;
pub mod symbol;

pub use completion::{completions_for, completions_for_with_uri};
pub use definition::definition_for;
pub use diagnostic::diagnostics_for;
pub use format::{format_edits, resolve_effective_config};
pub use hover::hover_for;
pub use position::{span_to_range, text_range_to_lsp_range};
pub use symbol::document_symbols_for;

#[cfg(test)]
mod tests;
