# TypedMark

TypedMark (`.tm` / `.tmt`) is a human-readable, strongly-typed markup language and Rust toolchain designed as a modern alternative to Markdown and JSON metadata formats.

## Features

- **Unified Element Syntax**: Standardized element format `<T>(args)[content]{value}` for attributes, raw/rendered content areas, and structured data.
- **Embedded Formats**: Seamlessly embed JSON, YAML, or TOML inside `{value}` blocks via `format: json|yaml|toml`.
- **Inferred References**: Compact `@` syntax for URLs (`@(url:...)`), files (`@(file:...)`), and reference links.
- **Bidirectional CommonMark Support**: Import from and export to CommonMark.
- **Serde Integration**: `serde_typedmark` for data-only `.tm` documents and `{value}` structures.
- **Toolchain & Integrations**: CLI tool (`typedmark`), Language Server Protocol (`typedmark-lsp`), tree-sitter parser, and VS Code / Zed editor integrations.

## Repository Layout

```
typedmark/
├── apps/
│   ├── typedmark/            # Main CLI tool (check, ast, html, serve, playground, to-md, format)
│   ├── typedmark-lsp/        # Language Server Protocol implementation
│   ├── bindings/             # Language bindings (Java, JS, Python)
│   └── integrations/         # Editor extensions (VS Code, Zed, Helix, Neovim)
├── crates/
│   ├── typedmark-ast/        # Shared AST & Value data models
│   ├── typedmark-lexar/      # Cursor lexer over &str
│   ├── typedmark-parser/     # Recursive-descent parser (source of truth for grammar)
│   ├── typedmark-semantics/  # I/O-free classification of what an Element means
│   ├── typedmark-resolve/    # File-referencing preprocessor (@settings(file:...))
│   ├── typedmark-validator/  # Schema & lint validation (in progress)
│   ├── converters/
│   │   ├── html/             # typedmark-html: Document -> HTML
│   │   └── markdown/         # typedmark-markdown: CommonMark <-> Document
│   ├── typedmark-formatter/  # Code formatting & whitespace normalization
│   ├── serde_typedmark/      # Serde deserializer/serializer for TypedMark Value model
│   └── tree-sitter-typedmark/# Tree-sitter grammar for editor syntax highlighting
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
cargo test -p tree-sitter-typedmark
```

## Architecture

For a detailed overview of the crate pipeline, parser design, tree-sitter grammar synchronization, and AST constraints, see [docs/develop/architecture.md](docs/develop/architecture.md).
