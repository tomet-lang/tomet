# Documents for the config, id and declarations rework

The code landed on `main` in three commits (2026-10-30): `9e97e427` (read only `vault.config.tmt` and stacked
`.config.tmt`), `05991625` (the document id is `::id`), `2e5598bb` (`::config` / `::settings` dissolved into
`::format` `::fields` `::workspace` `::export` `::blueprints` `::vocabularies` `::migration`). The documents were left
as debt on purpose, so that they are written once, in a bulk pass. This task is that pass. Delete this file when done.
Destination of what is worth keeping: `tmtroot/spec/` (listed below), the doc comments of the crates.
Related: `id-generation-settings`, `import-declaration`.

## The dictionary (landed in `d08aeac9`; in use since 2026-10-30)

Terms are mentioned by reference. The dictionary is `tmtroot/spec/dictionary/tomet.tmt` (registered in the repository's
`vault.config.tmt` under `::dictionaries`; listed in `tmtroot/layout.tmt`). It has `vault`, `vault.config.tmt`, `.config.tmt`; add a
term when a page needs one, not before. Write `@term(tomet#vault-config)` (by identifier, survives a rename) or
`@term(tomet#name("vault.config.tmt"))`. Rust comments and code fences still wait: nothing reads or rewrites a mention in a comment yet.

## Rules for this pass

- No `.md` by hand (generated). The two that have no `.tmt` source are listed below; give them one.
- `.writ.tmt` is the author's: propose the edit and stop.
- A name that appears in many places is defined once and mentioned by link (`@file(...)` in `.tmt`, a rustdoc link in
  Rust), so that `tomet` can rewrite it. That mechanism does not exist for comments yet; do not reword comments to dodge
  the old name, and do not spread the new one as a literal.
- Do not run `cargo fmt --all` / `rustfmt` outside the files being edited.

## 1. Spec pages to write

- [x] `tmtroot/spec/features/config.tmt` (written 2026-10-30, in the default workspace, uncommitted; it points to the `//!` of
  `tomet-load-config` and `tomet-config`'s `layer.rs` for the details). It was to say: the three layers (`vault.config.tmt`, `.config.tmt`, the document's own
  declarations) and their order; the merge (key by key, the inner layer wins, a list or a scalar is replaced, a macro, a
  variable and a registered name are replaced by name); the nearest `vault.config.tmt` ends the walk; no vault means no file
  config; a file that cannot be read or parsed contributes an empty layer; only `vault.config.tmt` may write
  `::workspace`, and a folder config or a document that does is ignored and reported (`VaultOnlyDeclaration`); a
  `::kind(config)` document under another name is not a layer. The truth is `tomet-load-config`'s `//!`; point to it.
  `xxx.config.tmt` other than the two names is left free for other applications.
