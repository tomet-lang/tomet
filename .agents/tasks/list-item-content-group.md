# `-` and `#` are sigils, and should take the same groups as `@name`

## The model

Every element is `sigil (args) [content] {value}`, all three groups
optional. `@name()[]{}` has this. So do `-` and `#`:

    - ()[ content ]{}          full form
    - [ content ]              args omitted, like any element
    - () content :{}           sugar
    - content                  sugar, args omitted

The sugar form is block-only and single-line **by specification**. Content
that spans lines needs an explicit trigger, and the trigger is the
`[ ... ]` group. A card notation (`> line` / `| line` prefixes) may be
added later, but in most cases spanning lines is simply not allowed. That
is the design, not a gap.

The same sugar is planned for headings: `#() content :{}` and
`# content :{}` alongside today's `#[ ... ]`.

## What is actually implemented

Neither sigil has the full shape, and they are missing opposite halves:

| | `(args)` | `[content]` | `{value}` | sugar |
| --- | --- | --- | --- | --- |
| `@name` | yes | yes | yes | -- |
| `#` | no | yes | no | no |
| `-` | yes (marker) | **no** | yes (trailing attrs) | yes |

Measured, all falling through to prose: `#()[ x ]`, `#(a)[ x ]{i:1}`,
`# x`, `- [ x ]`, `- ()[ x ]`. Working: `#[ x ]`, `- x`,
`- (x) content`, `- content {id: a}`.

So this is not fallout from removing the `- [x]` checkbox -- `#` never had
a checkbox and has the same class of gap. Both sigils were implemented ad
hoc rather than as an element with the universal shape.

Consequence today: `docs/README.tmt` is written with wrapped list items,
which the parser splits into loose paragraphs, so its Markdown export
breaks every list containing one.

## Scope: both sigils at once

The author's call, and the measurement supports it: doing `-` now and `#`
later means writing the ad hoc path twice and then reconciling it.

What existing documents would change meaning, swept across every `.tmt`
(28 lines start with `# ` or `#(`):

| site | count | effect |
| --- | --- | --- |
| inside fenced code (bash, markdown examples) | 5 | verbatim, unaffected |
| `crates/*/README.ja.tmt:1` -- `# tomet-xxx` | 10 | becomes a heading, which is what whoever wrote them meant |
| `docs/spec/syntax.tmt:61`, `docs/guide/name.tmt:33` -- `# []` | 2 | already works today; documents whitespace tolerance, not the absence of sugar |
| `docs/design/ideas/idea.tmt:47` -- `#( Header )[` | 1 | already sketched in this shape |
| `tests/fixtures/syntax/sigils.tmt:25` | 1 | **the one real conflict** |

`# [ x ]` and `#  [ x ]` already parse as headings, so whitespace between
the sigil and its groups is not the issue -- only the bracket-less sugar is
missing.

The conflict is a fixture section titled "本文に落ちる `#`" asserting that
`# 見出しではない行。...` is body text. Under the sugar it becomes a
heading. The next line, `これは #タグ ではない`, survives: `#` directly
followed by text with no space is not sugar and stays prose.

`docs/spec/syntax.tmt:17` says `[]` directly after a list marker "carries
no special meaning and is read as plain prose". Under the model above that
line means the *brackets are not a checkbox*, not that they are literal
text -- `- [x]` is an item whose content is `x`. The wording should be
fixed either way, because it currently reads as the second thing.

## Decisions taken while implementing

- **Trailing text after a group joins the item.** `- [T] content` gives an
  item reading `T content`, not an item `T` plus a loose paragraph
  `content`. This matches `@x[T] content`, where both halves stay in one
  paragraph; letting it split would silently move a sentence out of the
  list it was written in. Two existing tests asserted the old
  "brackets are literal text" behaviour and were rewritten.
- `parse_inline_seq` trims its own edges, so the space between a group and
  the text after it has to be re-inserted by hand -- except when the group
  is empty, where there is nothing to separate (`- [ ] text`).
