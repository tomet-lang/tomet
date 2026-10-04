# `@conflict` in std, and the content-model fix it depends on

Goal: promote immermemo's `@mobile.conflict` (its `crates/merge`) into tomet's
`std`, as a generic `@conflict` element for "two divergent candidates, pick
one." This repo only has tomet's half: the marker shape + tooling support
(`tomet check`, LSP, etc.). immermemo's own adoption of std's `@conflict` is
separate, later work in that repo -- not started, not planned here.

## Decisions made with the user (do not re-litigate without a new reason)

- Namespace: `std`, bare `@conflict`. Not `doc`: `doc` is for "tomet owns the
  syntax, the app owns the semantics" (e.g. `doc.icon` -- tomet can never
  resolve an icon name, only the app can). `conflict` is semantics tomet
  itself manages (`tomet check`/the indexer actually read and report it),
  which is the opposite situation. Not a new reserved namespace either: that
  would need the exact same `RESERVED_NAMESPACES`-style hardcoding `doc` got,
  for no payoff over `std`, and would split `conflict` from `draft`/`fixme`
  (same category, already bare-std) for no reason.
- Side labels: neutral, non-perspectival, `a`/`b` (not `mine`/`theirs` --
  "mine" is perspective-relative, and unlike git's ephemeral
  working-tree-only conflict markers, this marker is written into a file
  that gets synced on to other devices/viewers before it's resolved, so a
  perspective word baked into the text can go stale/wrong the moment someone
  else reads it).
- No "end" marker, and no separate "range-marker triple" form at all. That
  was only ever needed because the old content model couldn't hold multiple
  blocks in a slot, forcing three bare markers scattered through the flat
  block/inline sequence (`mine`/`theirs`/`end`) to delimit a span by
  position. Once `content` is `Vec<Block>` (see below), `@conflict(a: ...,
  b: ...)` is ONE self-closing element whose `a`/`b` can themselves hold full
  block content directly -- no scattered markers, no terminator needed. The
  earlier "block/inline range-marker form" vs. "value-slot form" distinction
  collapses into this single grammar: `a`/`b` hold a scalar when `@conflict`
  sits in a `{data}`/`(args)` value position, and block content when it sits
  in the document body. Same element shape either way.
- No timestamp/provenance field on the element -- that's git's job (commit
  history), not the document's.
- `children` stays a separate field from `content` (do NOT merge them, this
  was tried and corrected mid-design). A list item's nested sub-lists and an
  element's own content are different relationships even once both are
  `Vec<Block>`-typed.
