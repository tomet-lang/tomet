//! =[ tomet-address ]
//!
//! What a link or an element points at, recognized without ever touching
//! disk: the scheme of a link target ([`Address`], [`is_explicit_scheme`]),
//! whether an anchor exists in a document ([`has_anchor`]), and a
//! `::config(import:...)`/`::settings(file:...)` reference to another file
//! ([`config_import_ref`]/[`settings_file_ref`]), which is recognized and not
//! read.
//!
//! An expression's `#id` and `doc#id` are not resolved here any more: that
//! is the evaluator's job (`tomet-expr`), which answers it through a
//! resolver so that it can reach other documents.
//!
//! Actually resolving a `::config(import:...)` reference -- reading and
//! merging the target -- lives in `apps/lsp`, which reads it itself
//! rather than calling back in here.

mod address;
mod anchor;
mod import_ref;

pub use address::{Address, AddressKind, EXPLICIT_SCHEMES, LinkKind, is_explicit_scheme};
pub use anchor::{collect_anchors, find_anchor, has_anchor};
pub use import_ref::{config_import_ref, settings_file_ref};
