# docs/ content gaps

Carried over from the `docs/` restructure (2026-09-01), which moved and
re-pointed files but deliberately did not edit any document's content.
These are the content decisions it left alone.

## Steps

- [ ] 1. Resolve the duplicate readme. `docs/readme.tmt` and
      `docs/readme.ja.tmt` are near-identical, share `id: doc-gCzNE6yl`,
      are both Japanese despite one being named as the English version,
      and both declare `@config(export: { path: "README.ja.md" })`. Pick
      which prose survives, drop the other, and decide whether the
      surviving file exports to `README.ja.md` or `README.md` now that
      Japanese is the canonical language.
- [ ] 2. Restore `docs/develop/grammar-freeze.md`. `architecture.md`
      references it as the grammar-freeze process doc but it does not
      exist. A 76-line version survives in the gitignored, pre-rename
      vendored copy at
      `editors/zed/grammars/typedmark/docs/develop/grammar-freeze.md`
      (still says `tomed-parser`/`.tmd`, so it needs a rename pass).
- [ ] 3. Fill the empty files:
      `docs/guide/builtins/primitives/{link,references}.tmt` and
      `docs/guide/builtins/value_dsl/{settings_dsl,value_dsl}.tmt` are
      0 bytes; `docs/guide/cli.tmt` is a single heading.
- [ ] 4. Wire `@config(export:)` into `docs/spec/*.tmt` and
      `docs/guide/**/*.tmt` so the `.md` readers are meant to see is
      actually generated. `.gitattributes` already marks those two paths
      `linguist-generated`, but nothing produces the files yet.
- [ ] 5. Decide what `docs/tmt/tomet.tmt` meant in
      `docs/design/decisions/2026-08-09-commonmark-support.md` (cited
      twice as a spec reference). The path was already dangling before
      the restructure and was left alone rather than guessed at; the
      tree-sitter crate used the same name for what is now
      `tests/fixtures/readme.tmt`, but the citation reads as a spec, not
      a fixture.
- [ ] 6. Consider promoting
      `docs/design/decisions/2026-08-09-commonmark-support.md` to a
      proper `docs/spec/` entry. Four crates cite it as the CommonMark
      mapping spec, which is a spec role, not a dated decision record.

## Pre-existing check failures (not caused by the restructure)

`just docs-check` reports 18 failures under `docs/`, all of which
predate this work -- verified by running the same check against `HEAD`
before the move and getting a 1:1 match. Mostly `FORMAT FAIL`; the
`PARSE FAIL`s are `docs/docs.settings.tmt` (already recorded as a known
gap in `tomet-semantics-resolver`'s module doc), `docs/spec/builtin-functions.tmt`,
and `docs/design/ideas/idea.tmt`. Worth a separate cleanup pass.