- [x] `tmtroot/spec/declarations/`, written 2026-10-30 in the default workspace (uncommitted): `std.id`, `std.workspace`,
  `std.blueprints` (it holds the registry rules the other two registries point to), `std.vocabularies`, `std.dictionaries`,
  `std.dictionary`, `std.format`, `std.fields`, `std.export`, `std.migration`. `std.config` and `std.settings` are deleted after
  their worth-keeping text went into `std.format` and `std.vocabularies`.
  - Still to do here: `std.meta` must say `id` is not written there and why (what tomet itself interprets about the document leaves
    `::meta`; precedent: `type` became `::kind`; write the criterion once); `std.macros`, `std.vars`, `std.use`, `std.vocabulary`,
    `std.elements`, `std.element`, `std.macro` are 0-byte stubs, and `std.declaretions.declaration.tmt` is a misspelt empty file:
    write or delete (the author's files).
  - Gaps the pages record as "未実装": nothing checks that `::id` is unique across a vault; `ref:` does not read `::id`; a missing
    blueprint file is not reported (a vocabulary and a dictionary are); an inline `::dictionary` in a config layer.

## 2. Spec pages that name the old things (correct, point to the new page)

`tmtroot/spec/cli/{overview,check,format,export}.tmt`, `tmtroot/spec/lsp/{overview,editing}.tmt` (`editing.tmt` also says
the document's `::config` applies only when no config is found, and mentions `::config(import:)`),
`tmtroot/spec/syntax/addressable.tmt` (the id scope paragraph; `unique: true` under `meta:` is now `::fields`),
`tmtroot/spec/bindings/validate.tmt`, `tmtroot/spec/elements/std.{macros,vars}.element.tmt`,
`tmtroot/spec/expressions/include.expr.tmt` (the list of declarations that are not included), `tmtroot/docs/spec/elements.index.tmt`,
`tmtroot/index/elements.roots.tmt`, `tmtroot/dev/design/document-declarations.tmt`, `tmtroot/dev/plans.tmt`,
`tmtroot/spec/features/{vocabulary,blueprint}.tmt` (they register by path under `blueprints:` / `vocabularies:`).

## 3. Code-fence examples

The migration did not touch examples inside code fences. Remaining: the `::meta{ id }` examples (5, e.g.
`tmtroot/readme.tmt:21`) and `::config{ ... }` / `::settings{ ... }` examples (`tmtroot/spec/syntax/addressable.tmt`, the
`std.*` declaration pages). Rewrite each to the new declarations.

## 4. Generated pages

`tomet spec --all --out tmtroot/generated` then `sisho fix .` (`just docs`). Known drift: `tmtroot/generated/cli.tmt`
(`tomet refactor --meta-id`), `generated/config.tmt`, and the crate pages of every crate whose doc comments changed
(`tomet-config`, `tomet-load-config`, `tomet-load`, `tomet-semantics`, `tomet-declaration`, `tomet-validator*`,
`tomet-printer`, `tomet-pandoc`, `tomet-markdown`). `apps/cli/src/commands/spec/config.rs` has an `INTRO` that still says
`default.config.tmt` and `::config{ ... }`; `ConfigGroupSpec` and the derive group names speak of `::config.format`,
`::config.workspace`.

## 5. Doc comments that still name the old file

`default.config.tmt` / `tomet.config.tmt` appear in comments of: `apps/cli/src/commands/export.rs`,
`apps/cli/src/commands/spec/config.rs`, `bindings/java/.../Tomet.java`, `bindings/js/index.d.ts`, `bindings/js/src/lib.rs`,
`bindings/python/src/lib.rs`, `crates/tomet-convert-html/src/elements/mod.rs`, `crates/tomet-semantics/src/flatten.rs`,
`crates/tomet-syntax-parser/src/tests/fences.rs`, `crates/tomet-vault/src/lib.rs`, `crates/tomet-workspace-indexer/src/lib.rs`.
The name is `tomet_load_config::VAULT_CONFIG_FILE` / `FOLDER_CONFIG_FILE`; a comment should link it, not spell it.
(`tests/src/roundtrip.rs` and `tomet-load-config`'s own test spell the old name on purpose.)

## 6. Markdown with no `.tmt` source

`editors/helix/README.md` (still lists `tomet.config.tmt`, `default.config.tmt` as roots; the code and
`editors/helix/languages.toml` say `vault.config.tmt`) and `packages/astro/README.md` (default `configPath` is
`default.config.tmt`; the code says `vault.config.tmt`). Give them a `.tmt` source and an export, then regenerate.

## 7. The author's files (propose, do not edit)

- `.writ.tmt:153` and `:216` name `default.config.tmt`.
- `crates/tomet-syntax-parser/.writ.tmt:37-39` describe `::config(import:)` / `::settings(file:)`, which no longer exist.
- `tmtroot/dev/cotomet.next` (a draft) lists `default.config.tmt` with an upstream path in `kakusika/commonfiles`.

## 8. Open points to settle in the spec

- `aliases` (alternate names for `ref:` resolution) passes the criterion "tomet interprets it" but is still an ordinary
  `::fields` entry and `::meta` field; nothing reads it today. `::aliases(...)` or stays in `::meta`?
- `::migration` is a standard declaration only until `tandr` declares its own vocabulary; then `DeclKind::Migration` goes.
- `.config.tmt` next to `vault.config.tmt`: both apply, the folder one on top (chosen, not yet in a spec).

## Notes for the next migration

`tandr` is not installed in this environment (not in the toolchain, not on PATH), so the three migrations of 2026-10-30 were
made with throwaway scripts that are not kept, and no `tmtroot/migrations/0002-*.tandr` / `0003-*.tandr` records exist
(`0001-document-declarations.tandr` does). Write the records if `tandr` becomes available.
