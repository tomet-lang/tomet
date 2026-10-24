# Tomet for Helix

Syntax highlighting, indentation, and language server support for Tomet (`.tmt`) files in Helix, and
highlighting, diagnostics and formatting for standalone cotomet data files (`.ctmt`).

## Requirements

- `tomet-lsp` binary on `$PATH` (e.g. built via `cargo install --path apps/lsp` or via Nix).
- Helix 23.05 or later (with tree-sitter support).

## Quick Setup (using `just`)

If developing inside this repository, simply run:

```sh
just setup-helix
```

This links `.helix/languages.toml` and Tree-sitter queries into `~/.config/helix/runtime/queries/tomet/` automatically.

## Manual Setup

### 1. Configure Language and LSP

Add the contents of [`languages.toml`](languages.toml) to your Helix configuration at `~/.config/helix/languages.toml`:

```toml
[[language]]
name = "tomet"
scope = "source.tmt"
injection-regex = "tmt|tomet"
file-types = ["tmt"]
roots = ["tomet.config.tmt", "default.config.tmt", ".git"]
comment-token = "//"
block-comment-tokens = { start = "/*", end = "*/" }
indent = { tab-width = 2, unit = "  " }
language-servers = [ "tomet-lsp" ]

[language-server.tomet-lsp]
command = "tomet-lsp"

[[grammar]]
name = "tomet"
source = { git = "https://github.com/tefww/tomet", rev = "main", subpath = "crates/tree-sitter-tomet" }
```

[`languages.toml`](languages.toml) also declares `cotomet` (`.ctmt`) and its grammar,
`crates/tree-sitter-cotomet`; copy those two entries as well to open `.ctmt` files.

### 2. Install Tree-sitter Queries

Copy or symlink the query files in `queries/tomet/` to your Helix runtime directory:

```sh
mkdir -p ~/.config/helix/runtime/queries/tomet ~/.config/helix/runtime/queries/cotomet
cp queries/tomet/*.scm ~/.config/helix/runtime/queries/tomet/
cp queries/cotomet/*.scm ~/.config/helix/runtime/queries/cotomet/
```

### 3. Fetch and Build Grammar

Inside Helix, or via the command line:

```sh
hx --grammar fetch
hx --grammar build
```

Then open any `.tmt` file in Helix to enjoy syntax highlighting, diagnostics, formatting, and auto-completion.
