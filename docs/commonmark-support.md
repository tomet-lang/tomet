# CommonMark interop: requirements notes

Goal: bidirectional conversion between TypedMark (`.tm`) and CommonMark
Markdown -- `typedmark` should eventually be able to import a CommonMark
file into `.tm`, and export a `.tm` document back out as CommonMark.
Scope for the first pass is CommonMark only (no GFM tables, footnotes,
strikethrough, task lists, autolinks-as-extension, etc.).

This started as a requirements/gap-analysis note before any of it was
implemented. Two things have since landed (see the "Decided and
implemented" sections below, in chronological order): TypedMark's own
native shorthand syntax (`typedmark-ast`/`typedmark-parser`/
`typedmark-renderer`), and the `typedmark-markdown` importer/exporter
crate itself. Still not started: a `Document -> .tm source text`
serializer (belongs in `typedmark-formatter`, currently an unstarted
stub) and the `from-md` CLI subcommand that depends on it.

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

## Decided and implemented: `typedmark-markdown` crate (2026-08-09)

The importer/exporter crate sketched below has been built:
`typedmark_markdown::from_markdown(&str) -> Document` and
`typedmark_markdown::to_markdown(&Document) -> String`, plus a `typedmark
to-md <file>` CLI subcommand (mirrors `html`'s `PathBuf` + optional
`--out` pattern). All existing tests plus 26 new ones in the crate are
green (`cargo test --workspace`); `cargo clippy` is clean on the new/
touched crates.

- Markdown -> TypedMark: `pulldown-cmark` 0.13, `Options::empty()`
  (CommonMark only, no GFM extensions -- matches this doc's stated
  scope), event stream folded directly into `Document` with an explicit
  per-open-tag frame stack (`crates/typedmark-markdown/src/import.rs`),
  no intermediate tree.
- TypedMark -> Markdown: hand-written serializer
  (`crates/typedmark-markdown/src/export.rs`), as sketched.
- Clean mappings, as predicted: ATX headings, paragraphs, flat lists
  (ordered via `1.`/`2.`/.., unordered via `-`), links, and -- thanks to
  the native shorthand added earlier the same day -- emphasis/strong
  (`*`/`**`), thematic breaks (`---`), and code spans (backticks survive
  as literal text either direction) needed no new AST work at all.
- Autolinks (`<https://...>`) needed no disambiguation rule after all:
  `pulldown-cmark` already resolves `<scheme:...>` to a `Link` event
  (`LinkType::Autolink`) during parsing, so the collision this doc
  originally worried about (`<` as both TypedMark's element sigil and
  Markdown's autolink delimiter) never reaches `typedmark-markdown` --
  it's resolved one layer down, before any TypedMark-shaped text exists.
- Two new generic-element mappings (direction-1 escape hatch, no
  `typedmark_ast` changes): `<pre>(lang:xxx){code}` for fenced/indented
  code blocks, `<blockquote>[...]` for block quotes. Both also needed
  new render cases in `typedmark-renderer` (`pre` -> `<pre><code
  class="language-xxx">`, `blockquote` -> `<blockquote>`), since neither
  kind existed before this pass.
- Images -> `<embed>(file:..)[alt]` or `<embed>(url:..)[alt]` as decided
  earlier; the key is picked by a `dest.contains("://")` heuristic since
  Markdown's `![alt](dest)` doesn't distinguish local paths from URLs
  itself.
- Confirmed-lossy, as this doc predicted, with the fallback each takes:
  - Nested lists flatten to sibling items in the same flat list (no
    `ListItem` nesting slot). `- a\n  - b\n- c` imports as three
    siblings `a`, `b`, `c`.
  - Block quotes containing more than one block (e.g. two paragraphs, or
    a paragraph plus a nested list) get their content joined into a
    single inline run, space-separated (`Element::area` is `Vec<Inline>`,
    not `Vec<Block>`). Single-paragraph quotes -- the common case --
    round-trip cleanly.
  - Hard line breaks collapse to a single space, same as soft breaks
    (prose is already space-normalized elsewhere in this codebase).
  - HTML blocks and inline HTML are dropped entirely on import (decided
    out of scope, per the original gap analysis below).
  - `mark` (`==text==`) has no CommonMark form; exports as raw inline
    HTML `<mark>...</mark>` (valid CommonMark, round-trips through
    `pulldown-cmark` back to the same `mark` element since raw HTML
    survives parsing as `InlineHtml` -- but that's dropped on import per
    the point above, so a `.tm` -> `.md` -> `.tm` round trip loses it;
    only `.tm` -> `.md` -> HTML rendering is lossless here).
  - Any other hand-authored `<T>`/`@name` element with no dedicated
    mapping (`@links{}`, arbitrary generic elements) exports as raw
    `<div data-tm-kind="...">`/`<span data-tm-kind="...">` HTML, same
    reasoning.
- Heading `id`/`cssclass` attrs have no CommonMark form and are dropped
  on export (ATX headings can't carry them without an extension).
- Test strategy, as sketched: direct fixture tests per construct in both
  directions, plus round-trip tests in `crates/typedmark-markdown/src/
  lib.rs` that go Markdown -> `Document` -> Markdown -> `Document` and
  assert the *rendered HTML* (via `typedmark-renderer`) is equal, so the
  intermediate `.tm` shape is free to be lossy as long as the second
  parse produces something that renders identically to the first.

### Still open: `from-md` (Markdown -> `.tm` source text)

`typedmark-markdown` only goes as far as the in-memory `Document`; there
is still no `Document -> .tm source text` serializer, so there's no
`typedmark from-md` CLI subcommand yet (only `to-md`, the direction that
was actually buildable this pass, since `.tm` parsing into `Document`
already existed). That serializer is `typedmark-formatter`'s job by
name, and that crate is still the unmodified `cargo new` stub in
`crates/typedmark-formatter/src/lib.rs` -- writing a one-off serializer
inside `typedmark-markdown` instead would duplicate that crate's stated
purpose, and risks emitting `.tm` text that doesn't actually re-parse
(e.g. how a fenced code block's raw multi-line string round-trips
through `typedmark-parser`'s value grammar was never checked against the
real parser). Whoever picks this back up should implement
`typedmark-formatter` first (general `Document -> .tm` pretty-printer,
useful on its own beyond Markdown interop), then wire `from-md` on top
of it.
