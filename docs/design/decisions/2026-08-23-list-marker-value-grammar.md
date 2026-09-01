# List Item Marker: Split Checkbox From Value Grammar (2026-08-23)

## Background

`docs/develop/grammar-freeze.md` froze the list-item shape (`- (marker)
content` / `-. (marker) content`) with `marker` documented as "not the
`key:value` of `args`, but a free string wrapped in `()` or `[]`" --
i.e. `[x]`, `[T]`, `(12:01)`, `(?)` were all parsed identically, by
`eat_list_marker_with_indent` in `crates/tomet-syntax-parser/src/
list.rs`, as one raw-string capture with no record of which bracket was
used and no connection to the `Value` grammar that `<T>(args)` uses.

This surfaced two problems in review:

1. **Round-trip ambiguity.** With no record of which bracket produced a
   given marker string, the three re-serializers guessed independently
   and disagreed: `tomet-emit-printer` always re-emitted `(marker)`,
   `tomet-codegen-html` rendered any non-checkbox marker as
   `[marker]`, and `tomet-codegen-markdown::export` always emitted
   `[marker]`. `- [T] content`, round-tripped through the formatter,
   silently became `- (T) content`.
2. **Inconsistent `()` semantics.** Every other `()` group in the
   grammar (`<T>(args)`, `@name(args)`) means "parse this as a `Value`
   (`key:value` map, optionally with positional-key inference via
   `tomet-semantics::positional`)". A list item's `(...)` meaning
   "an arbitrary free string" was a second, incompatible meaning for the
   same bracket, undermining "one bracket, one meaning" as a readability
   property of the language.

## Decision

`[...]` and `(...)` after a list marker are no longer the same
construct with two spellings -- they're two different constructs:

- **`[...]` is a checkbox, and only a checkbox.** Recognized shapes:
  empty or `" "` (unchecked, `ListItem.checked = Some(false)`), `"x"`/
  `"X"` (checked, `Some(true)`). Any other content inside `[...]`
  (`[T]`, `[?]`, ...) is **not recognized as a marker at all** -- the
  old free-content-in-`[]` notation is formally removed. This is not a
  parse error: `[]` outside an element content span was already just
  literal text per the existing freeze (`docs/develop/grammar-freeze.md`
  -- "`[]` is reserved for element content spans only"), so `- [T]
  content` parses as a markerless item whose content starts with the
  literal text `[T] content`.
- **`(...)` is a real `Value`**, parsed with the exact same grammar
  `<T>(args)` uses (`crate::element::parse_paren_value` in
  `tomet-syntax-parser`, reused directly). `ListItem.marker` is now
  `Option<tomet_ast::Value>`, not `Option<String>`. A bare scalar/
  seq (`- (12:01) ...`) is normalized against a new builtin positional
  key, `"marker"`, by `tomet_semantics::positional::
  normalized_list_marker` -- the same mechanism already used for
  `codeblock`->`lang`, `embed`->`src`, `callout`->`variant`, `meta`/
  `config`->`format` (`tomet-semantics::positional::
  builtin_positional_arg_key`). So `- (12:01) woke up` normalizes to
  `{marker: "12:01"}`, and an explicit multi-key form works too: `-
  (color: red, priority: high) content` stays `{color: red, priority:
  high}` unchanged (a `Value::Map` always passes through
  `normalized_list_marker` as-is).

  Notably, `docs/spec/builtin-elements.tmt`'s existing
  (illustrative, not machine-checked) pseudo-grammar already wrote this
  shape as `- (marker:<string>) content` -- the builtin key name chosen
  here matches what that doc already called it.

Malformed `(...)` content (e.g. `- (?) text` -- `?` isn't a valid map
key or bare-scalar lead character, same restriction `<T>(?)` already
has) is a genuine parse error, matching how a malformed `<T>(args)` is
already treated -- not a silent fallback to "not a list item". This
differs deliberately from the `[...]` case above: `(...)` is a real,
committed grammar construct once the parser sees `-` followed by `(`, so
`tomet-syntax-parser::list::eat_list_marker_with_indent` now returns
`Result<Option<...>>` rather than a bare `Option`.

**No "keys can't start with a digit" grammar change was made.** The
concern that motivated this decision doc -- `("12:01")` being misread
via the same `key:value` misparse `12:01` would hit unquoted (`eat_ident`
in `value.rs` doesn't reject digit-led identifiers, so unquoted `12:01`
parses as the map `{12: 01}`) -- is resolved the same way it already is
for `<T>(args)`: quote it. `("12:01")` always parses as a bare `Value::
String`, since a `"` short-circuits straight to `parse_quoted` before
`eat_ident` is ever tried. This is an existing property of the value
grammar, not new surface added by this change.

## Consequences / what changed

- `tomet_ast::ListItem` gained `checked: Option<bool>` and changed
  `marker: Option<String>` to `marker: Option<Value>`.
- `tomet-emit-printer`, `tomet-codegen-html`,
  `tomet-codegen-markdown` (both `import` and `export`), and
  `apps/lsp` were all updated for the new two-field shape -- see each
  crate's own tests for the resulting round-trip behavior. `export.rs`
  (CommonMark has no non-checkbox marker equivalent) now drops a
  non-checkbox `marker` on export rather than misprinting it as
  `[marker]`, consistent with this crate's other documented lossy cases.
- Real project content using the old `(x)`/`( )` conflated spelling for
  checkboxes was migrated to `[x]`/`[ ]`: `docs/roadmap.tmt`,
  `tests/fixtures/roadmap.tmt`, `docs/guide/cheatsheet.tmt` (and its
  `tests/fixtures/` mirror), `docs/guide/builtins/from_markdown.tmt`. Non-`x`/
  empty markers that aren't valid bare identifiers were quoted (`(!)` ->
  `("!")`) rather than reinterpreted.
- `docs/spec/syntax.tmt`'s `#[ 雛形 ]` section documents the
  split.

## Process

Per `docs/develop/grammar-freeze.md`, a change to already-frozen list-item
syntax needs: this decision doc (done), `docs/spec/syntax.tmt`
updated (done), and `tree-sitter-tomet` kept in sync
(`cargo test -p tree-sitter-tomet` -- unaffected, its external
`list_checkbox` scanner already treated `[x]`/`(T)`/`(?)` shapes
generically at the lexer level for highlighting purposes and needed no
structural change).
