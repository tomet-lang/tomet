# Task: Rename embedded.rs to data.rs and Remove cli.md Export

## Background

1. `tomet/crates/tomet-semantics/src/elements/embedded.rs` contains only `element_data`, extracting `{...}` data groups. The name `embedded` is a legacy artifact of the deleted embedded-format feature (`format: json/yaml`). It should be renamed to `data.rs`.
2. `tomet/docs/reference/cli.md` is an isolated Markdown artifact generated via `@config(export: ...)` inside `cli_docs.rs`. Tomet's documentation lives in `tmtroot/generated/cli.tmt`. Generating `docs/reference/cli.md` is unnecessary and clutters the workspace with a stray `docs/` directory.

## Implementation Steps

- [x] 1. Work directly in default workspace (per user instruction to combine into single refactoring commit).
- [x] 2. Rename `crates/tomet-semantics/src/elements/embedded.rs` to `data.rs`, and update `elements/mod.rs`, `lib.rs`, and usages across workspace.
- [x] 3. Update `apps/cli/src/commands/cli_docs.rs` to remove `EXPORT_PATH` and `@config(export: ...)`.
- [x] 4. Remove `docs/reference/cli.md` and `docs/reference/`.
- [x] 5. Regenerate `tmtroot/generated/cli.tmt` with `tomet cli-docs --out tmtroot/generated/cli.tmt` and `tomet api .`.
- [x] 6. Run verification (`cargo test --workspace`, `cargo test -p tomet-tests`, `export --check`, `format --check`, `cli-docs --check`, `api --check`).
- [x] 7. Propose commit message to user and commit upon explicit confirmation.

## Current Status

Completed. Verified and committed with user approval.
