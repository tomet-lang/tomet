# tomet-formatter

Whitespace hygiene and config-gated structural patch formatter for Tomet source text.

## Responsibilities

1. **Lossless Whitespace Formatting (`format_source`)**:
   - Normalizes line endings to LF, strips trailing whitespace, collapses multiple blank lines.
   - AST-aware: uses source `Span`s to preserve raw literal content in `<codeblock>` and `content:raw` elements verbatim.
   - Idempotent: `format_source(&format_source(src)) == format_source(src)`.

2. **Config-Driven Structural Patches (`format_source_with_config`)**:
   - Splices `@meta` id additions or format conversions directly into the source text at exact target spans without rewriting untouched document blocks.
