# tomet-cst

Lossless Concrete Syntax Tree (CST) foundation for Tomet, powered by `rowan`.

## Architecture & Responsibilities

1. **Lossless Concrete Syntax Tree Foundation**:
   - `tomet-cst` provides a Rowan-based green/red tree architecture preserving 100% of source characters.
   - Preserves all trivia: whitespace, newlines, and comments.
   - Guaranteed Invariant: `syntax_node.text().to_string() == original_source`.

2. **Core Types & Exports**:
   - `SyntaxKind`: Enum of raw token and composite node kinds.
   - `TometLanguage`: Implementation of `rowan::Language`.
   - `SyntaxNode`, `SyntaxToken`, `SyntaxElement`: Green/red tree handles with offset navigation.
   - `TextRange`, `TextSize`: Precise byte-offset spans for LSP diagnostics and refactorings.

3. **Tree shape** (built by `tomet-parser::parse_cst`, documented in its `//!`):
   - `SECTION` (with a `SECTION_HEADING`) nests by `=` level; `THEMATIC_BREAK`, `PARAGRAPH`, `LIST`, `CODE_BLOCK`, `BLOCK_ELEMENT` are its blocks. `HEADING` is the legacy `#` form.
   - An element is a `SIGIL` plus `ARGS` / `VALUE_DATA` / `CONTENT`, `CONNECT` (`:name(...)`) and `FENCE` (`+++`); `@x`, `^x`, `$x` inside text are `INLINE_ELEMENT`, and `${...}` is an `INTERP_EXPR`.
   - Group bodies are `MAP_ENTRY` / `SEQ_ITEM`, with `MAP_LITERAL` / `SEQ_LITERAL` for nested values.

