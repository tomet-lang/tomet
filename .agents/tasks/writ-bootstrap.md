# Writ bootstrap

Stand up the first `.writ.tmt` files and the two guards behind them.

Background: `docs/design/ideas/documentation-layers.tmt` is the design
sketch. A *writ* is an author-owned normative file, one per directory,
named `.writ.tmt` (nameless form of the `<name>.<kind>.tmt` convention,
like the existing root `.arch.tmt`), declaring `@kind(writ)`. Scope is the
containing directory; a reader walks from a file up to the workspace root
collecting every `.writ.tmt`, the way `.gitignore` is read. Entries carry
**id / rule / why / guard** and no status field -- a withdrawn rule is
deleted and git holds the history.

`twrit` (in the sibling `../tomet-writ` repo) will eventually read these.
Until then `just docs-check` sweeps them for parse/format only.

## Steps

- [x] 1. Write `crates/tomet-syntax-parser/.writ.tmt` with the
      `no-vocabulary` entry, guarded by `tests/src/vocabulary.rs`.
- [x] 2. Extend `just docs-check` to sweep every `.writ.tmt` in the tree
      (`find`, which does pick up dotfiles) alongside `docs/**/*.tmt`.
- [x] 3. Fix `FileDiff::is_changed` (`crates/tomet-workspace/src/diff.rs:31`).
      It compares `original_src != modified_src`, but `refactor_source`
      re-prints and re-formats every document, so any file not already in
      canonical printer form counts as "needs refactor" with
      `changes_count == 0`. `tomet refactor --check` is therefore red on
      almost everything and unusable as a guard. Make it `changes_count > 0`.
      This also stops `refactor -i` from rewriting files no rule touched --
      reformatting is `tomet format`'s job.
      Note `batch_meta.rs:43` has a same-named method on a different struct;
      leave it alone. Callers: `apps/cli/src/commands/refactor.rs:22`,
      `crates/tomet-workspace/src/lib.rs:73` (test).
- [x] 4. Migrate the 11 files under `docs/` still carrying
      `@meta{ type: ... }` to `@kind(...)`, via
      `tomet refactor --meta-kind -i`. Verify with `just docs-check`.
      Done by hand, NOT with `refactor -i`: that pass re-prints the whole
      document, which collapses every raw `+++` block onto one line and
      drops comments. `--check` is still sound as a detector (it counts
      rule hits, not text diff); only `-i` is unsafe. Recorded below.
      Files (from `tomet refactor --check --meta-kind docs`):
      `docs/docs.settings.tmt`, `docs/roadmap.tmt`,
      `docs/spec/builtin-settings.tmt`, `docs/guide/features/template.tmt`,
      `docs/examples/{bookmark,dirs,image.meta,node_graph,roadmap.todo,
      scenario,video.memo}.tmt`.
      Scope later widened to the whole repository on the author's call.
      `docs/examples/bookmark.tmt` carried four copies of the declaration;
      the first became `@kind(bookmark)`, the other three were dropped,
      since a kind is declared once per document.
- [x] 5. Write the root `.writ.tmt` with two entries:
      - `crate-layering` -- dependency direction between crate layers.
        Written from the real `[dependencies]` edges, not from intent.
        Two corrections came out of reading them: the layer derives from
        the *directory* name, not the package name (`crates/tomet-syntax-
        parser` publishes `tomet-parser`), and `crates/tomet-workspace-
        config` is not in the workspace layer at all -- the format and
        convert crates sit on top of it, so it is its own layer below
        them, and the name is the thing that is wrong. Dev-dependencies
        are exempt; `tomet-syntax-tree` and `tomet-semantics` both reach
        up to the parser for their own tests.
        Guard: none yet -- this is `twrit`'s first job.
      - `kind-not-meta-type` -- a document declares its kind with
        `@kind(...)`; `@meta`'s `type:` field is the superseded spelling.
        Guard: `tomet refactor --check --meta-kind`.
