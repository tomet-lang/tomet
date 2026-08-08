# CommonMark interop: requirements notes

Goal: bidirectional conversion between TypedMark (`.tm`) and CommonMark
Markdown -- `typedmark` should eventually be able to import a CommonMark
file into `.tm`, and export a `.tm` document back out as CommonMark.
Scope for the first pass is CommonMark only (no GFM tables, footnotes,
strikethrough, task lists, autolinks-as-extension, etc.).

This started as a requirements/gap-analysis note before any of it was
implemented. The "Decided: native shorthand syntax" section below has
since been implemented (`typedmark-ast`, `typedmark-parser`,
`typedmark-renderer`, all tests green) -- everything else in this file
(the importer/exporter crate itself, `pulldown-cmark` integration, CLI
subcommands) is still just notes, not started.

## Current `typedmark_ast` shape

From `crates/typedmark-ast/src/lib.rs`:

- `Block`: `Heading { level, content, attrs }`, `Paragraph(Vec<Inline>)`,
  `List(Vec<ListItem>)` (flat, unordered only, no nesting), `Element`.
- `Inline`: `Text(String)`, `Element(Element)`. No dedicated node for
  emphasis, strong, or inline code -- backtick spans are recognized by the
  parser only to protect their contents from being read as a trigger
  (`` ` ``...`` ` ``), then kept as literal text including the backticks.
  There is no AST-level "this is a code span" marker.
- `Element` (`<T>(input)[area]{value}`, `@name...`): generic enough to
  represent arbitrary typed content, but nothing today gives specific
  elements (e.g. `<strong>`, `<em>`, `<pre>`) built-in rendering behavior
  beyond what `typedmark-renderer`'s HTML mapping already does ad hoc.

## CommonMark constructs with no direct `typedmark_ast` equivalent

Block-level:
- Ordered lists (with start number) -- `List` has no ordering/index field.
- Nested / multi-level lists -- `List` is a flat `Vec<ListItem>`.
- Code blocks (fenced and indented) -- no block variant preserves a raw,
  un-reflowed multi-line string with a language tag.
- Block quotes (incl. nested) -- no variant.
- Thematic breaks (`---`, `***`) -- no variant.
- Setext headings -- parseable into `Heading`, but only ATX (`#`) is
  emitted on the way back out; round-tripping the underline style would be
  lossy either way and is probably not worth tracking.
- HTML blocks -- no variant; likely out of scope entirely for v1 (would
  need an explicit decision: pass through verbatim, strip, or reject).

Inline-level:
- Emphasis (`*x*`) / strong (`**x**`) -- no `Inline` variant.
- Inline code spans -- see above; currently indistinguishable from plain
  text at the AST level (backticks survive as literal characters, which
  happens to make TypedMark's own serialization of them trivial, but a
  Markdown importer has no clean node to convert *into*).
  This also means "does this text tm-render span reflect an original code
  span" can't be recovered from the AST later.
- Hard line breaks (trailing double-space / backslash-newline) -- prose
  in `typedmark_ast` is already normalized to single spaces across
  newlines (`document.rs::normalize_text`), so hard breaks have nowhere to
  live without a new marker.
- Images -- distinct from links in CommonMark (`![alt](src)` vs
  `[text](url)`); TypedMark's closest existing construct is `@(file:..)`
  (per `docs/tmt/typedmark.tm`), which is semantically "reference to a
  local file", not necessarily "inline image". Needs an explicit mapping
  decision (reuse `@(file:..)` vs mint a dedicated `@(image:..)` key).
- Autolinks (`<https://...>`) -- collide syntactically with TypedMark's
  `<T>` element sigil (`<` followed by an identifier). Needs a parser-side
  disambiguation rule before this can be supported at all, independent of
  AST shape.

## What already maps cleanly

- ATX headings -> `Heading` (level = number of `#`, 1:1).
- Paragraphs -> `Paragraph`.
- Flat bullet lists -> `List` (ordering/nesting lost either way -- see
  above).
- Links `[text](url "title")` -> `@(url:..)[text]`, matching the existing
  spec'd inference key (`docs/tmt/typedmark.tm`: "url / file / ref / meta").
- Reference-style links/footnote-like backreferences map naturally onto
  `@links { (id)[content] }` + `@(ref:id)`, which is already spec'd as the
  TypedMark idiom for "define once, reference elsewhere".

## Open design fork (not yet decided)

Two directions were sketched when this was scoped out; picking one is a
prerequisite for implementation:

1. **Lossy-but-lossless-via-escape-hatch, no AST changes.** Map every
   CommonMark construct without a native `typedmark_ast` equivalent onto
   the existing generic `<T>[area]{value}` element grammar (e.g.
   `<strong>[text]`, `<em>[text]`, `<pre>{...}` for code blocks,
   `<blockquote>[...]`). Ordered-list numbering could survive as a `data-*`
   -style attribute on each item even though the list itself renders flat.
   Confined entirely to a new crate (e.g. `typedmark-markdown`); no changes
   to `typedmark-ast`, `typedmark-parser`, or `typedmark-formatter`.
   Faster to ship, but round-tripped Markdown won't look like idiomatic
   `.tm` and nesting/ordering fidelity is capped by what a flat `List` can
   hold.

2. **Extend `typedmark_ast` natively** (`Block::CodeBlock`,
   `Block::BlockQuote`, ordered/nested list support, `Inline::Emphasis` /
   `Inline::Strong` / `Inline::Code`). Higher fidelity, reads as "real"
   TypedMark, but touches every crate that matches on `Block`/`Inline`
   (parser, renderer, formatter, and anything downstream), and raises a
   further question of whether the `.tm` *grammar itself* should gain
   syntax for these constructs (nested lists, code fences, block quotes)
   or whether they'd only be producible by the Markdown importer.

