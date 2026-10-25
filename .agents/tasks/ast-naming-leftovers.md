# Names the sigil removal left behind

Raised after the `ast-element-split` task (2026-11-01). The author wants it done. Delete this file when done.
Destination of what is worth keeping: the spec `.tmt` files named below (a bulk docs pass, not now: the author's rule is
"renames: code only, docs later").

## Facts and decisions

- [x] The expression-language member `.sigil` is now `.name` (code + tests, 2026-11-01, in `ast-element-split`).
- [ ] Docs still say `.sigil`: `tmtroot/spec/expressions/address.expr.tmt` (lines ~75, ~82) and
      `tmtroot/spec/syntax/addressable.tmt` (line ~58). Update in the next docs pass; `tomet export` after.
- [ ] `tomet_validator_core::PLAIN_ITEM_KIND_NAME = "bare"`: what a `:rule(allow: list(..))` calls a plain `- item`.
      It collides with a user's `@bare`. No document or fixture uses it. Rename (`item`?) or forbid the collision.
- [ ] Pandoc: the attribute key `tomet-sigil` (value `bare`) in `tomet-convert-pandoc` (`SIGIL_KEY`, `BARE_SIGIL`) marks a
      nameless group entry. "sigil" is no longer a concept in the AST; a name like `tomet-entry` fits. Changing it changes
      the Pandoc JSON (`tests/ref/syntax/sigils.pandoc.json`) and breaks documents already converted.
- [ ] Left alone: `packages/react`, `packages/svelte` (`types.ts`, `utils.ts`) read `sigil: Sigil` with `Type` / `At` /
      `Dollar` variants that were removed long before this; `packages/astro/README.md` line ~64 quotes the old
      `index.d.ts`. Decide whether these packages are still alive before touching them.
- [ ] `Diagnostic::WrongSigil` and other uses of "sigil" for the *syntax* concept (`@` / `::`) stay: that is the right word.

## Steps

- [ ] Ask the author about the `bare` kind name and the `tomet-sigil` key.
- [ ] Docs pass for `.sigil` -> `.name`.
- [ ] Decide on `packages/*`.
