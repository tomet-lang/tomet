# Brief: one-container, stage 2 -- one container in the AST (no spelling changes)

You are working in a jj workspace of the `tomet` repository. Your current directory is that workspace. Work only here.

## The order

`tomet_ast::Value` has three kinds for what is one thing: `Seq`, `Map`, `Call`. Replace them by ONE container: an
ordered run of entries, each entry with a name or without one. A list is a container whose entries all have no name; a
map is one whose entries all have a name; a mix is legitimate. `name(a, b)` in the data space is not a kind of value: it
is a container with one named entry `name` whose value is the container `(a, b)`.

A nexus's `(args)` (`Data.args`) becomes that same container type, not a `Value`.

The design is `tmtroot/dev/design/one-container.tmt` (Japanese; sections 2 to 5 and 9). Read all of it first. It is the
target. This stage implements the **model** only: the grammar accepts exactly what it accepts today, and every source
means what it meant. The new spellings, the addresses, the constructors' move to `$`, the removal of `{ }` from values
and the tree's uniform JSON are later stages. Do not start them.

## Rules

- Read first: `AGENTS.md`, `CONTRIBUTING.md`, the root `.writ.tmt` (`crate-layering`), `tests/README.md`, the design
  memo, and the `//!` of every crate you touch.
- No backward compatibility: no shim, no alias, no adapter that rebuilds the old three kinds for callers, no
  `as_seq()` / `as_map()` kept so old code compiles. A caller is rewritten to ask the container what it wants to know.
- The existing implementation is not a reason for a design choice. Where this brief and the memo are silent and the
  choice is visible to a user, do NOT invent: keep what a source that exists today produces today, write the question
  in your progress file, and go on with what does not depend on it.
- Do not touch tree-sitter at all: nothing under `crates/tree-sitter-tomet` or `crates/tree-sitter-cotomet`, no
  `grammar.js`, no generated parser, no `queries/`. Tree-sitter is the last stage of the whole task. If a test there
  fails because of your change, report it; do not edit it.
- Do not edit `.md` files by hand, spec pages under `tmtroot/`, `.writ.tmt` files, or anything under `tests/fixtures/`
  (the corpus is frozen; this stage has no permission to change it). `tmtroot/dev/design/one-container.tmt` is in the
  working copy uncommitted: it is not yours, do not edit it.
- Do not touch `packages/*` (their types are already behind the AST; that is another task). `bindings/js/index.d.ts`
  follows the JSON you produce (see "JSON").
- Build with `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0` in the environment of every cargo command (the disk is
  small). If `cargo` is not on the path, use `nix develop -c`.
- No commit, no `jj describe`, no `jj new`, no push, no `git` writes. Nothing outside the current directory.
- Keep `.agents/tasks/one-container-2-progress.md` current: tick a step when it is done, not at the end. It must be
  enough to resume from. Questions and things you were unsure of go there as they come up.
- This is a large change (about 100 files). Go by layer, bottom up, following `.writ.tmt`; use `cargo check -p <crate>`
  as you go and the whole workspace at the end. The compiler is the list of sites: once the three variants are gone,
  every reader is an error until it is rewritten. Do not silence an error with a wildcard arm.

## The type

In `tomet-syntax-ast`. Name it `Container`, its entries `ContainerEntry { name: Option<String>, value: Value }` (the
author may rename them later; do not spend time on names). `Value::Container(Container)` replaces the three variants.
Fields private, with only the methods the readers turn out to need -- expected: entry by name, n-th unnamed entry, n-th
entry as written, iteration over all / the named / the unnamed, "all unnamed", "all named", length, building and
removing. Add nothing that no caller uses. A repeated name in one container is the parser's error, as a repeated key is
today.

There is no sentinel key any more: an unnamed entry has `name: None`. `POSITIONAL_ENTRY_KEY` (`""`) goes, everywhere.

