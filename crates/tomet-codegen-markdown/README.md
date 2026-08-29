# tomet-markdown

Bidirectional converter between CommonMark / Markdown and Tomet AST.

## Responsibilities

1. **Markdown to Tomet Import (`from_markdown` / `from_markdown_with_options`)**:
   - Parses CommonMark via `pulldown-cmark`.
   - Converts YAML frontmatter to `@meta(format:yaml){...}` blocks.
   - Converts Obsidian-style `[[wikilinks]]` to `@link(ref:...)[...]` elements.
   - Handles headings, lists, tables, blockquotes, code blocks, emphasis, and autolinks.

2. **Tomet to Markdown Export (`to_markdown`)**:
   - Converts `tomet_ast::Document` into clean CommonMark format.
   - Converts `@meta` blocks back to YAML frontmatter.
   - Converts typed elements and callouts into blockquotes or HTML tags.
