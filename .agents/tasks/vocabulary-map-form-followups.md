# Vocabulary map form: follow-ups

Left over from the commit "feat(vocabulary): read the ::elements map form" (the reader for `::elements{}` and `key: | prose`).
Each item is a separate decision or piece of work; none blocks landing that task.

## tomet

- [ ] tree-sitter: `grammar.js` has no block-content value (`key: [ ... ]` or `key: | ...`),
      so `syntax/block-value.tmt` and `syntax/pipe-value.tmt` are in `KNOWN_TS_ERRORS`.
      Regenerate with `just gen-treesitter` inside `nix develop` (pins 0.26.12 via npx);
      check the generated diff size first. Then remove both entries.
- [ ] Unknown keys in a vocabulary are silently ignored: a misspelt `display`, or `positional`
      outside `args`. Report them from `LoadedVocabularies::add`. Needs a decision: error or warning.
- [ ] `about:` is parsed but never kept. LSP hover/completion on a vocabulary element or argument
      could show it (`ElementDecl` / `ParamDecl` would carry the blocks).
- [ ] `@data` `open:` is still not read (was already unbuilt); `args:` absent still means "nothing said",
      not "takes none".
- [ ] `doc` and `std` vocabulary files vs the hardcoded ones: `RESERVED_NAMESPACES` rejects a vault
      vocabulary named `doc`, and `builtin_doc_vocabularies()` declares `doc.icon` as inline with
      `name`/`pkg` while `vocabularies/doc/doc@1.0.0.vocabulary.tmt` declares `index`/`icon` as block with `id`.
      `std@1.0.0` declares `callout`, which is a builtin. Decide what these files are for
      (see the note on `BUILTIN_KINDS`: `::vocabulary(std)` is meant to become writable).
- [ ] `tomet check .` fails on 6 files (`tmtroot/index/*.roots.tmt` `root`, `deprecated/docs/design/ideas/idea.tmt`,
      `deprecated/docs/examples/list.tmt`, `deprecated/docs/guide/cheatsheet.tmt`, `dev/plans/roadmap.tmt`).
      They look unrelated to the vocabulary work but were never compared against a clean baseline: check at `main`.
- [ ] `tmtroot/deprecated/docs/spec/vocabulary.tmt` still documents the old `@element` form. Leave it as history or
      mark it superseded by `tmtroot/spec/features/vocabulary.tmt`.
- [ ] `tomet-format-printer` prints a pipe value back as `[ ... ]` (same as `|content` everywhere); `tomet format`
      only normalizes whitespace, so vocabulary files keep their `|` form. Confirm that is acceptable.
- [ ] `::vocabulary(doc)` still takes its name as a parenthesised argument, the one place a name is not a key.
      Decide whether the header should be `::vocabulary{ name: doc, version: ... }`.
- [ ] A vocabulary declared locally at the head of an ordinary document (preamble block, same form as the
      shared file): `ElementKind::Elements` is preamble-region and singleton, but no loader reads a vocabulary
      out of a non-vocabulary document yet.

## vocabularies repo (`../vocabularies`)

- [ ] `rust@1.0.0` and `writ@1.0.0` were already pushed (`3b00caa0`) and are SHA-locked by fs-catasto; they were
      rewritten in place. Either bump (`2.0.0`, the old form is gone) and re-lock, or confirm nobody has taken them.
- [ ] `README.md` / `tmtroot/readme.tmt` "Included" lists `writ` and `rust` only; `std`, `ui`, `doc` are missing,
      and every `vocabularies/<name>/readme.tmt` is empty.
- [ ] Placeholder prose: `ui` and `doc` copy the "rule's identifier" `about:` from `writ`; `std`'s `callout` describes a Rust crate.

## Other repos still on the old form (not touched; each owns its copy via catasto)

- [ ] `twrit` (`tmtroot/schema/vocabularies/writ.vocabulary.tmt`, `rust.vocabulary.tmt`, `tests/fixtures/.tomet/vocabularies/`)
      2026-10-24: `writ` and the fixtures copy are in the map form on twrit `main` (`571665d5`); `rust.vocabulary.tmt` in the default
      workspace is converted too (the twrit default workspace is on `main`).
- [ ] `sisho`, `sisho-desk` (`writ.vocabulary.tmt`)
- [ ] `fs-catasto` (`rust@1.0.0`, `writ@1.0.0`)
- [ ] `examples/app-dev` (`api.vocabulary.tmt`), `examples/pkm` (`.tomet/vocabularies/vault.vocabulary.tmt`)
- [ ] `immermemo/.agents/tasks/task-memo-time-tracker.md` mentions a vocabulary; check it.

## Found on 2026-10-24 (the twrit "0 rules" incident)

- [ ] A vocabulary in the old form (`@element(rule){ ... @args{ @param(id){ ... } } }`) is **ignored without a word**. `@rule` then
      never resolves, `twrit` found 0 rules and printed `ok` for as long as it took anybody to notice (twrit, sisho, tomet itself:
      every guard was off). Say it from `LoadedVocabularies::add`: this file declares elements in the form that is no longer read,
      write `::elements{ ... }`. Same decision as "unknown keys" above (error or warning); an error is the one that would have
      caught it. A test: a vocabulary file in the old form in a fixture, `tomet check` must not stay silent.
- [ ] Related and already filed: `dev-shell-twrit.md` (this repository's own dev shell has `twrit 0.2.0`, `3 writs, 0 rules`) and
      `twrit/.agents/tasks/error-on-unresolved-rules.md` (twrit should refuse a writ whose `@rule` does not resolve).
