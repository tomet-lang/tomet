# Brief: the tree-sitter grammars follow the one container -- `( )` holds named and unnamed entries, `{ }` is not a value

You are working in the jj workspace `.agents/workspaces/one-container`. Your current directory is that workspace. Work
only here.

## Before anything else: the author's rules

> 場当たりな実装、後方互換性は悪、現在の実装を言い訳にせず、常に、長期的な最美を根拠に考えること
> AGENTS.md はちゃんと読むこと

- Your FIRST action is to read `AGENTS.md` in this directory, whole. Write the first line of your progress file as
  "AGENTS.md read", and follow it. In particular: the real parser (`tomet-parser`, the `crates/tomet-syntax-*` crates)
  is the source of truth for the grammar; the tree-sitter grammars are hand-maintained approximations that follow it.
- No makeshift grammar. A rule is done when it says what the language says, not when the corpus stops complaining. No
  rule, alias or token whose only reason is that the old grammar had it. Backward compatibility is not a goal: the old
  value-position `{ }` is gone from the language, so it goes from the grammar too.
- Where this brief is silent and the choice is visible to a user (a node name an editor's query depends on), do not
  invent: stop at that site, write the choice in the progress file, and go on elsewhere.

## Where things stand

The language changed in the last commits (read `git log`/`jj log` messages of the last five commits first):

- `( )` is the one container: an ordered run of entries, each with a name or without one. `(k: v)`, `(a, b)` and
  `(a, k: v)` are all the same kind of thing: a tuple. A call `name( ... )` takes the same entries.
- A value-position `{ ... }` is an error in the language. `{ }` remains only (a) as the postfix group after an element,
  a heading or a list item (the sugar of `::meta(...)`), and (b) as a part of a gate (`{$ ... }`). `::name{ ... }` with
  no primary group is an error; the spelling is `::name( ... )`.
- The real parser reads a tuple's entries uniformly: each entry is `key: value` or a value, separated by `,` or a
  newline; a key is a bare word or a string.

The tree-sitter grammars do not know this yet:

- `crates/tree-sitter-tomet/grammar.js`: value positions have `braced_map` (`{ ... }`), `sequence` (`( )` of values only,
  never `key: value`), `call` (`name( )` of values only), and `map`/`map_entry` (`key: value` entries) reachable only from
  `args_group`'s `value` and `value_group`'s `value`. `map_entry` also has a `key { ... }` form (`field("value",
  $.braced_map)`).
- `crates/tree-sitter-cotomet/grammar.js`: values are `{...}` map, `(...)` sequence, `name(...)` call or a scalar;
  `::meta{ a: 1 }` is described as not a declaration.
- `tests/src/lib.rs` `KNOWN_TS_ERRORS` lists, for the corpus test (`cargo test -p tomet-tests`, target `corpus`), the
  fixtures tree-sitter is known to read with an error. Four were added by this change and say "the grammar follows in the
  last step of the task": `syntax/tuple.tmt`, `default.config.tmt`, `syntax/nexus-declarations.tmt`, `test.config.tmt`.
  This is that step.

## What to do, in this order

0. **Survey, no edits.** Read both `grammar.js` files, `src/lib.rs` of each crate (its module doc lists the known
   simplifications), `queries/*.scm`, and `scanner.c` where it mentions a value or a brace. Find every use of the node
   names `braced_map`, `map`, `map_entry`, `sequence`, `call` in: the queries of both crates; `editors/` (helix, neovim,
   vscode, zed: their own query files, if they have copies); the Rust tests of both crates; `tests/src`. Write the list in
   the progress file. Then run the corpus test once, with the four entries of `KNOWN_TS_ERRORS` above removed locally in
   a scratch copy of the list (do not commit that), and write down for each of the four fixtures WHERE tree-sitter first
   errors (line and the construct). Some of these may fail for a reason that has nothing to do with this change (an
   older known gap): say which.
1. **The grammar.** In `crates/tree-sitter-tomet/grammar.js`:
   - Remove `braced_map` and the `key { ... }` form of `map_entry`; a `{` that starts a value is not a value any more.
     Keep `value_group` (the postfix group) and the gate.
   - Make the one container: a tuple whose entries are `map_entry` (`key: value`) or a value, comma- or
     newline-separated, used for a value position `( )`, for a call's `( )` and for `args_group`. Name the rule for what
     the language calls it (`tuple`), and an entry's node accordingly; if keeping an existing node name is what editors'
     queries need to keep working, say so in the progress file and decide the smaller change, but never keep the name
     `sequence` or `map` for a node that is neither.
   - The grammar's comments name real LR problems (a bare word before `:` is a key or a callee; `id: review` against
     `id:` followed by an entry; a lexer that prefers `identifier` over a scalar). Resolve them with the mechanisms the
     file already uses (token precedence, `word`, aliases, `conflicts`), and say in a comment why, as the file does. If a
     conflict cannot be resolved without a rule that is wrong for the language, STOP at it: write the exact conflict and
     what you tried in the progress file, and do not weaken the grammar to make it pass.
   - Do the same in `crates/tree-sitter-cotomet/grammar.js` (its `map` and `sequence` rules, and the value choice), and
     remove the sentence that `::meta{ a: 1 }` is described anywhere as a declaration, if it still is.
   - Update the module docs and the "known simplifications" in each `src/lib.rs` so that they say what the grammar does
     now, and delete a simplification the change removes.
