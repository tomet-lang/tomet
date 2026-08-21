//! Editing operations over a directory of `.tm`/`.tmt` files: batch
//! `@meta`/`@config` key updates (`batch_meta`) and AST-aware
//! structural search & replace -- rename tag, rename key, replace value
//! (`structural`). Split out of `typedmark-tui`'s `engine` module so
//! it's usable outside the TUI. Both modules follow the same shape:
//! scan (via `typedmark-indexer`) -> mutate the parsed `Document` in
//! place (via `typedmark-walker`'s mutable walk) -> re-serialize (via
//! `typedmark-printer`) -> `save()` writes changed files to disk.

pub mod batch_meta;
pub mod structural;
