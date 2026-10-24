//! Language server library for Tomet (`tomet-lsp`).
//! Provides diagnostics (what the text alone says, and what `tomet check`
//! says of a document in its vault), document formatting, hover, document
//! symbols, goto definition, and completions. A standalone cotomet file
//! (`.ctmt`) is served as data: see [`data`].

pub mod completion;
pub mod data;
pub mod definition;
pub mod diagnostic;
pub mod folding;
pub mod format;
pub mod hover;
pub mod position;
pub mod symbol;
mod vault_completion;
mod vault_definition;
pub mod workspace;

pub use completion::{completions_for, completions_for_with_uri};
pub use data::{data_diagnostics_for, is_data_uri, request_is_for_data};
pub use definition::definition_for;
pub use diagnostic::diagnostics_for;
pub use folding::folding_ranges_for;
pub use format::{format_edits, resolve_effective_config};
pub use hover::{Evaluate, hover_for, hover_in};
pub use position::{TextPositions, uri_to_file_path};
pub use symbol::document_symbols_for;
pub use workspace::{Debounce, Workspace};

#[cfg(test)]
mod tests;
