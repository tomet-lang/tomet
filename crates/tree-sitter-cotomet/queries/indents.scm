; Indent hints for cotomet: a `{...}` map, and a `(...)` tuple or a
; `name(...)` call that runs over several lines.

(map
  "}" @end) @indent

(tuple
  ")" @end) @indent

(call
  ")" @end) @indent
