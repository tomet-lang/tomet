AGENTS.md read

# Declaration Braces & CST Verification Progress

## Reading & Investigation
- [x] Read required documents (`CONTRIBUTING.md`, `tests/README.md`, `tomet-edit`, `tomet-syntax-cst-parser`, `Tuple` doc in `tomet-syntax-ast`)
- [x] Inspect declaration shape in `tomet-syntax-cst-parser/src/tests/shape.rs` and `tomet-edit/src/migrate.rs`

## Step 1: CST Verification in Rewriter
- [x] Design parallel CST walk comparing old CST and new CST (same tree except rewritten groups)
- [x] Update `MigrateError` / error reporting with precise mismatch information (`TreeMismatch { first_diff }` reporting byte offset, kind, and text)
- [x] Update `rewrite_value_braces` / `rewrite_tmt` / `rewrite_ctmt` to use parallel CST walk before writing, plus AST lowering check
- [x] Update documentation and module comments to reflect reality
- [x] Delete or update obsolete tests (`refusal_when_ast_differs` / `find_debug_diff`) and add failing-capable tests for CST tree mismatch

Proof:
```
     Running unittests src/lib.rs (target/debug/deps/tomet_edit-7cc10e96591df13f)
test migrate::tests::refusal_when_tree_differs_inside_group ... ok
test migrate::tests::refusal_when_tree_differs_outside_group ... ok
test result: ok. 52 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

## Step 2: Declaration Braces Rewrite Rule
- [x] Inspect CST representation of `::name{ ... }` bare declaration
- [x] Implement second rewrite rule for bare declaration `::name{ ... }` -> `::name( ... )`
- [x] Ensure non-bare declarations (postfix groups, etc.) are unaffected
- [x] Update CLI command (`migrate-value-braces`), help text, counts, and patch dumping
- [x] Add tests for bare declaration rewrite and refusal cases

Proof:
```
test migrate::tests::shape_declaration_empty ... ok
test migrate::tests::shape_declaration_one_line_generated ... ok
test migrate::tests::shape_declaration_multiline_with_comments ... ok
test migrate::tests::shape_declaration_ctmt_with_value_braces ... ok
test migrate::tests::shape_declaration_must_not_change_primary_group_postfix ... ok
test migrate::tests::shape_declaration_must_not_change_content_primary_group ... ok
test migrate::tests::shape_declaration_refusal_when_tree_differs ... ok
test result: ok. 52 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

## Step 3: Run Migration & Verification
- [x] Run `tomet check .` before migration and record list of failing files (68 files: catasto.ctmt, 63 under tmtroot/generated/crates/, plus .writ.tmt, 2 under crates/**/.writ.tmt, and tomet.dict.tmt)
- [x] Run `tomet migrate-value-braces --check` over target sets and record counts
```
64 file(s) would change (87 bracket pairs total: 64 declaration, 23 value)
```
- [x] Run `tomet migrate-value-braces` on `tmtroot/generated/crates/` and `catasto.ctmt`
```
Migrated 64 file(s) (87 bracket pairs total: 64 declaration, 23 value)
```
- [x] Run `git diff -w` on changed files to verify only brackets changed
```
Verified: all 64 files changed ONLY brackets { -> ( and } -> )!
```
- [x] Run `tomet check .` after migration and verify failing file counts decreased by exactly 64
```
4 of 299 file(s) failed (was 68 of 299 file(s) failed before; decreased by exactly 64)
```
- [x] Run `tomet format --check` on the 64 files
`catasto.ctmt` passes. The 63 files under `tmtroot/generated/crates/` were unparseable before migration (so format skipped them); now they parse and fail format-check because of long single-line declarations and unpadded headings in their original generated contents. The rewrite changed only brackets `{` -> `(` and `}` -> `)`.
- [x] Identify which generated crate pages name no existing crate
- [x] Note `catasto.ctmt` requirement for author
- [x] Run full test suite: cargo fmt, cargo check, cargo test (under timeout 1500)
Last lines of `cargo fmt --all -- --check`:
(clean, exited with code 0)

Last lines of `cargo check --workspace --all-targets`:
```
    Checking tomet-edit v0.5.0 (/home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-edit)
    Checking tomet-tests v0.5.0 (/home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/tests)
    Checking tomet-cli v0.5.0 (/home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/apps/cli)
    Checking tomet-merge v0.5.0 (/home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-merge)
    Checking tomet-benches v0.5.0 (/home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/benches)
    Finished `dev` profile [unoptimized] target(s) in 0.88s
```

Last lines of `cargo test --workspace --no-fail-fast`:
```
   Doc-tests tree_sitter_cotomet

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests tree_sitter_tomet

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```
- [x] Verify untouched areas (`git status --porcelain`) and `jj diff --stat --no-pager`
`git status --porcelain -- crates/tree-sitter-tomet crates/tree-sitter-cotomet packages tests/fixtures tests/ref tmtroot/dev` -> empty (clean).
