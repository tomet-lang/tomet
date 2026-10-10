# CST `[content]`/`|content` groups are inline-only — real LSP bug

## Motivation

Found while scoping `.agents/tasks/pipe-content-block-parity.md`. Confirmed
by a research fork plus follow-up verification, 2026-10-10:

`crates/tomet-syntax-parser/src/cst.rs::parse_content_group` (line 724)
only calls `self.parse_inline(InlineStop::Bracket)` for `[...]` content —
it never dispatches into block-level constructs (section, list, code
block, block element) the way `parse_body`/`parse_block` do at the
document root. The AST parser's twin, `element::parse_content`, instead
calls `document::parse_block_seq(cur, BlockStop::Bracket(']'))` — the same
recursive block grammar the whole document uses — so the AST correctly
represents a block element (and its own `#(id)`) nested inside
`[content]`. The CST does not.

**This is not theoretical.** `apps/lsp/src/diagnostic.rs` calls
`tomet_validator::validate_cst`, which runs duplicate-id detection via
`collect_ids_cst` (`tomet-semantics-validator/src/id.rs:85`) by walking
`root.descendants()` for `ID_GROUP` nodes. A block element's `#(id)`
nested inside `[content]` produces no `ID_GROUP` node in the CST at all
today, so a genuine duplicate id is silently invisible to this check.
Confirmed `apps/lsp` never calls the AST-based twin `validate_document`
anywhere (grep for it in `apps/lsp/src/*.rs` returns nothing) — the CST
path is the LSP's *only* duplicate-id check. Reproduces with plain
`[...]` nesting alone; `|content` is not required.

`tomet-edit`'s CST consumer (`crates/tomet-edit/src/import.rs`) is
unaffected -- it treats `CONTENT`'s text as an opaque string, never
inspects its children.

## The subtlety that makes this more than a one-line fix

Naively changing `parse_content_group` to call a `parse_body`-equivalent
instead of `parse_inline(InlineStop::Bracket)` is not safe by itself.
CST's paragraph-ending check, `paragraph_ends_after_newline`
(`cst.rs:304`), only stops a paragraph at EOF, a blank line, or the next
line starting a new block (`block_starts_at`) — it has **no concept of a
bracket boundary**. The AST's equivalent, `inline.rs`'s
`Stop::Paragraph { boundary: Option<char> }`, already tracks this
(depth-counted, so a literal `[`/`]` in prose doesn't end it early, but
the enclosing group's own closing bracket does, even mid-line with no
blank line in front of it). Without the same fix on the CST side, a
multi-line bare-paragraph content like `@card[ hello\nworld ]` would have
`parse_paragraph` swallow straight through the `]` as if it were part of
the paragraph text, corrupting everything after it.

So the real fix touches:
- `parse_body`/`parse_block`: needs a way to stop at `]` (when invoked
  from inside a content group) in addition to its existing EOF/
  section-level stop conditions.
- `paragraph_ends_after_newline` (and whatever calls it): needs the same
  bracket-boundary awareness AST's `Stop::Paragraph { boundary }` already
  has.
- `parse_content_group`: calls the now-bracket-aware body parser instead
  of `parse_inline(InlineStop::Bracket)`.
- `InlineStop::Bracket` becomes dead once `parse_content_group` stops
  calling it (it's the only call site, confirmed by grep) -- delete it
  rather than leave it orphaned, since nested `[...]` will now be handled
  structurally by the recursive grammar itself, not by raw depth-counting
  over bracket characters.

## Out of scope (separate task)

`|content`'s own asymmetry (`parse_pipe_content` not supporting
sections/fences at all) is `.agents/tasks/pipe-content-block-parity.md`'s
other open question — a design decision about feature parity, not a bug.
Not touched here. This task is scoped to the CST `[...]` block-recursion
bug only.

Also out of scope: whether `tree-sitter-tomet`'s `grammar.js` has an
analogous limitation (the research fork noted its `content_group`/
`marked_content` fields look like they reuse the top-level grammar's
block-repetition machinery, so it may *not* share this bug, but this
wasn't verified in depth). Worth an independent check later, not blocking
this fix.

## Steps

- [ ] 1. Add bracket-boundary awareness to the paragraph-stop path:
      give `InlineStop::Paragraph` (or `parse_inline`/
      `paragraph_ends_after_newline`) a way to know "also stop if the
      next line's first non-whitespace token is `]`", mirroring AST's
      `Stop::Paragraph { boundary: Option<char> }`. Decide the mechanism
      (a field on `CstParser` toggled around content-group parsing,
      consistent with how `self.allow_connect` is already threaded; vs.
      a parameter threaded through `parse_body`/`parse_block`/
      `parse_paragraph`/`parse_inline`) -- check which is more
      consistent with this file's existing style before choosing.
- [ ] 2. Give `parse_body` (or a new wrapper) a bracket-stop condition:
      break the loop when the current token is `]` and we're inside a
      bracket context, in addition to the existing EOF/section-level
      stop. Nested `parse_section`'s own recursive `self.parse_body(level)`
      call needs to inherit the bracket-awareness too (sections don't
      have a separate body-scanner in CST -- they recurse into
      `parse_body` directly, confirmed by reading `parse_section`,
      line ~380).
- [ ] 3. Rewrite `parse_content_group` to call the bracket-aware body
      parser instead of `parse_inline(InlineStop::Bracket)`. Keep the
      `allow_connect` toggle exactly as it is now.
- [ ] 4. Delete `InlineStop::Bracket` and its one match arm in
      `parse_inline` (dead once step 3 lands -- confirm with a fresh grep
      before deleting, in case step 1-3 ended up using it after all).
- [ ] 5. Add regression tests:
      - CST: a block element with `#(id)` nested inside `[content]`
        actually produces a `BLOCK_ELEMENT`/`ID_GROUP` node (not just
        inline text) -- extend `cst.rs`'s own test module (see
        `inline_elements_nest_in_content`/`id_group_is_structured_and_roundtrips`
        for the existing style to match).
      - Validator: two elements with the same `#(id)`, one of them nested
        inside another element's `[content]`, are now caught as a
        duplicate by `validate_cst` (`tomet-semantics-validator/src/tests.rs`).
      - The multi-line bare-paragraph-inside-brackets case from the
        "subtlety" section above (`@card[ hello\nworld ]` or similar),
        confirming the `]` is not swallowed and round-trips losslessly --
        extend `test_cst_lossless_roundtrip_all_syntaxes` or add a
        focused case.
- [ ] 6. Run `cargo test -p tomet-parser` (the CST tests live in this
      package), then `cargo test --workspace` for the full picture
      (validator, LSP). Then `just docs-check` equivalents (`tomet
      check`/`format --check`/`export --check`/`api --check`) --
      `twrit check` itself is confirmed broken in this environment
      regardless (pre-existing, unrelated).
- [ ] 7. Check whether any doc comment now describes stale behavior
      (e.g. `parse_content_group`'s own, if it has one; any prose
      elsewhere describing the CST as deliberately inline-only for
      content groups).

## Status

Not started. Task file created 2026-10-10 after investigation (a
research fork plus manual verification) confirmed this is a real bug,
not a design question. Workspace: `.agents/workspaces/cst-content-group-blocks`.
