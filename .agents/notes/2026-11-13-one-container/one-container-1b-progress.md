# one-container, stage 1b: remove nameless entry of a `{ }` group

## Goal
Remove the nameless entry construct `(marker)[content]` inside `{ ... }` (`tomet_ast::Bare`, `Entry::Bare`, and all associated parser/AST/CST/printer/converters/lowering/tools/bindings/packages/editor code). Produce a helpful parse error suggesting a named element `@name(...)[...]` instead.

## Steps
- [x] Initialize task tracking and read prerequisite documentation
- [x] Read section 7 of `tmtroot/dev/design/one-container.tmt`
- [x] Inspect existing `Bare` construct across the codebase
- [x] Implement parse error and remove nameless group entry lowering in `crates/tomet-syntax-lowering/src/element.rs`
- [x] Update AST (`crates/tomet-syntax-ast`): remove `Bare` struct and `Entry::Bare`
- [x] Update tree walk (`crates/tomet-syntax-tree`)
- [x] Update printer (`crates/tomet-printer`)
- [x] Update transform (`crates/tomet-transform`)
- [x] Update stats (`crates/tomet-stats`)
- [x] Update converters (`crates/tomet-convert-html`, `crates/tomet-convert-pandoc`)
- [x] Update apps (`apps/lsp`, `apps/cli`)
- [x] Update bindings (`bindings/js`)
- [x] Update tree-sitter (`grammar.js`)
- [x] Update tests in `crates/tomet-syntax-ast-parser`
- [x] Update `tests/fixtures/syntax/sigils.tmt` and `tests/src/`
- [x] Regenerate snapshot references per `tests/README.md`
- [x] Verify search check: `Entry::Bare|struct Bare\b|render_bare|bare_to_block|push_bare_entries|BARE_ENTRY`
- [x] Run full workspace test suite `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 cargo test --workspace`
- [x] Verify `jj diff --stat --no-pager` and doc comments
- [x] Final report