`Group` and `Entry` (the entries of a nexus's `{ … }` group: `Pair`, `Element`, `Hole`) stay as they are in this stage.
Where they convert to and from a `Value::Map` (`Group::from_map`, `Group::as_data`), they convert to and from a container.

## Sites whose meaning depends on the three kinds -- each one, what it becomes

The compiler will show you every site. These are the ones where a mechanical rewrite is wrong; the paths are starting
points, verify them.

1. **A nexus's args** (`tomet-syntax-lowering/src/element.rs`, `lower_paren_value`). Today `()` is an empty map, `(x)`
   is unboxed to the bare value `x`, and `(x, y)` / `(x, k: v)` is a map whose positional entries have the key `""`.
   After: always a container. `(x)` is a container with one unnamed entry -- **no unboxing**. Every reader that expected
   a bare scalar in `args` asks for the first unnamed entry instead.
2. **Naming positional arguments** (`tomet-semantics-core/src/positional.rs`, `bind_positional`, and its callers in
   `tomet-semantics` and `tomet-semantics-declaration`). After: over one container, each unnamed entry takes, left to
   right, the next signature name not already present as a name; an unnamed entry past the last name stays unnamed;
   nothing is dropped. (Today a `Seq` argument is zipped and the items past the signature are dropped: that drop goes.
   If a test relied on it, report it.)
3. **A call written in the data space** (`resolve_call` in lowering). The literal constructors `string(…)`, `int(…)`,
   `float(…)` keep being resolved by the parser in this stage, exactly as now. Any other `name(a, b)` becomes the
   one-entry container.
4. **A type written as a value** (`tomet-semantics-core/src/parse.rs`, `parse_type_spec`): `type: list(string)` was a
   `Call`. After: a container with exactly one entry, named, whose value is a container, is read as that type name with
   the inner container's unnamed entries as its positional arguments. A plain string is still a type name. Anything else
   is still `NotAType`.
5. **What a declared type admits** (`tomet-semantics-core/src/admits.rs`): `type: list` admits a container whose entries
   are all unnamed (an empty one included). Where a map was required, a container whose entries are all named (an empty
   one included). The words in messages: all unnamed or empty -> "a list"; all named -> "a map"; mixed -> "a list with
   named entries". Use the same three wordings wherever a value is described to a user (`EvalValue`'s type name too).
6. **`::rule(allow: …)`** (`tomet-validator-rule/src/decode.rs`): takes a container of unnamed entries, or the one-entry
   container named `enum` holding one (what `enum(a, b)` now is). Same two inputs as today.
7. **Merging** (`tomet-syntax-lowering/src/lower/connect.rs`, `merge_values` and friends; `tomet-merge` if it matches on
   the kinds): keep today's result for today's inputs -- two all-named containers merge name by name, an all-unnamed
   one replaces. For a mixed one the memo has no rule: named entries merge by name and the unnamed entries of the
   incoming side replace the unnamed entries of the other; say in your report where this is reachable.
8. **The printer** (`tomet-printer`). It writes from the tree alone, and the grammar is unchanged in this stage, so:
   - a nexus's args: `(…)`, unnamed entries as values and named ones as `k: v`, as now; empty is `()`;
   - a value that is an empty container: `()`;
   - all entries unnamed: `(a, b)`;
   - exactly one entry, named, whose value is a container with only unnamed entries (or empty), and whose name is not
     `string`, `int` or `float`: the short form `name(a, b)`;
   - otherwise all entries named: `{k: v, …}` (the spelling of this stage; it leaves in a later one);
   - mixed, which no source of today produces at a value position: `(a, k: v)`.
   Two of these change printed text for sources that exist: a value `{}` now prints `()`, and a one-key map whose value
   is a list (`{tags: (a, b)}`) now prints `tags(a, b)`. Both are intended. Every other change of printed text is a bug.
9. **The evaluator's values** (`tomet-semantics-expr`, `EvalValue`). `EvalValue::Call` goes. `EvalValue::Seq` and
   `EvalValue::Map` STAY in this stage (how methods treat named entries is undecided; a later stage redoes this type).
   The bridge from and to the AST: all unnamed (or empty) <-> `Seq`; otherwise <-> `Map`, an unnamed entry taking its
   position among the unnamed ones as its key (`"0"`, `"1"`). Going back from a `Map` gives named entries. Say in your
   report which expressions can observe a mixed container.
10. **`serde_cotomet`** (`de.rs`, `print.rs`): the receiving type decides. `deserialize_seq` / tuple: the unnamed entries.
    `deserialize_map` / struct: the named entries. `deserialize_enum`: a one-entry named container is the externally
    tagged variant (so `upstream(a, b)` reads into `enum Source { Upstream(String, String) }`); a bare string is a unit
    variant. `deserialize_any`: by shape, as in "JSON" below. The error "call syntax has no serde equivalent" goes. Add
    tests for each of these.
11. **`cotomet`** (`find_not_data`), **the LSP** (`hover.rs` renders a value), `tomet-syntax-tree` (`walk.rs`, and
    `value.rs`'s `as_seq` / `as_map`, which go), the converters' flattening of args into attributes.

## JSON (this stage: by shape, in both `Value`'s own `Serialize` and `tomet-semantics`' `value_to_json`)

| container | JSON |
|---|---|
| all entries unnamed, or empty | array |
| all entries named | object |
| mixed | object; an unnamed entry's key is its position among the unnamed ones, `"0"`, `"1"` |

`Deserialize`: an array gives unnamed entries, an object named ones. `{"call": …, "args": …}` is gone: `name(a, b)` is
`{"name": [a, b]}`. Update `bindings/js/index.d.ts` to what is really produced, and the Python and Java bindings where
they name the shapes. A mixed container does not survive this round trip; that is known and is a later stage's
(the tree's uniform JSON). Do not design that now.

## Done means

1. This finds nothing in `crates apps bindings benches tests/src` (outside `target`):
   `Value::(Seq|Map|Call)\b|EvalValue::Call\b|POSITIONAL_ENTRY_KEY|fn as_seq\b|fn as_map\b`
2. `git status --porcelain -- tests/fixtures crates/tree-sitter-tomet crates/tree-sitter-cotomet packages tmtroot`
   shows nothing but the memo that was already there.
3. `cargo check --workspace --all-targets` and `cargo test --workspace` pass (with the two environment variables).
   Paste the real final lines. A failure is reported as a failure, with the test's name.
4. Snapshot references (`tests/ref`): the `.html`, `.md`, `.typ` and `.pandoc.json` references do not change in value.
   The `.printed.tmt` and `.pandoc.roundtrip.tmt` references and `tests/SYNTAX.md` may change only by the two intended
   printer changes of item 8 and by how a container is shown in the tree dump. Regenerate as `tests/README.md` says,
   then **list every changed reference with the kind of change**, and for the printed ones quote each changed line. A
   change you cannot put under one of those headings: stop, do not accept it, report it.
5. Doc comments say what is true now: the `//!` of `tomet-syntax-ast` and the comments on `Value`, `Data`, `Group`, the
   positional binding, the printer's rules. None describes the three kinds.
6. `jj diff --stat --no-pager`: list any changed file you would not expect from this brief, with the reason.

## Report

End with: what changed, by layer; the answers this brief asked for (items 2, 7, 9); every changed reference and why;
the real test summary; the open questions from your progress file; what you did not do or did not check. "I do not
know" is an acceptable answer. A claim that turns out false is not.
