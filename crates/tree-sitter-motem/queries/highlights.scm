; Syntax highlighting for motem (`.mtm`), using the standard capture names
; shared across tree-sitter-based editors (Zed, Neovim, Helix, ...).
;
; A bare scalar is typed from its text, as the parser types it: `null`,
; `true`, `false` and a JSON number are not strings. The three patterns for
; `(scalar)` exclude one another, so it does not matter which an editor
; applies first.

(comment) @comment

(string) @string
(escape_sequence) @string.escape

; the key of a `key: value` entry
(key) @property
(quoted_key) @property

(pair
  ":" @punctuation.delimiter)
"," @punctuation.delimiter

[
  "{"
  "}"
  "("
  ")"
] @punctuation.bracket

; `::name(args){data}`, a declaration about the file: coloured like an element
; in a document, since the tomet grammar does the same for `::name`.
(declaration "::" @tag)
(declaration name: (name) @tag)

; `list(...)`, `string(...)`, `int(...)`, `float(...)` are resolved by the
; parser; any other callee is left for a later pass.
(call
  function: (name) @function.builtin
  (#match? @function.builtin "^(list|string|int|float)$"))
(call
  function: (name) @function
  (#not-match? @function "^(list|string|int|float)$"))

((scalar) @constant.builtin
  (#match? @constant.builtin "^(null|true|false)$"))

((scalar) @number
  (#match? @number "^-?(0|[1-9][0-9]*)(\\.[0-9]+)?([eE][+-]?[0-9]+)?$"))

((scalar) @string
  (#not-match? @string "^(null|true|false|-?(0|[1-9][0-9]*)(\\.[0-9]+)?([eE][+-]?[0-9]+)?)$"))
