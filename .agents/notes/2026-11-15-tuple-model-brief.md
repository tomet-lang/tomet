# Brief: the tuple -- one data container in the AST

You are working in a jj workspace of the `tomet` repository (`.agents/workspaces/one-container`). Your current
directory is that workspace. Work only here.

## The order

`tomet_ast::Value` has two kinds for what is one thing: `Seq` and `Map`. Replace them by ONE container, the **tuple**:
an ordered run of entries, each entry with a name or without one.

What a tuple is (decided by the author, 2026-11-15): the fields handed to a head. `name(a, k: v)` is a head with its
fields; a bare `(a, k: v)` is the same thing with no head written -- an object without a name. There is no list as a
kind of value: "any number of the same thing" is something a schema says about a place, not something a value is.

So:

- `Value::Seq` and `Value::Map` go. `Value::Tuple(Tuple)` replaces both.
- `Value::Call(name, args)` STAYS a kind of its own: a head and its fields. Only the type of its fields changes, from
  `Vec<Value>` to `Tuple`. It is never turned into an entry `name: (…)`, nor into a one-entry tuple `(name: (…))`. A
  run before this one did that and was thrown away for it.
- A nexus's `(args)` (`Data.args`) becomes `Option<Tuple>`, not a `Value`.

This run has two parts. Part 1 is the model: the grammar accepts exactly what it accepts today, and every source means
what it meant, except for the changes this brief names. Part 2 is the merge rule. Finish and record part 1 before
starting part 2.

Not in this run, do not start them: reading named and unnamed entries together in a `( )` at a value position; named
fields inside `name(…)`; addresses (`.0`); the constructors' move to `$`; removing `{ }` from value positions; anything
about `@`, `#`, `::`.

## Rules

- Read first: `AGENTS.md`, `CONTRIBUTING.md`, the root `.writ.tmt` (`crate-layering`), `tests/README.md`, and the `//!`
  of every crate you touch.
- `tmtroot/dev/design/one-container.tmt` is in the working copy, uncommitted. It is out of date in several places and
  is NOT the target; this brief is. Do not edit it and do not follow it where it differs.
- No backward compatibility: no shim, no alias, no adapter that rebuilds `Seq` / `Map` for callers, no `as_seq()` /
  `as_map()` kept so old code compiles. A caller is rewritten to ask the tuple what it wants to know.
- The existing implementation is not a reason for a design choice. Where this brief is silent and the choice is visible
  to a user, do NOT invent: keep what a source that exists today produces today, write the question in your progress
  file, and go on with what does not depend on it.
- Do not touch tree-sitter at all: nothing under `crates/tree-sitter-tomet` or `crates/tree-sitter-cotomet`. If a test
  there fails because of your change, report it; do not edit it.
- Do not edit `.md` files by hand, anything under `tmtroot/`, `.writ.tmt` files, anything under `tests/fixtures/` (the
  corpus is frozen), or `packages/*`.
- Build with `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0` in the environment of every cargo command (the disk is
  5 GB). If `cargo` is not on the path, use `nix develop -c`.
- No commit, no `jj describe`, no `jj new`, no push, no `git` writes. Nothing outside the current directory.
- Keep `.agents/tasks/tuple-model-progress.md` current: tick a step when it is done, not at the end. It must be enough
  to resume from. Questions and things you were unsure of go there as they come up.
- This is a large change (about 100 files match `Value::(Seq|Map|Call)`). Go by layer, bottom up, following
  `.writ.tmt`; use `cargo check -p <crate>` as you go and the whole workspace at the end. The compiler is the list of
  sites: once the two variants are gone, every reader is an error until it is rewritten. Do not silence an error with a
  wildcard arm.

## The type

In `tomet-syntax-ast`: `Tuple`, its entries `TupleEntry { name: Option<String>, value: Value }` (the author may rename
them; do not spend time on names). Fields private, with only the methods the readers turn out to need -- expected:
entry by name, n-th unnamed entry, n-th entry as written, iteration over all / the named / the unnamed, "has no named
entry", "has no unnamed entry", length, building and removing. Add nothing that no caller uses. A repeated name in one
tuple is the parser's error, as a repeated key is today.

There is no sentinel key: an unnamed entry has `name: None`. `POSITIONAL_ENTRY_KEY` (`""`) goes, everywhere (it is
defined twice today: in `tomet-syntax-lowering` and in `tomet-semantics-core`).

