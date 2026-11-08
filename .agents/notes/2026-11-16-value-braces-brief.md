# Brief: `{ }` leaves the value position -- `(k: v)` is the one spelling

You are working in a jj workspace of the `tomet` repository (`.agents/workspaces/one-container`). Your current
directory is that workspace. Work only here.

## Before anything else: the author's rules

The author's own words, which stand above everything in this brief:

> 場当たりな実装、後方互換性は悪、現在の実装を言い訳にせず、常に、長期的な最美を根拠に考えること
> AGENTS.md はちゃんと読むこと

- Your FIRST action is to read `AGENTS.md` in this directory, whole. Write the first line of your progress file as
  "AGENTS.md read", and follow its "Core Philosophy & Principles".
- No makeshift code. A change is done when the code says what it means, not when it compiles. No flag that remembers
  the old spelling, no branch whose only reason is that the old code had it.
- Backward compatibility is not a goal and is not allowed. After this change a value-position `{ }` is an error; there
  is no "accept it for a while".
- "It is implemented this way today" is never a reason. Where this brief is silent and the choice is visible to a
  user, do not invent a design: stop at that site, describe the choice in the progress file, and go on elsewhere.
- Never write a rule in several places.

## The decision (the author's, 2026-11-16)

`{k: v}` is "a `( )` whose entries all have names", and has no reason to have a bracket of its own. A `{ }` at a value
position goes away, and `(k: v)` is the one spelling. `{ }` stays in two roles only: after an element, a heading or a
list item, where it is the sugar of `::meta(...)`; and as a part of a gate (`{$ ... }`). Read
`tmtroot/dev/design/one-container.tmt`, section 7 and section 12, before you start: it is the current design and it
says why. (The older memos in `tmtroot/dev/design/` and every other `.md`/`.tmt` page describing the old spelling are
NOT yours to rewrite; see "Not in this step".)

The value is the same before and after: `{k: v}` and `(k: v)` are the same tuple, so every AST must be EQUAL across
the rewrite. That is how this change is checked.

## The order

Do the steps in this order. The old grammar must still read `{ }` while the repository is rewritten, because the
rewrite is verified by reading each file with the old grammar and the new text with the same grammar.

0. **Survey, no code.** Search the repository for every `{ }` that is at a value position, as the PARSER sees it: a
   `MAP_LITERAL` node in the CST (`tomet-syntax-cst-parser`), as opposed to the postfix `VALUE_DATA` group. Write in the
   progress file: how many files, per area (`tests/fixtures`, `tmtroot`, crates, root files); which of them hold an
   entry that is not a `key: value` pair (an element, a hole); which hold entries NOT separated by a comma or a
   newline (a `{ }` entry list does not require a separator, a `( )` does: `lower_tuple_entries` is called with
   `require_separator` false for `{ }` and true for `( )`); which are the root value of a `.ctmt`; how many sit in
   a `.writ.tmt`. Report anything the rewrite of step 1 cannot do and stop at it.
