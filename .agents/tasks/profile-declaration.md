# `::profile`: what is left

`::profile(oneline, standalone)` is a promise to stay inside a subset of tomet; a profile never changes what a document
means. `standalone` is built: declared, checked, converted to (`tomet standalone`) and read without the vault. What is left is
below. Delete this file when the unchecked items are done or moved; the lasting content is in
`tmtroot/spec/declarations/std.profile.declaration.tmt` (whose "未実装" section is this list's short form) and in the `//!` of
`tomet-validator-profile`, `tomet-convert-standalone`, `tomet-vault` (section "A document that reads alone").

Pushed 2026-11-01 as eight commits on `main` (`ff1f24ed` .. `688499ac`): the declaration, the table of what an expression takes
from outside (`tomet-expr`), the validator, the converter, a printer fix, `tomet standalone`, the vault reading, and a fix to the
validator's term check.

## Undecided (the author's)

- **`@term` in a standalone file.** The converter leaves it and reports it. Freezing it to the term's text loses the reference.
  The other way is to carry the dictionary in, as a vocabulary is to be carried; a file that holds a dictionary of the terms it
  uses is not designed (a `::dictionary` document is a dictionary, and `LoadedDictionaries::add` takes one per document).
- **Carrying a vocabulary.** Needs a reader for a vocabulary declared at the head of an ordinary document (`::vocabulary` +
  `::elements`; `ElementKind::Elements` is preamble-region and singleton, but no loader reads one out of a non-vocabulary
  document -- see `vocabulary-map-form-followups.md`). A file with `::kind(X)` plus `::use(Y)` needs more than one: not
  handled. Whether to keep a vocabulary's `about:` is the user's option at conversion time, default drop.
- **A document's own `::fields`.** Widens `::fields` from vault/folder to a document. Until then `${self.meta.x}` is left by
  the converter, or frozen with `--freeze-fields`. Decided: carrying is the default once it exists, freezing stays opt-in.
- **`oneline`.** Not checkable: the subset has no spec yet (author, 2026-11-01). When it has one: the validator needs the lines
  of the source (a parsed `Document` has byte spans only; `LineIndex` makes lines from them), and the list of what it excludes
  comes from the real grammar (`tomet-syntax-parser`), not from `../oneline`'s notes.
- **Heading level relative to folder depth** (`node-tree.md`) may be an ambient dependency of a document. Not surveyed.
- **Chosen without asking, to confirm:** an unknown, empty or repeated `::profile` is an error, and `::profile` is `.tmt`
  only (a `.ctmt` is standalone by nature: its evaluation has no vault). A document that reads alone keeps the vault's
  constraints (blueprints, the `type:` a metadata field must have) and finds no other file.

## To do

- [ ] The same gap, left alone because it is not this work's: `Vault::unresolved_terms` and `fill_terms` (`tomet-vault`) find
      `@term` mentions with `for_each_element`, which does not visit a list item written `-@term(...)` (a `ListItem` with a
      name). The validator's check was fixed for it (`fix(validator): find an @term mention written as a list item`); these two
      were not. Probed: `-@term("word")` is not reported by `tomet check`, a mention in running text is.
- [ ] Test `self.meta` going through the empty vault (it shares `scope` with the macros, so it should; there is no test).
- [ ] `tomet standalone`: a directory as input (one file at a time today); a pointer from `tomet check`'s report to the command
      (belongs in the CLI, not in the library message).
- [ ] Generated docs, a later bulk pass: `tmtroot/generated/cli.tmt` was out of date before this work and now lacks
      `standalone` too; no generated crate docs for `tomet-validator-profile` and `tomet-convert-standalone`; `Diagnostic` has a
      new variant `NotStandalone` that the language bindings (`bindings/*`) may need to know -- not checked.
- [ ] `apps/cli/src/commands/cli_docs.rs` lays sections out flat as a workaround for the printer gap fixed in `851bc28b`
      (from an old task note, not re-read); it can probably go.
- [ ] When the above settle, remove the "未実装" lines they finish from the spec.

## Things learned (so they are not met again)

- `Span`'s `==` is always true by contract (`tomet-syntax-ast`); compare positions with `exact_eq`.
- `tomet_printer::document_to_tm` used to join a section's blocks with no blank line; fixed in `851bc28b` (50 snapshot refs
  changed by blank lines only).
- A line that starts with `${...}` and goes on is a `Block::Interp` followed by a paragraph, not one paragraph.
- The sandbox's `/` is 5 GB: build with `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0` (memory `reference-build-disk-5g`).
