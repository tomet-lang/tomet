# Generalize `{}` via a uniform `format` key; rename `pre` -> `codeblock`

## Decisions made this session (confirmed with user)

1. **`pre` -> `codeblock`.** Matches CommonMark's own term ("code block",
   fenced/indented) instead of leaking the HTML tag name; also matches the
   `blockquote` naming pattern (compound word, no underscore). User had
   already scratch-noted this exact rename (and alternatives `code`/
   `execute`) as commented-out lines in `docs/cheatsheet.tm`.
2. **New key name for "parse `{}` as a real embedded format": `format`**,
   not `syntax` (rejected -- too close to "programming language syntax",
   which is what `codeblock`'s own `lang` key already means: a *display*
   hint for syntax highlighting, never a parse-mode switch). `lang` keeps
   its current, narrow meaning on `codeblock` only and is otherwise
   untouched.
3. **`{}`'s behavior becomes uniform across every element, not just
   `@meta`:** if `(input)` has a `format` key naming a recognized format
   (`json`/`yaml`/`toml`), `{}`'s raw text is handed to that format's real
   parser (`serde_json`/`serde_yaml`/`toml`) exactly like today's
   `@meta(tag){...}` special case did -- but the check is now purely
   "does `input` have `format:X`", independent of the element's name/sigil.
   Falls back to TypedMark's own lightweight `Value` grammar when `format`
   is absent or unrecognized, same as today.
4. **`@meta` moves onto this generic mechanism with an explicit key:**
   `@meta(format:json){...}` is now the (only, for now) correct spelling.
   The current bare-tag `@meta(json){...}` form is **removed** (breaking
   change, confirmed acceptable by user). A terser shorthand
   (`@meta(json){...}` as sugar for `@meta(format:json){...}`) may come
   back later, once the explicit form is solid -- **not** part of this
   task, don't add it now.
