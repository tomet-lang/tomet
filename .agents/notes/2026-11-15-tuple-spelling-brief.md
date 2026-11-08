# Brief: the tuple, step 2 -- named and unnamed entries read together

You are working in a jj workspace of the `tomet` repository (`.agents/workspaces/one-container`). Your current
directory is that workspace. Work only here.

## Before anything else: the author's rules

The author's own words, which stand above everything in this brief:

> 場当たりな実装、後方互換性は悪、現在の実装を言い訳にせず、常に、長期的な最美を根拠に考えること
> AGENTS.md はちゃんと読むこと

- Your FIRST action is to read `AGENTS.md` in this directory, whole. Write the first line of your progress file as
  "AGENTS.md read", and follow its "Core Philosophy & Principles".
- No makeshift code. A change is done when the code says what it means, not when it compiles: no flag that remembers
  an old spelling, no branch whose only reason is that the old code had it.
- Backward compatibility is not a goal and is not allowed.
- "It is implemented this way today" is never a reason. Where this brief is silent and the choice is visible to a
  user, do not invent a design: stop at that site, describe the choice in the progress file, and go on elsewhere.

## Where things stand

The commit under your working copy (`feat(syntax)!: one tuple in place of sequences and maps`) made the model:
`tomet_ast::Value::Tuple(Tuple)` is the one container, an ordered run of entries each with a name or without one;
`Value::Call(String, Tuple)` is a head and its fields; a nexus's `(args)` is a `Tuple`. Read the doc comments of
`Tuple`, `Value::Tuple` and `Value::Call` in `crates/tomet-syntax-ast/src/lib.rs`, and the module comment of
`crates/tomet-syntax-lowering/src/lower/connect.rs` (how two tuples merge), before you change anything.

What a tuple is (decided by the author): the fields handed to a head. `name(a, k: v)` is a head with its fields; a
bare `(a, k: v)` is the same thing with no head written. There is no list and no map as kinds of value.

The grammar was not changed by that commit. A nexus's `(args)` already takes named and unnamed entries together. A
`( )` at a value position still refuses a named entry, and a call's `( )` too. This step removes those two refusals.

## The order

1. **A `( )` at a value position takes named and unnamed entries together**, in any order, exactly as a nexus's
   `(args)` does: `tags: (a, b, sep: ",")`, `size: (w: 10, h: 20)`, `x: ((a, version: 3), b, c)`.
2. **A call's `( )` takes them too**: `icon: glyph(triangle, pkg: lucide)`, `use: vocab(a, version: 3)`. The value
   is `Value::Call(name, tuple)` with the entries as written. A call is NEVER turned into the entry `name: (…)` nor
   into the one-entry tuple `(name: (…))`.
3. **A key is text, and there is one key space** (decided by the author, 2026-11-15). tomet is a language of
   documents: a key is not a value and has no type. Quoting a key is spelling only (`"a": 1` is `a: 1`), so the key
   `"0"` is the key `0`. Leaving a key out is spelling too: an unnamed entry is an entry whose key was not written,
   and its key is its position among the unnamed entries of its tuple, in decimal with no sign and no leading zero
   (`0`, `1`, `12`). "A list is `(0: a, 1: b, 2: c)`."
   - Inside one tuple a key is written once. `("0": a, b)` is an error, `duplicate key `0``, at the second of the
     two as written, like any repeated name (`b` is the first unnamed entry, so its key is `0`). `("1": a, b)` is
     not an error. `("404": x, "500": y)` is a tuple with two keys and nothing more.
   - The AST keeps no spelling, so `(a, b)` and `("0": a, "1": b)` are the SAME tree: equal as values, printed the
     same, written to JSON the same. `("1": b, "0": a)` has the same keys in another order and is a different
     tuple (a tuple is ordered); it prints so that it reads back as itself.
   - The merge of two tuples (`merge_tuples` in `lower/connect.rs`) compares keys, so `("0": a)` and `(b)` clash at
     `0`. Today it keeps names and positions apart (`enum Key`); the key is one thing there.
   - JSON: a tuple is written as an array when its keys are `0`, `1`, … in the order written, with none missing
     (that is, when every entry could be written unnamed); empty is `{}`; anything else is an object keyed by the
     keys. The table of the commit below said "none named" and "mixed": this is the same table said by keys.
   - Naming positional arguments (`bind_positional`) takes the entries whose keys are `0`, `1`, … in order.
   Decide how `Tuple` holds this so that the rule is stated ONCE in `tomet-syntax-ast` and the parser's duplicate
   check, equality, the printer, the merge, the JSON writer and the positional binding all ask it. Two candidates:
   every entry holds its key and "could be written unnamed" is asked of the tuple; or the tuple normalizes when it
   is built, so that an entry is unnamed exactly when it could be written so. Pick the one that leaves no caller
   able to get it wrong, and say why in the doc comment of `Tuple`. Do not write the rule in several places.
   - Whether a bare integer may be written as a key (`0: a`, `12: 30`) is NOT decided. Do not change what the parser
     does with one today, and report what that is.

Everything else stays as the commit left it. In particular, do not start: removing `{ }` from value positions (a
tuple whose entries are all named is still printed `{k: v}` in this step, although `(k: v)` now reads as the same
value); addresses (`.0`); the evaluator's own value type; the constructors' move to `$`; anything about `@`, `#`,
`::`; tree-sitter.

## Sites

The compiler will not find these for you; search, and verify each path.

