# Core Grammar Freeze (2026-08-16)

Pillar 4 of `docs/reviews/2026-08-12-impressions.md`'s 1.0 roadmap asked
for two things: finalize the core grammar, and keep `typedmark-parser`
and `tree-sitter-typedmark` in sync. Both are in place, so this declares
the core grammar frozen for the 0.1 -> 1.0 track.

## What "frozen" means here

`typedmark-parser` is the source of truth for the grammar (see
`docs/develop/architecture.md`). "Frozen" means: the constructs currently
accepted or rejected by `typedmark-parser`, as documented in
`docs/ja/specifications/*.tm`, do not change silently. A change to what
parses, what it means, or what's an error requires:

1. An explicit decision recorded in `docs/reviews/` (or a successor of
   this file), not just a parser diff.
2. `docs/ja/specifications/*.tm` updated to match.
3. `crates/tree-sitter-typedmark`'s `grammar.js` updated in the same
   change if the construct affects highlighting, and
   `cargo test -p tree-sitter-typedmark` kept green -- that crate's
   `*_fixture_has_only_known_error_cases` tests are what catch the two
   grammars drifting apart (see `docs/develop/architecture.md`).

This is a freeze on *breaking* changes to already-decided syntax, not a
promise that the language stops growing. Constructs marked `// 未実装`
throughout `docs/ja/specifications/*.tm` (variable expansion `$()`/
`${}`, dotted-key nesting shorthand `group.key:value`, `@import`, the
`kdl` embedded format, `@this`, and the `<icon>`/`<index>`/`<callout>`
built-ins) don't exist yet and remain open to design -- adding one of
them is new surface, not a break of frozen surface.

## What's frozen

- **Element shape**: `<T>(args)[content]{value}` / `@name(args)[content]
  {value}`, each group optional and reorderable, but capped at one each
  (a second `(args)` etc. is an error) -- see `docs/ja/specifications/
  syntax.tm`'s `#[ 雛形 ]`/`#[ 派生 ]`.
- **`[]` is reserved for element content spans only** (`[content]`,
  `#[ Heading ]`). It is not overloaded onto list items. List items are
  `- (marker) content` / `-. (marker) content`, single-line
  (`Stop::Line` in `crates/typedmark-parser/src/document.rs::parse_list`);
  multi-line or nested content under a list item is written as a nested
  `<T>()[...]` element, not by wrapping the item in `[]`. Resolves
  `2026-08-07-impressions.md` item 2 and `2026-08-12-impressions.md`
  Pillar 1 -- see `docs/reviews/2026-08-16-syntax-decisions.md`.
- **One link mechanism, not two**: `@(url:...)`, `@(file:...)`,
  `@(ref:...)` are the only reference-inferring forms
  (`typedmark-semantics::ElementKind::{Url,File,Ref}`); `@(url:...)
  [Wiki]` and `@[Wiki](url:...)` are the same element with `args`/
  `content` reordered, not competing constructs. Resolves
  `2026-08-07-impressions.md` item 3.
- **The parser stays pure**: `typedmark-parser` performs zero I/O and no
  dynamic evaluation (the "Deterministic Static Parser Boundary" in
  `docs/develop/architecture.md`). File-referencing constructs
  (`@settings(file:...)`) resolve in `typedmark-resolve`, not the parser.
  Resolves Pillar 2.
- **Every AST node carries a `Span`** (line, column, byte offset) --
  already fully integrated per `docs/develop/architecture.md`, closing
  Pillar 3.
- **Embedded formats stay an escape hatch, not a `Value` syntax
  expansion**: `format: json|yaml|toml` inside `{value}`, not new
  `Value` grammar competing with dedicated serialization formats.

## What this unblocks

Tooling that depends on the grammar being stable (`typedmark-validator`'s
schema/lint work chief among them) can now proceed without the rework
risk `2026-08-07-impressions.md` item 4 flagged.
