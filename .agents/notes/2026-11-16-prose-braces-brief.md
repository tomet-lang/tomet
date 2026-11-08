# Brief: the prose and the spec say what is true now -- `( )` is the one container, `{ }` is not a value

You are working in the jj workspace `.agents/workspaces/one-container`. Your current directory is that workspace. Work
only here.

## Before anything else: the author's rules

> 場当たりな実装、後方互換性は悪、現在の実装を言い訳にせず、常に、長期的な最美を根拠に考えること
> AGENTS.md はちゃんと読むこと

- Your FIRST action is to read `AGENTS.md` in this directory, whole. Write the first line of your progress file as
  "AGENTS.md read", and follow it, in particular "Never edit `.md` directly", "Reference over Restatement" and "Code
  Comments & Tomet".
- This is a writing task, and a wrong sentence in a spec is worse than an old one: the spec is read by people who do
  not know the history. Where you are not certain what is true, do NOT write a guess: leave the sentence, and list it in
  the progress file under "Questions for the author" with the file, the line, the sentence, and what you were unsure of.
- Never write a rule in several places: point to the place where it lives (a spec page, a doc comment) with the
  repository's own link form, and do not restate it.
- Backward compatibility is not a goal: do not write "formerly", "was", "used to" or a migration note into a spec page.
  A spec page says what is true. (A page that exists to record history keeps its history: see "Do not touch".)

## What changed in the language (the truth you write from)

The code and its doc comments are the truth (AGENTS.md, "Source of Truth Hierarchy"). Read, before you write anything:
`Tuple`, `TupleEntry`, `Value::Tuple` and `Value::Call` in `crates/tomet-syntax-ast/src/lib.rs`; the `//!` of
`crates/tomet-syntax-lowering/src/lower/connect.rs` (merging); `TupleView` in
`crates/tomet-semantics-core/src/positional.rs`; the methods in `crates/tomet-semantics-expr/src/methods.rs`; and the
tests `crates/tomet-syntax-lowering/src/tests.rs`, `tests/fixtures/syntax/tuple.tmt`, `tests/SYNTAX.md` (generated: read
only). In short:

1. `( )` is the one container: an ordered run of entries, each with a name or without one. `(k: v)` is a tuple whose
   entries all have names; `(a, b)` is one whose entries have none; they may be mixed. There is no list kind and no map
   kind of value. "Any number of these" is a statement a schema makes about a PLACE.
2. A value-position `{ ... }` is an error (`{` cannot start a value: write `(k: v)`). `{ }` stays in two roles only:
   after an element, a heading or a list item, where it is the sugar of `::meta(...)`; and as a part of a gate
   (`{$ ... }`). `::name{ ... }` with no primary group is an error; the spelling is `::name( ... )`.
3. `name( ... )` is a call: a value with a head. It is never read as the entry `name: ( ... )`.
4. A key is text and there is one key space. An unnamed entry's key is its position among the unnamed entries.
   `(a, b)` and `("0": a, "1": b)` are the same tuple. A key written twice is an error.
5. Merging is key by key; a clash that is not tuple-with-tuple is an error at the second writer.
6. Addresses: `x.0` is the entry whose key is `0`, with or without a vocabulary; a signature's slot name (`x.target`)
   is an alias that exists where the signature is known. `b.0` is `b` for a scalar only.
7. `len`, `join`, `contains`, `map`, `filter`, `sort` work on the unnamed entries; `.positional()`, `.named()`,
   `has(key)`, `get(key, default)`.
8. JSON: an empty tuple is `{}`; keys `0`, `1`, ... in order are an array; otherwise an object.

## Scope

Find and fix the places that say or show otherwise. Three kinds, and ONLY these:

- **A. An example that shows a value-position `{ }`** in a `.tmt` page (a fenced `tmt` block, an inline code, an
  example in a table cell): rewrite it to `( )`, changing only the two brackets and leaving every other byte as it is (`{ a: 1 }` -> `( a: 1 )`, `{a: 1}`
  -> `(a: 1)`), when the example is meant to be valid. An example that is there to show the OLD spelling as an error
  stays, and says so.
- **B. A sentence that says `{ }` (or `{...}`) is a map, a value bracket, or "the data"** in a value position, or that
  `::name{...}` is a declaration form, or that a list and a map are two kinds of value: rewrite it to say what is true.
