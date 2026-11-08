# Brief: the tuple, step 3 -- the evaluator's one tuple, and addresses by position

You are working in a jj workspace of the `tomet` repository (`.agents/workspaces/one-container`). Your current
directory is that workspace. Work only here.

## Before anything else: the author's rules

The author's own words, which stand above everything in this brief:

> 場当たりな実装、後方互換性は悪、現在の実装を言い訳にせず、常に、長期的な最美を根拠に考えること
> AGENTS.md はちゃんと読むこと

- Your FIRST action is to read `AGENTS.md` in this directory, whole. Write the first line of your progress file as
  "AGENTS.md read", and follow its "Core Philosophy & Principles".
- No makeshift code. A change is done when the code says what it means, not when it compiles: no adapter that turns a
  tuple back into "the list" or "the map" so that old logic can run unchanged, no flag that remembers an old kind, no
  branch whose only reason is that the old code had it.
- Backward compatibility is not a goal and is not allowed. A spelling, a method name or an error text that the new
  design makes wrong is removed, not kept beside the new one.
- "It is implemented this way today" is never a reason. Where this brief is silent and the choice is visible to a
  user, do not invent a design: stop at that site, describe the choice in the progress file, and go on elsewhere.
- Never write a rule in several places. If you find yourself copying the same rule into a second function, stop and
  put it in one place that both ask.

## Where things stand

Two commits sit under your working copy: `feat(syntax)!: one tuple in place of sequences and maps` and
`feat(syntax): read named and unnamed entries together in a value's tuple and a call`. Read the doc comments of
`Tuple`, `TupleEntry`, `Value::Tuple` and `Value::Call` in `crates/tomet-syntax-ast/src/lib.rs` first, then the module
comment of `crates/tomet-syntax-lowering/src/lower/connect.rs`. What they say, in short:

- A tuple is an ordered run of entries, each with a name or without one: the fields handed to a head. A key is text
  and there is one key space. An unnamed entry's key is its position among the UNNAMED entries, in decimal. Quoting a
  key and leaving it out are spelling only: `(a, b)` and `("0": a, "1": b)` are the same tuple. A key is held once.
- The tuple keeps that by how it is built: one checked constructor (`Tuple::try_from_entries`); no entry stores a
  position; `get`, `set`, `rename`, `remove`, `map_values`, `keyed` are the ways to read and change it.

The evaluator (`crates/tomet-semantics-expr`) still has its own three kinds, `EvalValue::Seq`, `EvalValue::Map` and
`EvalValue::Call(String, Vec<EvalValue>)`, behind a bridge (`from_ast` / `to_ast` in `value.rs`). Positional arguments
get their names from `bind_positional` (`crates/tomet-semantics-core/src/positional.rs`), which REWRITES a tuple:
the unnamed entry that takes a slot name becomes a named entry and the unnamed ones after it move down by one. This
step replaces both.

## Decided by the author, 2026-11-16

1. **The position is the address, and it survives any schema.** `x.0` is the same entry whether or not a vocabulary
   is loaded. A signature's slot name (`x.target` for `@link(tm:foo)`) is an ALIAS: it exists only where the schema is
   known, and it never changes the tree. Binding a signature must stop rewriting a tuple: it becomes a view that
   answers "which entry does this name mean".
2. **`b.0 == b` for scalars.** The position-0 entry of a scalar is the scalar itself. Scalars are `String`, `Int`,
   `Float`, `Number` and `Bool`. NOT tuples, NOT calls, NOT `Null`, not the other kinds: they keep the members they
   have. A string's `.0` is the string, not its first character. Reason: `::use((a, version: 3), b, c)` has `use.0.0`
   for `a` and must have `use.1.0` for `b`, so that an address does not depend on how an item was spelled.
3. **Ordered operations act on the positional entries only; named entries are fields.** `len`, `join`, `contains`,
   `map`, `filter`, `sort` see the unnamed entries, in order. A named entry is reached by `.name`. The other half is
   asked for explicitly.

Decided by Claude, under the author's leave for implementation-level choices (report each one you hit a reason to
doubt, do not silently change it):

