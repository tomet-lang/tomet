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

# Validate that every .tm/.tmt file under docs/ parses cleanly and is
# correctly formatted. Not part of `cargo test` on purpose -- these are
# the repo's real, evolving documentation, not fixed fixtures, so a run
# here is meant to be triggered manually (or from CI) rather than
# failing unrelated code changes.
docs-check:
    cargo build -p typedmark
    fail=0; \
    tmp=$(mktemp); \
    find docs -type f \( -name '*.tm' -o -name '*.tmt' \) > "$tmp"; \
    while IFS= read -r f; do \
        if ! ./target/debug/typedmark check -q "$f"; then echo "PARSE FAIL: $f"; fail=1; fi; \
        if ! ./target/debug/typedmark format --check "$f" >/dev/null 2>&1; then echo "FORMAT FAIL: $f"; fail=1; fi; \
    done < "$tmp"; \
    rm -f "$tmp"; \
    exit $fail

# Clean Zed editor extension build and installation cache.
clean-zed-cache:
    rm -rf ~/.local/share/zed/extensions/installed/typedmark ~/.local/share/zed/extensions/work/typedmark