- [x] 6. Verify: `just docs-check`, `cargo test -p tomet-tests`,
      `cargo build`.

- [x] 7. Widen the migration to the whole repository. 14 source files by
      hand (`default.config.tmt` at the root, `tests/fixtures/**`,
      `crates/tomet-semantics-resolver/tests/fixtures/valid_settings.tmt`),
      then `TOMET_UPDATE_REF=1 cargo test -p tomet-tests` to regenerate the
      23 files under `tests/ref/`. `tomet refactor --check --meta-kind .`
      now exits 0 repo-wide, so the writ's wording ("not in documentation,
      not in examples, not in fixtures") is true as written.
- [x] 8. Document `@kind` (author's call: spec only, no enforcement yet).
      - `docs/spec/builtin-settings.tmt`: `kind` and `version` entries added
        to the `elements` map, both marked as having unenforced
        `placement`/`singleton`; `type` removed from `meta`'s `values`.
      - `docs/spec/builtin-elements.tmt`: `@kind`/`@version` added to the
        vocabulary listing, which had neither.
      - `docs/guide/builtins/primitives/kind.tmt`: new page.
      - `docs/guide/builtins/primitives/meta.tmt`: `type:` dropped from the
        example -- it was the last place teaching the old spelling.
      `@version` is not a per-file constraint: it is written once in the
      workspace-root config and applies to everything. Do not write a rule
      about it.

- [x] 9. Fold `docs/develop/docs-guide.md` into `docs/.writ.tmt` and delete
      it. It called itself "the rulebook", which is what a writ is; it just
      predated the word. Of its six sections, three became entries
      (`document-placement`, `docs-language`, `generated-md-not-edited`) and
      three were dropped:
      - "Records are not revised" governed `design/decisions/`, deleted in
        `4055fe5`. The placement table's replacement says plainly that there
        is no home for a dated record and that git is that record.
      - "Checking" repeated `just docs-check` and its rationale, already
        present in `docs/README.tmt` and in the `justfile` recipe comment --
        three copies of the same paragraph. The justfile's is the one next
        to the thing that runs, so it stays.
      - The `design/decisions/` row of the placement table.
      Corrected while moving: docs-guide claimed all of `docs/design/` is
      English, but every file in `design/ideas/` is Japanese and
      `docs/README.tmt` already said "mixed". The writ says sketches are in
      whichever language the thinking happened in.
      Pointers updated: `AGENTS.md` (two places, plus a new paragraph
      explaining what a `.writ.tmt` is), `docs/README.tmt` (rulebook link,
      and its two dead `design/decisions/` entries).
      `docs/develop/` now holds `architecture.md` alone.

- [x] 10. Delete `docs/develop/architecture.md` (354 lines). Seven of its
      claims had gone false: `docs/develop/grammar-freeze.md` (the process
      it routes breaking grammar changes through) does not exist; three
      "depends only on X" lists were wrong (`tomet-semantics`,
      `tomet-markdown`, `tomet-transform`); the CLI section listed 8 of 16
      subcommands; `packages/svelte` was missing; three references pointed
      into the deleted `design/decisions/`.
      Almost nothing needed folding in: the crates' own `//!` docs already
      carried the cross-crate rationale, in more detail and without the
      staleness (`tomet-format-style`'s cycle-avoidance reason,
      `tomet-workspace-config`'s split reason, `tomet-format-field-utils`'s
      "not a rules engine"). `tests/README.md` covered the Tests section;
      `crates/tree-sitter-tomet`'s 137-line `//!` covered the two-grammars
      section.
      Two things were only there:
      - The "Deterministic Static Parser Boundary", cited by two doc
        comments -> now `parser-purity` in
        `crates/tomet-syntax-parser/.writ.tmt`.
      - The Nix packaging paragraph -> root `README.md`, along with a
        short "finding your way around" list pointing at `//!`,
        `Cargo.toml`, `tomet --help` and `.writ.tmt`.
      Back-references fixed in `README.md`, `AGENTS.md`,
      `docs/README.tmt`, `crates/README.dirs.tmt`,
      `docs/spec/builtin-elements.tmt`,
      `crates/tomet-semantics/src/lib.rs`,
      `crates/tomet-semantics/src/positional.rs` and
      `crates/tomet-semantics-resolver/src/lib.rs`. The one in
      `docs/design/ideas/documentation-layers.tmt` is left alone: it is a
      sketch recording the diagnosis, and it was true when written.
      `docs/develop/` is now empty and gone, so `docs/.writ.tmt` dropped
      its placement row and its language guard now covers `.writ.tmt`
      only. The writ says plainly that workspace prose has no home under
      `docs/`.

## Known gaps, recorded but not fixed here

- `singleton` and `placement` are declared in `docs/spec/builtin-settings.tmt`
  for several elements but implemented nowhere -- the word appears in two doc
  comments and no code. `tomet check` only parses
  (`apps/cli/src/commands/check.rs:10`), so a document with two `@meta`, or
  two `@kind`, or a `@kind` in the middle of the body, passes. Enforcing this
  is its own task and touches every element that claims to be a singleton.
- Dangling prose references to `docs/design/decisions/`, deleted in
  `4055fe5`. 18 remain across 12 files, and the earlier count was too low
  because it only swept `docs/`: 10 of them are in Rust source
  (`tomet-convert-markdown` 4, `tomet-convert-html`, `tomet-semantics`,
  `tomet-syntax-parser`, `tomet-workspace-links`, `tree-sitter-tomet` 2).
  The rest are in `docs/spec/` (3), `docs/guide/cheatsheet.tmt` and
  `docs/design/ideas/idea.tmt` (2). The two in
  `docs/design/ideas/documentation-layers.tmt` are that sketch's own record
  of the deletion and should stay. These are prose mentions, not
  `@link(file:...)`, so the link checker cannot see them. Fixing them is a
  judgement call per site -- delete the sentence, or repoint it at whatever
  now carries the reasoning.
- `docs/README.tmt` still describes `spec/` and `guide/` as "`.tmt` (`.md`
  is generated)", and `.gitattributes` has `linguist-generated` rules for
  `docs/spec/**/*.md` and `docs/guide/**/*.md`. No such `.md` exists and no
  `.tmt` there declares `@config(export:)`. Recorded in the writ's
  `generated-md-not-edited` entry as governing zero files today.
