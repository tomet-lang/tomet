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
│   ├── tomet/            # Main CLI tool (check, ast, html, serve, playground, to-md, format)
│   ├── tomet-lsp/        # Language Server Protocol implementation
│   ├── bindings/             # Language bindings (Java, JS, Python)
│   └── integrations/         # Editor extensions (VS Code, Zed, Helix, Neovim)
├── crates/
│   ├── tomet-ast/        # Shared AST & Value data models
│   ├── tomet-lexar/      # Cursor lexer over &str
│   ├── tomet-parser/     # Recursive-descent parser (source of truth for grammar)
│   ├── tomet-semantics/  # I/O-free classification of what an Element means
│   ├── tomet-resolver/    # File-referencing preprocessor (@settings(file:...))
│   ├── tomet-validator/  # Schema & lint validation (in progress)
│   ├── converters/
│   │   ├── html/             # tomet-html: Document -> HTML
│   │   └── markdown/         # tomet-markdown: CommonMark <-> Document
│   ├── tomet-formatter/  # Code formatting & whitespace normalization
│   ├── serde_tomet/      # Serde deserializer/serializer for Tomet Value model
│   └── tree-sitter-tomet/# Tree-sitter grammar for editor syntax highlighting
└── docs/                     # Architecture notes, spec, and cheat sheet
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