`Group` and `Entry` (the entries of a nexus's `{ … }` group) stay as they are. Where they convert to and from a
`Value::Map` (`Group::from_map`, `Group::as_data`), they convert to and from a tuple.

## Part 1: sites whose meaning depends on the kinds -- each one, what it becomes

The compiler shows every site. These are the ones where a mechanical rewrite is wrong. The paths are starting points;
verify them.

1. **A nexus's args** (`tomet-syntax-lowering/src/element.rs`, `lower_paren_value`). Today `()` is an empty map, `(x)`
   is unboxed to the bare value `x`, and `(x, y)` / `(x, k: v)` is a map whose positional entries have the key `""`.
   After: always a tuple. `()` is the empty tuple. `(x)` is a tuple with one unnamed entry -- **no unboxing**. Every
   reader that expected a bare scalar in `args` (`::id(x)` in `lower/connect.rs` is one) asks for the first unnamed
   entry instead.
2. **Naming positional arguments** (`tomet-semantics-core/src/positional.rs`, `bind_positional`, and its callers in
   `tomet-semantics` and `tomet-semantics-declaration`). After: over one tuple, each unnamed entry takes, left to
   right, the next signature name not already present as a name; an unnamed entry past the last name stays unnamed;
   nothing is dropped. If a `Seq` argument's extra items are dropped today and a test relies on it, report it.
3. **A call in the data space** (`resolve_call` in lowering). The literal constructors `string(…)`, `int(…)`,
   `float(…)` keep being resolved by the parser, exactly as now. Any other `name(a, b)` stays a `Value::Call` whose
   fields are a tuple of unnamed entries.
4. **A type written as a value** (`tomet-semantics-core/src/parse.rs`): `type: list(string)` is still a `Value::Call`;
   its positional arguments are the unnamed entries of its tuple. Nothing else changes here.
5. **What a declared type admits** (`tomet-semantics-core/src/admits.rs`): where a sequence was required, a tuple with
   no named entry (an empty one included). Where a map was required, a tuple with no unnamed entry (an empty one
   included). The words in messages: no named entry and at least one entry -> "a list"; no unnamed entry and at least
   one entry -> "a map"; empty -> "an empty tuple"; mixed -> "a list with named entries". Use the same wordings
   wherever a value is described to a user.
6. **`::rule(allow: …)`** (`tomet-validator-rule/src/decode.rs`): takes a tuple of unnamed entries, or the call `enum`
   holding one. The same two inputs as today.
7. **The printer** (`tomet-printer`). It writes from the tree alone, and the grammar is unchanged, so:
   - a nexus's args: `(…)`, unnamed entries as values and named ones as `k: v`, as now; empty is `()`; one unnamed
     entry is `(x)`;
   - a value that is an empty tuple: `()`;
   - no named entry: `(a, b)`;
   - no unnamed entry: `{k: v, …}` (the spelling of this run);
   - a call: `name(a, b)`, as now;
   - mixed, which no source of today produces at a value position: `(a, k: v)`. Say in your report whether anything can
     put a mixed tuple at a value position in this run; the grammar cannot read that text back yet.
   One change of printed text for sources that exist is intended: a value `{}` now prints `()`. Every other change of
   printed text is a bug.
8. **The evaluator's values** (`tomet-semantics-expr`, `EvalValue`). `EvalValue::Seq`, `EvalValue::Map` and
   `EvalValue::Call` STAY (a later run redoes this type). The bridge from and to the AST: no named entry (the empty
   tuple included) <-> `Seq`; otherwise <-> `Map`, an unnamed entry taking its position among the unnamed ones as its
   key (`"0"`, `"1"`). Say in your report which expressions can observe an empty tuple or a mixed one.
9. **`serde_cotomet`** (`de.rs`, `print.rs`): the receiving type decides. `deserialize_seq` / tuple: the unnamed
   entries. `deserialize_map` / struct: the named entries. `deserialize_any`: by shape, as in "JSON" below. A call keeps
   the error it has today. Add tests for each of these, the empty tuple into a `Vec` and into a map among them.
10. **The rest that match on the kinds**: `cotomet` (`find_not_data`), the LSP (`apps/lsp/src/hover.rs`),
    `tomet-syntax-tree` (`walk.rs`; `as_seq` / `as_map` go if they exist), `tomet-semantics/src/flatten.rs`,
    `tomet-search/src/structural.rs`, `tomet-workspace-indexer`, `tomet-transform`, `tomet-semantics-eval`, the
    converters' flattening of args into attributes, `tests/src/syntax_report.rs`.

## JSON (in both `Value`'s own `Serialize` and `tomet-semantics`' `value_to_json`)

Decided by the author, 2026-11-15. The tuple is an object at bottom; an array is how it is shown when every entry is
positional.

| tuple | JSON |
|---|---|
| empty | `{}` |
| at least one entry, none named | array |
| at least one entry, none unnamed | object |
| mixed | object; an unnamed entry's key is its position among the unnamed ones, `"0"`, `"1"` |

`Deserialize`: an array gives unnamed entries, an object named ones, `{}` and `[]` both the empty tuple. A call keeps
the JSON it has today, its fields written by the table above. Update `bindings/js/index.d.ts` to what is really
produced, and the Python and Java bindings where they name the shapes.

An empty sequence that printed `[]` in JSON before now prints `{}`. That is intended; list every reference it changes.

## Part 2: the merge rule

Only after part 1 passes "Done means" 1 to 5 and you have written the test summary into the progress file.

Decided by the author: there is ONE way two tuples merge, and a clash is an error.

- Every entry has a key: a named entry its name, an unnamed entry its position among the unnamed ones (`"0"`, `"1"`).
- Two tuples merge key by key. A key on one side only is kept. A key on both sides: if both values are tuples, they
  merge by this same rule; anything else is an error -- two scalars, two calls, a scalar and a tuple -- **also when the
  two values are equal**.
- The order of the result: the entries of the first tuple as written, then the new ones of the second.

Today's rule is different (`merge_values_inner` in `tomet-syntax-lowering/src/lower/connect.rs`: the direct value wins
a clash, two sequences are joined without repeats). Replace it. The error is a parse error at the second writer, and
says which key was written twice and where the first one is.

