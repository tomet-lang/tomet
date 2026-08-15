# Task: Make typedmark-renderer output SSG-ready

## Context

From evaluating `cargo run -p typedmark -- html docs/cheatsheet.tm` (no
`--advanced`) from the POV of someone building a static site generator on
top of TypedMark. Table syntax (`| a | b |` not being understood) is
**intentionally** out of scope -- ignore it, it's by design.

Four real gaps found, in priority order:

1. **`@meta` is invisible to the renderer.** `render_element` in
   `crates/typedmark-renderer/src/lib.rs` does `"meta" => {}` and drops
   the data entirely. `render_page`/CLI `html` derives `<title>` purely
   from the filename (`apps/typedmark/src/main.rs::render_file`,
   `file.file_stem()`), never from `@meta`. Anyone building an index
   page, an RSS feed, or per-page `<title>`/`<meta description>` needs
   to pull that data out of the `Document` themselves today -- there's
   no supported extraction path.
2. **No auto-slug/id generation for headings.** `split_attrs` (same
   file) only picks up an *explicit* `{id:...}` attr; nothing derives an
   id from heading text the way Markdown processors do. Every anchor/
   permalink/TOC-target has to be hand-authored (see
   `docs/cheatsheet.tm`'s `#[ heading ]{ id:1 }`).
3. **`<html lang="ja">` is hardcoded** in `render_page_with`
   (`crates/typedmark-renderer/src/lib.rs`). No way to override it via
   `RenderOptions` or otherwise; anyone not writing Japanese content
   can't use `render_page`/`render_page_with` as-is.
4. **Naming an inferable `@` element opts it out of inference, silently.**
   `element_kind`/`render_element`: a bare `@(url:...)` gets inferred as
   a link (`infer_at_kind`) and rendered as `<a href>`, but the
   *same* input with an explicit name (`@link(url:...)`) is treated as
   kind `"link"` literally, falls through to `render_generic_element`,
   and produces a content-less `<span data-url="...">` -- no visible
   link at all. Confirmed in `docs/cheatsheet.tm`'s side-by-side
   `@link[](url:https://)` vs `@[](url:https://)` example. A real
   footgun: naming a link-like element for organizational reasons
   quietly breaks it.

## Steps

- [ ] **Step 1**: Design and add a `@meta` extraction API. Likely a
  function in `typedmark-ast` or a new small helper (maybe
  `typedmark-renderer` or a new `typedmark-frontmatter`-style module)
  that scans `Document::blocks` for `@meta`/`@config` elements and
  returns their parsed `Value` (title, date, tags, etc.) as something
  callers can consume before/alongside rendering. Wire it into
  `apps/typedmark`'s `html`/`serve` commands so `<title>` prefers
  `@meta`'s `title` key over the filename when present, falling back to
  the filename as now.
- [ ] **Step 2**: Add heading auto-slugging. A `slugify(text) -> String`
  helper (lowercase, spaces/punctuation to `-`, collision suffixes
  `-2`/`-3` for duplicates within one document) used as the `id` when a
  heading has no explicit `{id:...}`. Needs a `RenderOptions` flag
  (default probably *on*, since it's almost always wanted) so it stays
  opt-out rather than silently changing existing golden-output tests --
  check `crates/typedmark-renderer/src/lib.rs`'s existing heading tests
  before deciding the default.
- [ ] **Step 3**: Make the page shell's language configurable. Add a
  `lang: &str` (or `Option<&str>` defaulting to `"ja"` for backward
  compat, or defaulting to `"en"` -- decide) param to
  `render_page`/`render_page_with`/`RenderOptions`, threaded through
  `apps/typedmark`'s `html`/`serve` CLI commands as a `--lang` flag.
- [ ] **Step 4**: Fix (or at least document loudly) the named-vs-inferred
  `@` element footgun. Options to weigh: (a) make `infer_at_kind` also
  apply when a name is present but happens to match no other special
  case and the input shape matches (i.e. infer regardless of naming,
  only skip inference for names that collide with another real kind),
  (b) have `render_generic_element` fall back to rendering `area` content
  visibly even for unrecognized kinds with a `url`/`file`/`ref` key
  (already partially true -- check why the cheatsheet example rendered
  empty; `@link[]( url:... )` has an *empty* `[]` area, so the real bug
  might just be "no area + no fallback to the href text", closer to
  `render_href_element`'s `render_area_or_fallback` pattern), or (c)
  leave the behavior but make it discoverable (lint/warning in `check`,
  or a doc callout). Investigate `render_element`/`element_kind` in
  `crates/typedmark-renderer/src/lib.rs` before picking an approach --
  don't assume (a) is right without re-reading `infer_at_kind` in
  `typedmark-ast`.
- [ ] **Step 5**: Add/update tests in
  `crates/typedmark-renderer/src/lib.rs`'s `#[cfg(test)] mod tests` for
  each fix (frontmatter extraction, slug generation incl. collisions,
  `lang` override, whichever fix Step 4 lands on). Run
  `cargo test --workspace`.
- [ ] **Step 6**: Clean up this task file on completion.
