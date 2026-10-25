# Make "a walker forgot a node kind" a compile error

Raised after the `ast-element-split` task (2026-11-01). The author wants it done. Delete this file when done.
Destination of what is worth keeping: a rule in the `.writ.tmt` (propose and stop: the author edits those), and the
`//!` of `tomet-syntax-tree`.
Related: `group-children-api`, `ast-body-view`.

## Facts

Taking declarations out of the element walkers silently changed behavior in places that had no test:
the workspace indexer's `::meta`, structural search / actions, LSP hover for `::kind` / `::blueprint` / `::version`.
They were found by reading, not by a failing test. Bare entries dropped from html / pandoc output were found by a
snapshot only. The common shape: a `match` with `_ => {}` or `Entry::Element(..) | _`, or a helper that filters to one
role (`as_children`, `for_each_element`).

## Leans (assistant, not confirmed)

- Walkers and renderers that must see every node `match` without `_` on `Block` / `Inline` / `Value` / `Entry`.
- A guard: a `.writ.tmt` rule or a clippy `wildcard_enum_match_arm` allow-list for the crates that walk the AST.
  Needs the author's hand on the `.writ.tmt`: propose, do not edit.
- A test per converter that holds a bare entry, a declaration, a hole, a list item with a name (the fixtures
  `tests/fixtures/syntax/sigils.tmt` already hold some).

## Steps

- [ ] Grep for `_ =>` / `| _` on those four enums in `crates/` and `apps/`; classify each as "may ignore" / "must not".
- [ ] Propose the guard to the author (writ rule vs clippy lint).
- [ ] Fix the "must not" ones by naming the variants.
