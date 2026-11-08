; Indent hints for cotomet: a `{...}` map, and a `(...)` sequence or a
; `name(...)` call that runs over several lines.

(map
  "}" @end) @indent

(sequence
  ")" @end) @indent

(call
  ")" @end) @indent
