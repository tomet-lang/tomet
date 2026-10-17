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
3. `tmtroot/generated/**` is written by `twrit api` (a sibling repository; `just docs`). It was not regenerated:
   `generated/crates/tomet-compute.tmt` is stale (the crate is `tomet-expr`) and other pages still say
   `tomet-compute`; `generated/crates/tomet-workspace.tmt` does not know `Created` / `Instantiation` / `Unfilled`.
4. `tmtroot/deprecated/**` still has old forms (`$gh(...)`, `${filter(...)}`, `${uuid()}`). Left alone on purpose;
   migrate or leave, it is the owner's call.
5. The Markdown exporter writes `@link(tmt:path)` as the bare path (HTML makes a real anchor). Same for a
   hand-written link, so it is not specific to `.link()`.
6. Unresolved expressions inside an included file are not reported by the including file's `tomet check`
   (they are, when that file is checked). Make it report them if that turns out to be wanted.
7. tree-sitter: a call inside a value group, `@meta{ id: ${f()} }`, errors in `scanner.c` (the scalar ends at `(`).
   Recorded in `KNOWN_TS_ERRORS` (`tests/src/lib.rs`, `blueprints/daily-note.blueprint.tmt`). Fix the scanner or
   leave it. Also unchecked: the CST (`tomet-syntax-parser/src/cst.rs`, `parse_interp_expr`) ends `${...}` at the
   first matching `}`, so a `}` inside `addr(...)` would end it early.
8. `check_field_types` is wired into `tomet check` (and so `stats`) only. The LSP and the bindings call
    `validate_document_with` and do not pass the settings' field types.
9. Not from this work, found on the way: `tomet check .` fails on 7 files of the `tmtroot` reorganization in
    progress (`index/*.roots.tmt`, `dev/plans/roadmap.tmt`, `schema/vocabularies/writ.vocabulary.tmt`, three under
    `deprecated/`). (The `cargo fmt --check -p tomet-validator-core` failure noted earlier is gone.)

## How to work on it

- Build and test: `cargo test --workspace --exclude tomet-python --exclude tomet-java` (the Python and Java
  bindings need toolchains this setup lacks). Expect about 1140 tests, all passing.
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