5. **`codeblock`'s content moves from `{value}` to `[area]`, and
   `codeblock` is a deliberate, sole exception to `[area]`'s usual inline
   grammar.** Two reasons, both from the user:
   - `[area]` isn't used for anything else on `codeblock` -- free to
     repurpose.
   - `{}`'s established role elsewhere (a heading's `{ id:x, cssclass:y }`)
     is reference/attribute metadata, not primary content. Leaving the
     *code itself* in `{}` permanently blocks `codeblock` from ever having
     that same `id`/`cssclass` capability (a `{value}` group is one shape
     -- either the code text or an attrs map, never both). Moving the code
     to `[area]` frees `{}` up for `<codeblock>(lang:rust){id:snippet1}[code]`,
     matching how every other element already uses `{}`.
   - Every *other* element's `[area]` still goes through the full inline
     grammar (`parse_inline_seq`: em/strong/mark, element triggers, ...) --
     unchanged. `codeblock`'s `[area]` is the one exception: parsed as raw
     verbatim text, not interpreted as TypedMark markup at all (mirrors
     how CommonMark itself treats code blocks as structurally special,
     never markup-interpreted -- this isn't a generalizable "verbatim
     mode" any element can opt into, just this one). This also incidentally
     fixes the pre-existing bug where a hand-written multi-line
     `<pre>(lang:x){...}` fails to parse today (confirmed by testing: the
     lightweight `Value` grammar's scalar-eating stops at the first
     newline) -- raw verbatim scanning has no such problem.
   - Implementation: reuse the brace-depth + quote-aware raw-slice
     approach already built for `@meta`'s `{}` extraction
     (`typedmark-parser`'s `find_matching_brace` in the format-value
     module), adapted to `[`/`]` instead of `{`/`}` (so a nested `[...]`
     in real code, e.g. an array literal, doesn't miscount the closing
     `]`). `Element::area`'s type (`Option<Vec<Inline>>`) doesn't change --
     `codeblock`'s raw content just becomes a single `Inline::Text(raw)`,
     the same shape the Markdown-import path already produces directly.

## Steps

### `typedmark-parser`
- [ ] Rename `src/meta_format.rs` -> e.g. `src/embedded_format.rs` (name
      TBD at implementation time, doesn't need to be user-confirmed).
      `MetaFormat` -> `EmbeddedFormat` (or similar), `parse_meta_format_value`
      -> `parse_embedded_format_value`.
- [ ] Replace `document.rs`'s `meta_format_for(el: &Element)` (currently:
      sigil is exactly `@meta` AND input is a bare `Value::String` tag)
      with a generic check: `el.input` is `Value::Map` containing key
      `"format"` whose value is `Value::String("json"|"yaml"|"toml")`.
      No sigil/name check at all -- applies to every element.
- [ ] `document.rs::parse_element`'s `Some('[') if el.area.is_none()`
      branch: add a check mirroring `meta_format_for`'s old shape --
      `Sigil::Type("codeblock")` gets a new `parse_raw_area(cur)` instead
      of the usual `parse_area(cur)`. `parse_raw_area` scans for the
      matching `]` (bracket-depth + quote-aware, adapted from
      `find_matching_brace`) and returns `vec![Inline::Text(raw)]` without
      calling `parse_inline_seq` at all -- no em/strong/mark/element-
      trigger recognition inside a codeblock's `[...]`. Every other
      element keeps using `parse_area` unchanged.
- [ ] Add tests: a codeblock containing `*`/`<T>`/`@`/backticks stays
      literal; a codeblock with a nested `[...]` (array literal) doesn't
      truncate early; `<codeblock>(lang:x){id:snippet1}[code]` gives the
      codeblock a real `id` (once the renderer step below wires
      `split_attrs`/`push_named_attrs` for it, mirroring headings).
- [ ] Update/rename tests in the (renamed) format-value test module:
      `@meta(json){...}` -> `@meta(format:json){...}` throughout; the
      "no tag" and "unrecognized tag" fallback tests become "no `format`
      key" / "unrecognized `format` value"; add a test confirming the
      mechanism works on a **non-meta** element too (e.g.
      `<config>(format:json){...}` or similar), proving it's genuinely
      generic now.

### `typedmark-ast`
- [ ] No AST shape changes needed (`ElementValue`/`Value`/`Element`
      already support everything here) -- double check once the above is
      implemented.

### `typedmark-renderer`
- [ ] `render_element`: `"pre"` -> `"codeblock"` match arm.
- [ ] `render_pre_element` -> `render_codeblock_element`: read code from
      `el.area` instead of `el.value` (a single `Inline::Text`, extract via
      `inlines_to_plain` or similar). `lang` handling from `input`
      unchanged. New: if `el.value` is `Some(ElementValue::Data(v))`, run
      it through `split_attrs`/`push_named_attrs` (same as
      `render_heading`) so `<codeblock>(lang:rust){id:snippet1}[code]`
      gets a real `id`/`class` on the rendered `<pre>`.
- [ ] Update/rename the corresponding test(s); add one for the new `id`
      support.

### `typedmark-markdown`
- [ ] `export.rs`: `"pre"` -> `"codeblock"` match arm, `render_code_block`
      reads from `el.area` instead of `el.value`.
- [ ] `import.rs`: fenced/indented code blocks now construct
      `Element { sigil: Sigil::Type("codeblock".to_string()), area: Some(vec![Inline::Text(text)]), value: None, .. }`
      instead of putting `text` in `value` (`value`/attrs stay `None` --
      Markdown has no `id`/`cssclass` concept to import from).
- [ ] Update/rename tests in both files.

### Docs / fixtures
- [ ] `docs/commonmark-support.md`: update the `<pre>(lang:xxx){code}`
      references (there are a few) to `<codeblock>(lang:xxx)[code]`.
- [ ] `docs/cheatsheet.tm`: update the `<pre>(lang:json){...}` block to
      `<codeblock>(lang:json)[...]`; the commented-out `// <codeblock>`/
      `// <code>(lang:sh)[]`/`// <execute>(lang)[]` scratch lines can
      probably be cleaned up/uncommented appropriately once this lands.
- [ ] `docs/tmt/typedmark.tm`, `docs/tmt/image_meta.tm`,
      `docs/roadmap.ja.tm`: update all `@meta(json|yaml|toml){...}` bare-
      tag examples to `@meta(format:json|yaml|toml){...}`.
- [ ] `docs/tmt/typedmark.tm`'s "残っている論点"/decision note about
      `@meta`'s format parsing (added earlier this session) needs
      rewriting to describe the new, generalized `format` key mechanism
      instead of the old meta-specific bare-tag one.

### `crates/tree-sitter-typedmark`
- [ ] Re-run `cargo test -p tree-sitter-typedmark` after the above --
      `@meta(format:json)` is an ordinary `key:value` `map_entry`, already
      well-supported by the grammar, so this is expected to need no
      grammar changes, just verification (including against the updated
      `docs/cheatsheet.tm`/`docs/tmt/*.tm` fixtures).

### Wrap-up
- [ ] `cargo test --workspace` green, `cargo clippy`/`cargo fmt` clean.
- [ ] Delete this task file once done.
