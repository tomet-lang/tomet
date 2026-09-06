# Two fidelity bugs in the Markdown bridge

Blockers for `tmtroot/agents.tmt` -- the author wants `AGENTS.md` (and
`CLAUDE.md`) generated from `.tmt` like `README.ja.md` already is, and the
bridge damages the text on the way.

## What the writ says, and what is actually true

`docs/.writ.tmt`'s `generated-md-not-edited` names two export bugs as the
reason `docs/README.tmt` is not generated: backticks escaped in prose, and
a `<div>` emitted for the `@settings` header. **Both are gone.** Measured
2026-09-06: `tomet to-md docs/README.tmt` produces zero escaped backticks
and zero `<div>`. The `@settings` line was itself removed when
`docs/docs.settings.tmt` was deleted.

The writ entry is therefore stale and should be corrected -- author's
file, so propose and stop.

## 1. A code span containing `//` is corrupted on import

```
the `//!` module doc   ->   the ``//`!` module doc
the `//` doc           ->   the ``//`` doc
```

`enclose_sigils_in_backticks` (`import/wikilink.rs:236`) wraps a literal
`//` in backticks so it is not read back as a Tomet comment. Right in
principle, wrong in reach: it runs from
`post_process_document_wikilinks` (`import/mod.rs:177`), a pass over the
**whole document after every event is collected**, and by then
`Event::Code` has already been flattened to backticked plain text
(`import/mod.rs:144`). The pass cannot tell a code span from prose, so it
protects the `//` a second time.

Tomet's AST has no code-span node -- `` `x` `` parses to a `Text` whose
value includes the backticks -- so the fix cannot be "keep code spans
distinct". It has to be: the escaper skips what is already inside a
backtick run, matching CommonMark's own code-span rule (a run of N
backticks closes on the next run of exactly N).

## 2. A wrapped Japanese line gains a space -- in the *parser*, not export

```tmt
日本語の段落を手で折ると、
ここに空白が入ってしまう。
```

exports as `日本語の段落を手で折ると、 ここに空白が入ってしまう。` -- an
ASCII space (0x20, confirmed with `od -c`) at the fold. Joining wrapped
lines with a space is right for English and wrong for CJK, and every
document that would be exported under `docs/` is Japanese.

**Not a Markdown bug.** `normalize_text`
(`crates/tomet-syntax-parser/src/inline.rs:323`) folds a newline plus any
following whitespace into one space, unconditionally, so the space is in
the AST before any writer sees it. Every consumer inherits it -- HTML,
Typst, Pandoc, the printer -- and `docs/README.tmt`'s export is unusable
for this reason rather than the two the writ blames.

Correct for English (CommonMark's softbreak). Wrong between two wide
characters, which is what every CJK-aware tool special-cases.

**Author's call, because it is language semantics.** `docs/spec/` does not
say what a fold becomes; it only says a continuation line stays in the
paragraph, which is about placement. Blast radius measured: 27 folds
between two wide characters across 10 corpus fixtures, so their snapshots
move.

## Out of scope

- **Hand-wrapping is not preserved.** `AGENTS.md` is wrapped at ~72
  columns; a round trip returns one long line per paragraph. Not
  corruption, but it makes every future diff a whole-paragraph diff.
  A re-wrapping export is a feature, not a bug fix; decide separately.
- **`CLAUDE.md` should probably not be generated at all.** Its entire
  content is `@AGENTS.md`, which Tomet reads as an element in namespace
  `AGENTS` and rejects. Producing that line needs a fence or an escape,
  making the source uglier than the 11-byte output. `tmtroot/cluade.tmt`
  is empty, misspelled, and a candidate for deletion instead.

## Steps

- [x] 1. `enclose_sigils_in_backticks` skips backtick-delimited runs,
      by CommonMark's exact-length rule.
- [x] 2. Tests for both, plus a third bug the first one uncovered:
      `Event::Code` was re-wrapped with a fixed pair, so ``` ``a`b`` ```
      flattened to `` `a`b` ``. `backticked()` grows the fence past any
      run inside, the rule the printer already used.
- [ ] 3. **Blocked on a decision.** The CJK fold is in `normalize_text`,
      not in the Markdown writer. Rule to apply if taken: a fold between
      two wide characters joins with nothing, otherwise with a space.
- [ ] 4. Test both, in one document, once 3 is decided.
- [ ] 5. Re-measure `tomet to-md docs/README.tmt` and the `AGENTS.md`
      round trip; record what is left.
- [ ] 6. Propose the `generated-md-not-edited` correction to the author.
