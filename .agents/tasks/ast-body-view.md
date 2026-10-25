# AST: one view of "what holds data and content", instead of cloning a ListItem into an Element

Raised after the `ast-element-split` task (2026-11-01). The author wants it done. Delete this file when done.
Destination of what is worth keeping: the `//!` of `tomet-syntax-ast` (the "A Node's Role Is Its Type" policy), then the
crates that stop cloning.
Related: `ast-element-split` (done), `node-kinds-unify`, `ast-exhaustive-walks`.

## Facts (seen while splitting `Element`)

- `Element`, `Connect`, `Declaration`, `Bare`, `Hole` (and `ListItem`, `Section` in part) all hold `data: Data`,
  `content: Option<Vec<Block>>` and, mostly, `id`. Code that reads "the groups of a node" is written once per type.
- `ListItem::named_element()` (`crates/tomet-syntax-ast/src/lib.rs`) **clones a whole item into an `Element`** so the
  checks and renderers written for elements can take it. It has 14 callers outside the AST crate (validators, converters,
  printer, stats, LSP). A copy per item, and a way of not having the shared shape.
- The same "content, then the `{...}` group's children" walk is written by hand in:
  `tomet-format/src/clean.rs` (`walk_held`), `tomet-format/src/reorder.rs` (`collect_swaps_in_held`),
  `tomet-convert-pandoc/src/to_pandoc.rs` (`body_blocks`), `tomet-convert-html/src/elements/mod.rs` (`render_generic`).
  Each had to learn about `Entry::Bare` separately; html and pandoc silently dropped bare entries until the snapshot test
  caught it.

## Leans (assistant, not confirmed)

- A small trait in `tomet-syntax-ast` (name open: `Held`, `Body`), `fn data(&self) -> &Data`,
  `fn content(&self) -> Option<&[Block]>`, `fn id(&self) -> Option<&Id>`, implemented by the roles above; a `ListItem`'s
  content is `Vec<Block>`, so it returns `Some(&self.content)`.
- Checks that only read name + data + content take `(&Name, &impl Held)` or a borrowed view struct instead of `&Element`.
  Then `named_element()` has no caller and goes.
- Not a common supertype: roles stay separate types (the point of the split).

## Open

- [ ] Trait or borrowed view struct? (A view can carry `name: Option<&Name>` and `placement` too.)
- [ ] Which of the 14 `named_element()` callers really need an `Element` (anything that takes `connects` or `placement`).

## Steps

- [ ] List the 14 callers and what each reads from the `Element` it is handed.
- [ ] Decide trait vs view with the author.
- [ ] Add it to `tomet-syntax-ast`; move `walk_held` / `collect_swaps_in_held` / `body_blocks` / `render_generic` onto it.
- [ ] Move the callers of `named_element()`; delete it.
- [ ] `cargo test --workspace`, `sisho check .`; `tests/ref` must not change.