- `tests/fixtures/roadmap.tmt` writes 21 items as `- [ ] text`. Those now
  parse as an empty content group plus trailing text and render without
  the brackets. `docs/roadmap.tmt` has none -- already migrated. The
  supported spelling for a todo marker is `- ( ) text`, per
  `docs/guide/cheatsheet.tmt`. Snapshots regenerated rather than rewriting
  the fixture: it is a frozen corpus, and it now covers the empty-group
  case, which is worth having.

## Steps

- [x] 0. Done: the `(args)`/`[content]`/`{value}` loop is extracted from
      `element::parse_element` into `element::parse_groups`, and list items
      call it. Headings must call the same function, not a copy.
      Original note: Find the shared shape first. `-` and `#` should end up going
      through one "sigil, then optional `(args)`/`[content]`/`{value}`"
      path, not two hand-written ones. Read `element::parse_element` before
      writing anything: it already has this logic for `@name`, including
      group reordering and the colon-connect rule.
- [x] 1. `parse_list_internal` (`crates/tomet-syntax-parser/src/list.rs`):
      after the marker, if the next character is `[`, parse a bracketed
      inline sequence that may span lines, then an optional `{value}`.
      Today content is `Stop::Line` or up to `peek_trailing_attrs`; this is
      a third branch.
- [x] 2. `eat_list_marker_with_indent` currently requires whitespace after
      `)` before it will accept a marker, which is why `- ()[ x ]` does not
      even parse its args. Accept `[` there too.
- [x] 3. Keep the sugar single-line. A wrapped line under a sugar item
      stays a paragraph -- specified behaviour, not a bug to fix in
      passing.
- [x] 4. Do not disturb: `- (12:01) 本文 {id: a}` marker parsing, trailing
      `{...}` attrs, colon-connect `:{...}`, and `-- 子` still falling out
      as a paragraph.
- [x] 4b. Headings gain `(args)`, `{value}` and the bracket-less sugar:
      `#() content :{}` and `# content :{}` alongside `#[ ... ]`. `#` with
      no space before the text (`#タグ`) stays prose.
      Watch the existing rule that a heading's level comes from the count
      of `#`, which currently arrives as the element's `args` -- an
      explicit `(args)` group has to coexist with that, not overwrite it.
- [x] 4c. Rewrite `tests/fixtures/syntax/sigils.tmt`'s "本文に落ちる `#`"
      section: its first example becomes a heading. Keep the `#タグ` case,
      which does not change.
- [x] 5. `crates/tomet-syntax-parser/src/cst.rs`'s `parse_list_item` needs
      the same shape -- the CST is a second implementation of the grammar
      inside the same crate.
- [x] 6. **Done.** `grammar.js` does not accept the new
      forms: `npx tree-sitter-cli@0.26.12 parse
      tests/fixtures/syntax/groups.tmt` reports 14 ERROR nodes. Needed:
      - `unordered_list_item`/`ordered_list_item` (grammar.js:196-217):
        accept `content_group` and `value_group` after the marker, and
        make `_list_marker_gap` optional when a group follows, so
        `- ()[ x ]` and `- [ x ]` parse.
      - `heading` (grammar.js:81): `[...]` becomes optional, `args_group`
        (grammar.js:420) is allowed, and the bracket-less sugar needs
        whitespace then `repeat($._line_item)`.
      `content_group` already admits `_newline` via `_bracket_item`, so
      multi-line content needs nothing extra.
      Regenerate with `npx tree-sitter-cli@0.26.12 generate` (works here,
      takes about two minutes through npx) and rebuild.

      Three changes, each regenerated with
      `npx tree-sitter-cli@0.26.12 generate`:
      - list items take `content_group`/`value_group`, and the gap after
        a marker is optional when a group follows;
      - `heading` takes `args_group`, makes `[...]` one arm of a choice,
        and gains the whitespace-then-content sugar;
      - `interp_call`'s callee may be a call followed by `.name`, so
        `${ref(id(x).contents(y))}` parses.

      `syntax/groups.tmt` and `syntax/interpolation.tmt` now parse with
      zero error nodes and are out of `KNOWN_TS_ERRORS`. The sweep is what
      told us to remove them.

      Still recorded, all predating this work:
      - `examples/dirs.tmt`, `examples/node_graph.tmt`,
        `examples/scenario.tmt` -- a group on the line after its element
        (`@file(x)` then `[ text ]`). The real parser allows one newline
        between groups; `inline_element`'s `repeat` has no gap.
      - `syntax/sigils.tmt` -- a `{...}` mixing a pair with a bare
        element.
      - `roadmap.tmt` -- `- [ ] item` on every line.
      - `templates/template.daily-note.tmt` -- one missing token.
