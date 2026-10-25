# tree-sitter-tomet: blocks inside `[content]` and `|` runs, and the double-backtick inline raw

Raised 2026-11-01 after `;` and the line raw landed (commits `afc8f9a3`, `482f67c8`). Delete this file when done.
Destination of what is worth keeping: the header comment of `crates/tree-sitter-tomet/grammar.js` and the module doc of
`crates/tree-sitter-tomet/src/lib.rs` (its list of known simplifications), `KNOWN_TS_ERRORS` in `tests/src/lib.rs`.
`tomet-parser` is the source of truth; `grammar.js` is a hand-kept approximation used only for editor highlighting and folds, so
none of this changes what a document means. Regenerate with `tree-sitter generate --abi 14` in the crate (parser.c is committed).
Check with `cargo test -p tomet-tests -p tree-sitter-tomet`.

## 1. `;` and `` `` `` lines only work at the top level

`semi_break` and `line_raw` are alternatives of `_block` (`document: repeat(choice($._block, $._newline))`). Inside `[content]`
the grammar reads `_bracket_item` = `_line_item | _newline`, a flat run of line parts with no block structure, and a `|` run
(`marked_content`) is the same. So in

```
@quote[
a
;b
``  a raw line
]
```

the parser breaks the block at `;b` and reads the second line as one `raw` block, while tree-sitter sees one stretch of text:
`;` is `punctuation`, and `` `` `` is an empty `code_span` followed by text (an `ERROR` node is possible). Only colours and folds
are affected.

Why it is not a one-line fix: a block needs to know it stands at the start of a line, which the flat bracket run does not. Two
ways in, to compare before choosing:

- give `_bracket_item` a block alternative (`semi_break`, `line_raw`, and then the other blocks) and let the paragraph rule of the
  bracket run end at a line start. Likely to add conflicts to the `conflicts:` list, since the closing `]` and the group/args
  tokens after an element compete in the same states;
- an external token in `scanner.c` ("at the start of a line") that the bracket run and the `|` run take, which `_block` could then
  also use instead of relying on where the parser happens to be.

### Steps

- [ ] Write the failing cases first, as `tree-sitter-tomet` unit tests: `;` and a line raw inside `[ ]`, inside a `|` run, inside a
  list item's `[ ]`, and after an element's group on the next line.
- [ ] Try the first way (block alternatives in `_bracket_item`); count the new conflicts. If they do not resolve with `conflicts:`
  and `prec`, try the scanner token.
- [ ] Decide how far to go: `;` and `` `` `` only, or every block (sections, lists, fences) inside brackets? The real parser reads
  all of them there (`tests/src/pipe.rs` holds the `|` form equal to the `[ ]` form).
- [ ] Regenerate `parser.c`, `grammar.json`, `node-types.json`; check `highlights.scm` / `folds.scm` for the new nodes.
- [ ] Run the corpus: any fixture that now parses without an `ERROR` must come off `KNOWN_TS_ERRORS` (the sweep rejects a stale
  entry).

## 2. The double-backtick inline raw (old limitation)

`code_span: /`[^`\n]*`/` reads a span fenced by exactly one backtick. The real parser's inline raw takes a fence of any length
(``` ``a`b`` ```, the way a span holds a backtick: `try_parse_code_span` in `cst.rs` takes a run of backticks, then the first run of
the same length on the same line). tree-sitter reads ``` ``a`b`` ``` as the empty span `` `` ``, `a`, the span `` `b` ``, and a stray
`` ` ``, which is an `ERROR`. It is why `tests/fixtures/syntax/line-raw.tmt` has no mid-line example, and the reason the
tree-sitter unit test for the line raw uses a single-backtick span in its last line.

A regex cannot match "a run of n backticks, then the same n" in tree-sitter's lexer; the options are an external token in
`scanner.c` that counts the opening run and scans for the closing one, or accepting runs of 1 and 2 as separate rules (`` `x` ``
and ``` ``x`` ```), which covers what is written in practice and nothing longer.

Take care at a line start: there two backticks are a line raw (part 1 above) and three or more a fence
(`fence_marker` has lexical priority 3 for that reason). The new rule must not take those back.

### Steps

- [ ] Failing test: ``` a ``b`c`` d ``` has no `ERROR`, and the span is one `code_span`.
- [ ] Pick the scanner token or the two fixed lengths; the second is a few lines of grammar, the first is exact.
- [ ] Add the mid-line example back to `tests/fixtures/syntax/line-raw.tmt` (and `fences.tmt` if it has one) and regenerate the
  references with `TOMET_UPDATE_REF=1 cargo test -p tomet-tests`; read the diff.
- [ ] Remove whatever `KNOWN_TS_ERRORS` entries this fixes.

## Order

Independent; part 2 is smaller. Both need `tree-sitter` (in the dev shell) and change generated files, so commit the regenerated
`parser.c` with the grammar change that made it.
