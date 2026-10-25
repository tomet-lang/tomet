# Where the settings of the document id live

`::id(x)` is a declaration (`05991625`), but the settings that govern the id are still written as if it were a `::meta`
field: `::fields{ id: { type: nanoid, length: 8, prefix: "doc-", force: true, overwrite: true } }`. Decide where they go.
Delete this file when done.
Destination of what is worth keeping: the doc comments of `tomet-printer` (`ensure_document_id_with_config`) and
`tomet-validator-core` (`check_field_types`), `tmtroot/spec/declarations/std.id.declaration.tmt` (see `docs-config-rework`).
Related: `docs-config-rework`.

## What reads it today (code, 2026-10-30)

- `tomet_printer::ensure_document_id_with_config` (`crates/tomet-format-printer/src/id.rs`) reads
  `config.meta.fields.get("id")`: `type` (nothing is generated without one), `force` (default true: add an `::id` when
  there is none), `overwrite` (default false: replace an id whose format is wrong), `length`, `prefix`.
  Its only caller is `tomet convert` (`apps/cli/src/commands/convert.rs`); `tomet format` does not generate ids.
- `tomet_validator::check_field_types` (`crates/tomet-validator-core/src/lib.rs`) holds `::id(x)` to the `type` declared for
  the field `id` (`uuid` is checked; `nanoid` has no text form). `Vault::diagnostics` hands it the types from the same
  `fields` map.
- `tomet-field-utils` (`generate_id_for_field`, `is_valid_id_format`) takes the `FieldConfig`.
- `tomet new`'s `::id(${generate.uuid()})` in a blueprint is separate: it does not read these settings.
- Where it is written: `tests/fixtures/default.config.tmt` and `tests/fixtures/test.config.tmt` (frozen corpus; their
  `tests/ref` outputs), and the printer tests. The repository's own `vault.config.tmt` writes none.

## The problem

`::fields` is the schema of `::meta`. The id is not a `::meta` field any more, so `fields.id` mixes two things that are not a
field's schema:

1. what an id may look like (`type`, and with it `length` / `prefix`): validation, and the `type: uuid` check;
2. how a missing id is made and when it is replaced (`force`, `overwrite`): generation, a policy of the tool, not of the value.

## Options (not decided)

- Keep `fields.id` as the schema of the value (a `::fields` entry whose subject is the `::id` declaration) and move only the
  generation policy (`force`, `overwrite`, perhaps `length` / `prefix`) elsewhere.
- Give the id its own vault declaration for both (for example a table of settings for `::id`), read by the printer and the
  validator, and drop `fields.id`.
- Make the generation parameters arguments of the `::id` declaration written in the vault config. (`::id` already means the
  identity of the document that writes it; reusing it for settings would blur that.)

Whatever is chosen: the two frozen fixtures and their `tests/ref` outputs have to be migrated on purpose, and the printer tests
(`test_nanoid_*`) follow.

## Related, same family

- The per-field format hints that sit in a `::fields` entry (`always_newline`) and `::format{ meta: { always_newline } }`: move
  both to `::format`, keyed by field.
- Nothing reads `::id` to resolve anything: `ref:` resolves by file stem only, `aliases` is read by nothing, and the claim that
  an id is unique across the documents of a vault is not checked. Decide whether to enforce uniqueness
  (`DuplicateId` across documents, in `Vault`/`tomet check`) and whether `ref:` should resolve by id.
