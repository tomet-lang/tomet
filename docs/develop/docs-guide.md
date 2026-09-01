# Documentation conventions

Rules for `docs/` itself: where a new document goes, what language it is
written in, and which files are generated. `docs/README.md` is the map;
this file is the rulebook behind it.

## Where does a new document go?

Ask what the document *is*, not what it is about:

| The document is... | Put it in | Then |
| --- | --- | --- |
| normative -- it defines what the language accepts and what it means | `spec/` | keep it in step with `tomet-parser`; see the grammar-freeze process |
| instructional -- it shows how to get something done | `guide/` | link it from `docs/README.md` if it is an entry point |
| a runnable sample document | `examples/` | it must pass `just docs-check` |
| about the Rust workspace, not the language | `develop/` | |
| a decision with a date, kept as a record | `design/decisions/` | name it `YYYY-MM-DD-<slug>.md` |
| an idea not yet decided | `design/ideas/` | |

The `spec/` vs `guide/` split is the one that matters most. Before the
restructure these were `ja/specifications/` and `ja/builtins/`, split
"spec vs spec", which meant a reader had to check both every time.
Splitting on *normative vs instructional* means a reader picks one.

## Language

- `spec/`, `guide/`, `examples/` are written in **Japanese**. Japanese is
  the default, so filenames carry no `.ja` suffix and there is no `ja/`
  directory level. If English translations are ever added they go under
  `docs/en/` mirroring the same tree.
- `develop/` and `design/` are written in **English**, matching the
  language of the in-code doc comments they sit next to and cross-link
  with. This is the scope of `AGENTS.md`'s "write documentation in
  English" rule.

## Generated files

`.md` under `spec/` and `guide/` is exported from the sibling `.tmt` via
`@config(export:)`. Do not hand-edit it -- edit the `.tmt` and re-export.
`.gitattributes` marks those two paths `linguist-generated` so GitHub
collapses them in diffs, scoped by path so the hand-written `.md` under
`develop/` and `design/` is unaffected.

`docs/README.md` is a hand-written exception: it is the map, not a
rendering of anything.

## Records are not revised

Files under `design/decisions/` record what was decided on a given date.
Do not rewrite their reasoning when the design later changes -- add a new
dated record instead. Correcting a file path that moved is fine; changing
an argument is not.

## Checking

```bash
just docs-check     # every .tmt under docs/ parses and is formatted
```

Kept out of `cargo test` deliberately: these documents evolve, and prose
edits should not fail unrelated code changes. The frozen inputs that
*do* gate `cargo test` live in `/tests/fixtures` -- see `tests/README.md`
for why they are a separate, non-synced copy.
