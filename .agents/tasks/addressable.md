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

- ~~The standard unique property is `_id` (DECIDED; `_`-prefix is tomet-reserved).~~
  **Superseded 2026-10-02**: the unique property is a dedicated AST field
  (`Element.id`/`Section.id`, `tomet-syntax-ast`), spelled `id` with no
  underscore, fed by new syntax `#(foobar)` -- not an attribute key at
  all, so the underscore-reservation convention (which existed to avoid
  colliding with user data keys in the attribute namespace) is moot:
  `#(...)` isn't in that namespace. The old `{id: x}`/`(id: x)`
  *attribute* spelling is fully retired as an id-establishing mechanism.
  Confirmed with the user as the official spelling, explicitly choosing
  it over renaming to match this entry's original `_id`/`#id(x)`.
- ~~Step name is `#id(x)` (DECIDED)~~ **Superseded 2026-10-02**: the
  shipped spelling is bare `#(x)`, no `id(...)` keyword -- see above.
  Step vocabulary is `id`/`item`/`prop` (DECIDED, add later if needed).
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
**The spec file itself was updated to match the 2026-10-02 supersession (`#(x)`).**

Implementation (the last "Open" item below) landed 2026-10-02: `Element.id`/
`Section.id: Option<Id>` in both parser pipelines, the validator, printer, every
converter, and `tree-sitter-tomet`'s grammar; `tests/fixtures/syntax/id.tmt`-style
coverage lives in `crates/tomet-syntax-parser/src/tests/id.rs` and new
`tests/src/syntax_report.rs` CASES instead of a dedicated fixture file. ~947
workspace tests green. Still only covers the unique-property/addressability half
of this file's original scope -- the reference-writing question and the
dangling-reference/rename-update checker below are untouched.

## Open

(Template expansion with no `_id`: DECIDED - unreachable by unique address; only the user naming it makes it reachable. Range inside strings: dropped.)

- [x] `.settings` spelling: `meta: { isbn: { type: string, unique: true } }` in `default.config.tmt` (DECIDED)
- [ ] how references are *written* in text: `@link(ref:)` vs `^...` vs `[[ ]]`
- [ ] dangling-reference check in `tomet check`; renaming an `_id` with reference update
- [ ] vault-wide `@meta` `_id`: duplicates across the vault, cross-vault refs
- [ ] step 5: does the parser/AST carry parent+sibling info to compute addresses
- [x] implementation + fixtures (done 2026-10-02 as `id`/`#(x)`, not `_id`/`#id(x)` --
      see the supersession note above; `cargo test -p tomet-tests -p tree-sitter-tomet`
      passes)