- **C. The words "マップ", "リスト", "辞書", "map", "list" used as the name of the value's KIND** (not of a place's
  expectation): where the sentence is unambiguous (for example "`args` の中身は、引数の名前をキーにしたマップ" says
  the tuple whose entries are named), say it with "タプル" and "項目"/"キー", and keep the sentence's meaning. Where the
  sentence could mean something else (a merge rule, a type's name such as `list(string)`, a general word for "a
  collection"), do not touch it: list it under "Questions for the author".

NOT in scope, and NOT to be changed:

- The postfix `{ ... }` after an element, a heading or a list item, and the words that describe it (the "data" of an
  element, `::meta`): they stay true.
- `list(...)`, `set(...)` and other type constructors: calls, not a kind.
- A page that does not mention what changed.

## Do not touch

- `tmtroot/dev/` (records of decisions, dropped ADRs, plans, `done.tmt`, old design memos), `tmtroot/deprecated/`,
  `tmtroot/generated/`, `tmtroot/migrations/`: history and generated files.
- `tests/fixtures/`, `tests/ref/`, `tests/SYNTAX.md`, `tests/src/`.
- `.md` files by hand; `.writ.tmt` files; `packages/*`; `crates/tree-sitter-tomet`; `crates/tree-sitter-cotomet` (their
  doc comments mention the old form: list them, do not change them).
- Any Rust code other than doc comments (`//!`, `///`, and a plain `//` that is prose about syntax).
- A new spec page. If no page states the tuple at all (point 1 above), say so in the report: the author decides where it
  goes.

## The order

0. **Survey, no edits.** Search `tmtroot/` (except the excluded directories) and the doc comments of `crates apps
   bindings` for each of the three kinds. The reviewer's first look found, with a rough pattern (`:\s*\{[^$]`,
   `\(\s*\{[^$]`, `,\s*\{[^$]`), about 15 files in `tmtroot/spec`, `tmtroot/docs` and 23 Rust files; the pattern is
   narrow (it misses `` `{}` `` and `{...}` in prose): widen it. Write a table in the progress file: file, line, kind (A,
   B, C, or "postfix: leave"), and what you will do. Count per directory. Stop here if the table is not what this brief
   describes (for example a spec page that is organised around `{ }` as a map, so that a rewrite is a new page).
1. **Spec and docs.** `tmtroot/spec`, `tmtroot/docs` (and any other `tmtroot` directory the survey finds, outside the
   excluded ones). Edit the `.tmt` sources only.
2. **Doc comments** in `crates apps bindings`: the same three kinds, in `//!`, `///` and prose `//`. Do not change a line
   of code. A doc comment that points at a spec page keeps the pointer.
3. **Derived files.** Read `tomet export --help`. Run `tomet export --check` BEFORE your first edit and write the list of
   files it says are out of date. After your edits, regenerate ONLY what your edits made stale (use the path argument if
   the command has one). The list of out-of-date files after must be the list before minus what you regenerated: it
   must not contain a file that was not on it before, and no regenerated file may change in a line that your edit to its
   `.tmt` does not explain. If the command cannot regenerate only those, do not run it, and say so.
4. **Format.** For each `.tmt` you changed: if `tomet format --check` passed before your edit, it passes after; if it
   failed before, do not format the file (you would reformat lines you did not mean to touch).
5. **Check.** `tomet check` on each changed `.tmt`: no new error or warning.

## Rules

- Build with `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0` in the environment of every cargo command. The disk is 5 GB
  and was full once: if a command says "No space left on device", STOP, write that in the progress file, and end. Do not
  run `cargo clean`. Do not delete files outside the ones you made.
- Every command that can wait on something runs under `timeout`. Do not poll a long command in a loop.
- No commit, no `jj describe`, no `jj new`, no push, no `git` writes. Nothing outside this directory.
- Do not run `cargo fmt --all` and do not run `tomet format` over a directory; only on a file you changed, as step 4 says.
- Keep `.agents/tasks/prose-braces-progress.md` current. Tick a step only after you ran its proof in this run and pasted
  the last lines of the real output. A tick without output is treated as a false claim. The reviewer re-reads every
  changed sentence against the code.

## Done means

1. `cargo fmt --all -- --check`, `cargo check --workspace --all-targets` and `cargo test --workspace --no-fail-fast`
   (under `timeout 1500`) pass. Paste the real last lines of each.
2. `jj diff --stat --no-pager` lists only `.tmt` sources under `tmtroot/` (not in the excluded directories), Rust files
   whose diff is only comment lines, and the `.md` files that step 3 allows. `jj diff --git` of every `.rs` file shows
   only lines that start with `//` after the `+`/`-`.
3. `git status --porcelain -- tests crates/tree-sitter-tomet crates/tree-sitter-cotomet packages tmtroot/dev
   tmtroot/deprecated tmtroot/generated` shows nothing.
4. `tomet check .`: it fails on no file it did not fail on before your first edit (write the list before).
5. A search of `tmtroot/spec`, `tmtroot/docs` and the doc comments for a value-position `{ }` in an example finds only the
   cases you decided to keep, and each is in your table with the reason.

## Report

The survey table (counts per directory and per kind); what you changed, by directory, with the sentences you rewrote for
kind B and C quoted before and after; the examples you kept and why; the "Questions for the author" list; whether any
page states the tuple; what step 3 did; the real test summary; what you did not do or did not check. "I do not know" is
an acceptable answer. A claim that turns out false is not.
