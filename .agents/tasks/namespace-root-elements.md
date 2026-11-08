# Namespace root elements and raw-only embeds (`@typst`, `@mermaid`, `@latex`)

Decided by the author on 2026-11-07. Not started.

Relates to `tmtroot/spec/features/vocabulary.tmt`, `tmtroot/spec/features/content-shape.tmt`,
`.agents/tasks/nexus-syntax.md` (`()` args, `{}` data, `[]` sugar), and the vocabulary definitions under
`../vocabularies/vocabularies/{typst,mermaid,latex}/`.

## Decided

- **A namespace (vocabulary) may itself declare a root element (callable namespace):**
  - A vocabulary namespace `ns` (in scope via `::use(ns)` or `::kind(ns)`) is not restricted to acting solely as a
    prefix for child elements (`@ns.child`). When the vocabulary declares a root element, `@ns` itself is a valid
    element.
  - Examples of how root and child elements pair across languages:
    - `@mermaid` — embeds a Mermaid diagram directly at the namespace root, avoiding stutter (`@mermaid.mermaid` /
      `@mermaid.diagram`).
    - `@typst` alongside `@typst.math`, `@typst.canvas`, etc. — embeds raw Typst markup/code at the root, while child
      elements represent specific Typst modes or constructs.
    - `@latex` alongside `@latex.math`, `@latex.tikz`, etc. — embeds raw LaTeX at the root, while child elements
      separate math mode (renderable via KaTeX/MathML on HTML export) or TikZ figures from general LaTeX fragments.
    - `@html` alongside `@html.div`, `@html.span`, etc. — embeds raw HTML source at the root as an escape hatch, while
      child elements construct structured HTML elements whose `[content]` continues to hold normal Tomet markup.
  - **Opt-in per vocabulary:** Only a vocabulary that explicitly declares a root element may be invoked as `@ns`. Writing
    `@rust` when `rust` only declares child elements (`@rust.crate`) remains a validation error, with a diagnostic that
    distinguishes "in-scope namespace `rust` has no root element" from an unbound/unknown bare name.
  - **Consistent with the bare-name scoping rule:** Although `@mermaid` is syntactically un-dotted
    (`Name { namespace: None, name: "mermaid" }`), `::use(mermaid)` in the document preamble explicitly binds the
    identifier `mermaid` in scope — unlike child names of a `::use`d vocabulary, which never appear in the preamble and
    remain forbidden bare. `std` wins unconditionally: no vocabulary namespace may collide with a `std` builtin name or
    `RESERVED_NAMESPACES` (checked once when loading/validating the vocabulary).

