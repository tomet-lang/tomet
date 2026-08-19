# Project dev-utility recipes for TypedMark toolchain.

# List available recipes.
default:
    @just --list

# Build the entire workspace.
build:
    cargo build --workspace

# Run all workspace tests (including Tree-sitter tests).
test:
    cargo test --workspace

# Run tree-sitter-typedmark tests only.
test-treesitter:
    cargo test -p tree-sitter-typedmark

# Regenerate Tree-sitter parser C files (src/parser.c, src/grammar.json) from grammar.js and test.
gen-treesitter:
    cd crates/tree-sitter-typedmark && npx -y tree-sitter-cli@0.26.12 generate
    cargo test -p tree-sitter-typedmark

# Launch the Web Real-Time Playground.
playground port="8787":
    cargo run -p typedmark -- playground --port {{port}}

# Launch the TUI workbench.
tui path=".":
    cargo run -p typedmark -- tui {{path}}

# Check formatting across workspace.
format-check:
    cargo fmt --all -- --check
