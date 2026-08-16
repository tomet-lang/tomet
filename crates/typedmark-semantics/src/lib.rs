//! I/O-free classification of what a parsed `Element` officially means --
//! `url`/`file`/`ref` inference, and recognizing TypedMark's own built-in
//! vocabulary (`@meta`, `@config`, `@links`, ...). Depends only on
//! `typedmark-ast`; consumers (`typedmark-html`, `typedmark-markdown`,
//! the CLI/TUI) use this instead of each carrying their own copy of this
//! logic. See `docs/develop/architecture.md` for how this crate fits into
//! the rest of the pipeline.

mod infer;
mod kind;

pub use infer::{INFERRED_AT_KEYS, infer_at_kind};
pub use kind::{ElementKind, classify};