- **Lowering** (`crates/tomet-syntax-lowering/src/element.rs`): the refusal is the error
  "a sequence '(...)' holds values, not 'key: value' entries; write a map as '{...}'" (near line 693), and the place
  that lowers a call's arguments into `resolve_call`. The CST already holds `MAP_ENTRY` nodes there; check that it
  does for a call's arguments too, and if the CST parser refuses them, change it (`crates/tomet-syntax-cst-parser`).
  The literal constructors `string(…)`, `int(…)`, `float(…)` keep being resolved by the parser; a named field given to
  one of them is an error that names the constructor.
- **A type written as a value** (`crates/tomet-semantics-core/src/parse.rs`): its comment says a call has no named
  arguments. It has now: `type: list(string, length: 6)`. A type seat `<$list(string, length: 6)>` already reads named
  arguments of a type; read the call's named fields the same way, through the same code. If there is no shared code to
  go through, stop and describe what the two readers are, do not write a second one.
- **`::rule(allow: enum(a, b))`** (`crates/tomet-validator-rule/src/decode.rs`): a named field inside `enum(…)` is an
  error there.
- **The printer** (`crates/tomet-printer/src/value.rs`): a call prints `name(a, k: v)`, entries as written. A mixed
  tuple at a value position prints `(a, k: v)` (the code is there; it now reads back). Entries are printed in the
  order they were written; do not sort unnamed ones first.
- **The formatter and the editor model** work on the CST (`crates/tomet-formatter`, `crates/tomet-edit`): formatting
  the new fixture twice gives the same bytes, and where either one matches on "an entry of a sequence" against "an
  entry of a map", say what it does with a mixed `( )`.
- **Highlighting** (`crates/tomet-syntax-highlight`): a name before `:` inside a value's `( )` is a key, as it is in a
  nexus's args.
- **JSON** (`Serialize for Tuple` in `tomet-syntax-ast`, `value_to_json` in `tomet-semantics`): as item 3 says it.
  It cannot write one key twice, because a tuple cannot hold one twice. `Deserialize`: an array gives the keys `0`,
  `1`, …; an object its keys, so `{"0": "a"}` and `["a"]` read as the same tuple.
- **The evaluator's bridge** (`crates/tomet-semantics-expr/src/value.rs`) and **`serde_cotomet`**: no change of rule;
  add a test for a mixed tuple read from source going through each.

## Fixtures and references

- The corpus is frozen: do not edit an existing fixture. ADD `tests/fixtures/syntax/tuple.tmt` holding every form of
  "The order" that is not an error, each once, small. Generate its references as `tests/README.md` says.
- Tree-sitter's grammar does not know these forms and is not to be touched. The `corpus` test of `tomet-tests` will
  report the new fixture; record it in `KNOWN_TS_ERRORS` (it is in `tests/src`), with a comment saying the grammar
  follows in the last step of the task.
- Errors are tested in the crates (`tomet-syntax-lowering/src/tests.rs`, `tomet-syntax-ast-parser/src/tests/`), with
  the exact message and position, as the merge tests in `tomet-syntax-lowering/src/tests.rs` do.
- No existing reference under `tests/ref` may change, and `tests/SYNTAX.md` only by the new fixture's cases. A change
  you cannot put under that heading: stop, do not accept it, report it.

## Rules

- Read first: `AGENTS.md`, `CONTRIBUTING.md`, the root `.writ.tmt` (`crate-layering`), `tests/README.md`, and the `//!`
  of every crate you touch.
- Do not touch: `crates/tree-sitter-tomet`, `crates/tree-sitter-cotomet`, existing files under `tests/fixtures/`,
  anything under `tmtroot/` (the file `tmtroot/dev/design/one-container.tmt` in the working copy is out of date and is
  not yours), `.writ.tmt` files, `.md` files, `packages/*`.
- Build with `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0` in the environment of every cargo command. If `cargo` is
  not on the path, use `nix develop -c`. A crate's directory name and its package name differ: read `name` in its
  `Cargo.toml` before `cargo check -p`.
- No commit, no `jj describe`, no `jj new`, no push, no `git` writes. Nothing outside the current directory.
- Keep `.agents/tasks/tuple-spelling-progress.md` current. Tick a step only after you ran the command that proves it,
  in this run, and paste the last lines of its real output under the step. A tick without output is treated as a
  false claim.
- Never add `_ =>` to make a `match` compile, never delete a test or mark it ignored. If you cannot make something
  work, leave it failing and write it down.
- If you are running out of time or quota, stop at the end of a crate, with the progress file saying exactly where.

## Done means

1. `cargo fmt --all -- --check`, `cargo check --workspace --all-targets` and `cargo test --workspace --no-fail-fast`
   pass. Paste the real final lines.
2. The text "holds values, not 'key: value' entries" is gone from the workspace.
3. `git status --porcelain -- crates/tree-sitter-tomet crates/tree-sitter-cotomet packages tmtroot` shows nothing but
   the memo that was already there, and `git status --porcelain -- tests/fixtures tests/ref` shows only added files.
4. There is one definition of an entry's key, and the duplicate check, equality, the printer, the merge, the JSON
   writer and the positional binding use it. `(a, b)` and `("0": a, "1": b)` parse to equal trees, print the same
   text and give the same JSON: there is a test for each of the three.
5. Doc comments say what is true now (`Value::Call` no longer "positional", the lowering functions, `parse.rs`).
6. `jj diff --stat --no-pager`: list any changed file you would not expect from this brief, with the reason.

## Report

End with: what changed, by crate; what the formatter and the editor model do with a mixed `( )`; how the type
reader's named arguments were done; every added reference; the real test summary; the open questions from your
progress file; what you did not do or did not check. "I do not know" is an acceptable answer. A claim that turns out
false is not.
