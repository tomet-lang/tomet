# Group nesting depth and bracket tags in tomet-highlight

Add group nesting depth (`depth: u8` in `HighlightSpan`) across all group kinds (`(...)`, `[...]`, `{...}`), separate pair brackets (`Tag::Bracket`) from general delimiters (`Tag::Punctuation`), and preserve group hierarchy so that `[]`, `{}`, and `()` everywhere retain their nesting layer.

Delete this file when done.
Destination of what is worth keeping: doc comments (`//!` and `///`) in `crates/tomet-syntax-highlight/src/lib.rs`.

Rules that apply: AGENTS.md (no edit to generated `.md`, commit convention `type(scope): description`, jj with `--no-pager`, ask before commit/push, dedicate jj workspace if non-trivial).

## Decisions

1. **`HighlightSpan.depth` for group nesting**:
   - Add `pub depth: u8` to `HighlightSpan`.
   - Represents the nesting depth of enclosing groups (`ARGS`, `CONTENT`, `VALUE_DATA`, `MAP_LITERAL`, `ID_GROUP`, `INTERP_EXPR`).
   - Top-level text/tokens have `depth: 0`.
   - The first enclosing group has `depth: 1`, nested groups have `depth: 2, 3, ...`.
   - All spans within a group (its brackets, inner keys, strings, nested tags) carry that group's depth.

2. **Differentiate `Tag::Bracket` from `Tag::Punctuation`**:
   - `Tag::Bracket`: The paired delimiters `(`, `)`, `[`, `]`, `{`, `}` of groups.
     - `as_str()` returns `"bracket"`.
   - `Tag::Punctuation`: Non-bracket delimiters such as `,`, `:`, `|`, `#`.
     - `as_str()` returns `"punctuation"`.
   - This allows clients/editors to colorize bracket pairs (rainbow brackets, depth-based styling, or styling `=[` and `]` specifically) without unintentionally coloring commas or colons.

3. **Heading frame (`=[ ... ]`) behavior**:
   - For `=[ title ]`:
     - `=` has `Tag::Heading`, `depth: 0`.
     - `[` and `]` have `Tag::Bracket`, `depth: 1`.
     - `title` is untagged (prose) within `PARAGRAPH`.
   - Editors can easily match `Tag::Bracket` at `depth: 1` under a heading (or style brackets by depth) to render `=[` and `]` in blue while leaving the title text as prose.

4. **Span Merging Rule Update**:
   - Adjacent tokens are merged if both `tag` AND `depth` match (`last.tag == tag && last.depth == depth && last.to == from`).

5. **No extra Cargo feature**:
   - Included in core `tomet-highlight` without a feature flag, keeping the build clean and overhead negligible.

## Steps

- [ ] Check consumers of `tomet-highlight`:
  - `crates/tomet-syntax-highlight` (implementation and tests)
  - `crates/tomet` (re-exports)
  - `bindings/js/src/highlight.rs` (`HighlightSpan` shape: add `depth: usize`)
- [ ] Implement `Tag::Bracket` in `crates/tomet-syntax-highlight/src/lib.rs`.
- [ ] Add `depth: u8` to `HighlightSpan` in `crates/tomet-syntax-highlight/src/lib.rs`.
- [ ] Update `highlight(src)` and helper functions in `lib.rs`:
  - Calculate `group_depth(token)` by counting enclosing group ancestor nodes.
  - Distinguish `Bracket` (`()`, `[]`, `{}`) from `Punctuation` (`,`, `:`, `|`, `#`).
  - Update adjacent span merging to require `last.depth == depth`.
- [ ] Update tests in `crates/tomet-syntax-highlight/src/lib.rs` to verify:
  - Nested groups `@foo(a: [b: {c: 1}])` produce brackets with depths 1, 2, 3.
  - Heading `=[ Title ]` produces `=` (depth 0, heading) and `[` / `]` (depth 1, bracket), with `Title` untagged.
  - Commas and colons remain `Tag::Punctuation` with corresponding group depth.
  - Top-level spans (like comments or headings) have `depth: 0`.
- [ ] Update `bindings/js` to expose `depth` on its `HighlightSpan`.
- [ ] Run test suite (`cargo test -p tomet-highlight -p tomet-js`) and verify all pass.
