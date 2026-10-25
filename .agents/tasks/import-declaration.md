# `::import(...)`: bring in the declarations of another file

Split out of the config rework (author, 2026-10-30; commit `2e5598bb`): not implemented there. Delete this file when done.
Destination of what is worth keeping: `tmtroot/spec/declarations/std.import.declaration.tmt` (new) and the doc-comments of
`tomet-load-config` / `tomet-config`.
Related: `docs-config-rework` (the declarations this imports are documented there), the layers (`tomet-load-config`) it sits in.

## What it is

`::import(path, ...)` makes another file's declarations part of this file's layer. It is what `::config(import:)` always
meant (a document pointing at a config file), without the word `config`: the file imported is a composite one, carrying
`::format`, `::fields`, `::blueprints`, `::vocabularies`, `::macros`, ... A file whose own `::kind` is `blueprint` or
`vocabulary` is NOT imported this way; an entry of `::blueprints` / `::vocabularies` references it with `@file(path)`.

`::import` makes declarations known by path; `::use` binds a vocabulary by name in a document. They are different
concepts.

## Leans (assistant, not confirmed)

- A file contributes one layer; its imports sit just below its own declarations: its own wins, among several imports the
  later wins, a cycle is an error.
- The vault-only check applies to imported declarations too (a folder config importing a file that writes `::workspace`).
- Paths are relative to the importing file's directory (the ancestor walk in LSP `resolve_effective_config` is wrong and
  goes away with the old `::config(import:)`).
- Real uses are secondary: splitting one config file, sharing one across vaults. Implement when someone needs it.

## Open

- [ ] Whether it is allowed in a document, or only in `vault.config.tmt` / `.config.tmt`.
- [ ] How a rename of the imported file is followed (an `@file` link would be rewritten by `tomet`; a plain path is not).
- [ ] Registry paths. `::blueprints` / `::vocabularies` entries (`name: @file(path)`) are read relative to the vault root,
  whichever layer wrote them, because the merge keeps values and not where they came from. A folder config that registers a
  vocabulary scoped to its folder, and a file that imports another, both want the path relative to the file that wrote it. Decide
  together with this task (the layer would have to carry its directory).
- [ ] Today nothing imports: the old `::config(import:)` / `::settings(file:)` was read only by the language server's hover
  fallback and is removed (`2e5598bb`); there is no compatibility to keep.
