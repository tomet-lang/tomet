; Syntax highlighting for Tomet, using the standard capture names
; shared across tree-sitter-based editors (Zed, Neovim, Helix, ...).
; Loaded by `apps/zed-extension` via a symlink at
; `apps/zed-extension/languages/tomet/highlights.scm`.
;
; Zed's themes don't key off the `@markup.*` names at all -- Zed has its
; own fixed capture vocabulary (`@title`, `@emphasis`, `@emphasis.strong`,
; `@text.literal`, ...; see `zed.dev/docs/extensions/languages`), and an
; unmatched capture just renders uncolored rather than falling back to
; anything. Zed resolves multiple captures on one node right-to-left (first
; capture it has theme styling for, tried from the right), so the Zed name
; is appended after the standard one below rather than replacing it -- this
; keeps the query working for both Zed and standard-capture editors.

; The `[`/`]` wrapping a heading's content have no node of their own in
; the grammar (unlike `args_group`/`content_group`/`value_group`'s
; delimiters below), so without an explicit capture here they'd just
; inherit `heading`'s own `@title` below -- same color as the content,
; not as `heading_marker`. Capturing them to match `heading_marker`
; instead keeps "structural" (`#`/`[`/`]`) and "content" visually
; distinct, without touching the content's own color.
(heading_marker) @markup.heading.marker @punctuation.special
(heading "[" @punctuation.special)
(heading "]" @punctuation.special)
(heading) @markup.heading @title

(thematic_break) @punctuation.special

; Same reasoning as the heading brackets above: `titled_thematic_break`'s
; `[`/`]` have no node of their own, so without an explicit capture they'd
; just inherit the whole node's `@title` below.
(thematic_break_marker) @punctuation.special
(titled_thematic_break "[" @punctuation.special)
(titled_thematic_break "]" @punctuation.special)
(titled_thematic_break) @markup.heading @title

; `@comment` is shared vocabulary between the standard capture set and
; Zed's own (unlike `@markup.*` above), so one capture covers both.
(line_comment) @comment
(block_comment) @comment

(emphasis) @markup.italic @emphasis
(strong) @markup.bold @emphasis.strong
(mark) @markup.strikethrough

(code_span) @markup.raw.inline @text.literal

; ``` fence sugar for `<codeblock>(lang:xxx)[code]` -- same raw/literal
; treatment as `code_span` above, plus its own marker color so the
; ``` delimiters read as structural, not as part of the code.
(fenced_code_block) @markup.raw.block @text.literal
(fence_marker) @punctuation.special

(unordered_list_item) @markup.list.unnumbered @keyword.control
(ordered_list_item) @markup.list.numbered @keyword.control

; Outliner status markers (`- (T)`, `- (?)`, `- ("in progress")`) -- a
; distinct color from the rest of the item's content, same idea as
; `heading_marker` above.
(list_marker) @markup.list.marker @constant

; The `@name` element sigil and its name. There is one element sigil
; whatever the element's placement -- see `grammar.js`.
(inline_element "@" @tag)
(inline_element name: (identifier) @tag)

; A `+++` fence body is verbatim text, like a code block's.
(raw_fence) @string.special

; `${...}` interpolation -- sigil/brace treated like other structural
; delimiters (`@punctuation.special`, matching `heading_marker`/
; `fence_marker` above), the call name colored like a function call.
(interpolation "${" @punctuation.special)
(interpolation "}" @punctuation.special)
(interp_call name: (identifier) @function)
(number) @number

; `(args)`/`[content]`/`{value}` group delimiters.
(args_group "(" @punctuation.bracket)
(args_group ")" @punctuation.bracket)
(content_group "[" @punctuation.bracket)
(content_group "]" @punctuation.bracket)
(content_group) @variable.parameter @variable.other
(marked_content "|" @punctuation.special @keyword.control)
(value_group "{" @punctuation.bracket)
(value_group "}" @punctuation.bracket)

; well-known `@(url:..)`/`@(file:..)`/`@(ref:..)` link-shaped elements
; render as links even though the grammar doesn't special-case them
; structurally (that inference is `tomet-html`'s job, not the
; grammar's) -- highlighting the `map_entry` key is close enough here.
(map_entry key: (identifier) @property)

(string) @string
(scalar) @string.special

(punctuation) @punctuation.delimiter