Recommendation when this gets picked back up: start with (1) to get a
working bidirectional path quickly and learn from real `.tm` <-> `.md`
samples, then decide whether specific constructs (most likely: code blocks
and nested/ordered lists, since those are extremely common in real
Markdown) are common enough to justify promoting to native AST support.

## Decided and implemented: native shorthand syntax (2026-08-09)

Human readability is a founding TypedMark goal, not just an interop
concern -- forcing everyday formatting through `<strong>[..]`-style
generic elements would fight that goal even before any Markdown importer
exists. So before the importer/exporter crate itself, TypedMark's own
`.tm` grammar gains dedicated shorthand for the constructs that are both
extremely common in real prose *and* safe to add without colliding with
existing sigils (`<`, `@`, `#`, `-`, `` ` ``, `(`, `[`, `{`). Each form
below still desugars to the same `Element`/`Block` shapes the generic
syntax already produced, so the renderer/exporter only need to understand
one representation.

| Shorthand | Meaning | Desugars to |
|---|---|---|
| `*text*`, `_text_` | emphasis | `Sigil::Type("em")`, `area` = inner inlines |
| `**text**`, `__text__` | strong | `Sigil::Type("strong")`, `area` = inner inlines |
| `==text==` | highlight/mark | `Sigil::Type("mark")`, `area` = inner inlines |
| `---` (3+ dashes, alone on a line) | thematic break | `Sigil::Type("hr")`, no input/area/value |
| `-. item` | ordered list item | `Block::List { ordered: true, .. }` |

Rejected/adjusted from the earlier proposal:

- **No `!` sigil for images.** `!` reads as negation in most programming
  languages the target audience already knows; using it for "embed
  something" was judged confusing rather than clever. Images (and other
  embeddable references) use the existing generic `<T>(input)[area]`
  grammar with an explicit type name instead: `<embed>(file:path)[alt]` or
  `<embed>(url:..)[alt]`. This needed *zero* grammar changes -- `<embed>`
  is just an ordinary `Sigil::Type` element, distinguished from
  `@(file:..)` (a plain reference/attachment link, no particular render
  intent) by actually meaning "render this inline".
- **`1.[text]` rejected in favor of `-.`.** `1.[text]` looks like the
  "wrap in a delimiter pair" family (`*..*`, `` `..` ``, `==..==`) but
  behaves like the marker family (`- item`), which is an inconsistent
  mix. `-.` stays in the marker family (consistent with plain `-` lists)
  and sidesteps manually-typed numbers going stale when items are
  reordered -- numbering is computed at render time, not authored.
  Deliberately not offering both `-.` and `1.[text]` as equivalent
  spellings: the project's own design notes already flag the
  `@link(ref:..)` vs `[text](url:..)` dual-notation problem as a past
  mistake worth avoiding a repeat of (see
  `docs/impressions/impressions-2026-08-07.md`, point 3).
- **List nesting is still out of scope for this pass.** Both `-` and `-.`
  remain flat (`ListItem { content: Vec<Inline> }` has nowhere to hold a
  nested sub-list). Adding it means giving `ListItem` room for child
  blocks, which is a bigger structural change than anything else in this
  batch -- deferred rather than rushed.
- `Block::List(Vec<ListItem>)` became
  `Block::List { ordered: bool, items: Vec<ListItem> }` -- the one actual
  `typedmark_ast` change in this pass (small, additive, all 3 existing
  match sites updated).
- Emphasis/strong/mark delimiter matching is a simplified heuristic, not
  full CommonMark flanking-delimiter-run rules: an opening delimiter must
  not be immediately followed by whitespace, a closing one must not be
  immediately preceded by whitespace, and for `_`/`__` specifically the
  character *before* the opening delimiter must not be alphanumeric (so
  `foo_bar_baz` doesn't misfire). Unmatched delimiters (no valid close
  before EOF or a blank-line paragraph break) fall back to literal text,
  same failure mode Markdown itself has.
- Backtick code spans needed no changes -- they already survive as literal
  text including the backtick characters (see `document.rs`'s existing
  backtick handling), which happens to also be valid CommonMark on export
  with no extra work.

## Implementation sketch (for whenever this resumes)

- New crate `crates/typedmark-markdown`, added to the workspace members
  list (note: `typedmark-renderer` already exists as a crate but was
  *not* wired into `members` for a while -- double check `Cargo.toml`
  members when adding this one, see git history around 2026-08-09).
- Markdown -> TypedMark: parse with `pulldown-cmark` (confirmed available
  on crates.io, v0.13.4 as of writing) and fold its event stream into
  `typedmark_ast::Document`, rather than hand-rolling a CommonMark parser.
- TypedMark -> Markdown: hand-written serializer walking `Document`
  (no existing crate needed -- output space is much smaller than input).
- CLI: likely new `typedmark` subcommands alongside `check`/`ast`/`html`/
  `serve` (e.g. `from-md`/`to-md`, or `import`/`export`), following the
  same `PathBuf` + optional `--out` pattern as the existing `html`
  subcommand in `apps/typedmark/src/main.rs`.
- Test strategy: round-trip a handful of real-world CommonMark files
  (README-style prose, nested lists, fenced code, links, images) and
  assert the *rendered HTML* is equivalent even where the intermediate
  `.tm` representation is lossy, plus direct fixture tests for the clean
  mappings (headings, paragraphs, links).
