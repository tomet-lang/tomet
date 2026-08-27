# tree-sitter-typedmark

Tree-sitter grammar and queries for [TypedMark](https://github.com/tefww/typedmark) (`.tm` / `.tmt`).

## Role & Design Boundary

TypedMark maintains a clear division of responsibility between editor integrations and parser tooling:

- **`typedmark-parser` (Rust)**: The single source of truth for the language grammar. Powers core CLI functionality, converters, and `typedmark-lsp` (diagnostics, schema validation, and formatting).
- **`tree-sitter-typedmark` (Tree-sitter Grammar)**: A lightweight, resilient approximation used **exclusively** for fast editor syntax highlighting, code folding, and bracket matching in Tree-sitter-based editors (e.g., Zed, Helix, Neovim). It intentionally delegates semantic analysis and strict parsing validation to `typedmark-lsp`, ensuring fast and fault-tolerant highlighting during live user editing.

## Structure

- `grammar.js`: Hand-maintained Tree-sitter grammar specification.
- `src/scanner.c`: Hand-written external C scanner for token lookaheads (list markers, scalar tokens).
- `queries/`: Tree-sitter query definitions (`highlights.scm`, `indents.scm`, `brackets.scm`).

## Testing

Run tests to verify grammar parsing and query validity:

```bash
cargo test -p tree-sitter-typedmark
```

## Regenerating Parser C Code

If `grammar.js` is modified, regenerate the C parser files using `tree-sitter-cli`:

```bash
npx tree-sitter-cli generate
```