- [x] 7. Fixture under `tests/fixtures/` covering: full form, multi-line
      content, `- [ x ]` with args omitted, sugar staying single-line,
      trailing attrs still attaching. Regenerate `tests/ref/` with
      `TOMET_UPDATE_REF=1`. Keep it deterministic -- no `$uuid()`/`$date()`,
      which broke the interpolation fixture on its first run.
- [x] 8. `tests/SYNTAX.md` regenerates from the parser; read the diff and
      confirm it says what we think.
- [x] 9. `docs/spec/syntax.tmt:17`: reword per the note above.
- [x] 10. Rewrite `docs/README.tmt`'s three wrapped items in the full form;
      confirm `tomet to-md docs/README.tmt` keeps each list intact.
- [x] 11. `just docs-check`, `cargo test --workspace`.

## The drift check, widened (done)

`tests/src/corpus.rs` now sweeps every fixture through the tree-sitter
grammar and holds the errors against `KNOWN_TS_ERRORS` in
`tests/src/lib.rs`. Default is "no error nodes"; exceptions are named.
Two sentinels: `MISSING_NODE` for an error node with no text, `ANY_ERROR`
for a file that fails once per line with nothing in common.

Widening it turned up:

- **five fixtures drifting unseen** -- `examples/dirs.tmt`,
  `examples/node_graph.tmt`, `examples/scenario.tmt`, `roadmap.tmt`,
  `templates/template.daily-note.tmt`. Recorded, not fixed: the sweep's
  "listed but now clean" half will force each entry out as `grammar.js`
  catches up.
- **`readme.tmt` was clean** and had been carrying three recorded error
  cases the grammar stopped making. Entry removed.
- the old readme check was **vacuous**: its marker list contained `""`,
  and `text.contains("")` is always true. A `known_ts_error_markers_are_not_vacuous`
  test now rejects an empty marker outright.

## A hole found while doing this

`AGENTS.md` says drift between the two grammar implementations "is caught
by `tomet-tests`'s `corpus` target, which runs the shared `.tmt` corpus
through both implementations". It does not. `tests/src/corpus.rs` has
exactly two tree-sitter tests, `tree_sitter_readme_fixture_...` and
`tree_sitter_cheatsheet_fixture_...`, each reading one named file. Every
other fixture -- including the two added today -- is parsed by
`tomet-parser` alone and never shown to the grammar.

So the full workspace suite passed while `grammar.js` rejected the new
syntax outright. The claim in `AGENTS.md` should either be narrowed to
what is true, or the test widened to sweep `parseable_corpus()`. The
second is the better fix and is small: the two existing tests already have
the shape, they just take a hard-coded filename.

## Why this blocks the `.md` work

The goal is to make every `.md` an export of a `.tmt`, so their links come
under `check-links`. That cannot start while the exporter turns a wrapped
list item into a loose paragraph: the first documents to convert
(`README.md`, `AGENTS.md`) are mostly wrapped prose and lists.

The two export bugs recorded in `docs/.writ.tmt`'s
`generated-md-not-edited` entry -- escaped backticks, a `<div>` for the
`@settings` header -- are **already fixed**. That entry is stale and should
be corrected when this lands.
