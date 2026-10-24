; Indent hints for cotomet: a `{...}` map, and a `name(...)` call that runs
; over several lines.

(map
  "}" @end) @indent

(call
  ")" @end) @indent