- `tomet refactor --check` silently skips files that fail to parse (warns
  and moves on): `docs/design/ideas/idea.tmt:156`,
  `docs/spec/builtin-functions.tmt:8`. A guard that skips broken files has
  a hole exactly where it matters most.
- `tests/src/vocabulary.rs` has no case asserting the guard still detects a
  violation; gut the comparison and it stays green. Recorded in the writ
  entry itself.
- `tomet check-links` skips hidden files on directory walks, so the guard
  links inside `.writ.tmt` are not link-checked today. Deliberate for now:
  a writ is not part of the document corpus, and `twrit` is the tool that
  should read it. Revisit only if a general `--hidden` flag is wanted.
- Single-file `tomet check-links <file>` reports false positives (existence
  is judged against the set of scanned files, so `docs/README.tmt` alone
  reports all 15 of its links broken). Directory mode is the only usable
  form.
- `docs/README.tmt`'s own links do not resolve: bare `file:fact/` resolves
  against the project root (`crates/tomet-workspace-links/src/check.rs:94`),
  so it looks for `./fact/`. Pre-existing, unrelated to this task.
- `@kind` appears nowhere in `docs/spec/`, and
  `docs/guide/builtins/primitives/` has no `kind.tmt` while `meta.tmt`
  still shows `type:` as the canonical example. The spelling was decided
  and tooled but never written down normatively -- which is the case study
  that motivated this whole task.