1. **The rewriter.** A tool that rewrites value-position `{ }` to `( )` on the CST and changes nothing else: the bytes
   of everything outside the two brackets are untouched, comments and raw text included (`tomet-edit` imports and
   exports the CST byte for byte; `tomet refactor` does NOT, it prints from the AST and loses comments: do not use it).
   Requirements:
   - Before it writes a file, it parses the old text and the new text and refuses (writes nothing, reports the file
     and the first difference) unless the two ASTs are equal. Spans are ignored by `==`; use that.
   - A `--check` mode that changes nothing and says which files it would change.
   - It leaves raw text alone: a fenced block, an inline `raw`, a raw line (`` ` ``) and a comment are not parsed as
     data and are not touched. Count such occurrences in `.tmt` prose so that the report can name them (they belong to
     the spec pass).
   - It handles `.tmt` and `.ctmt`. A `.ctmt` that does not parse is left alone and reported, as `tomet format` does.
   - It lives where the author can run it on the sibling repositories later (`catasto`, `sisho`, `twrit` have `.ctmt`
     and `.tmt` files that the same rewrite must reach): a `tomet` subcommand, one-shot by nature. Name it so that it
     is clear what it is for, and say in its help that it is to be removed once the siblings are migrated. Put the
     work itself in a library function with tests, not in the command.
   Tests: each shape of `{ }` at a value position (`key: {a: 1}`, `({a: 1})`, `list(...{})` nested, the empty `{}`,
   padded `{ a: 1 }`, multi-line with comments inside, a map in a map, a map as a call's argument, one holding an
   element); each shape that must NOT change (`@x(){ k: v }`, `=[ t ] { k: v }`, `- item { k: v }`, `{$ expr }`, braces
   in a string and in a raw block); and the refusal when the ASTs would differ.
2. **Run it.** On `tmtroot`, `tests/fixtures`, the crates' own `.tmt`/`.ctmt` files, and the root `.ctmt`. Do NOT run
   it on `.writ.tmt` files: they are the author's. Instead, run the rewriter in `--check` mode on them, and write what
   it would change as a patch to `.agents/notes/2026-11-16-writ-braces.patch` (the author applies it), and say so in
   the report. The files under `tests/fixtures/` are the frozen corpus; the author's word for this change is this
   brief, and only this mechanical rewrite is permitted there: no other edit to a fixture. Every file it changed is
   listed in the progress file with its count of brackets changed. After the run, `git diff -w` of every changed file
   shows only brackets.
3. **The grammar.** Remove the `MAP_LITERAL` arm of the CST parser, the node kind `MAP_LITERAL`, and every place that
   handles it (`lower_value_group`'s callers, `lower_single_value_from_children`, `groups.rs`, `connect.rs`, `form.rs`,
   `tail_validity.rs`, `tomet-edit`'s import, `tomet-syntax-highlight`). A `{` that starts a value and is not a gate is
   an ERROR: it must not fall through to the scalar reader and be read as the text `{k: v}`. The error says to write
   `(k: v)`, and it is at the `{`. The postfix `{ }` (`VALUE_DATA`) is not touched. Check that a `{` in the MIDDLE of
   a value (`a: x {y}`) still behaves as it does today and say in the report what that is.
4. **Writers.** The printer writes a tuple whose entries all have names as `(k: v)`; the empty tuple stays `()`. Search
   every other place that writes a brace map (`tomet-printer`'s value rendering, `serde_cotomet`'s printer,
   `tomet-workspace-indexer`'s display string, `tomet-semantics`, the converters, the LSP's hover) and make each one
   say the same, through ONE function if two of them need the same rule. The `{ k: v }` of an element's postfix group
   stays as it is.
5. **Tests and references.** Update tests that held the old spelling. References under `tests/ref`: only the printed
   ones (`*.printed.tmt`, `*.pandoc.roundtrip.tmt`) may change, and only by `{...}` becoming `(...)` at a value
   position; the HTML, Markdown, Typst and Pandoc JSON references must NOT change (the trees are equal). `tests/SYNTAX.md`
   is regenerated, and a case whose heading or text says that `{ }` is a map at a value position is rewritten to say
   what is true now. Cases of the old spelling that must now be an error stay as cases and show the error.
6. **Tree-sitter.** Do not touch it. The `corpus` test runs the fixtures through tree-sitter, whose grammar knows the
   old spelling; after step 2 some fixtures will no longer parse there. Add each such fixture to `KNOWN_TS_ERRORS` with
   a comment saying the grammar follows in the last step of the task, and report the number.

## Not in this step, do not start

Prose and spec pages that describe the old spelling (`tmtroot/spec`, `tmtroot/docs`, the older design memos, `.md`
files): they are rewritten in one pass later, and a raw example inside a `.tmt` page that shows `{k: v}` stays as it
is. `::name{...}` declarations with no primary group: they are an error today (`` `::meta` needs a primary group ``)
and 60-odd documents still have them; list the count and the forms in the report, do not change them. The `.tandr`
migration file in `tmtroot/migrations`: it is not a `.tmt`; list it, do not change it. Tree-sitter. The type seat. Any
change to what `( )` accepts. Any change to `{ }` after an element, a heading or a list item.

## Rules

- Read first: `AGENTS.md`, `CONTRIBUTING.md`, the root `.writ.tmt` (`crate-layering`), `tests/README.md`, and the `//!`
  of every crate you touch. A crate's directory name and its package name differ: read `name` in its `Cargo.toml`
  before `cargo check -p`.
- Do not touch: `crates/tree-sitter-tomet`, `crates/tree-sitter-cotomet`, `.writ.tmt` files, `packages/*`, `.md`
  files, `tmtroot/dev/design/` (the memo is current), any file under `tmtroot/spec` or `tmtroot/docs` except the
  mechanical bracket rewrite of step 2.
- Build with `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0` in the environment of every cargo command (the disk is
  5 GB). If `cargo` is not on the path, use `nix develop -c`.
- No commit, no `jj describe`, no `jj new`, no push, no `git` writes. Nothing outside the current directory.
- Keep `.agents/tasks/value-braces-progress.md` current. Tick a step only after you ran the command that proves it,
  in this run, and paste the last lines of its real output under the step. A tick without output is treated as a
  false claim. The reviewer re-runs everything, including the format check.
- Never add `_ =>` to make a `match` compile, never delete a test or mark it ignored. If you cannot make something
  work, leave it failing and write it down.
- If you are running out of time or quota, stop at the end of a step, with the progress file saying exactly where.
  Never stop between step 2 and step 3: a repository rewritten and not yet checked by the new grammar is the state
  that is hardest to resume.

## Done means

1. `cargo fmt --all` has been run and `cargo fmt --all -- --check`, `cargo check --workspace --all-targets` and
   `cargo test --workspace --no-fail-fast` pass. Paste the real final lines of each.
2. For every file step 2 changed: the AST before and after is equal (the rewriter's own check, plus one independent
   run of `tomet ast` on the old text from `jj` and the new text for ten files of your choice, compared).
3. `tomet check` over the repository fails on NO file that it did not already fail on before this step: record the
   list of failing files BEFORE step 2 (60-odd, the bare `::name{...}` ones) and AFTER step 6, and show that the second
   is a subset of the first.
4. `tomet format --check` passes on every file step 2 changed that it passed before.
5. `git status --porcelain -- crates/tree-sitter-tomet crates/tree-sitter-cotomet packages` shows nothing, and
   `git status --porcelain -- tests/ref` shows only the printed references and nothing else.
6. This finds no `MAP_LITERAL` in `crates apps bindings benches tests/src` (outside `target`).
7. `jj diff --stat --no-pager`: list any changed file you would not expect from this brief, with the reason.

## Report

End with: the survey (step 0); what the rewriter is, where it lives and how it checks itself; every file changed by
the rewrite, grouped by area, and the `.writ.tmt` patch; what the error says and where; every writer you changed and
the one function the rule lives in; every changed reference and why; the number of fixtures added to
`KNOWN_TS_ERRORS`; the raw examples that still show the old spelling (count by area) for the spec pass; the real test
summary; the open questions from your progress file; what you did not do or did not check. "I do not know" is an
acceptable answer. A claim that turns out false is not.
