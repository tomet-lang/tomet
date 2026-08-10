# `typedmark-formatter` mangles content inside raw/verbatim areas

## Status

Not started -- filed as a follow-up, split out of
`.agents/tasks/memo-area-content-fidelity.md` at the user's request (kept
separate on purpose, not implemented as part of that task).

## Problem

`typedmark_formatter::format_source` (`crates/typedmark-formatter/src/lib.rs`)
is a blind, line-based whitespace pass: trims trailing whitespace per line
and collapses runs of blank lines, with no `Document`/AST awareness at all.
Its own doc comment claims this is a no-op as far as
`typedmark_parser::parse_document` is concerned, backed by crate tests --
but that claim only holds for ordinary prose, where a blank line/trailing
space genuinely carries no meaning.

It does **not** hold for raw/verbatim areas (`<codeblock>[...]`, and the new
opt-in `area:raw` from the memo-content-fidelity task): those preserve
source bytes -- including blank lines and trailing whitespace -- literally.
A blank line inside a codeblock (a real, meaningful empty line in the
source code) or inside a memo's `area:raw` (a paragraph break the author
intended) gets silently collapsed/trimmed whenever this formatter runs.

This is wired into paths that run automatically, not just on explicit
manual invocation:
- `apps/typedmark-lsp/src/lib.rs::format_edits` -- `textDocument/formatting`,
  i.e. editor format-on-save.
- `apps/typedmark/src/main.rs` -- the CLI's format command.

## Why it matters now

Not a new bug (already existed for `<codeblock>`), but the memo/notes use
case this repo is adding `area:raw` for is exactly the case most likely to
contain meaningful blank lines (paragraph breaks in freeform notes), so the
blast radius is bigger going forward.

## Likely direction (not decided/started)

`format_source` would need enough structure-awareness to locate raw/
verbatim spans (codeblock's `[...]`, any `area:raw` element's `[...]`) and
skip its line-trim/blank-collapse pass inside them, while still applying it
to everything else. This likely means either:
- parsing enough to find those spans' byte ranges and excluding them from
  the line-based pass, or
- switching to a real AST-aware reprint (the crate's own doc comment
  already notes `Document` carries no span/position info today, which is
  why a lossless reprint isn't possible yet -- that gap would need
  addressing too, or a parallel raw-span-finding pass that doesn't need
  full span info, just start/end offsets of `[...]` raw bodies).