2. **Queries.** Update `queries/*.scm` of both crates, and the editors' copies in `editors/` ONLY where they name a node
   that no longer exists or has been renamed. A highlight must still land on the same text as before.
3. **Generate.** Run, under `timeout 900` each: `cd crates/tree-sitter-tomet && npx -y tree-sitter-cli@0.26.12 generate`
   and the same in `crates/tree-sitter-cotomet`. This downloads a tool; if it cannot run (no network, denied, the
   command is not found), STOP, write exactly what you tried and what is left (the grammar edits are done and the
   generated files are not), and end. The author will run `just gen-treesitter` and tell the reviewer. Never edit
   `src/parser.c`, `src/grammar.json` or `src/node-types.json` by hand.
4. **Tests.** `cargo test -p tree-sitter-tomet -p tree-sitter-cotomet`, then `cargo test -p tomet-tests` (under
   `timeout 1500`). The Rust tests of the grammar crates that hold the old spelling are updated to say what is true now
   (a `{ }` at a value position is an ERROR in tree-sitter too; the tuple forms parse); a test whose subject is gone is
   rewritten to the new subject, not deleted and not ignored.
5. **`KNOWN_TS_ERRORS`.** Remove the four entries (and their two comments) whose fixtures now parse cleanly. A fixture
   that still errors for an older reason stays, with a comment that says that reason and not "the grammar follows". Do
   not remove or reword any other entry. Report the final count of entries before and after.

## Rules

- Do not touch: `crates/tomet-syntax-*`, `crates/tomet-semantics*`, `tomet-parser`, `tests/fixtures/`, `tests/ref/`,
  `tmtroot/`, `.writ.tmt` files, `.md` files, `packages/*`, anything not named in this brief. If a fixture needs a
  change to be readable by tree-sitter, the fixture is not changed: the grammar is.
- Build with `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0` in the environment of every cargo command. The disk is 5 GB
  and was full once: if a command says "No space left on device", STOP and write that in the progress file. Do not run
  `cargo clean`. Do not delete files outside the ones you made.
- Every command that can wait on something runs under `timeout`. Do not poll a long command in a loop.
- No commit, no `jj describe`, no `jj new`, no push, no `git` writes. Nothing outside this directory.
- Never delete a test to make a run pass, never mark one ignored, never add `_ =>` to make a `match` compile.
- Keep `.agents/tasks/tree-sitter-progress.md` current. Tick a step only after you ran its proof in this run and pasted
  the last lines of the real output. A tick without output is treated as a false claim. The reviewer re-runs
  everything, including the generation check, and reads each grammar comment against the rule under it.

## Done means

1. `cargo fmt --all -- --check`, `cargo check --workspace --all-targets`, `cargo test -p tree-sitter-tomet -p
   tree-sitter-cotomet`, and `cargo test --workspace --no-fail-fast` (under `timeout 1500`) pass. Paste the real last
   lines of each.
2. `src/parser.c`, `src/grammar.json` and `src/node-types.json` of both crates are what `generate` produces from the
   committed `grammar.js` (the generation was run in this run; paste its last lines).
3. No fixture of the four listed is in `KNOWN_TS_ERRORS` unless the progress file says which older reason keeps it there.
4. A search for `braced_map` finds nothing in `crates/tree-sitter-*/grammar.js`, the queries and `editors/`.
5. `jj diff --stat --no-pager`: list any changed file you would not expect from this brief, with the reason.

## Report

The survey (node-name users); for each of the four fixtures, where tree-sitter first errored before and what fixed it (or
why it stays); the rules you changed and the LR problems you met, with how each was resolved; every node name you
renamed or removed and every editor query you changed; the real test summary; the `KNOWN_TS_ERRORS` count before and
after; the open questions from your progress file; what you did not do or did not check. "I do not know" is an
acceptable answer. A claim that turns out false is not.
