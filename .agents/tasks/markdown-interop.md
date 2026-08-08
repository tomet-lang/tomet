# Markdown interop (typedmark-markdown crate)

See `docs/commonmark-support.md` for the requirements/gap-analysis this
task implements ("Implementation sketch" section). Native shorthand
syntax (`*em*`, `**strong**`, `==mark==`, `---`, `-.`) was already done
in a prior session (uncommitted at task start) -- this task is the
importer/exporter crate itself.

## Scope decided for this pass

- New crate `crates/typedmark-markdown`, `pulldown-cmark` for MD -> AST,
  hand-written serializer for AST -> MD (per doc's sketch).
- Direction-1 escape hatch (generic `<T>` elements) for gaps that don't
  already have native AST support: fenced/indented code blocks (`<pre>`),
  block quotes (`<blockquote>`, single-paragraph fidelity only --
  multi-block quotes are known-lossy, flattened).
- Reuse existing native support for: headings, paragraphs, flat lists
  (ordered/unordered), em/strong (`*`/`**`), hr (`---`), links (`@(url:)`
  / `@(file:)`), images (`<embed>`).
- Nested lists: flattened to sibling top-level items (existing `List`
  shape has no nesting slot) -- documented lossy, not a regression.
- HTML blocks/inline HTML: out of scope, dropped on import (per doc).
- Hard line breaks: collapsed to a space (prose is already
  space-normalized elsewhere in the codebase).
- `typedmark-renderer` needs new cases for `pre` / `blockquote` kinds
  (currently would fall through to the generic div renderer).

## Steps

- [x] Read `docs/commonmark-support.md`, current AST/parser/renderer,
      confirm uncommitted native-shorthand changes are green
      (`cargo test --workspace` passed before starting).
- [x] Add `pulldown-cmark` (0.13) to workspace deps; scaffold
      `crates/typedmark-markdown` (Cargo.toml, src/lib.rs), add to
      workspace `members`.
- [x] Implement `from_markdown(&str) -> typedmark_ast::Document`
      (pulldown-cmark event fold) -- `src/import.rs`.
- [x] Implement `to_markdown(&Document) -> String` (hand-written
      serializer) -- `src/export.rs`.
- [x] Add `pre` / `blockquote` rendering cases to `typedmark-renderer`.
- [x] Unit/fixture tests in `typedmark-markdown` (clean mappings +
      round-trip-via-rendered-HTML for lossy constructs). 26 tests green.
- [ ] Wire CLI subcommands (`from-md` / `to-md`) in
      `apps/typedmark/src/main.rs`, following the existing `html`
      subcommand's `PathBuf` + optional `--out` pattern.
- [ ] `cargo build --workspace` / `cargo test --workspace` green.
- [ ] Update `docs/commonmark-support.md`: mark importer/exporter as
      implemented, document the `pre`/`blockquote` element decisions and
      known-lossy cases (nested lists, multi-block quotes, HTML blocks,
      hard breaks), same style as the existing "Decided and implemented"
      section.
- [ ] Delete this task file once everything above is done.
