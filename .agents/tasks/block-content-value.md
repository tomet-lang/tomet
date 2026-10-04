${macro.generated\_by(self.path)}

# `key: [...]` block content as a `Value`, for `@conflict(a: ..., b: ...)`

Blocking prerequisite for `.agents/tasks/conflict-element-and-content-model.md`'s
step 6 (`@conflict` itself). `@conflict(a: [...], b: [...])` cannot parse
today: a value position (`(args)`/`{data}` entries) rejects `[` outright
("'[...]' list literal was removed -- write 'list(...)' instead"), and
`Value` has no variant that can hold `Vec<Block>`.

## Decision (from the user, no spec doc backs this -- it's the author's
## call, do not look for prior written confirmation)

`@el()[content]{data}` is conceptually `@el(content: [content]){data}` --
the anonymous bracket content slot is, in spirit, the `content:` key of
`(args)`. This is *why* `[a, b]` array literals were retired in favor of
`list(...)`: freeing `[...]` to mean "block content" uniformly, not just
at the top level. The general capability -- any `key: [...]` inside
`(args)` parses as real `Vec<Block>` block content, via the same
recursive grammar as the main content slot -- was always planned for
later and is needed now for `@conflict`'s `a`/`b`.

**Scope decision, confirmed with the user**: `Element.content` stays its
own physical AST field for now (NOT unified into `args` this round --
that migration is explicitly deferred, "いつか" (someday), separate task).
This round is additive only: `key: [...]` becomes a new legal value
shape, parallel to the existing dedicated `content` field, with no change
to how the dedicated field itself parses or is stored.

## Design

- New `Value::Blocks(Vec<Block>)` in `tomet-syntax-ast` (`Value` and
  `Block` are already the same crate -- `Value::Element(Box<Element>)`
  already embeds an `Element` holding `Vec<Block>`, so this is a
  same-crate, non-circular addition).
- `tove`'s `(args)`/`{data}` value grammar never has to change: it
  already runs every value through a `ValueHook` first
  (`crates/tove/src/parser.rs`'s `ValueHook` trait, consulted at the top
  of `parse_entry_value_with`, *before* the hard `'['` rejection).
  `tomet-syntax-parser`'s `TometValueHook`
  (`crates/tomet-syntax-parser/src/value.rs`) already uses this seam for
  `@element`/`$interp`; add a third branch there for `[`, calling
  `crate::document::parse_block_seq(cur, BlockStop::Bracket(']'))`
  (same-crate, already `pub(crate)`) and wrapping the result in
  `Value::Blocks`. No upward Cargo dependency, no `tove` changes.
- Printer fidelity is the real work. `tomet-format-printer::render_element`
  renders every element's `(args)` through
  `tomet_style::render_args_with_config` unconditionally, regardless of
  placement -- so `@conflict(a: [...], b: [...])` standing in the
  document body (the primary, expected case) has its `a`/`b` block
  content rendered through `tomet-format-style`, a LOWER layer than
  `tomet-format-printer` (`.writ.tmt`'s `crate-layering`: `tomet-format*`
  is one layer, `tomet-printer` depends on `tomet-style`). `tomet-style`
  cannot call back into `tomet-printer`'s `render_content_blocks`/
  `render_element` (full element rendering, with every kind's
  special-casing) -- that would be an upward dependency.
  `tomet-style` already has this exact problem for `Value::Element`'s own
  `content` field and accepted a restricted answer (`render_nested_block`:
  paragraph-inlines only, silently drops other block kinds) because "a
  value-embedded element's content is inline in practice" -- true for
  existing builtins, **not true for `@conflict`**, whose whole point is
  multi-paragraph/heading block content in `a`/`b`. Accepting the same
  restriction here would silently corrupt exactly the content `@conflict`
  exists to hold on every `tomet fmt`/round-trip.
  - Fix: dependency inversion, not a layering change. Generalize
    `render_value_inner_with_config` into a `_with_blocks` form taking a
    `&mut dyn FnMut(&[Block], &PrinterConfig) -> String` renderer,
    threaded through every recursive call (`render_nested`,
    `render_args_with_config`, the `Value::Element` arm's own content
    rendering too -- same benefit there for free). The existing public
    names become thin wrappers passing the old restricted
    `render_nested_block`-based closure as the default (so
    `tomet-formatter`, which patches spans incrementally and doesn't have
    `render_content_blocks` either, keeps exactly its current behavior,
    unaffected). `tomet-format-printer` adds a local `render_args`
    wrapper passing its own `render_content_blocks` as the closure, and
    replaces its `render_args_with_config` call sites with it.
