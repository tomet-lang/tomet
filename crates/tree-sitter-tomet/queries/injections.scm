; Language injection queries for Tomet.
; Injects syntax highlighting into fenced code block content according to its language identifier.

(fenced_code_block
  lang: (text) @injection.language
  content: (text) @injection.content)
