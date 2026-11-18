<!-- Generated from tmtroot/contributing.tmt. Edit that, then `tomet export .`. -->

# Contributing to Tomet

## Documentation Policy

- All Markdown files are generated artifacts. Never edit `.md` files directly.
- Always edit the source `.tmt` files under `tmtroot/` (or crate-local `.tmt` files).
- Run `tomet export` to update Markdown files, and verify with `tomet export --check`.

## Repository Layout

```
tomet/
├── apps/
│   ├── cli/               # Main CLI binary (`tomet`)
│   └── lsp/               # Language Server Protocol implementation (`tomet-lsp`)
├── bindings/               # Language bindings (Java, JS, Python)
├── editors/                # Editor extensions (VS Code, Zed, Helix, Neovim)
├── crates/
│   ├── tomet-syntax-*     # Syntax layer: cst, lexer, parser, ast, tree
│   ├── tomet-semantics*   # Semantics layer: semantics, validator, compute, resolver
│   ├── tomet-transform/   # In-memory AST refactoring & macro rewrite engine
│   ├── tomet-convert-*    # Document format converters: html, markdown, typst, pandoc
│   ├── tomet-format-*     # Source output: formatter, printer, style, field-utils
│   ├── tomet-workspace*   # Multi-file I/O: workspace, indexer, links, config
│   ├── cotomet/           # cotomet data language parser and Serde adapter
│   └── tree-sitter-tomet/ # Tree-sitter grammar for editor syntax highlighting
└── tmtroot/                # Canonical source documents and generated API docs
    ├── generated/crates/   # Generated API docs for workspace crates
    ├── docs/               # Specs, design notes, and guides
    └── locales/            # Multilingual localized documentation (e.g. ja/)
```

## Building and Testing

### Build workspace

```bash
cargo build
```

### Run tests

This is a parser/CLI/LSP/editor-extensions project with no standalone GUI app. Verify with automated checks and code reading:

- **`cargo test --workspace`**: Runs every test in the workspace (`just test`).
- **`cargo test -p tomet-tests`**: Runs the cross-crate corpus and snapshot suite in `tests/` (`just test-suite`).
- **`cargo test -p tomet-tests --test corpus`**: Runs one target of that suite.
- **`cargo test -p tree-sitter-tomet`**: Runs the tree-sitter grammar tests.
- **`TOMET_UPDATE_REF=1 cargo test -p tomet-tests`**: Rewrites the snapshot references in `tests/ref/` with the current output.

When a snapshot fails, the actual output is written to `tests/store/` at the same relative path as its reference,
so the two can be diffed directly.

Editor extensions (`editors/vscode`, `editors/zed`, `editors/helix`) carry their own tests; run those
and read the code.

### Write tests

- Layer-local tests stay in their own crate. Only tests that need the shared `.tmt` corpus, or that cross a
crate boundary, live in the `tests/` package (`tomet-tests`).
- A failing snapshot means "this changed", not "this broke": the references pin the output at the time they were
written, bugs included. Read the diff, decide whether the new output is better, and only then accept it with
`TOMET_UPDATE_REF=1`.
- `tests/fixtures/` is frozen. A fixture changes only when a test needs it to. When a grammar change lands, add a
fixture covering the new construct instead of expecting an existing file to pick it up. A fixture that still
exercises retired syntax is regression coverage, not a bug.
- `tests/SYNTAX.md` says what the parser accepts; `tmtroot/spec/` says what it should accept. Where they
disagree, the disagreement is the finding: do not silently edit one to match the other.
- The known-exception lists (`KNOWN_UNPARSEABLE`, `KNOWN_FORMAT_CHANGES_DOCUMENT`) are asserted to match reality
exactly, and each entry carries its reason. They are defect records, not permanent carve-outs.

## Packaging

Nix is the packaging story.
`nix/pkgs/tomet.nix` and `nix/pkgs/vscode-extension.nix` build the CLI and the VS Code extension; `nix/dev.nix` is the dev shell.

## Finding your way around

- Crate layers and the dependency direction between them: the `crate-layering` entry in `.writ.tmt`.
- What a crate is and why it exists: that crate's `//!` module doc and `tmtroot/generated/crates/`.
- What a crate depends on: its `Cargo.toml`.
- What the CLI does: `tomet --help`.

## Commit Convention

All commits must follow the Conventional Commits format: `type(scope): description`.

### Types

- **feat**: New feature or capability.
- **fix**: Bug fix.
- **docs**: Documentation changes (`.tmt` or exported docs).
- **refactor**: Code restructuring without changing behavior.
- **perf**: Performance improvement.
- **test**: Adding or correcting tests.
- **chore**: Build scripts, dependencies, tooling, repo maintenance.

### Scopes

Use the affected component or crate name as the scope and `nix`, `ci`, `deps`, `rule` (for `.writ.tmt`)

> [!tip]
> If a change spans multiple components or the whole workspace,
> omit the scope or use a broad category (e.g. `refactor(crates): ...` or `chore: update dependencies`).

## Verification

Before committing or submitting changes, ensure all checks pass:

Useful recipes defined in `justfile`:

- **`just test`**: Runs all workspace tests (`cargo test --workspace`).
- **`just test-suite`**: Runs the cross-crate corpus and snapshot test suite (`cargo test -p tomet-tests`).
- **`just docs-check`**: Runs the build of `tomet-cli`, `sisho check .` (`tomet check`, `tomet format --check`, `tomet export --check`, `twrit check`, `catasto check`), and `tomet spec --all --check --out tmtroot/generated`.
- **`just docs`**: Updates, regenerates, and formats all derived documentation.
- **`just writ-check`**: Verifies `.writ.tmt` guard rules and language conventions.

Direct checks (if running individually):

- `cargo check` / `cargo build` / `cargo test`
- `twrit check`: Reads `.writ.tmt` files.
- `tomet export --check`: Verifies that all exported `.md` documents match their source `.tmt` files.
- `tomet spec --all --check --out tmtroot/generated`: Verifies that generated specifications match source definitions.