Then:

- Run the whole test suite and `tomet check` over the repository's own documents.
- Do NOT edit a fixture, a reference or a document to make the new error go away. List every test and every source
  that now fails with this error, with the two places that clash. That list is the result of part 2; a red suite with a
  complete list is an acceptable end.
- Report every other place in the workspace that merges two values (the crate `tomet-merge` among them) and what rule
  it uses. Do not change those.

## Done means

1. This finds nothing in `crates apps bindings benches tests/src` (outside `target`):
   `(^|[^A-Za-z_])Value::(Seq|Map)\b|POSITIONAL_ENTRY_KEY|fn as_seq\b|fn as_map\b`
2. `git status --porcelain -- tests/fixtures crates/tree-sitter-tomet crates/tree-sitter-cotomet packages tmtroot`
   shows nothing but the memo that was already there.
3. `cargo check --workspace --all-targets` and `cargo test --workspace` pass after part 1 (with the two environment
   variables). Paste the real final lines. A failure is reported as a failure, with the test's name.
4. Snapshot references (`tests/ref`) after part 1: the `.html`, `.md`, `.typ` references do not change. The
   `.printed.tmt` and `.pandoc.roundtrip.tmt` references may change only by `{}` -> `()` at a value position. JSON
   references and `tests/SYNTAX.md` may change only by the empty tuple (`[]` -> `{}`), by `(x)` in a nexus's args now
   being a one-entry tuple, and by how a tuple is shown in the tree dump. Regenerate as `tests/README.md` says, then
   **list every changed reference with the kind of change**, and for the printed ones quote each changed line. A change
   you cannot put under one of those headings: stop, do not accept it, report it.
5. Doc comments say what is true now: the `//!` of `tomet-syntax-ast` and the comments on `Value`, `Data`, `Group`, the
   positional binding, the printer's rules. None describes a sequence and a map as two kinds.
6. Part 2: the list described there, and the test summary after it.
7. `jj diff --stat --no-pager`: list any changed file you would not expect from this brief, with the reason.

## Report

End with: what changed, by layer; the answers this brief asked for (items 2, 7, 8 and part 2); every changed reference
and why; the real test summaries after part 1 and after part 2; the open questions from your progress file; what you
did not do or did not check. "I do not know" is an acceptable answer. A claim that turns out false is not.
