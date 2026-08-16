# Syntax Stabilization Check-in (2026-08-16)

Follow-up on two open questions from `2026-08-07-impressions.md` (item 2,
`[]` reuse colliding with list syntax; item 3, dual link notation) and
`2026-08-12-impressions.md` Pillar 1 (strict `[]` disambiguation). Both
are confirmed resolved in the current implementation, not just in the
roadmap doc -- verified against code, not just docs:

1. **`[]` collision with lists**: `parse_list` in
   `crates/typedmark-parser/src/document.rs` reads each list item's
   content with `Stop::Line` -- it never opens a `[...]` group. `[]` stays
   reserved for element content spans (`<T>()[content]{value}`,
   `#[ Heading ]`, ...). The `-[ multi-line ]` / `--(T)[ ... ]` outline
   syntax explored in `docs/develop/idea.tm` was not adopted; multi-line
   or nested content in a list item is written as a nested `<T>()[...]`
   element, not by wrapping the item itself in `[]`. This matches Pillar
   1 of `2026-08-12-impressions.md` ("Keep List Syntax Standard").

2. **Dual link notation**: `typedmark-semantics::ElementKind` has no
   separate `Link` kind -- only `Url`/`File`/`Ref`, inferred from the
   `(url:...)`/`(file:...)`/`(ref:...)` key inside a bare `@(...)`
   element. `@(url:...)[Wiki]` and `@[Wiki](url:...)` are the same
   element with `args`/`content` reordered (see `docs/ja/specifications/
   syntax.tm`'s `#[ 柔軟性 ]` section), not two competing constructs.

`docs/develop/idea.tm` still carried the pre-decision brainstorming for
item 1 as if it were open; trimmed it down to a pointer to this doc so
the file doesn't mix settled and unsettled ideas (the exact split
`2026-08-07-impressions.md` item 1 warned about).

Still open from the 2026-08-12 roadmap: Pillar 4 ("Freeze Core Grammar")
has no formal freeze declaration yet, even though the two concrete
blockers it was gating on (this doc's items 1-2, plus Pillar 3's AST span
metadata -- already done per `docs/develop/architecture.md`) are cleared.
