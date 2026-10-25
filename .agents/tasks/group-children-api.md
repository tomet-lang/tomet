# `Group::as_children` / `has_children` / `from_children` do not agree

Raised after the `ast-element-split` task (2026-11-01). The author wants it done. Delete this file when done.
Destination of what is worth keeping: the doc comments on `Group` in `tomet-syntax-ast`.

## Facts (`crates/tomet-syntax-ast/src/lib.rs`, `impl Group`)

- `as_children()` returns only `Entry::Element`. `has_children()` is true for `Element`, `Bare` **and** `Hole`. So
  `if group.has_children() { for c in group.as_children() { ... } }` skips bare entries while believing it saw them.
- That is how html and pandoc lost `@memo{ (1)[...] }`'s entries after `Entry::Bare` was introduced; only the snapshot
  test (`tests/ref/syntax/sigils.*`) caught it. `clean.rs` and `reorder.rs` had the same trap (fixed by hand).
- `from_children(Vec<Element>)` has no caller left in the workspace.

## Leans (assistant, not confirmed)

- Delete `as_children`; callers `match` on the entries (`Entry::{Pair, Element, Bare, Hole}`), so a new entry kind is a
  compile error where it matters.
- `has_children` -> a name that says what it counts (`has_nested`?), or delete it.
- Delete `from_children`, or make it take `Vec<Entry>`.

## Steps

- [ ] `grep as_children has_children from_children` (callers: pandoc, html, format clean/reorder, parser `element.rs`).
- [ ] Replace callers with a `match` on entries; add a helper only if three or more would write the same loop.
- [ ] Delete / rename; `cargo test --workspace`; `tests/ref` must not change.
