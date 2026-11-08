# Brief (resume): `{ }` leaves the value position -- steps 4 to 6 and the checks

You are working in the jj workspace `.agents/workspaces/one-container`. Your current directory is that workspace. Work
only here.

## Before anything else: the author's rules

> 場当たりな実装、後方互換性は悪、現在の実装を言い訳にせず、常に、長期的な最美を根拠に考えること
> AGENTS.md はちゃんと読むこと

Your FIRST action is to read `AGENTS.md` whole. Then read `.agents/notes/2026-11-16-value-braces-brief.md` whole: it is
the brief of this task and it still holds, including its rules and its "Done means". This file only says where the first
run stopped and what is left. Where the two differ, this file wins.

## Why there is a second run

The first run was cut off in the middle of step 3's verification: the disk filled up (a download job of the author's,
not your build), and the run died with no final report. The reviewer found the working copy intact (no empty or
truncated file) and re-checked it. The state, as the reviewer measured it:

- Steps 0 to 3 are in the working copy: the survey, the rewriter (`crates/tomet-edit/src/migrate.rs`, subcommand
  `migrate-value-braces`), 17 files rewritten, `.agents/notes/2026-11-16-writ-braces.patch`, `MAP_LITERAL` removed.
- `cargo check --workspace --all-targets` passes.
- `cargo fmt --all -- --check` FAILS in: `crates/tomet-syntax-lowering/src/element.rs`,
  `crates/tomet-syntax-highlight/src/lib.rs`, `crates/tomet-syntax-cst-parser/src/cst/tail_validity.rs`,
  `apps/lsp/src/workspace.rs`, `apps/cli/src/commands/migrate_value_braces.rs`.
- The test `smoke` of `apps/lsp` HANGS (13 minutes, 0 % CPU, two `tomet-lsp` child processes left). The reviewer killed
  it. It is not known whether this hang is caused by this task or was there before.
- Steps 4, 5, 6 are not done. The progress file `.agents/tasks/value-braces-progress.md` says so and it is right.

- The full `cargo test --workspace --no-fail-fast` (with `smoke` killed) shows failures that are tests still written in
  the old spelling, so they are step 5's: `tomet-semantics` `vocabulary::tests::*` (panics at `vocabulary.rs:754` and
  `:1023`), a `failed to parse blueprint: 3:10: ``{`` cannot start a value` (two tests), a `failed to parse format ...
  line 2, column 12` (two tests), and an `assertion left == right failed`. That list is partial: the reviewer's output
  was cut. Get the full list yourself, and fix each one by saying what is true now (the test's source text becomes
  `(k: v)`; a test that is about the old spelling being an error says so and shows the error), never by loosening
  the grammar.

Do not trust the first run's ticks. The reviewer re-runs everything. A tick whose proof you did not re-run in this run is
treated as a false claim.

## What to do, in this order

0. **Re-prove steps 0 to 3 in this run**, quickly: `cargo test -p tomet-edit -p tomet-syntax-cst-parser
   -p tomet-syntax-lowering -p tomet-syntax-highlight` (package names: read each `Cargo.toml`), and the `rg MAP_LITERAL`
   check of "Done means" 6. Then run `cargo fmt --all` and write the real result.
1. **The `smoke` hang.** Find out whether it is this task's. Method: run `cargo test -p tomet-lsp --test smoke` WITH A
   TIMEOUT (`timeout 120 ...`; never run a test that can wait for a child without one), read what the test starts and
   what it waits for, and look at `apps/lsp/src/workspace.rs` and `apps/lsp/src/tests.rs`, which the first run changed.
   If a fixture the smoke test opens now holds a value-position `{` that the new grammar answers with an error, the cause
   is this task's and the fix is in the fixture or the code, whichever says what is true (a frozen `tests/fixtures`
   file may be changed ONLY by the mechanical rewrite). If it hangs on the parent commit as well (use `jj` to look at
   the parent's tree in a separate directory under the scratchpad, never by restoring files here), it is not this task's:
   write the evidence in the progress file and go on. Either way, the test must not be left able to hang: if it waits
   on a child, say in the report what bounds the wait.
2. **Step 4, writers.** As in the main brief, step 4: the printer writes a tuple whose entries all have names as
   `(k: v)`, the empty tuple stays `()`, through ONE function. The first run changed `tomet-printer/src/value.rs`,
   `serde_cotomet/src/{lib,print}.rs` and `tomet-workspace-indexer/src/lib.rs`: read what it did, do not assume it is
   right, and check every other place that writes a brace map (`tomet-semantics`, the converters, the LSP hover, the
   JS/Python/Java bindings' text output). List the places you checked, with the search you used.
3. **Step 5, tests and references.** As in the main brief, step 5. Run the whole `tomet-tests` suite. Only
   `tests/ref/*.printed.tmt` and `*.pandoc.roundtrip.tmt` may change, only by `{...}` becoming `(...)` at a value
   position. Regenerate `tests/SYNTAX.md` the way the repository does it (read `tests/README.md`); never edit it by hand.
   A case in `tests/fixtures` that now shows an error is kept as a case.
4. **Step 6, tree-sitter.** As in the main brief, step 6. Do not touch the grammar. Add the fixtures that no longer
   parse in tree-sitter to `KNOWN_TS_ERRORS` with the comment, and report the number.
5. **The checks of "Done means"**, all of them, with the real final lines. In particular item 3: the list of files that
   `tomet check` fails on, before (`.agents/notes/failing-files-before.txt` holds the first run's list; check that it was
   made on the parent commit's tree and say how you know) and after. Show that "after" is a subset of "before".
   The two scratch files `.agents/notes/all-repo-files.txt` and `.agents/notes/tomet-check-before.txt` are not part of
   the result: delete them at the end, and say so.

## Extra rules for this run

- Disk: it is 5 GB and was full once. Build with `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0`. If a command says "No
  space left on device", STOP, write that in the progress file, and end. Do not delete anything outside the files you
  made; the author's other files are theirs. Do not run `cargo clean` (it makes the next build too long for the disk and
  the quota).
- Every command that can wait on something (a test that starts a server, a child process, a watch) runs under
  `timeout`. A hung test is a finding, not something to wait for.
- Do not poll a long command in a loop: run it once, read the output.
- Same as the main brief: no commit, no `jj describe`, no `jj new`, no push, no `git` writes; no `_ =>` to make a
  `match` compile; no deleted or ignored test; nothing outside this directory.
- Keep `.agents/tasks/value-braces-progress.md` current, in the same form: a step is ticked only after you ran its proof
  in this run and pasted the last lines of the real output.

## Report

As the main brief's "Report", plus: the `smoke` finding (cause, evidence, what was done); what the first run's steps 0
to 3 turned out to be when re-proved; and a list of everything in the working copy that you would not expect from the
brief, with the reason.