- `Element.content: Vec<Inline>` must become `Vec<Block>`, uniformly, for
  every element -- not gated by element identity (the parser stays
  vocabulary-free, a hard project-wide rule) and not gated by `Placement`
  either. Simplest: always `Vec<Block>`; a bare run with no block markers
  just becomes `[Paragraph(...)]` naturally. Legality ("can `@em` take 2
  paragraphs?") is NOT the parser's job -- it's `tomet-semantics`'s, via
  `@content{allow:}` for vocabulary elements and a new builtin content-shape
  table for std's own 37.

## What's already real (verified against current, non-deprecated code --
## don't re-verify unless something looks off)

- `tmtroot/docs/` is the current spec. Top-level `docs/` was stale and has
  been moved to `tmtroot/deprecated/docs/` (commit `0d61be8`). Ground every
  claim in `tmtroot/docs/` + actual source, never the old location.
- `Element.content: Option<Vec<Inline>>`, `Element.children:
  Option<Vec<Block>>` (`crates/tomet-syntax-ast/src/lib.rs:702-712`).
- `[content]` and `|content` parse identically (`crates/tomet-syntax-parser/
  src/element.rs:357-358`, pinned by `tests/src/pipe.rs`). No per-name
  branching anywhere in `parse_groups`.
- `children` is populated ONLY for list items' nested sub-lists
  (`crates/tomet-syntax-parser/src/list.rs:201-220`) -- not a general
  "indented block" container. Don't assume it's wider than that.
- `list(...)`/`enum(...)` already parse to `Value::Call(String, Vec<Value>)`,
  fully implemented and tested (`crates/tomet-syntax-parser/src/tests/
  values.rs`). `[a, b]` array literals were retired in its favor. This is
  what `@content{allow: list(...)}` should read -- no new value grammar
  needed.
- `@args` is read by `ElementDecl`; `@data`/`@content` are not yet
  (`crates/tomet-semantics/src/vocabulary.rs:78-81`).
- `:rule(allow:list(...), direct:@bool)` (`crates/tomet-semantics-validator/
  src/rule.rs`, enforced by `lib.rs:412-430`'s `check_one_rule`) is the exact
  template for `@content{allow:}`'s eventual decoder + enforcement: same
  `Value::Call` decode shape, same `for_each_descendant`/`classify_in`
  comparison. Different axis though -- `:rule` constrains one *position* in
  one document; `@content{allow:}` constrains a *vocabulary-declared
  element's* `[content]`/`|content`, everywhere it's used.
- `BUILTIN_KINDS` (37 entries), `required_shape`/`builtin_region`/
  `builtin_singleton`/`shape_mismatch` (`crates/tomet-semantics/src/
  kind.rs:261-576`) is the current, authoritative builtin table. No existing
  axis for "may this kind's content hold other blocks" -- needs a new table
  alongside these four.
- `RESERVED_NAMESPACES = ["doc"]` (`crates/tomet-semantics/src/
  vocabulary.rs:137`) -- `doc.*` needs no `@use`, hardcoded like `std`.
  Confirmed by direct `tomet check` test, both with and without `@use(doc)`.
- `tomet-format-printer`'s `render_element`/`render_inlines` call
  `el.content` as `Vec<Inline>` directly at 8+ sites; `children` only prints
  through `render_list_with_indent`'s separate indent logic. Unifying
  content to `Vec<Block>` needs a new general block-renderer (generalizing
  that indent logic) plus a single-paragraph fast path so ordinary inline
  usage (`@link(...)[Tomet]`) keeps printing on one line, unchanged.

## `|content`'s own fix, needed for step 3 (decided)

`|`-content (`crates/tomet-syntax-parser/src/inline.rs`'s `Stop::PipeRun`/
`pipe_run_continues`/`split_softbreaks`'s `fold_pipes` branch) currently
folds an empty marker line (`|` with nothing after it) into a plain
`SoftBreak`, same as an ordinary wrapped line -- so `|content` can never
express more than one paragraph, even though `pipe_run_continues` already
treats a bare `|` line as "run continues" (only a line with no `|` marker
at all ends the run; confirmed by testing, not just reading). Fix: when
building the new block-sequence parser for `|content` (step 3), a bare
marker line (nothing after the `|`) ends the current paragraph and starts
a new one, instead of folding into the same paragraph -- symmetric with a
blank line inside `[...]`. A line with real content after the marker keeps
joining the current paragraph exactly as today.

This is a single-level fix only (one active marker column). Multi-level
nested `|content` (an element's own `|content` living inside another
element's `|content`, needing one marker per ancestor level per line) is a
separate, deferred problem -- see `.agents/tasks/nested-pipe-markers.md`.
Not needed here: `@conflict`'s spec uses the bracket form for block
content, not `|`.

## Step plan (not started)

0. Spec first: write the new content model + `@conflict` into
   `tmtroot/docs/spec/` (never the deprecated `docs/`). Review with the user
   before touching code.
1. Classify all 37 `BUILTIN_KINDS` (+ new `conflict`) into inline-only vs.
   block-permitting content, explicitly, together with the user.
2. AST: `Element.content` -> `Vec<Block>`. `children` untouched (stays
   list-sub-list-only, a separate field).
3. Parser: `[...]`/`|...` always parse as `Vec<Block>`, via the same
   recursive block grammar `Document.blocks`/`Section.blocks` already use.
   No element-identity branching.
4. Semantics: new builtin content-shape table (parallel to
   `required_shape` etc.) for the 37; implement `@content{allow:}` reading
   (via `Value::Call`, mirroring `decode_rule_args`) + enforcement
   (mirroring `check_one_rule`/`for_each_descendant`).
5. Printer: new general block-content renderer + single-paragraph collapse
   fast path. Verify round-trip (`tomet roundtrip`/`format --check`) stays
   stable across the existing corpus -- not to preserve old accidental blank
   lines (surfacing those is fine and expected), but because ordinary
   single-paragraph content must keep printing the way it does today.
6. Add `@conflict` to `BUILTIN_KINDS`, `std`, bare. Shape/content-shape per
   step 1.
7. Fixtures under `tests/fixtures/`: block content in a content-permitting
   element, an inline-only element correctly rejecting multi-block content,
   `@conflict(a: ..., b: ...)` both as a document-body element (block
   content in `a`/`b`) and embedded as a `{data}`/`(args)` value (scalar
   `a`/`b`). `cargo test -p tomet-tests -p tree-sitter-tomet`
   (grammar-affecting change, per `AGENTS.md`).
8. Update convert-html/markdown/pandoc/typst, LSP, TUI consumers of
   `.content`.
9. OUT OF SCOPE here: immermemo's (and tomet-web-editor/tomet-zed/
   tomet-tui/tomet-book's) own adoption once tomet ships this. Separate task,
   separate repo, later -- per immermemo's own `AGENTS.md`, a gap in what
   tomet exposes is a feature request against this repo, not something to
   route around there.

## Status

Step 0 done (spec written, not yet reviewed line-by-line with the user):
- `tmtroot/docs/spec/feature/content-shape.tmt` -- the `Vec<Block>` content
  model, `@content{allow: list(...)}`, builtin content-shape split.
- `tmtroot/docs/spec/elements/std-experimental/conflict.tmt` -- `@conflict`
  itself, single `(a: ..., b: ...)` form (no "end", no separate range-marker
  form -- see decisions above).
- `tmtroot/docs/spec/elements.index.tmt` -- `@conflict` added under
  Experimental, next to `draft`/`fixme`.
- `tmtroot/docs/spec/features.index.tmt` -- "content shape" entry added.

Step 1 done -- content-shape classification of all 37 `BUILTIN_KINDS` +
`conflict`, confirmed with the user:

- **N/A** (doesn't use `[content]`/`|content` at all, untouched by this
  change): `kind` `version` `meta` `config` `settings` `use` `include`
  `blueprint` `vocabulary` (directives, `{data}`/`(args)` only) -- `hr`
  (self-closing) -- `ol` `ul` (their items' body is `value:
  ElementValue::Children`, not `content`) -- `raw` (body is the `+++...+++`
  fence, `ElementValue::Raw`, exclusive with `[content]`) -- `args` `data`
  (their `@param` entries sit in `{...}`/`ElementValue::Group`'s
  `Entry::Element`, not `[content]`) -- `tag` (no real usage found, default
  inline, revisit if one shows up).
- **Inline-only** (content stays a short single run, no nested blocks):
  `element` `param` (vocabulary-doc prose description) -- `draft` `fixme`
  (one-line note) -- `file` `dir` `link` `embed` (display label) -- `em`
  `strong` `mark` `strikeout` `ruby` (character-level, running text) --
  `heading` (title string).
- **Block-permitting** (content may hold multiple paragraphs/headings/etc.):
  `quote` (blockquotes commonly span multiple paragraphs -- today's parser
  can't do this at all, likely the most user-visible fix in this whole
  change), `callout`, `card`, `table` (cells/rows), `footnote` (can be
  long-form), `conflict` (new).
- `references` dropped from consideration entirely -- a draft/placeholder
  element in the same category the now-removed `@links` was (see `git log`
  `d25c698`), not worth curating here. Whether `references` itself gets
  removed from `BUILTIN_KINDS` is a separate, undecided question -- not
  touched by this task.

Steps 2+3 done (AST + parser): `Element.content` is `Vec<Block>`.
`children` stays separate, untouched, list-sub-lists-only. Key points for
whoever picks this up next:

- `tomet-syntax-parser/src/document.rs`: `parse_document`'s old loop body
  was extracted into `parse_block_seq(cur, stop: BlockStop)`, parameterized
  by `BlockStop::Eof` (top level) or `BlockStop::Bracket(char)` (bracket
  content, stops before the closing char, errors on EOF instead of
  silently stopping). `Stop::Paragraph` (inline.rs) grew a `boundary:
  Option<char>` field so a paragraph parsed inside bracket content also
  stops at the bracket's own closer (bracket-depth-tracked, so a literal
  `[`/`]` in prose doesn't end it early) -- without this, `@card[ Hello
  world ]` would have the paragraph scanner run straight past the `]`.
- `element.rs::parse_content` (bracket form) now calls `parse_block_seq`.
  `parse_pipe_content` (`|` form) is its own loop: a marked line that
  bare-opens a block element (`@name` ending its own line) becomes its own
  `Block::Element`, matching bracket-form sibling-by-sibling (this is what
  `tests/src/pipe.rs`'s parity test requires -- originally missed, caught
  by that test); a line that's the marker alone (nothing after it) ends
  the current paragraph without ending the whole run, the only way
  `|content` expresses >1 paragraph (`crate::inline::PipeContinuation`,
  `pipe_run_state`). Multi-level nesting still punted to
  `.agents/tasks/nested-pipe-markers.md`.
- `section.rs::title_from_content_blocks`: a heading's title
  (`Section.title: Vec<Inline>`, unchanged type) is extracted from the
  parsed `Vec<Block>` -- one `Paragraph` is the normal case; a single bare
  element (`=[ ${x} ]`, nothing else) is demoted to `Placement::Inline` and
  taken directly (same demotion `continue_into_paragraph` already does
  elsewhere); anything else is a parse error, since `Section.title` has no
  field to hold a real violation -- this one spot can't defer to
  `tomet-semantics` the way other content-shape rules will.
- Every downstream consumer of `.content` updated to match: all of
  `tomet-semantics*`, `tomet-format-printer` (new `render_content_blocks`:
  single-paragraph fast path byte-identical to before, else `render_block`
  per block, no blank line between them -- same joining `render_section`
  already uses, see `.agents/tasks/man-import-and-printer-gaps.md`),
  `tomet-convert-{html,markdown,pandoc,typst}` (pandoc's `from_pandoc.rs`
  also gained `pandoc_blocks_to_content`, converting a `Div`/`BlockQuote`/
  `Figure`/list-item/footnote body through real `block_from_pandoc` now,
  not the old flatten-to-one-inline-run `blocks_to_content` -- needed for
  `a_block_inside_content_comes_back_block_placed` to keep passing),
  `tomet-transform`, `apps/lsp`, `apps/cli`. `tomet_tree::for_each_descendant`'s
  `direct_only` branch needed a fix too: a `Block::Paragraph` in `content`
  isn't itself an element, but its own inline items (an `@`-element placed
  *inline*, e.g. `@outer[ @mid[...] ]` all on one line) are still direct
  children of `el` for `:rule(direct:true)`'s purposes -- missed on the
  first pass, caught by `tomet-tree`'s own `direct_only_stops_at_the_first_level`.
- `cargo build`/`cargo test --workspace` (minus `tomet-python`, unrelated
  pyo3 build-env issue in this sandbox) both fully green, including
  `tests/src/pipe.rs` and the `tomet-tests` snapshot suite (references
  regenerated with `TOMET_UPDATE_REF=1`, confirmed by hand first -- the
  diffs were `@card`/etc. now rendering as proper block siblings instead
  of being squashed into `<span>`s on one line, the intended effect of
  this whole change, not a regression).

Step 4 done (builtin content-shape table + `@content{allow:}` reading and
enforcement):

- `tomet-semantics/src/kind.rs`: new `ContentShape` enum (`Inline`/
  `Block`) and `builtin_content_shape(kind) -> Option<ContentShape>`,
  exhaustively matching every `ElementKind` variant (compiler-checked, so
  a future new kind can't silently go unclassified) -- the Step 1 table
  above turned into code. `references` stays unclassified (`None`), same
  reasoning as Step 1.
- `tomet-semantics/src/vocabulary.rs`: `ElementDecl` gained
  `content_allow: Option<ContentAllow>`, read from an `@element`'s own
  `@content{allow:}` child the same way `params_from_element` finds
  `@args` (search `{...}`'s children by classified kind). `ContentAllow`
  has `Names(Vec<Name>)` / `Inline` / `Any`. Correction to the old "can't
  be read even in principle" comment that used to sit on `ElementDecl`:
  `list(...)` already parses fine (`Value::Call`); `element_data`'s own
  `normalize_data_value` turns it into `Value::Seq` before this ever sees
  it, so the decoder matches `Value::Seq`, not `Value::Call`.
- `tomet-semantics-validator`: new `Diagnostic::ContentNotInline` (a kind
  whose content-shape is `Inline`/`ContentAllow::Inline` holds more than
  one plain paragraph) and `Diagnostic::DisallowedInContent` (a name not
  in an `@content{allow:list(...)}`), both default `Severity::Error` via
  the existing wildcard. New `check_content_shape`, wired into
  `validate_document_with` alongside the other checks. `@content{allow:}`
  enforcement is deliberately **one level only** (direct content items,
  not recursed into found elements' own content) -- unlike `:rule`'s
  default -- so an outer `allow:` list doesn't double as a rule for a
  found element's *own* nested content, which gets checked independently
  when `for_each_element` reaches that element itself. New
  `for_each_direct_content_element` helper does this one-level walk
  (`Block::Element` directly, or `Inline::Element` inside a
  `Block::Paragraph` -- an element placed inline rather than at its own
  line start).
- Full `cargo test --workspace` (minus `tomet-python`) stays green with
  the new check wired in live -- no existing fixture anywhere in the repo
  trips `ContentNotInline`/`DisallowedInContent`, and no vault's existing
  `.tomet/vocabularies/*.vocabulary.tmt` declares `@content{allow:}` yet
  (brand new feature), so `content_allow` is `None` (no check) for every
  real custom element today.

Not yet started: step 5 (printer round-trip stability double-check --
largely already covered by `render_content_blocks`'s design in the prior
commit, but not separately re-verified against the full corpus beyond
what `cargo test` already does) onward. Step 6 (add `@conflict` itself to
`BUILTIN_KINDS`) needs `builtin_content_shape`/`required_shape`/etc. each
to grow a `Conflict` arm once it exists.