- `.positional()` is the tuple of the unnamed entries (renumbered from 0), `.named()` the tuple of the named ones.
- On a tuple, `contains(x)` is membership among the positional values; key membership is `has(key)` (it was
  `contains(key)` on a map). `get(key, default)` stays and takes a name or a position in decimal.
- `item(n)` on a tuple stays "the n-th entry as written", named or not. `.0` is "the entry whose key is 0".
- The type name in messages and in `UnknownMember { on, .. }` is `"tuple"` for a tuple, `"call"` for a call.
- The tuple type becomes generic over its value, `Tuple<V = Value>` and `TupleEntry<V = Value>`, so that the
  evaluator's tuple is the same type with the same key rules. The defaults keep every existing `Tuple` as it is.

## The order

1. **Generic `Tuple`** in `tomet-syntax-ast`. No rule is copied for the evaluator. `Serialize` / `Deserialize` stay
   for `Tuple<Value>`. All existing tests pass unchanged.
2. **Aliases instead of rewriting.** Replace `bind_positional` and `Declaration::bound_args` (and the equivalent for
   body elements and list markers in `crates/tomet-semantics/src/positional.rs`) with a view over a tuple and a
   signature:
   - a slot name means the unnamed entry that would take it, taking the slots left to right, skipping a slot that an
     entry already has as a NAME; an unnamed entry past the last slot has no alias;
   - the tuple is not changed, and printing, equality, JSON, iteration and addresses of the tuple are not touched;
   - every reader that asks for a slot by name (`read.rs`, `check.rs`, `vars.rs`, the element readers of
     `tomet-semantics`, `parts.rs`) asks the view; a place that lists entries with their slot names (the header
     listing in `read.rs`) asks the view for each entry's alias.
   The function `recover_target_scheme_entry` in `tomet-semantics/src/positional.rs` renames a NAMED entry
   (`tm:foo` read as `tm: foo`); it is not about positions. Leave its behaviour, say in the report what it does and
   why it is not an alias.