- `tomet-semantics::flatten`: `scalar_string` gets a `Value::Blocks(_) =>
  None` arm (not a scalar, same bucket as `Map`/`Call`/`Element`).
  `value_to_json` gets a lossy placeholder arm (same spirit as its
  existing `Value::Element` arm, which already only keeps `sigil`/`args`
  -- block content has no JSON shape anywhere else in the codebase and
  isn't worth inventing one for a path nothing production exercises:
  real `@conflict` consumers read `a`/`b` directly, not through generic
  attribute flattening).
- `tove`: `print.rs::write_value` and `de.rs::deserialize_any`/
  `deserialize_enum` are separately exhaustive matches needing a
  `Value::Blocks` arm each. `print_value` is essentially unused in the
  real pipeline (only `tove`'s own tests/`format_value` convenience, not
  called by `tomet-format-printer`) -- minimal best-effort text is fine
  there. `de.rs` already errors for `Call`/`Element` (no serde
  equivalent); `Blocks` joins that list.
- `Value::serialize` (the `impl Serialize for Value` in
  `tomet-syntax-ast/src/lib.rs`) needs a `Value::Blocks` arm -- `Block`
  already derives `Serialize`, so a plain seq-serialize (same shape as
  `Value::Seq`'s arm) works. `Deserialize` gets no visitor arm, per the
  existing comment pattern for `Call`/`Element` (parser-only values).

## Step plan

1. `Value::Blocks(Vec<Block>)` in `tomet-syntax-ast` + `Serialize` arm +
   comment updates (list `Call`/`Element`/now `Blocks` as deserialize-less
   together).
2. `TometValueHook` (`tomet-syntax-parser/src/value.rs`): intercept `[`.
3. `tomet-format-style`: `_with_blocks` generalization (see Design above);
   keep the restricted public wrappers for existing callers.
4. `tomet-format-printer`: local `render_args` wrapper using
   `render_content_blocks` as the closure; swap call sites.
5. `tomet-semantics::flatten`: the two new arms.
6. `tove`: the two exhaustiveness fixes (`print.rs`, `de.rs`).
7. `cargo build --workspace --exclude tomet-python` clean, then
   `cargo test --workspace --exclude tomet-python` clean. Fix whatever
   else the compiler finds (this list is from manual grep, not
   necessarily complete).
8. New fixtures/tests: `@conflict(a: [multi-paragraph], b: [...])` round-
   trips through `tomet fmt`/printer with full fidelity (the actual bug
   this task exists to prevent); a plain `key: [one paragraph]` on an
   ordinary custom element; `tomet-tests` snapshot update if needed.
9. Hand back to `conflict-element-and-content-model.md`'s step 6 (add
   `@conflict` to `BUILTIN_KINDS` itself) -- blocked on this task, not
   part of it.

## Status

Done, steps 1-8 all complete. Step 9 (hand back to `conflict-element-
and-content-model.md`'s step 6) is next, in that file.

- Step 1: `Value::Blocks(Vec<Block>)` added (`tomet-syntax-ast/src/lib.rs`),
  plus a `Serialize` arm (plain seq, `Block` already derives `Serialize`)
  and the `Deserialize` comment updated to list it alongside `Call`/
  `Element` as parser-only.
- Step 2: `TometValueHook::try_parse_value`
  (`tomet-syntax-parser/src/value.rs`) intercepts `[`, calls
  `document::parse_block_seq(cur, BlockStop::Bracket(']'))`, wraps in
  `Value::Blocks`.
- Step 3: `tomet-format-style`'s `render_value_inner_with_config`/
  `render_nested`/`render_args_with_config` generalized into
  `_with_blocks` forms taking a `&mut BlockRenderer` (`dyn FnMut(&[Block],
  &PrinterConfig) -> String`), threaded through every recursive call
  including `Value::Element`'s own `content` (which now goes through the
  closure too, gaining the same fidelity for free instead of staying on
  the old `render_nested_block`-only path). The non-`_with_blocks` public
  names became thin wrappers defaulting to `default_block_renderer`
  (= the old restricted `render_nested_block` loop), so
  `tomet-formatter`/anything else calling the old names is unaffected.
  **First implementation forgot to wrap the `Value::Blocks` arm's output
  in `[...]`** -- caught by step 8's own fixture (printed output had no
  brackets at all, a hard round-trip break); fixed by wrapping in
  `format!("[{}]", render_blocks(blocks, config))`.
