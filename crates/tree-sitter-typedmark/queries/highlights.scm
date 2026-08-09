; Syntax highlighting for TypedMark, using the standard capture names
; shared across tree-sitter-based editors (Zed, Neovim, Helix, ...).
; Loaded by `apps/zed-extension` via a symlink at
; `apps/zed-extension/languages/typedmark/highlights.scm`.

(heading_marker) @markup.heading.marker
(heading) @markup.heading

(thematic_break) @punctuation.special

(emphasis) @markup.italic
(strong) @markup.bold
(mark) @markup.strikethrough

(code_span) @markup.raw.inline

(unordered_list_item) @markup.list.unnumbered
(ordered_list_item) @markup.list.numbered

; `<T>`/`@name` element sigils and their name.
"<" @tag
">" @tag
"@" @tag
(type_element name: (identifier) @tag)
(at_element name: (identifier) @tag)

; `(input)`/`[area]`/`{value}` group delimiters.
(input_group "(" @punctuation.bracket)
(input_group ")" @punctuation.bracket)
(area_group "[" @punctuation.bracket)
(area_group "]" @punctuation.bracket)
(value_group "{" @punctuation.bracket)
(value_group "}" @punctuation.bracket)

; well-known `@(url:..)`/`@(file:..)`/`@(ref:..)` link-shaped elements
; render as links even though the grammar doesn't special-case them
; structurally (that inference is `typedmark-renderer`'s job, not the
; grammar's) -- highlighting the `map_entry` key is close enough here.
(map_entry key: (identifier) @property)

(string) @string
(scalar) @string.special

(punctuation) @punctuation.delimiter
