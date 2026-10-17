# tmtroot directory structure

Companion to `spec-capabilities-and-drift-prevention.md`. Resume from this file alone.

Fold destination when done: `tmtroot/dev/tmtroot-structure.tmt` (the decisions below),
then delete this file.

## Decided

- Top level of `tmtroot/`: `dev/` (for developers), `docs/` (for users),
  `generated/` (only tool output, never hand-written), `deprecated/` (temporary),
  `locales/` (undecided, leave alone). Root-level files are entry points + book config.
- `roots.tmt` idea: every document declares which capability node it hangs from in its
  own `@meta`; the node skeleton is hand-written and the contents are `filter` queries.
  Status (current / proposed / extending) is derived from what hangs under a node.
  The author is already prototyping this in `tmtroot/tree/*.roots.tmt` with `@root(<id>)`.
- `spec/elements/` is flat. File name = `<namespace>.<name>.element.tmt`
  (`std.` for the default namespace, e.g. `std.meta.element.tmt`, `doc.icon.element.tmt`).
  Each carries `@kind(element)` and `@meta{ id: <namespace>.<name> }`.
  `id` is the element name, NOT the file stem: `@root(std.meta)` points at it.
- Status / category must not live in the path (promotion would mean a move).

## Done

- [x] Flatten `spec/elements/` (std-* dirs removed).
- [x] `std.` prefix on every namespace-less element.
- [x] `@meta{ id: ... }` on all 24.
- [x] `@kind(element)` on all 24, `.element.tmt` suffix.

## Next (small, independent; pick any)

- [ ] Draft `tmtroot/schema/blueprints/element.blueprint.tmt` (currently empty): require `id`,
      sections 仕様 / 例. Makes the 24 files checkable. (agent drafts, author reviews)
- [ ] Add one line to `tmtroot/dev/tmtroot-structure.tmt` recording the naming rule
      (author-owned file, propose text first).

## Deferred (do not start without the author)

- `.index.` filename collision: `doc.index.element.tmt` contains `.index.`, so the
  book index's `not(contains(filename, ".index."))` drops it. Fix by matching the
  `.index.tmt` suffix or by `kind`. The author is mid-change on `.index.tmt`.
- Same flat + `.kind.tmt` treatment for the other spec dirs
  (`feature/` -> `features/`, `connects/`, `functions/`, `syntax/`).
- Guard that `@meta(roots)` names a real node, and that no spec lacks a node
  (`twrit` / `.writ.tmt` -- author-owned, propose only).
- Tasks as `.tmt` so `filter` can see them (roadmap already lists it).
- Move `dist/` out of `tmtroot/`.
- Executable examples in specs (separate mechanism from roots).
