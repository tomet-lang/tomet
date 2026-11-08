# one-container: `( )` is the one data container; `name(…)` is a value

> **PAUSED by the author, 2026-11-13 night: no more code until the syntax is complete.** The run of stage 2 that was
> already going is allowed to finish and is then reviewed and parked; no later stage is launched. Read "Paused" at the
> end of this file before anything else. The stage list below is kept as it was, and parts of it are now wrong.

Workspace: `.agents/workspaces/one-container` (based on `0a615800`, the newest committed revision; the default
workspace's uncommitted changes are documents and manifests only and are not in it).
Design, decided with the author on 2026-11-13: `tmtroot/dev/design/one-container.tmt` **in that workspace**. Read it first;
it is the only copy of the decisions. This file is the order of work and its state.

Delete this file when the stages are done. Destination of what is worth keeping: the design memo above (already written),
the `//!` of `tomet-syntax-ast` (the value model), `tmtroot/spec/syntax/` (the bulk pass of the last stage), commit messages.

## Rules for whoever works on this

- The implementer is a delegated agent (agy, launched by the author). The session that wrote this file gives the brief
  and reviews; it does not implement.
- No backward compatibility: no shims, no re-exports, no "accept the old spelling for a while". A stage that removes a
  spelling removes it, and ships the rewrite of the repository's own files with it.
- The existing implementation is not a reason for a design choice. Where the memo and the code disagree, the memo is the
  target; where the memo is silent, stop and ask, do not invent.
- `.writ.tmt` files are the author's: propose an edit and stop.
- `.md` files are generated: edit the `.tmt`. Spec pages are the last stage, in one pass; do not touch them earlier.
- No commit, no push, no `jj describe` without the author's word. Do not touch other workspaces.
- Build with `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0` (the disk is 5 GB).
- `tree-sitter generate` is run by the author (`just gen-treesitter`); edit `grammar.js` and say so.
- The line that must hold: the tree is readable without evaluating anything.

## Stages

Each stage is one delegated run, reviewed before the next one starts. Every stage ends with `cargo test --workspace` green.

- [x] 0. Design memo written, `tomet check` and `tomet format --check` pass on it. Waiting for the author to read it.
- [ ] 1. **Survey and AST proposal (no code).** Every reader and writer of `Value::Seq`, `Value::Map`, `Value::Call`,
      `Group`, `Entry`, `Data`; what each one asks of the value (by position, by name, in order, "is it a list"). A written
      proposal for the one container type in `tomet-syntax-ast` (its name is the author's to approve), how nexus args and
      a value's container become the same type, and the order in which the crates move. Output: a file in the workspace,
      reviewed here, then shown to the author.
- [x] 1b. (done in the workspace 2026-11-13, uncommitted, reviewed -- see "Review of stage 1b") **Remove the nameless entry of a `{ }` group** (`(marker)[content]`, `Entry::Bare`, `struct Bare`). The author's
      order, 2026-11-13: the construct is to go. It is not in any spec page; it is in one fixture
      (`tests/fixtures/syntax/sigils.tmt`, the `@memo(){ (1)[…] (2)[…] }` block -- the corpus is frozen, so this edit is
      by the author's word and is named in the commit), in the CST and lowering, the AST, the printer, the tree walk,
      `tomet-transform`, `tomet-stats`, the HTML and Pandoc converters (both directions: `from_pandoc` builds these
      entries from a nested `Div`, so say what it builds instead, or stop and ask), the LSP's folding, the CLI's spec
      page, and `bindings/js/index.d.ts` with the `packages/*` types.
- [ ] 2. **Data model.** One container in the AST; `Seq`, `Map`, `Call` are gone as separate kinds. No spelling changes:
      every existing source still parses to the same meaning and prints to the same text. Acceptance: the printed and
      round-trip snapshot references are byte-identical; only references that serialize the tree itself may change, and
      each such change is explained.
- [ ] 3. **New spellings read.** A bare `( )` takes named and unnamed entries together; a bare integer is not a name, a
      quoted one is; a repeated name is an error; `name(…)` in the data space is the value `(name: (…))` and no longer a
      kind of its own. New fixtures (the corpus is frozen: add, do not edit).
- [ ] 4. **Addresses.** The step `.0` (the n-th unnamed entry); `.item(n)` is the n-th entry as written, named or not.
- [ ] 5. **Constructors move to `$`.** The parser stops resolving `string(…)`, `int(…)`, `float(…)`; they are functions
      of the expression space. A reader with no evaluation keeps rejecting a bare `$…`.
- [ ] 6. **`{ }` leaves the value position.** Removed from the grammar there, together with a rewrite of every `.tmt` and
      `.ctmt` in the repository and the tool that does it (catasto, sisho and twrit files are rewritten with the same
      tool, in their own repositories, later). `{ }` stays as the meta sugar after a nexus and as a gate's part.
      `grammar.js` edited; the author regenerates.
- [ ] 7. **Output.** The tree's JSON is uniform (entries, in order, with or without a name). The data JSON goes by shape
      (all unnamed: array; all named: object; mixed: object with `"0"`, `"1"`; empty: array). `serde_cotomet`: the
      receiving type decides; `name(…)` reads as an externally tagged enum. Bindings and `index.d.ts` follow.
- [ ] 8. **Spec pages, in one pass**; the check that warns about an unnamed entry that is a one-entry container; delete
      this file.

Decided by the author on 2026-11-13: **every tree-sitter change is the last stage**, after stage 8. No stage before it
touches `crates/tree-sitter-*`. The one change already made (the `bare_element` rule removed from `grammar.js` in 1b)
was taken out of the working copy and is kept as a patch in the session's scratchpad
(`deferred-grammar-js-1b.patch`); if the scratchpad is gone, it is: delete the `bare_element` rule and its mention in
the two comments above `_group_item`.

- [ ] 9. **Tree-sitter, all at once**: `grammar.js` of both grammars follows everything above; the author runs
      `just gen-treesitter`; `KNOWN_TS_ERRORS` and the highlight queries follow.

How stage 2 is cut (so the later stages are read right): it also does the data JSON by shape and `serde_cotomet`
(a container must serialize somehow, and there is no half-way), so stage 7 is only the tree's uniform JSON and the
bindings. It leaves `EvalValue::Seq` / `Map` in the evaluator (bridged by shape, `EvalValue::Call` gone): redoing the
evaluator's value belongs with stage 4, once "methods over named entries" is decided.

## Open, not blocking (the list is in the memo's last section)

Methods over named entries; the type seat on a value; `at(key, default:)`; dotted names inside data; `X.X == X` restated
as "position 0 is the node's own value"; `auto` as a value; spreading one container into another.

## Review of stage 1 (2026-11-13)

The run took 24 minutes, wrote only its report, ran nothing. What the review found, re-running the searches:

- Section A (what exists) is accurate where checked, except that `string()` with two arguments is an error, not a
  fall-back to a call. Facts worth keeping: a nexus's `()` lowers to an empty map, `(x)` to the bare scalar, `(x, y)` to
  a map whose positional entries have the key `""` (`lower_paren_value`); `bind_positional` puts the names on.
- Section B (the inventory) is **not complete and does not say so**: of 87 non-test files that match
  `Value::(Seq|Map|Call)` or `Entry::*`, 45 are missing, among them `tomet-semantics-core/src/parse.rs` (a call read as a
  type: `type: list(string)`), `tomet-semantics-core/src/admits.rs` (a sequence admitted by `type: list`),
  `cotomet/src/lib.rs` (`find_not_data`), `apps/lsp/src/hover.rs`, `tomet-semantics-declaration/src/check.rs`,
  `tomet-semantics-expr/src/vars.rs`. Do not use B as the list of sites. Removing the variants makes the compiler the
  list; the brief for stage 2 must name the sites whose meaning depends on the three kinds, B's eight plus those above.
- Section C's sketch (entries with an optional name; `get`, `nth_unnamed`, `nth_entry`, `named`, `unnamed`) is a sound
  starting point. Its names: `Container`, `Tuple`, `Series`.
- Two findings the memo has to answer before stage 2:
  1. `tomet-printer` writes from the AST alone and `tomet-formatter` works on the CST. If `name(…)` and `(name: (…))`
     are one tree, the printer cannot write back what was written. The memo's "the spelling is kept, in the spelling
     layer" does not hold as written: in this project a choice between spellings is a setting, not the writer's
     (`tomet-syntax-style`), and no setting can choose here without rewriting one intent into the other.
  2. A nexus's `{…}` group holds entries that are not `name: value` (`Entry::Bare`, `Entry::Element`, `Entry::Hole`).
     The memo says `{ }` is the meta sugar and says nothing of these. Not needed for stage 2 if `Group` is left alone.
- Its other questions: a lone `(x)` in a nexus stays a container (the memo says so); mixed JSON is in the memo; how two
  containers merge, what `enum(…)` in `allow:` becomes, and whether a `.ctmt` root may hold unnamed entries are open.

## Review of stage 1b (2026-11-13)

agy, launched by the author with a shell: 31 minutes, no forbidden action (`jj op log` shows snapshots only). Re-run
here: the brief's search finds nothing; `cargo test --workspace` passes (147 suites, 2328 passed, 0 failed, 1 ignored).
The construct is a parse error from lowering, with the advice to write a named element, and two tests for the message.

Open points, for the author:

- The fixture lost a second block the brief did not name: `@deck.card(){ title: 混在, (a)[…] }`, the same construct in
  the same section (the run said so itself). The section heading ``=[ `{}` は常にデータ ]`` in
  `tests/fixtures/syntax/sigils.tmt` is now empty and says something the design no longer says; removing it needs the
  author's word (frozen corpus).
- `tests/ref/syntax/sigils.pandoc.json` was rewritten in the expanded layout the test writes (`to_string_pretty`); the
  committed references are compact. Its content differs from the old one by the two removed blocks only (checked by
  parsing both). Layout churn to undo or accept.
- `crates/tree-sitter-tomet/grammar.js` lost the `bare_element` rule; `parser.c` is not regenerated (the author runs
  `just gen-treesitter`).
- `packages/react` and `packages/svelte` still list `"Bare"` in their `Sigil` unions: that model is already stale
  (task `js-packages-follow-ast`), left as it is.
- The progress file was not kept current during the run (3 of 22 steps ticked until the end).
- The disk is at 94% after the builds (315 MB free).
- Working papers to delete from the workspace before a commit: `.agents/tasks/one-container-survey.md`,
  `.agents/tasks/one-container-1b-progress.md`.

## State

2026-11-13: stage 0 done; the author said to go ahead. Stage 1 launched with `agy --print … --mode accept-edits` (no
shell permission, which a survey does not need) from the workspace. Its report is to appear at
`.agents/workspaces/one-container/.agents/tasks/one-container-survey.md`; that file is a working paper, not part of the
change -- remove it from the workspace before anything is committed. Stage 1 is done and reviewed (above).

Decided by the author after the review (2026-11-13): the two spellings are **one tree**. The AST is the tree for reading
and keeps no spelling; the CST keeps what was written; the printer is for documents that have no `.tmt` source and
picks a spelling by a setting (default: the short form `name(…)`). Section 4 of the memo is corrected accordingly. The
reviewer had recommended keeping `name(…)` as a node of the AST; that was wrong, and came from not having read the
crates' own module docs (`tomet-syntax-ast`: "names nodes by role, never by spelling").

A finding left for the author, not part of this task: `tomet refactor` writes a user's whole file back through the AST
printer (`tomet-workspace/src/refactor.rs`, `structural.rs`), so a refactor already replaces every spelling the writer
chose with the setting's. Under the split above that belongs on the CST (`tomet-edit` is byte-identical on a round
trip). Not started, not briefed.

Stage 1b is committed in the workspace as `lvlswvkt` (`feat(syntax)!: remove the nameless entry of a value group`), by
the author's word, with the empty heading removed from the fixture too. Not pushed. Left out of that commit on purpose
and still in the working copy: `crates/tree-sitter-tomet/grammar.js` (the `bare_element` rule removed; it waits for the
author's `just gen-treesitter` so that the grammar and `parser.c` change together -- squash both into `lvlswvkt` or
commit them after it, as the author says) and the design memo. The survey and the 1b progress file were moved to the
session's scratchpad. (Superseded for `grammar.js`: see "every tree-sitter change is the last stage" above; it is no
longer in the working copy.)

Stage 2: the brief is written (`one-container-stage2-brief.md` in the scratchpad) and handed to the author to launch.
On its return: re-run its searches and the tests, check `tests/fixtures`, tree-sitter, `packages` and `tmtroot` are
untouched, read every changed `.printed.tmt` line against the two intended printer changes (an empty `{}` value prints
`()`; a one-key map whose value is a list prints as `name(…)`), and time nothing by trust. Stages need a
shell (cargo), so the author launches them; the briefs are in the session's scratchpad and are rebuilt from this file
and the memo if lost.

## Paused (2026-11-13 night) -- what changed after the stages were written

While stage 2 was running, the author and the reviewer went back over `name(…)` and reached decisions that the stage
list above does not have. The memo's sections 4 and 5 are rewritten to them; this is the short form.

- **`name(…)` is a tuple with a name, a value of its own kind.** It is NOT the one-entry container `(name: (…))` and is
  never read as the entry `name: (…)`. A call does not disappear: its arguments become the tuple (named and unnamed).
  The author's test, which settles it: `(name())` has `name()` at position 0; a reading that makes it the entry `name`
  changes what `(0: name())` and `(name())` mean. Access: `x: name(a, k: v)` gives `x.0`, `x.k`, and `x::name`.
- **Stage 2 as briefed is therefore wrong in one place**: its brief told the implementer to turn a call into a
  one-entry container (items 3, 4, 6, 8, 10 of the brief: the type reader, `enum(…)` in `allow:`, the printer's short
  form, serde's enum). The rest of stage 2 (lists and maps into one tuple, a nexus's args as that tuple, no `""`
  sentinel, positional binding) stands. The author chose to let the run finish and correct the call afterwards. The
  correction: a `Value` variant for the named tuple (a name and a `Container`), restored at every site that built a
  one-entry container from a call -- the old `Value::Call` sites, findable from the diff of stage 2.
- **`::name(…)` attaches a named tuple to a node** (to `self` when nothing is on its left) and is read back with
  `X::name`. It is a call in shape and in meaning, as attributes are in Rust, C++, C#, Java, Python, PHP.
- **The dotted head that dives into a key is the oddity**: `::format.meta(always_newline: true)` being the same as
  `::format(meta: (always_newline: true))` is the rejected reading again. That it is odd is agreed; what replaces
  `::field.author(…)[ about ]`, `::macro.copyright("…")`, `::element.rule.arg.id(…)` is open.
- Withdrawn along the way, do not revive: "bare `name(` is always the key `name: (`"; "remove bare `name(…)` from the
  data space"; "a declaration is written `::key: value`"; "the tree keeps the spelling of `name(…)`" (the AST keeps no
  spelling; the CST does -- that part stands).

