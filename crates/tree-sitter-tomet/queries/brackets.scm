; Bracket-pair declarations for Tomet. Also read by Zed's non-LSP fold
; path (`Editor::closing_bracket_indent_len` in `crates/editor/src/display_map.rs`
; upstream in zed-industries/zed): a folded `@indent` range only merges
; its closing line onto the placeholder (`{...}` on one line) when that
; line's leading text matches a registered closing bracket here --
; otherwise the closing line is left visible on its own, as `indents.scm`
; alone was doing before this file existed.

("(" @open
  ")" @close)

("[" @open
  "]" @close)

("{" @open
  "}" @close)
