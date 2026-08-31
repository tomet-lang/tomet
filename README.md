# Tomet

Tomet (`.tmt` / `.tmt`) is a human-readable, strongly-typed markup language and Rust toolchain designed as a modern alternative to Markdown and JSON metadata formats.

## Features

- **Unified Element Syntax**: Standardized element format `<T>(args)[content]{value}` for attributes, raw/rendered content areas, and structured data.
- **Embedded Formats**: Seamlessly embed JSON, YAML, or TOML inside `{value}` blocks via `format: json|yaml|toml`.
- **Inferred References**: Compact `@` syntax for URLs (`@(url:...)`), files (`@(file:...)`), and reference links.
- **Bidirectional CommonMark Support**: Import from and export to CommonMark.
- **Serde Integration**: `serde_tomet` for data-only `.tmt` documents and `{value}` structures.
- **Toolchain & Integrations**: CLI tool (`tomet`), Language Server Protocol (`tomet-lsp`), tree-sitter parser, and VS Code / Zed editor integrations.

## Repository Layout

```
tomet/
├── apps/
│   ├── cli/              # Main CLI binary (`tomet`)
│   ├── web/              # Web playground / wasm frontend
│   ├── lsp/              # Language Server Protocol implementation (`tomet-lsp`)
│   └── tui/              # Interactive TUI workbench (`tomet-tui`)
├── bindings/             # Language bindings (Java, JS, Python)
├── editors/              # Editor extensions (VS Code, Zed, Helix, Neovim)
├── crates/
│   ├── tomet-syntax-*    # Syntax layer: cst, lexer, parser, ast, tree
│   ├── tomet-semantics*  # Semantics layer: semantics, validator, compute (eval), resolver (linker)
│   ├── tomet-transform/  # In-memory AST refactoring & macro rewrite engine
│   ├── tomet-convert-*   # Document format converters: html, markdown, typst
│   ├── tomet-format-*    # Source output: formatter, printer, style, field-utils
│   ├── tomet-workspace*  # Multi-file I/O: workspace, indexer, links, config
│   ├── serde_tomet/      # Serde deserializer/serializer for Tomet Value model
│   └── tree-sitter-tomet/# Tree-sitter grammar for editor syntax highlighting
└── docs/                 # Architecture notes, spec, and documentation
```

## Building and Testing

### Build workspace
```bash
cargo build
```

### Run tests
```bash
cargo test --workspace
```

### Run tree-sitter tests
```bash
cargo test -p tree-sitter-tomet
```

## Architecture

For a detailed overview of the crate pipeline, parser design, tree-sitter grammar synchronization, and AST constraints, see [docs/develop/architecture.md](docs/develop/architecture.md).
