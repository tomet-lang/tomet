# The dev shell still has the old twrit: the repository's own rules are not checked

> Cross-repository coordination: `bearings/initiatives/follow-declarations.tmt` (the table and the order live there; the work stays here).

`just docs-check` here prints `tomet 0.5.0, twrit 0.2.0`. `twrit 0.2.0` reads **0 rules** from a `.writ.tmt` whose vocabulary is
in the `::elements` map form (it cannot resolve `@rule`), says `ok`, and exits 0. Measured on 2026-10-24 in this repository's dev shell:
`twrit check .` prints `3 writs, 0 rules,`. So `crate-layering` and every other guard in this repository's `.writ.tmt` files are off.
`twrit 0.4.0` (`571665d5`) reads them (in twrit's own repository: `2 rules`; in sisho: `5 rules`, all ok).

The dev shell gets its tools from `sisho`'s flake (`flake.lock` input `sisho`), which pins `twrit` and `catasto`.

## Steps

- [ ] In a workspace: `nix flake update sisho`. Enter `nix develop`; `twrit --version` must say 0.4.0 or newer, and
      `twrit check .` must print more than `0 rules`.
- [ ] Whatever the rules say now is new information, not a regression: read each violation. Layers, placement, mentions and the
      Rust API documentation (`twrit api`) are the guards that can have drifted unnoticed. Fix the drift, or propose an edit of the
      `.writ.tmt` (only the author edits it).
- [ ] Generated API pages under `tmtroot/generated/` are rebuilt by `twrit check --fix` / `just docs`. `twrit 0.4.0` writes the
      `::kind` form; if a page comes back with `@kind` / `@meta`, the twrit in use is too old.
- [ ] `just docs-check` passes; commit `chore(nix): take sisho main` (flake.lock) apart from the fixes.

Related: `vocabulary-map-form-followups.md` (an old-form vocabulary is ignored without a word, which is how this stayed unseen).
