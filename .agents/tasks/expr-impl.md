# Expression language: what is left

The `$` expression language is designed (`tmtroot/spec/expressions/`, 12 pages) and implemented. This file is
the list of what remains, so the work can be picked up from here without the conversation that produced it.
Delete it when the list is empty.

## State

- Spec: every page under `tmtroot/spec/expressions/` says "実装済み". Two (`address`, `types`) keep a short
  "現状の実装との差"; every page keeps its own "まだ決まっていないこと".
- Syntax (`tomet-syntax-parser/src/interp.rs`, `tomet-syntax-ast`): literals, `( )`, comparison, `&&`, `||`, lambdas
  (`=>`, read only in `lambda_head`, it may become `:>`), `doc#id`, `addr(...)`, line breaks inside `${...}`.
- Evaluator: crate `tomet-expr` (`crates/tomet-semantics-expr`, layer 2, no I/O). Typed `EvalValue`, names by position,
  `Resolver` and `Generate` traits for everything it cannot know. `include` is `include.rs`.
- One pass: `tomet_transform::resolve_expressions` (`tomet-transform/src/expressions.rs`) puts values back by
  position, tracks the heading level, and drops duplicate ids after splicing.
- Vault: `Vault::prepare` and `Vault::prepare_blocks` (`tomet-vault`), typed `@meta` (`resolve.rs`),
  `DiskResolver` (`tomet-load/src/resolver.rs`). `tomet new` uses a creating context
  (`tomet-workspace/src/blueprint.rs`).
- The old evaluator, the `filter` machinery, `VaultIndex` and the `@include` element are deleted.

## Remaining

1. `file:` and `https:` addresses are an error (`address.rs`); their values are undecided (`address.expr.tmt`).
2. Open in the spec pages, decide when needed: a non-recursive `.docs`, datetime members (`.year`) and offset
   conversion, `.group` and other collection methods, an `emoji` table users can extend (`extension.expr.tmt`).
3. The bindings (`bindings/{js,java,python}`) build their `Vault` from `PrinterConfig::default()`, so the `meta`
   `type:` declarations are always empty there and `check_field_types` finds nothing. Passing the settings' source in
   is a small change to three public APIs. `tomet-edit`'s post-edit report (`diagnostics.rs`) calls
   `validate_document` with no config either.
4. `tomet check` reports a duplicate id twice for a document that has one in itself: the error (`duplicate id`) and
   a warning that reads `unresolved ${#(a)}` -- the note that the later id was dropped, which `Unresolved` has no way
   to say is not an expression (`tomet-transform/src/expressions.rs`, `Ids`). A kind on `Unresolved` would fix the
   wording; the LSP shows both as `check` does.

## Decided to leave (2026-10-18, by the owner)

- `tmtroot/deprecated/**`: ignored. Its old forms (`$gh(...)`, `${filter(...)}`, `${uuid()}`) are not migrated, and
  what `tomet check` and `twrit check` report about it (three of the failing files, 41 `document-placement`
  violations, many unresolved-expression warnings) is not worked on.
- The other failures of `tomet check .` (`index/*.roots.tmt`, `dev/plans/roadmap.tmt`,
  `schema/vocabularies/writ.vocabulary.tmt`): ignored. They belong to the `tmtroot` reorganization in progress.
- Seen and not touched, since a writ is the owner's: `file-io-doors` reports `crates/tomet-vault/src/lib.rs:23`, a
  doc comment that names `std::fs` in a crate that is not a door (it predates this work). `tomet format --check .`
  also reports `tmtroot/dev/project-layout.tmt`.

## How to work on it

- `twrit` comes from the flake: `nix develop -c twrit check .` and `nix develop -c twrit api .`, which
  regenerates `tmtroot/generated/**` (it does not delete the page of a crate that was renamed away). The sibling
  `../twrit/target/debug/twrit` also works and gave the same results.

- Build and test: `cargo test --workspace --exclude tomet-python --exclude tomet-java` (the Python and Java
  bindings need toolchains this setup lacks). Expect about 1170 tests, all passing.
- Docs: `./target/debug/tomet check tmtroot/spec`, `format --check tmtroot/spec/expressions`, `export --check .`.
- tree-sitter: edit `crates/tree-sitter-tomet/grammar.js`, then `just gen-treesitter` (pins
  `tree-sitter-cli@0.26.12`, which writes different files from nixpkgs' 0.26.11), then `just gen-treesitter-check`.
  CI runs the same check. A new construct needs a fixture under `tests/fixtures/` and
  `TOMET_UPDATE_REF=1 cargo test -p tomet-tests` (read the diff of `tests/ref` first).
- Commit with `jj`, only when asked, by path (`jj commit -m ... <paths>`), English `type(scope): description`,
  no `Co-Authored-By`. `main` is moved by hand (`jj bookmark set main -r @-`) and pushed only when asked.
- Edit Rust with the edit tool, not with `perl` one-liners over files: braces in the replacement have corrupted
  a file more than once.

## Where the decisions are

The spec pages hold the language. The `//!` of `tomet-expr` holds the evaluator's rules. The memory note
`project-tomet-expression-redesign` holds the list of what was decided and why, in the order it was decided.