3. **One `EvalValue::Tuple`**, `EvalValue::Seq` and `EvalValue::Map` gone, `EvalValue::Call(String, Tuple<EvalValue>)`.
   `from_ast` and `to_ast` become a plain map over the values. Every user of `Seq` / `Map` (about 14 files: the
   compiler lists them; `vars.rs`, `methods.rs`, `value.rs`, `tomet-vault/src/resolve.rs`, `tomet-load/src/resolver.rs`,
   `tomet-transform/src/seats.rs`, the LSP's `hover.rs` are the big ones) is rewritten to ask the tuple what it wants
   to know, by the rules above. No wildcard arm to make one compile.
4. **Members.** `x.key` on a tuple reads the entry whose key is `key`: a name, or a position in decimal. `x.0` on a
   scalar is the scalar; `x.k` on a scalar for any other `k` is the error it is today. The expression language must
   read `use.0.0` and `x.1.0` as TWO steps, not as the number `0.0`: check how a digit after a dot is lexed and
   fix it where it is decided, with a test that fails if `0.0` is read as a float after a dot. A float literal in
   value position (`1.5`, `x == 0.0`) is not affected: test that too.
5. **Methods** by the rules above. Search the repository's `.tmt` and `.ctmt` files and the tests for a `contains`
   on a map (a key membership) and for `.len()` / `.join()` on something that may hold named entries; list what you
   find in the report, and rewrite the ones in the repository to `has` where they mean a key. Do not edit
   `tests/fixtures/` (frozen) or `tmtroot/`: if one of those needs a change, list it and stop at it.
6. **Addresses** (`tmtroot/spec/expressions/address.expr.tmt` is the spec, in `tmtroot/`: do not edit it, it is
   rewritten in one pass later). `#x.0` reads the entry whose key is 0 in the data of the element or declaration with
   the id `x`; `#x.target` reads the same entry when the signature of that element is known and gives it that slot.
   Both come out of the one view of step 2. Where the address code resolves a data key today
   (`parts.rs`, `address.rs` in `tomet-semantics-expr`, the vault's resolver), a position works the same way.

Not in this step, do not start: prose addresses (a block of `[ ]` by position); removing `{ }` from value positions;
the design memo; any change to what the parser accepts; tree-sitter; the type seat; `at(key, default:)`.

## Tests

- In the crates, with exact messages: a signature alias and the same entry by position give the same value, with and
  without the vocabulary; an alias never shows up in the tuple's printed text, JSON or equality; the slot an entry
  already has by NAME is skipped; `b.0 == b` for each scalar and not for a tuple, a call, `Null`; `use.0.0` and
  `use.1.0` over `((a, version: 3), b, c)`; `len`/`join`/`contains`/`map` over `(a, b, sep: ",")` see two items and
  `.sep` reads `","`; `.positional()` and `.named()`; `has`; `get` with a position; the old name of a method that
  was removed is an error with the right message.
- A mixed tuple read from source goes through the evaluator and back (`to_ast`) as the same tuple.
- No existing reference under `tests/ref` may change. If one does, stop, do not accept it, and report it with the
  line that changed.
- A test whose subject no longer exists (a `Seq`-specific or `Map`-specific one) is rewritten to the new subject, not
  deleted and not ignored.

## Rules

- Read first: `AGENTS.md`, `CONTRIBUTING.md`, the root `.writ.tmt` (`crate-layering`), `tests/README.md`, and the `//!`
  of every crate you touch. A crate's directory name and its package name differ: read `name` in its `Cargo.toml`
  before `cargo check -p`.
- Do not touch: `crates/tree-sitter-tomet`, `crates/tree-sitter-cotomet`, existing files under `tests/fixtures/`,
  anything under `tmtroot/`, `.writ.tmt` files, `.md` files, `packages/*`.
- Build with `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0` in the environment of every cargo command (the disk is
  5 GB). If `cargo` is not on the path, use `nix develop -c`.
- No commit, no `jj describe`, no `jj new`, no push, no `git` writes. Nothing outside the current directory.
- Keep `.agents/tasks/tuple-eval-address-progress.md` current. Tick a step only after you ran the command that proves
  it, in this run, and paste the last lines of its real output under the step. A tick without output is treated as a
  false claim. The last run's report was wrong about the test results, the formatting and a file it said it had
  changed; the reviewer re-runs everything.
- Never add `_ =>` to make a `match` compile, never delete a test or mark it ignored. If you cannot make something
  work, leave it failing and write it down.
- Go by layer, bottom up, following `.writ.tmt`; check each crate on its own before the whole workspace.
- If you are running out of time or quota, stop at the end of a crate, with the progress file saying exactly where.

## Done means

1. `cargo fmt --all` has been run and `cargo fmt --all -- --check`, `cargo check --workspace --all-targets` and
   `cargo test --workspace --no-fail-fast` pass. Paste the real final lines of each.
2. This finds nothing in `crates apps bindings benches tests/src` (outside `target`):
   `EvalValue::(Seq|Map)\b|fn bind_positional|fn bound_args`
3. `git status --porcelain -- crates/tree-sitter-tomet crates/tree-sitter-cotomet packages tmtroot tests/fixtures`
   shows nothing but the design memo that was already there, and `git status --porcelain -- tests/ref` shows nothing.
4. Whether a tuple's entries, printed text, JSON and equality are the same as before for every source in the
   repository: the existing snapshot tests say so; paste their result.
5. Doc comments say what is true now: the `//!` of the crates you changed, `EvalValue`, the view of step 2, the
   evaluator's methods. None describes the old three kinds or a rewriting binding.
6. `jj diff --stat --no-pager`: list any changed file you would not expect from this brief, with the reason.

## Report

End with: what changed, by crate; the API of the view of step 2 and why it has that shape; how step 4 found and fixed
the `0.0` lexing; what you found for step 5 (every `contains` on a map, every place that changed); the real test
summary; the open questions from your progress file; what you did not do or did not check. "I do not know" is an
acceptable answer. A claim that turns out false is not.
