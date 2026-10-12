# Address system follow-ups

Core address system design & implementation (Steps 1-5, address syntax, path traversal,
intra-document dangling reference check in `tomet check`) completed and folded into
`tmtroot/docs/spec/syntax/addressable.tmt` on 2026-10-12.

This task file tracks remaining tooling and vault-level follow-ups.

## Open

- [ ] Renaming an `id` with reference update:
      - Update `@link(#id)` and `${#id...}` references across document / workspace when an element's `#(id)` is renamed.
      - Intended for `tomet refactor` CLI command or LSP Rename Provider.
- [ ] Vault-wide `@meta` `id` uniqueness check:
      - Spec states `@meta` `#(id)` has vault-wide uniqueness scope.
      - Check for duplicate `@meta` IDs across files in `tomet check <dir>` or workspace sweeps.
      - Cross-vault reference integrity.
