# Shared files across the repositories: a template

Surveyed 2026-10-24 (`git ls-files`, byte-identical, > 200 bytes, lockfiles excluded).

## Byte-identical files

- `LICENSE-APACHE`: 12 copies (fs-catasto, immermemo, oneline, sisho-desk, sisho, tomet-corpus, tomet-shelf, tomet, twrit, vocabularies,
  and `tomet/editors/vscode`, `tomet/editors/zed`); `LICENSE-MIT`: 11.
- `.editorconfig`: 9 (441 bytes). `nix/formatter.nix`: 5 (fs-catasto, sisho-desk, sisho, tomet, twrit; 656 bytes).
- `.gitignore`: 3 identical (oneline, tandr, twrit) and 2 (sisho, sisho-desk); `.gitattributes`: 2.
- The `writ` vocabulary: tomet's and twrit's copies (and twrit's fixture) are the same 2278-byte Japanese text; fs-catasto and sisho hold
  `writ@1.0.0` (1171 bytes, English) from `vocabularies`. See `twrit/.agents/tasks/writ-vocabulary-source.md`.

## Shared text

`tmtroot/agents.tmt` of fs-catasto, sisho, sisho-desk, twrit, tomet-shelf and tomet all have the sections *Core Philosophy & Principles*
(4 subsections), *Single Source of Truth & Documentation*, *Environment & Tooling*, *Task tracking & Workspaces* and *Commits & Version
Control (jj)*; 18 paragraphs are word for word the same in three or more of them. They differ in small places (only tomet, twrit and
tomet-shelf have "If a session has to end early..."), so the first step of sharing is to pick the wording.

`${include(addr(tmt:path/to/core.tmt))}` puts another document's body where it is written and moves its headings one level below the
include site (checked in a scratch directory: `tomet check` and `tomet export --check` pass, `AGENTS.md` comes out whole).

## Proposal

1. A: the byte-identical files come from one home through `catasto add ... --upstream` (`LICENSE-*`, `.editorconfig`, `nix/formatter.nix`).
2. B: `agents-core.tmt` (the five shared sections) lives in the same home; each repository's `tmtroot/agents.tmt` includes it and keeps only
   its own sections. A rule changed once reaches every repository by `catasto sync --update --only`.
3. C (later): a blueprint that makes a new repository with all of the above (`tomet new`).

## Decisions needed (user)

- [ ] The home: a new repository (name?), `vocabularies` widened, or tomet. Recommendation: a new small repository, since the license text
  and the formatter should not depend on the language repository.
- [ ] The wording of the shared sections, where the repositories differ.
- [ ] Which `.gitignore` variants are really the same file.

## Steps (after the decisions)

- [ ] Create the home; put A and B in it.
- [ ] Adopt in each repository with `catasto add`; replace the repeated sections of `tmtroot/agents.tmt` by the include; `just docs-check`.
- [ ] `tomet/editors/vscode` and `editors/zed` copies of the licenses: `catasto add ... --local ../../LICENSE-APACHE` inside tomet.

## Status 2026-10-24

`tomet-lang/commonfiles` exists (its `base/` is the upstream root: `.editorconfig`, `.envrc`, `.gitattributes`, `LICENSE-*`, `nix/formatter.nix`,
`.github/workflows/nix-ci.yml` and `pages.yml`). The first rollout (pushed without being asked) was undone. Then it was done again one repository at a time,
each commit and push approved: fs-catasto, sisho-desk, sisho, twrit, tomet, vocabularies, jobserver-gate, tomet-corpus, oneline, tomet-shelf, tandr.
immermemo is prepared in `immermemo/.agents/workspaces/commonfiles` and not committed (see `immermemo/.agents/tasks/commonfiles-leftovers.md`; the user
thinks of a `kakusika/commonfiles` for repositories outside tomet-lang).

## Left

- [ ] immermemo: decide `kakusika/commonfiles` or taking the tomet-lang one after it joins the organisation; the prepared workspace is waiting.
- [ ] `catasto init` writes a 16-line comment block into every new manifest (`SKELETON` in `crates/catasto-core/src/manage.rs`). Nobody asked
  for it and it stays at the top after the first `add`. Shorten it to one line, or write nothing; the user decides.

- [ ] Not taken because they differ (each is either a real difference or drift to look at, one by one):
  - `nix/formatter.nix`: tomet-shelf (adds a `biome` formatter), oneline (28 lines, other comment style), tomet-corpus (10 lines: blank
    lines and trailing comments), immermemo (20 lines: takes `pkgs`, same comments).
  - immermemo: `LICENSE-MIT` (copyright line "immermemo contributors"), `.editorconfig` (adds a `[*.slint]` section).
  - Question for each: does commonfiles gain an option (a formatter module list), or does the repository keep its own?
- [ ] `agents-core.tmt` (the five shared sections of `tmtroot/agents.tmt`): not started; needs the wording decision above.
- [ ] `.gitignore` (3 identical, 2 others), `.gitattributes` (2): not examined.
- [ ] `sisho check` does not pass in tomet-shelf (8 of 22 documents), immermemo (12 of 25), oneline (3 of 5 steps), tomet-corpus (8 of 9) and
  `twrit check` has 47 violations in tomet. All of it was so before the common files were taken (checked for tomet: 47 on main, 47
  after); the documents there still use `@meta` / `@config` heads that tomet 0.5 does not read as declarations.
- [ ] The default workspaces of all of these repositories are behind `main` now (the work was done in workspaces off `main`).

## `.gitattributes` and `.envrc` (examined 2026-10-24)

`.envrc`: 71 bytes in fs-catasto, immermemo, oneline, tomet-corpus, tomet-shelf, tomet, twrit; 53 bytes in sisho and sisho-desk (no final
`PATH_add scripts`, since they have no `scripts/`). `PATH_add` of a missing directory is harmless, so the 71-byte file could be the one file.

`.gitattributes`: eight repositories, five blocks.
- Core in all eight: LF normalisation; `tmtroot/ linguist-documentation`, `*.md` Markdown, `*.tmt` Tomet.
- Git LFS block (8 patterns): in six; sisho and sisho-desk lack it.
- `docs/ linguist-documentation`: immermemo, tomet-corpus, tomet.
- `.md` under `tmtroot/spec/**` and `tmtroot/guide/**` as generated: fs-catasto, twrit (with a comment).
- Paths of one repository: tomet (`docs/spec`, `docs/guide`, `/AGENTS.md`, `/README.ja.md`, `tests/fixtures/`, the three generated files of
  `crates/tree-sitter-tomet`), twrit (`tests/fixtures/`).
- Identical pairs: immermemo = tomet-corpus; sisho = sisho-desk.
- A pattern for a path that does not exist matches nothing, so one superset file would be harmless everywhere.

- [ ] Found: tomet's `.gitattributes` marks the generated files of `crates/tree-sitter-tomet` but not those of `crates/tree-sitter-cotomet`
  (`src/parser.c`, `grammar.json`, `node-types.json`), added by the cotomet rename. A regeneration shows up as tens of thousands of lines in review.
- [ ] Decide: one superset `.gitattributes` and `.envrc` in commonfiles, taken by all (the user's view: they should be the same).

## Decided by the user (2026-10-24)

- `docs/ linguist-documentation=true` is in every repository's `.gitattributes`; no `.md` rules for `tmtroot/` (only a deprecated `cli.md` in
  tomet lives there); `/AGENTS.md`, `/README.md`, `/CONTRIBUTING.md`, `/CLAUDE.md` are marked generated; `/README.ja.md` is not.
- **tomet is the exception: it does not take its `.gitattributes` from commonfiles.** Its tree-sitter lines stay in tomet's own file.
- [ ] tomet's own `.gitattributes` still lacks the generated files of `crates/tree-sitter-cotomet` (`src/parser.c`, `grammar.json`,
  `node-types.json`); add them there (tomet's file, the user commits).
- [ ] `tests/fixtures/ linguist-vendored=true` is in the draft commonfiles `.gitattributes`; with tomet out, only twrit has such a line.
  Keep it in the shared file, or keep it in twrit's own?

## Workflows (decided 2026-10-24, pushed in commonfiles `9dafeb38`)

- `.github/workflows/nix-ci.yml` (shared; builds `nix build .#ci`) and `.github/workflows/pages.yml` (shared; for a repository that publishes).
  `ci.yml` is not shared: each repository's own holds what only it checks (tomet: cargo, clippy, tree-sitter freshness, docs checks).
- The contract for `nix-ci.yml`: the flake provides `packages.ci = pkgs.linkFarm "ci" [ ... ]` of the packages that go to Cachix (no dev shell in it).
  Tried on a throwaway sisho-desk workspace: the closure holds the package and no dev shell; a flake without it fails with a clear message.
  Chosen over a `just nix-packages` recipe (considered, tried on paper) because the list belongs where the packages are defined.
- Which repository takes which: nobody yet. Repositories with no CI today (fs-catasto, vocabularies, tandr, jobserver-gate, tomet-corpus,
  immermemo, oneline) do not get one for this.
- Rollout is one repository at a time, each commit and push approved by the user. Order started with fs-catasto.