What the author wants next is not code: a ledger of the syntax, one line per construct (`tests/SYNTAX.md` lists 181
cases in 28 groups), each marked settled or open, with the test that settles it and how Typst, Rust, AsciiDoc, Python
and Swift/Dart write the same thing. The tests found so far: the tree is readable without evaluating; a place's name is
the container's and a value's name is the value's; the shape and the meaning agree; one sign, one meaning.

When code resumes: review and park stage 2, correct the call (above), then rewrite stages 3 to 9 from the ledger.

## State, 2026-11-15 (appended; everything above is kept as it was and is partly out of date)

The author lifted the pause for the tuple alone ("タプルは決定") and had it implemented. What was said and decided that
day is in `.agents/notes/2026-11-15-syntax-conversation.record.md`; read it before the stage list above.

- Done and committed in the workspace, not pushed: `tkktklmr`, `feat(syntax)!: one tuple in place of sequences and
  maps`. `Value::Seq` and `Value::Map` are `Value::Tuple`; `Value::Call(String, Tuple)` stays a kind of its own (the
  correction the "Paused" section asks for is therefore not needed: the run that got it wrong was thrown away);
  a nexus's `(args)` is a `Tuple`, `(x)` not unboxed; JSON by shape with the empty tuple as `{}`; one merge rule, a
  clash is an error. 147 suites, 2339 passed, 0 failed, 1 ignored.
- Next, brief written and not launched: `.agents/notes/2026-11-15-tuple-spelling-brief.md` (a value's `( )` and a
  call's `( )` take named and unnamed entries together; one key space).
- After that, in this order: `{ }` leaves value positions (with the rewrite of the repository's files); addresses
  (blocked on `x.0` against `x.name` and on `b.0 == b`, both open); the evaluator's value; output; spec pages;
  tree-sitter last.
- Still uncommitted in the workspace and out of date: `tmtroot/dev/design/one-container.tmt`.
