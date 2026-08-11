# Project dev-utility recipes. `just` handles dispatch and `--list` doubles
# as a usage listing on its own, so only the fzf-based option picker used by
# build/dev/release/install (see `common` below) needed any real porting.

# List available recipes.
default:
    @just --list
