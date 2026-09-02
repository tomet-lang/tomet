# Tomet documentation

A map of what lives where. If you are looking for something and this page
does not point at it, the page is wrong -- fix it here rather than adding
a second index somewhere else.

| Directory | Contents | Language | Source format |
| --- | --- | --- | --- |
| [`spec/`](spec/) | Language specification: grammar, types, inference, built-in elements/functions/settings | Japanese | `.tmt` (`.md` generated) |
| [`guide/`](guide/) | How to use Tomet: cheatsheet, CLI, config, features, built-ins | Japanese | `.tmt` (`.md` generated) |
| [`examples/`](examples/) | Runnable `.tmt` sample documents and templates | Japanese | `.tmt` |
| [`develop/`](develop/) | For contributors: architecture, docs conventions | English | `.md` |
| [`design/decisions/`](design/decisions/) | Dated design decision records. Historical -- not revised after the fact | English | `.md` |
| [`design/ideas/`](design/ideas/) | Unresolved sketches and notes | mixed | `.tmt` |

Two things live outside `docs/`:

- `/tests` -- the `tomet-tests` package: the frozen `.tmt` corpus, the
  cross-crate invariants, and the end-to-end conversion snapshots. Its
  fixtures started as copies of documents in here, but they are **not**
  kept in sync; see `tests/README.md`.
- [`/tests/SYNTAX.md`](../tests/SYNTAX.md) -- **every construct the parser
  currently accepts**, with an example and the AST it produces. Generated
  from `tomet-parser` itself and checked by `cargo test`, so unlike
  `spec/` it cannot drift. `spec/` is normative (what the language
  *should* accept) and hand-written; that file is descriptive (what the
  implementation *does*). Read them side by side -- where they disagree,
  the disagreement is the finding.
- `/default.config.tmt` -- the real config this repository runs under.
  `docs/examples/default.config.tmt` is a *sample* showing the
  `@version`/`@kind` header style, and is not the same file.

## Start here

- New to the language: [`guide/cheatsheet.tmt`](guide/cheatsheet.tmt)
- Looking up exact syntax: [`spec/syntax.tmt`](spec/syntax.tmt)
- Checking what the parser *actually* accepts today:
  [`/tests/SYNTAX.md`](../tests/SYNTAX.md)
- Working on the Rust crates: [`develop/architecture.md`](develop/architecture.md)
- Wondering why a syntax decision went the way it did:
  [`design/decisions/`](design/decisions/), sorted by date
- Writing or moving a doc: [`develop/docs-guide.md`](develop/docs-guide.md)

## Checking documents

`.tmt` files under `docs/` are real Tomet documents, so they double as
dogfooding. To confirm they all still parse and are formatted:

```bash
just docs-check
```

This is deliberately not part of `cargo test` -- these documents evolve,
and editing prose should not fail an unrelated code change.