- Step 4: `tomet-format-printer` added a local `render_args` wrapping
  `tomet_style::render_args_with_blocks` with its own
  `render_content_blocks` as the closure (full fidelity -- the whole
  reason step 3 exists), and all 8 `render_args_with_config` call sites
  switched to it via `sed`.
- Step 5: `tomet-semantics::flatten`'s `scalar_string`/`value_to_json`
  got `Value::Blocks` arms (not a scalar; lossy JSON placeholder, same
  bucket as `Element`/`Call` -- nothing production reads block content
  through generic attribute flattening).
- Step 6: `tove`'s `print.rs`/`de.rs` exhaustiveness fixed (best-effort
  plain text for `print_value`, which nothing in the real pipeline calls;
  an error for `de.rs`, same as `Call`/`Element`, no serde equivalent).
- Step 7: `cargo build`/`cargo test --workspace` (minus `tomet-python`)
  clean. Exhaustive-match fallout landed in (not an exhaustive list of
  the list -- found by compiler, not grep): `tomet-syntax-tree/src/
  walk.rs` (`inlines_in_value`/`walk_value`/`walk_value_mut` all needed a
  `Value::Blocks` arm recursing into the blocks, the same as `content`/
  `children` already do), `tomet-semantics/src/positional.rs`,
  `tomet-convert-{typst,pandoc,html,markdown}`, `tomet-workspace-indexer`,
  `tomet-semantics-compute/src/functions.rs` (`truthy`), `tomet-format-
  printer`'s own `value_to_json`, `apps/lsp/src/hover.rs`. One
  pre-existing unit test
  (`tomet-syntax-parser/src/tests/values.rs::a_bracket_list_literal_in_a_
  value_position_is_a_parse_error`) asserted `[a, b]` in a value position
  was a hard error -- genuinely no longer true by design -- renamed to
  `a_bracket_value_parses_as_block_content_not_a_list` and rewritten to
  assert the new, correct behavior (one paragraph of plain text `"a,
  b"`, explicitly not a two-item list).
- Step 8: new fixture `tests/fixtures/syntax/block-value.tmt` (genuine
  multi-paragraph block content, including a nested `@em`, inside
  `(a: [...])`/`(b: [...])`) -- this is what caught the missing-brackets
  bug above. Recorded in `tests/src/lib.rs`'s `KNOWN_TS_ERRORS` (`tree-
  sitter`'s `grammar.js` has no `Value::Blocks` counterpart either, same
  kind of pre-existing gap as `Value::Element`'s own entry right above
  it -- not fixed here, same reasoning: no `tree-sitter generate` CLI
  available in this environment).
  Two pre-existing corpus fixtures, `examples/bookmark.tmt` and
  `syntax/value-element.tmt`, were in `KNOWN_UNPARSEABLE` for using the
  retired `[a, b]` list literal; both now parse (to `Value::Blocks`, not
  a list), so both came out of that list. `bookmark.tmt`'s `tags:[ a, a
  ]` was rewritten to `tags:list(a, a)` to keep meaning "a list of tags"
  (its one-paragraph-of-text new meaning would have been wrong);
  `value-element.tmt`'s `icons: [...]` was left as-is, since embedding
  `@`-elements inside a `[...]` value is exactly what that fixture
  exists to demonstrate. Snapshot refs regenerated
  (`TOMET_UPDATE_REF=1 cargo test -p tomet-tests`); the only
  non-mechanical diffs were on `bookmark.*` and were themselves stale
  references catching up to unrelated, already-shipped behavior that had
  never been exercised against this fixture while it sat excluded (Pandoc
  `Attr.identifier` only ever came from `#(id)`, never a generic `id:`
  data key -- confirmed against `to_pandoc.rs`'s own doc comment, not a
  regression from this task).
- `cargo test --workspace --exclude tomet-python` and
  `cargo test -p tomet-tests --test corpus`/`--test roundtrip` all green,
  including `formatting_does_not_change_the_parsed_document` and
  `printed_source_parses` on the new fixture -- the actual round-trip
  fidelity guarantee this task exists to provide.
