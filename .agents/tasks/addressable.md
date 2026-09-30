# Addressable: design the address system

Source sketch: `tmtroot/docs/spec/syntax/addressable.tmt`.
Destination when done: fold the decisions into that spec file (Japanese,
user-facing), delete this file.

Existing conventions to stay consistent with:
- `scheme:address` links: `file:`, `dir:`, `tm:path#section`, `ref:target`
  (`docs/spec/builtin-functions.tmt`, `docs/spec/builtin-elements.tmt`)
- call-chain in `${}`: `ref(id(asdf).contents(default))` (parses today, no semantics)
- `^` sigil is reserved for `^footnote(1)` in `tmtroot/docs/spec/sigils.tmt`
- `:` is a sigil (`tmtroot/dev/roadmap/colon-as-sigil.tmt`), avoid new uses

## Decisions added later (2026-09-25)

- The standard unique property is `_id` (DECIDED; `_`-prefix is tomet-reserved). No settings
  needed to reference. Step name is `#id(x)` (DECIDED), step vocabulary is `id`/`item`/`prop` (DECIDED, add later if needed).
  Values inside lists/tables: `prop(tags).item(2)`, `prop(cells).item(1).item(2)` (`item` reused; DECIDED).
- User-defined unique properties ARE addressable, same mechanism as `id`.
  Unique = the same value is never declared twice. Scope depends on where it is
  declared: inside `@meta` = across documents, any other element = within the document.
  User-defined unique properties are declared in `.settings` (DECIDED; only the key spelling is missing).
- No docs existed for "unique property" before this; the spec now defines it.
- Do NOT justify design by "the code already does this" (user feedback).
- Written into `tmtroot/docs/spec/syntax/addressable.tmt`.

## Status

Spec (`tmtroot/docs/spec/syntax/addressable.tmt`) now holds: address shape, steps,
unique property + scope, templates, reference stability. Steps 1-4 of the original
plan are DONE and live in the spec (that file is the source of truth, not this one).

## Open

(Template expansion with no `_id`: DECIDED - unreachable by unique address; only the user naming it makes it reachable. Range inside strings: dropped.)

- [x] `.settings` spelling: `meta: { isbn: { type: string, unique: true } }` in `default.config.tmt` (DECIDED)
- [ ] how references are *written* in text: `@link(ref:)` vs `^...` vs `[[ ]]`
- [ ] dangling-reference check in `tomet check`; renaming an `_id` with reference update
- [ ] vault-wide `@meta` `_id`: duplicates across the vault, cross-vault refs
- [ ] step 5: does the parser/AST carry parent+sibling info to compute addresses
- [ ] implementation + fixtures under `tests/fixtures/` (grammar change also needs
      `cargo test -p tomet-tests -p tree-sitter-tomet`)
