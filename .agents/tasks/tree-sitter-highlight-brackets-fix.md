# Task: Enhance Syntax Highlighting for List Markers & Fix Bracket Autopair Behavior in Zed

## Goal

1. Expose `list_checkbox` node in `tree-sitter-typedmark` and add capture queries in `highlights.scm` to properly highlight outliner status markers (e.g. `- (T)`, `- (?)`, `- [x]`, `- ( )`) and trailing attributes `{tag: dev}`.
2. Update `config.toml` in Zed extension (`apps/integrations/zed/languages/typedmark/config.toml`) with `[[bracket_pairs]]` configuration so typing `{` and pressing Enter formats as indented block braces instead of collapsing onto a single line.

## Steps

- [ ] **Step 1**: Update `crates/tree-sitter-typedmark/grammar.js` to rename `_list_checkbox` to `list_checkbox` (named node).
- [ ] **Step 2**: Regenerate tree-sitter C parser with `npx tree-sitter-cli@0.26.12 generate` or via tree-sitter build steps.
- [ ] **Step 3**: Update `crates/tree-sitter-typedmark/queries/highlights.scm` to capture list markers and status indicators.
- [ ] **Step 4**: Update `apps/integrations/zed/languages/typedmark/config.toml` with `[[bracket_pairs]]` settings for `{...}`, `[...]`, `(...)`, `<...>`.
- [ ] **Step 5**: Run `cargo test -p tree-sitter-typedmark` and `cargo test --workspace`.
- [ ] **Step 6**: Clean up task file upon completion.
