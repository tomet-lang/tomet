; Indent (and, via Zed's non-LSP fold path, fold-affordance) hints for
; TypedMark. Zed has no separate "fold query" -- `Editor::starts_indent`
; treats any row that opens a node captured `@indent` here as foldable,
; the same query that also drives auto-indent after pressing Enter. See
; `crates/editor/src/fold.rs` and e.g. `crates/grammars/src/json/indents.scm`
; upstream in zed-industries/zed for the pattern this follows.

(input_group
  ")" @end) @indent

(area_group
  "]" @end) @indent

(value_group
  "}" @end) @indent

(braced_map
  "}" @end) @indent

(seq
  "]" @end) @indent

(unordered_list) @indent
(ordered_list) @indent
