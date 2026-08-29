# Project dev-utility recipes for Tomet toolchain.

# List available recipes.
default:
    @just --list

# Build the entire workspace.
build:
    cargo build --workspace

# Run all workspace tests (including Tree-sitter tests).
test:
    cargo test --workspace

# Run tree-sitter-tomet tests only.
test-treesitter:
    cargo test -p tree-sitter-tomet

# Regenerate Tree-sitter parser C files (src/parser.c, src/grammar.json) from grammar.js and test.
gen-treesitter:
    cd crates/tree-sitter-tomet && npx -y tree-sitter-cli@0.26.12 generate
    cargo test -p tree-sitter-tomet

# Launch the Web Real-Time Playground.
playground port="8787":
    cargo run -p tomet -- playground --port {{port}}

# Launch the TUI workbench.
tui path=".":
    cargo run -p tomet -- tui {{path}}

# Check formatting across workspace.
format-check:
    cargo fmt --all -- --check

# Validate that every .tmt/.tmt file under docs/ parses cleanly and is
# correctly formatted. Not part of `cargo test` on purpose -- these are
# the repo's real, evolving documentation, not fixed fixtures, so a run
# here is meant to be triggered manually (or from CI) rather than
# failing unrelated code changes.
docs-check:
    cargo build -p tomet
    fail=0; \
    tmp=$(mktemp); \
    find docs -type f \( -name '*.tmt' -o -name '*.tmt' \) > "$tmp"; \
    while IFS= read -r f; do \
        if ! ./target/debug/tomet check -q "$f"; then echo "PARSE FAIL: $f"; fail=1; fi; \
        if ! ./target/debug/tomet format --check "$f" >/dev/null 2>&1; then echo "FORMAT FAIL: $f"; fail=1; fi; \
    done < "$tmp"; \
    rm -f "$tmp"; \
    exit $fail

# Clean Zed editor extension build and installation cache.
clean-zed-cache:
    rm -rf ~/.local/share/zed/extensions/installed/tomet ~/.local/share/zed/extensions/work/tomet

