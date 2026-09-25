# Tomet

[![VS Code Marketplace](https://vsmarketplacebadges.dev/version/tomet-lang.tomet.svg)](https://marketplace.visualstudio.com/items?itemName=tomet-lang.tomet)
[![Installs](https://vsmarketplacebadges.dev/installs-short/tomet-lang.tomet.svg)](https://marketplace.visualstudio.com/items?itemName=tomet-lang.tomet)

Tome to me!!

Tomet (`.tmt` / `.tmt`) is a human-readable, strongly-typed markup language and Rust toolchain designed as a modern alternative.

## Example

```tm
@meta{id: doc-asdfadsf}

=[ heading ]

- list #(tag, tag2, tag3, tag4)
  -. orderd

@table
|[][][]

@callout(info)
| content

=
```

## Repository Layout

```
tomet/
├── apps/
│   ├── cli/              # Main CLI binary (`tomet`)
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
│   ├── tove/             # TOVE data language parser and Serde adapter
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

## Packaging

Nix is the packaging story. `nix/pkgs/tomet.nix` and
`nix/pkgs/vscode-extension.nix` build the CLI and the VS Code extension;
`nix/dev.nix` is the dev shell.

## Finding your way around

- Crate layers and the dependency direction between them: the
  `crate-layering` entry in `.writ.tmt`.
- What a crate is and why it exists: that crate's `//!` module doc.
- What a crate depends on: its `Cargo.toml`.
- What the CLI does: `tomet --help`.
- The language itself: `docs/README.tmt` is the map.
