# Brief: the rewriter checks itself again, and `::name{ ... }` becomes `::name( ... )`

You are working in the jj workspace `.agents/workspaces/one-container`. Your current directory is that workspace. Work
only here.

## Before anything else: the author's rules

> 場当たりな実装、後方互換性は悪、現在の実装を言い訳にせず、常に、長期的な最美を根拠に考えること
> AGENTS.md はちゃんと読むこと

- Your FIRST action is to read `AGENTS.md` in this directory, whole. Write the first line of your progress file as
  "AGENTS.md read", and follow its "Core Philosophy & Principles".
- No makeshift code. A claim in a doc comment must be true of the code under it. A test must be able to fail.
- Backward compatibility is not a goal. Where this brief is silent and the choice is visible to a user, do not invent a
  design: stop at that site, describe it in the progress file, and go on elsewhere.
- Never write a rule in several places.

## Where things stand

The last commit (`feat(syntax)!: remove { } from value positions; (k: v) is the one spelling`) removed the `MAP_LITERAL`
node: a `{` that starts a value is now a CST `ERROR` node, and the lowering says "`{` cannot start a value: write
`(k: v)`". The one-shot command `tomet migrate-value-braces` (`apps/cli/src/commands/migrate_value_braces.rs`, library
function `rewrite_value_braces` in `crates/tomet-edit/src/migrate.rs`) rewrites the old spelling. It will also be run on
three sibling repositories (`catasto`, `sisho`, `twrit`) that still use it, and then removed.

## The defect to fix first (step 1)

The reviewer found that the rewriter no longer checks itself, and says it does:

- The module comment and the doc of `rewrite_value_braces` say "the AST before and after the rewrite must be EQUAL".
  The code does not do that: `rewrite_tmt` and `rewrite_ctmt` only check that the NEW text parses. The old text cannot be
  parsed to an AST any more (the grammar dropped the old spelling), so the check was lost when the grammar changed.
- `MigrateError::AstMismatch` is never returned. `find_debug_diff` is `#[allow(dead_code)]`. The test
  `refusal_when_ast_differs` only asserts that `find_debug_diff(&"hello", &"world")` is not empty: it cannot fail
  because of the rewriter.

Make the claim true, with a check that does not need the old grammar to produce an AST. The rewriter has the old CST
(it finds the brackets in it, as `ERROR` nodes at a value position) and can produce the new CST. Check, before anything is
written, that the two CSTs are the SAME TREE except at the rewritten groups: walk them in parallel; every node and token
outside a rewritten group has the same kind and the same text; at a rewritten group, the old `ERROR` node (`{`, entries,
`}`) corresponds to a group node in the new tree (`(`, entries, `)`) whose entries have the same kinds and the same text,
compared the same way, recursively. If the walk finds a difference, write nothing and return an error that names the
file and the first difference (kind, text, byte offset), and reuse the variant `AstMismatch` only if the name is still
true: rename it to what it is (a tree mismatch) otherwise. Then the new text must also lower without error
(`tomet_ast_parser::parse_file` / `parse_data_file`), as today.

Tests that can fail: each shape of step 1 of the earlier brief (`2026-11-16-value-braces-brief.md`, "The rewriter" tests)
still passes; and a test that feeds the walk a pair of trees that differ outside the group (for example, a rewrite
function deliberately changed in the test to also uppercase a letter, or two hand-made texts passed to the comparison
function) and sees the refusal with the right first difference. Delete `refusal_when_ast_differs` and
`find_debug_diff` if nothing uses them afterwards. Make the module comment and the doc comments say exactly what is
checked.

## The second rule (step 2): a declaration with only a `{ ... }` group

`::meta{crate: x, dependencies: list(a, b)}` and `::upstreams{ ... }` (a nexus whose only group is `{ ... }`) used to be
the way to write a declaration's data. Since the previous change they are an error: "`::meta` needs a primary group,
`(...)` or `[...]`: `{...}` cannot stand without one". The spelling that replaces them is `::meta(...)`. 64 files
carry the old form:

- 63 pages under `tmtroot/generated/crates/` (one line each: `::meta{crate: ..., dependencies: list(...), dependents:
  list(...)}`);
- the root `catasto.ctmt` (`::upstreams{ ... }`, several lines; it also holds 12 value-position `{ }` that rule 1 handles).

Add this as a second rule of the SAME rewriter and the same command, with the same check as step 1 (parallel CST walk:
the declaration's brace group on the old side, its paren group on the new side, same entries). Read how the CST holds
such a declaration before you write the rule (a `BLOCK_FORM` whose group is the postfix data group: look at
`crates/tomet-syntax-cst-parser/src/tests/shape.rs`, `a_document_declaration_is_a_block_form_with_a_double_colon_sigil`,
and print the tree). It applies to a declaration written with `::` that has NO primary group and has a `{ ... }` group
straight after its name. It does NOT apply to `::meta(a: 1){ k: v }` or to `@name(){ ... }`, to `=[ T ]{ ... }` or to
`- item { ... }`: those `{ }` are the postfix sugar of `::meta(...)` and stay as they are. Empty `::meta{}` becomes
`::meta()`.

For a `.ctmt`, both rules run in one call: rule 2 first, then rule 1, so that a file that fails today only because of its
bare declaration can be read. The command's `--check`, `--dump-patch` and the counts say how many bracket pairs each rule
changed. Update the command's help text and the module comment (it migrates two old spellings now, and says it is to be
removed once the siblings are migrated).

