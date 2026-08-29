# tomet-parser

The recursive-descent parser (`&str -> Document / Value`) and authoritative source of truth for the Tomet grammar.

## Architecture & Responsibilities

1. **Deterministic Static Parser Boundary**:
   - `tomet-parser` is strictly a pure, side-effect-free, deterministic static parser. It performs zero I/O, external file resolution, or dynamic code execution.
   - Given identical input text, it produces identical AST output with guaranteed linear/predictable time complexity.

2. **Module Layout**:
   - `document.rs`: Block-level dispatch loop, paragraphs, and document-level parsing.
   - `element.rs`: Typed elements (`<T>`, `@name`, bare `@`), group ordering (`(args)`, `[content]`, `{value}`), and colon connect syntax (`:(...)`, `:{...}`).
   - `heading.rs`: Headings (`#[...]`) and thematic breaks (`---`, `---[ title ]---`).
   - `list.rs`: Ordered and unordered lists (`-`, `-.`), markers, and item attributes.
   - `codeblock.rs`: Fenced code blocks and raw verbatim spans (`content: raw`).
   - `inline.rs`: Inline scanning, formatting delimiters (`*em*`, `**strong**`, `==mark==`), autolinks, and inline comments.
   - `interp.rs`: `${...}` interpolation expressions (identifiers, member chains, calls, literals).
   - `value.rs`: Lightweight Tomet data grammar (maps, sequences, scalars, comments).
   - `embedded_format.rs`: Embedded JSON/YAML/TOML in `{value}` groups.
   - `error.rs`: Source location (`Position`/`Span`) aware error diagnostics.

3. **Tree-sitter Synchronization**:
   - `tomet-parser` is the source of truth for grammar definitions.
   - `tree-sitter-tomet` is a separate hand-maintained grammar approximation used for editor syntax highlighting. Run `cargo test -p tree-sitter-tomet` to verify parser synchronization.
