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

# Run the cross-crate corpus/round-trip/snapshot suite only.
test-suite:
    cargo test -p tomet-tests

# Accept new snapshot output as the reference (read the diff first).
update-refs:
    TOMET_UPDATE_REF=1 cargo test -p tomet-tests

# Regenerate Tree-sitter parser C files (src/parser.c, src/grammar.json) from grammar.js and test.
gen-treesitter:
    cd crates/tree-sitter-tomet && npx -y tree-sitter-cli@0.26.12 generate
    cargo test -p tree-sitter-tomet

# Launch the TUI workbench.
tui path=".":
    cargo run -p tomet -- tui {{path}}

# Check formatting across workspace.
format-check:
    cargo fmt --all -- --check

# Check every `.tmt` this workspace owns: it parses, every element it
# writes exists in a namespace the document has in scope, and it is
# formatted -- plus that every document under docs/ sits somewhere
# `docs/.writ.tmt`'s placement table names.
#
# The sweep is the workspace index, so `workspace.ignore` decides what is
# in it. That is why `tests/fixtures`, `tests/ref` and `tests/store` are
# absent: frozen input, where a broken link or a stale export is the
# coverage.
#
# Not part of `cargo test` on purpose -- these are the repo's real,
# evolving documentation, not fixed fixtures, so a run here is meant to be
# triggered manually (or from CI) rather than failing unrelated code
# changes.
#
# The placement check stays here because it is this repository's docs
# layout, not a rule of the language. It is `twrit`'s job when `twrit`
# reads `.writ.tmt` itself.
docs-check:
    cargo build -p tomet
    ./target/debug/tomet check .
    ./target/debug/tomet format --check .
    ./target/debug/tomet export --check .
    ./target/debug/tomet cli-docs --check --out tmtroot/generated/cli.tmt
    @stray=$(find tmtroot -name '*.tmt' -exec grep -L 'export:' {} +); \
    if [ -n "$stray" ]; then \
        echo "$stray" | while IFS= read -r f; do \
            echo "TMTROOT FAIL: $f -- declares no @config(export:), so it produces nothing"; \
        done; \
        exit 1; \
    fi
    @if command -v twrit >/dev/null 2>&1; then \
        twrit check .; \
    elif [ -x ../tomet-writ/target/debug/twrit ]; then \
        ../tomet-writ/target/debug/twrit check .; \
    fi

# Run the two guards that `.writ.tmt` entries name but nothing executed.
#
# Both were written into a writ under `---[ Guard ]---` as a shell block
# and wired to nothing, so the entry looked guarded while the rule was
# held by nobody. Both held when this recipe was first written; the point
# is that nothing would have noticed if they stopped.
#
# Separate from `docs-check` on the same reasoning that keeps that recipe
# out of `cargo test`: these are checks over prose this repository is
# still writing, and a run aimed at one should not fail on the other.
writ-check:
    @stray=$(find . -name '.writ.tmt' -not -path './target/*' \
        -exec grep -lP '[\x{3040}-\x{30ff}]' {} +); \
    if [ -n "$stray" ]; then \
        echo "$stray" | while IFS= read -r f; do \
            echo "LANGUAGE FAIL: $f -- kana in a writ; docs-language says every .writ.tmt is English"; \
        done; \
        exit 1; \
    fi
    @stray=$(git ls-files --others --ignored --exclude-standard .tomet); \
    if [ -n "$stray" ]; then \
        echo "$stray" | while IFS= read -r f; do \
            echo "AUTHORED FAIL: $f -- ignored file under .tomet; dot-tomet-is-authored says every file there is hand-written"; \
        done; \
        exit 1; \
    fi

# Clean Zed editor extension build and installation cache.
clean-zed-cache:
    rm -rf ~/.local/share/zed/extensions/installed/tomet ~/.local/share/zed/extensions/work/tomet

# Set up Helix workspace config and user runtime queries for local development.
setup-helix:
    @mkdir -p .helix
    @ln -sfr editors/helix/languages.toml .helix/languages.toml
    @mkdir -p ~/.config/helix/runtime/queries
    @ln -sfr editors/helix/queries/tomet ~/.config/helix/runtime/queries/tomet
    @echo "✅ Linked .helix/languages.toml and ~/.config/helix/runtime/queries/tomet"
    @if command -v hx >/dev/null 2>&1; then hx --health tomet; fi

# Remove Helix development runtime queries.
clean-helix:
    @rm -rf ~/.config/helix/runtime/queries/tomet
    @echo "🧹 Removed ~/.config/helix/runtime/queries/tomet"