Tests (each must be able to fail): the one-line generated shape, the multi-line shape with a comment inside, the empty
`::meta{}`, a `.ctmt` with a bare declaration and a value brace together, the shapes that must NOT change (`::meta(a:
1){ k: v }`, `@name(){ k: v }`, a heading and a list item with a postfix group, `{$ expr }`, braces in a string and in a
raw block), and a refusal.

## Run it (step 3)

1. `target/debug/tomet migrate-value-braces --check` over the two sets, and write the counts in the progress file.
2. Run it for real on `tmtroot/generated/crates/` and `catasto.ctmt`. Do NOT run it on anything else in this step.
3. `git diff -w` of every changed file shows only brackets. Write the list of changed files with their counts of bracket
   pairs.
4. `tomet check .` before (write the list of failing files before you change a file) and after. Expected: nothing is
   fixed or broken except the 64 files, which now pass, so the failing count falls by 64.
5. `tomet format --check` on each of the 64 files. If a file fails now and failed before, say so; if the rewrite made it
   fail, stop and report it.
6. A page under `tmtroot/generated/crates/` is generated by `twrit` (a sibling tool), not by `tomet spec` (which writes only
   `cli.tmt` and `config.tmt`). Its updates are stopped for now: `twrit` is being reworked and cannot read the
   repository's `.writ.tmt` files yet. The rewrite of these 63 pages is therefore only the bracket change, so that the
   repository parses; `twrit` will overwrite them when it runs again. DO NOT delete, regenerate, rename or otherwise
   edit any of them, and do not write anything about them being stale or orphaned. In the report, say that the
   generator that will overwrite them must write `::meta( ... )` and not `::meta{ ... }`.
7. `catasto.ctmt` is read by the sibling tool `catasto`, which has its own pinned parser. Say in the report that the
   author must check that `catasto` reads `::upstreams( ... )` and `( )` values before the next sync.

## Rules

- Read first: `AGENTS.md`, `CONTRIBUTING.md`, `tests/README.md`, the `//!` of `tomet-edit` and `tomet-syntax-cst-parser`,
  and the doc comments of `Tuple` in `crates/tomet-syntax-ast/src/lib.rs` (why `{ }` left the value position: `{k: v}` is
  a `( )` whose entries all have names). A crate's directory name and its package name can differ: read `name` in its
  `Cargo.toml` before `cargo check -p`.
- Do not touch: `crates/tree-sitter-tomet`, `crates/tree-sitter-cotomet`, `.writ.tmt` files, `packages/*`, `.md` files,
  `tests/fixtures/`, `tests/ref/`, `tmtroot/dev/`, and any file under `tmtroot/` other than `tmtroot/generated/crates/`.
- Build with `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0` in the environment of every cargo command. The disk is 5 GB
  and was full once: if a command says "No space left on device", STOP, write that in the progress file, and end. Do not
  run `cargo clean`. Do not delete files outside the ones you made.
- Every command that can wait on something (a test that starts a server, a child process) runs under `timeout`. Do not
  poll a long command in a loop: run it once, read the output.
- No commit, no `jj describe`, no `jj new`, no push, no `git` writes. Nothing outside this directory.
- Never add `_ =>` to make a `match` compile, never delete a test to make a run pass (deleting `refusal_when_ast_differs`
  is the one allowed case, and only for the reason above), never mark a test ignored.
- Keep `.agents/tasks/declaration-braces-progress.md` current. Tick a step only after you ran its proof in this run and
  pasted the last lines of the real output. A tick without output is treated as a false claim. The reviewer re-runs
  everything, and reads the doc comments of the rewriter against its code.

## Done means

1. `cargo fmt --all -- --check`, `cargo check --workspace --all-targets`, and `cargo test --workspace --no-fail-fast`
   (under `timeout 1500`) pass. Paste the real last lines of each.
2. The doc comments of `migrate.rs` and the command say what the code checks, and the test that says "refusal" can fail.
3. `tomet check .` fails on 64 fewer files than before, and on no file it did not fail on before.
4. `git status --porcelain -- crates/tree-sitter-tomet crates/tree-sitter-cotomet packages tests/fixtures tests/ref
   tmtroot/dev` shows nothing.
5. `jj diff --stat --no-pager`: list any changed file you would not expect from this brief, with the reason.

## Report

The check you wrote and where it lives; what the CST looks like for a bare declaration; the two rules and their counts;
the 64 files; the real test summary; the open questions from
your progress file; what you did not do or did not check. "I do not know" is an acceptable answer. A claim that turns
out false is not.