- **Foreign-language / verbatim embeds require existing raw syntax (no vocabulary-dependent parser mode):**
  - The parser remains strictly vocabulary-free: `[content]` and `|content` never switch lexing or parsing modes based
    on the element's name.
  - Elements that embed foreign syntax (`@mermaid`, `@latex`, `@typst`, `@typst.math`, `@latex.math`, `@html`, etc.)
    **require** their content to be written with Tomet's existing raw forms:
    - **Inline:** single-backtick code span inside `[...]` — e.g. `` @latex[`x \in [0, 1)`] `` (or `@raw[\`...]` when
      the snippet itself contains a literal backtick, since Tomet inline code spans are single-backtick only and
      non-empty). Unmatched `]` inside `` `...` `` already cannot close `[...]`.
    - **Block / multi-line:** pipe + two-backtick line-raw —
      ```tmt
      @mermaid
      | `` flowchart TD
      | ``   A[Start] --> B["arr[0]"]
      ```
      (or `|``...` without the separator space), or a fenced code block nested inside `[ ... ]`
      (`@mermaid[\n```\n...\n```\n]`). Both already lower to the identical AST (`content` holding a single block `@raw`
      element) and have their verbatim whitespace preserved by `tomet format` (`collect_raw_spans`).
  - Top-level fenced code blocks (` ```mermaid `) continue to lower to `@raw(lang: "mermaid")` (a displayed source-code
    listing), keeping code display cleanly separated from rendered embeds (`@mermaid`).

- **Validation enforces raw-only content (`ContentAllow::Raw`):**
  - Today's `content: { allow: list(raw) }` (`ContentAllow::Names`) only filters child *elements* and lets bare
    `Inline::Text`, `Block::List`, or mixed prose + code spans pass `tomet check` silently (so `@latex[ x^2 + y^2 ]`
    without backticks would go uncaught).
  - Add a dedicated raw-only content rule (`ContentAllow::Raw`, written e.g. `content: { allow: raw }`) that requires
    non-empty `[content]` / `|content` to consist solely of a single `@raw` element (inline
    `[Block::Paragraph([Inline::Element(@raw)])]` or block `[Block::Element(@raw)]`) and rejects bare prose, surrounding
    text, or non-raw blocks with a targeted diagnostic pointing to `` [`...`] `` and `| `` ...`.

## Details to confirm before or during implementation

- **Vocabulary syntax for declaring the root element (Decided 2026-11-09 via `X.X == X`):**
  - Under the unified singular dot-path declaration syntax (`tmtroot/dev/design/nexus-brackets-and-singular-declarations.tmt`),
    a vocabulary `.vocabulary.tmt` declares `::vocabulary.<ns>(version: "1.0.0")` and declares its root element using its
    own namespace name `::element.<ns>(...)` (because `@<ns>.<ns>` contracts to `@<ns>` under `X.X == X`). Child elements
    are declared alongside it as `::element.<child>(...)`.
- **Resolution precedence & same-name child collision (`::kind(html)` vs `@html.html`):**
  - If a document's `::kind(k)` declares a child element `foo` and the same document writes `::use(foo)` where `foo` has
    a root element, decide whether `tomet check` rejects the collision as ambiguous or defines a fixed precedence.
  - HTML literally has an `<html>` element (`@html.html`). Decide whether a vocabulary with a root element may also
    declare a child element sharing the namespace's own name, and if so, what bare `@html` resolves to in a document
    with `::kind(html)`.
- **Canonical `ElementKind::Custom` / declaration lookup key:** today a bare element `card` in `::kind(deck)` classifies
  as `ElementKind::Custom("deck.card")`. Ensure the classified representation of a root element (`"mermaid"` vs a
  dedicated variant/form) cannot collide with `"html.html"` and resolves cleanly through `Bindings::declaration` and
  `normalized_element_args`.
- **Exporter targets (`tomet-convert-*`):**
  - `tomet-convert-typst`: `@typst` emits verbatim Typst; `@typst.math` wraps in `$...$` (inline `$x$` vs block
    `$ x $` by `Placement`); confirm fallback or conversion behavior for `@mermaid` and `@latex`.
  - `tomet-convert-html`, `tomet-convert-markdown`, `tomet-convert-pandoc`: confirm target markup for `@mermaid`,
    `@latex` / `@latex.math`, and `@typst` (e.g. `<pre class="mermaid">`, math containers / Pandoc `Math` & `RawBlock`
    / `RawInline`, CommonMark fences or passthrough).
- **AST printer layout (`tomet-format-printer`):** `render_groups` currently renders a block `@raw` inside `[content]`
  as `@mermaid[```\n...\n```\n]` without a newline after `[`. Confirm whether AST-to-source printing (used by `from-md`
  and `from-pandoc`) should prefer `@mermaid\n| `` ...` or newline-separated fences.

## Steps

- [ ] **1. `tomet-semantics` (callable namespace root & `ContentAllow::Raw`):**
  - Extend `Vocabulary` (`crates/tomet-semantics/src/vocabulary.rs`) with `root: Option<ElementDecl>` and parse it from
    vocabulary documents; reject vocabulary namespaces that shadow `std` builtins (`builtin_kinds()`).
  - Update `Bindings::classify`, `Bindings::declaration`, and `positional.rs` so `@ns` resolves to `vocab.root` when
    `ns` is in scope (`self.kind` or `self.used`), and `UnknownName` distinguishes a non-callable in-scope namespace
    from an unknown bare name.
  - Add `ContentAllow::Raw` (`content: { allow: raw }`) and a shared helper to extract the verbatim string from a
    validated raw-only element's content.
- [ ] **2. Validation (`tomet-validator-core`, `tomet-validator`):**
  - Enforce `ContentAllow::Raw` in `check_content_allow` (`crates/tomet-validator-core/src/lib.rs`), rejecting bare
    `Inline::Text`, multiple inlines/blocks, or non-`@raw` nodes with a clear diagnostic suggesting `` [`...`] `` or
    `| `` ...`.
  - Add unit and integration tests covering root elements, non-callable namespaces, namespace/builtin collisions, and
    raw-only content validation.
- [ ] **3. AST printer (`tomet-format-printer`):**
  - Improve `render_groups` (`crates/tomet-format-printer/src/element.rs`) when an element's `content` is a block
    `@raw`, ensuring clean formatting and lossless round-tripping.
- [ ] **4. Exporters & importers (`tomet-convert-*`):**
  - Implement rendering (and Pandoc/Markdown round-trip mapping where applicable) for `@typst`, `@mermaid`, and `@latex`
    (plus initial mode sub-elements such as `.math`) across `tomet-convert-typst`, `tomet-convert-html`,
    `tomet-convert-markdown`, and `tomet-convert-pandoc`.
- [ ] **5. Vocabularies, specification, and cross-crate test suite:**
  - Author `typst@1.0.0.vocabulary.tmt`, `mermaid@1.0.0.vocabulary.tmt`, and `latex@1.0.0.vocabulary.tmt` (and their
    `readme.tmt` files) in `../vocabularies/vocabularies/`.
  - Update `tmtroot/spec/features/vocabulary.tmt` and `tmtroot/spec/features/content-shape.tmt`, regenerate derived
    Markdown with `just docs`, add fixtures/snapshots in `tests/`, and verify with `just test`, `just test-suite`,
    `just docs-check`, and `just writ-check`.

## Destination when done

- Root element resolution, namespace shadowing rules, and `ContentAllow::Raw` -> `//!` / `///` doc-comments in
  `tomet-semantics` (`vocabulary.rs`, `kind.rs`) and `tomet-validator-core`.
- Normative user-facing documentation -> `tmtroot/spec/features/vocabulary.tmt` and
  `tmtroot/spec/features/content-shape.tmt`.
- Ecosystem vocabulary definitions -> `../vocabularies/vocabularies/{typst,mermaid,latex}/`.
- Delete this file once folded.
