# Resume: the tuple -- you are continuing a run that stopped half-way

## Before anything else: the author's rules

The author's own words, which stand above everything in this file and in the brief:

> 場当たりな実装、後方互換性は悪、現在の実装を言い訳にせず、常に、長期的な最美を根拠に考えること
> AGENTS.md はちゃんと読むこと

- Your FIRST action is to read `AGENTS.md` in this directory, whole. Two runs before you were told to and did not.
  Write the first line of your progress notes as "AGENTS.md read", and follow its "Core Philosophy & Principles".
- No makeshift code. A rewrite of a site is done when it says what the code now means, not when it compiles: no helper
  that turns a tuple back into "the list" or "the map" so that the old logic can run unchanged, no flag that remembers
  which of the two it used to be, no branch whose only reason is that the old code had it.
- Backward compatibility is not a goal and is not allowed: nothing is kept so that an old caller, an old spelling or an
  old output survives.
- "It is implemented this way today" is never a reason. The brief says to keep today's result where it is silent: that
  is only a way not to invent a design the author has not decided, and it applies to nothing else. If keeping today's
  result needs code you would be ashamed of in five years, do not write it: stop at that site, describe the choice in
  the progress file, and go on elsewhere.
- Judge each choice by what is right for the language in the long run, and say in the progress file where you had to
  choose.

---

A run before you worked from the brief below and stopped in the middle (its quota ran out; nothing was wrong with the
work). You continue it in the same workspace. The brief below is unchanged and is still the order. Read it whole, then
read `.agents/tasks/tuple-model-progress.md`, which is the earlier run's list of steps: steps 1 and 2 are ticked, step 3
was in progress.

## The state you start from (checked by the reviewer, not by the earlier run)

- 38 files are changed. The types `Tuple` and `TupleEntry` exist in `tomet-syntax-ast`; `Value::Seq` and `Value::Map`
  are gone from the AST; `Value::Call(String, Tuple)` is kept. Do not redo or redesign any of that. Use the methods
  `Tuple` already has; add one only when no existing method can answer the caller's question.
- These packages build and their tests pass: `tomet-tree`, `tomet-lowering`, `tomet-ast-parser`, `serde_cotomet`,
  `tomet-semantics-core`, `cotomet`.
- `tomet-ast`'s own tests do NOT compile, although the progress file says they were verified: near line 1858 of
  `crates/tomet-syntax-ast/src/lib.rs` a test passes `[("key", …)]` to `Tuple::from_named`, which takes
  `(String, Value)`. Fix the test first.
- `tomet-declaration` (`crates/tomet-semantics-declaration`) is half rewritten and does not compile (18 errors). Start
  step 3 there.
- 66 files outside those still use `Value::Seq`, `Value::Map` or `POSITIONAL_ENTRY_KEY`. The workspace as a whole does
  not build. Part 2 (the merge rule) is not started: `merge_values_inner` still has the old rule, adapted to the new
  type. Leave it until part 1 is done, as the brief says.

## Rules on top of the brief's, because of what went wrong before

- Tick a step in the progress file only after you ran the command that proves it, in this run, and saw it pass. Paste
  the last lines of that command's output under the step. A tick without output is treated as a false claim.
- A package's directory name and its package name differ (`crates/tomet-syntax-lowering` is the package
  `tomet-lowering`). Read `name` in the crate's `Cargo.toml` before `cargo check -p`.
- When a `match` on `Value` stops compiling, write the arm for `Value::Tuple`. Never add `_ =>` to make it compile, and
  never delete a test or mark it ignored to make the suite pass. If you cannot make something work, leave it failing
  and write it in the progress file.
- Do not start a build of the whole workspace until every layer below it checks on its own.
- If you are running out of time or quota, stop at the end of a crate, with the progress file saying exactly where.

---

