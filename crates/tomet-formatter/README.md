# tomet-formatter

Whitespace hygiene and table layout formatter for Tomet source text.

## Responsibilities

1. **Lossless Whitespace Formatting (`format_source`)**:
   - Normalizes line endings to LF, strips trailing whitespace, collapses multiple blank lines to one.
   - AST-aware: uses source `Span`s to preserve raw literal content in `<codeblock>` and `content:raw` elements verbatim.
   - Idempotent: `format_source(&format_source(src)) == format_source(src)`.
   - Never mutates AST structure or document metadata.

2. **Config-Driven Table Layout Formatting (`format_source_with_config`)**:
   - Formats `@table[...]` column widths and cell alignments according to `PrinterConfig` (e.g. `table.adjust_width`, `table.max_col_width`, `table.align`).
