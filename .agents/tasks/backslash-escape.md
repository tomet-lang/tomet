# `\;` -> a general backslash escape

The author's plan (2026-11-01), split off from the finished `;` / line-raw task. Delete this file when done.
Destination of what is worth keeping: the `break` and `raw fence` sections of `tmtroot/docs/spec/syntax.tmt`, the `//!` of
`tomet-syntax-parser` (`cst.rs`, `lower.rs`), a test in `tests/src/` for the printer's round trip.

## Where it stands

- Today there is exactly one escape: `\;` at the start of a line is a literal `;` (a bare `;` there breaks the block). It is read in
  `lower.rs` (`escaped_semicolons`, `push_text_run`: the backslash is dropped from the text) and the CST keeps it as text; the printer
  writes `\;` for a text that starts a line with `;` (`render_block` in `tomet-format-printer/src/block.rs`, and `render_inlines` in
  `inline.rs` for a `;` after a newline). The `break` section of the syntax spec documents it.
- There is no escape for a backtick. Consequence found with the line raw: a paragraph that begins with an inline raw written with a
  fence of two or more backticks (its content holds a backtick, so the printer picks ``` ``a`b`` ```) prints back at a line start, where
  it now reads as a line raw (two) or a fence (three or more). Nothing can be put in front of it to stop that. The printer's inline
  raw is in `tomet-format-printer/src/element.rs` (`fence = "`".repeat(longest + 1)`).

## To decide

- [ ] Which characters `\` escapes, and where. Only at the start of a line (as now, where a sigil would act), or anywhere a character
  could be read as syntax (`\@`, `\*`, `` \` ``, `\$`)? The old `\@` / trailing `\` continuation was removed with the `;` change, so
  `\@` is free again but must not read as the old thing.
- [ ] What `\` before any other character means (itself, as now: `a\b` is text). Windows paths and regexes in prose are why that
  matters.
- [ ] Whether the escape lives in the CST (a token / node) or only in lowering, which is where `\;` is handled today.

## Steps

- [ ] Spec: say the rule once in `syntax.tmt`, replacing the `\;` paragraph of the `break` section.
- [ ] CST + lowering: one mechanism for every escape; drop `escaped_semicolons`.
- [ ] Printer: escape exactly what would be read back as syntax, so `parse(print(x)) == x`; fix the inline-raw-at-a-line-start case
  above (a failing test first).
- [ ] tree-sitter: `\x` as text (`tree-sitter-block-contexts.md` touches the same grammar).
- [ ] A fixture under `tests/fixtures/syntax/` and its references.
