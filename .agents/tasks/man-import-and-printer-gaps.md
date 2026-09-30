# man import via pandoc, and the printer/escape gaps it exposed

State (2026-09-26): `tomet cli-docs` is done and committed (9b4a905; see the `//!` of
`apps/cli/src/commands/cli_docs.rs`). This file is what is LEFT. Nothing below is started.
Do not start the items marked BLOCKED: they wait on the author.

## Direction (decided)

Importing a man page = `pandoc -f man -t json page.1 | tomet from-pandoc -`. No native
roff crate (`convert-roff`) unless pandoc becomes unavailable, mdoc pages are needed, or
quality is too low. `from-pandoc` is the place to improve, so every pandoc source benefits.

Check it works: `pandoc -f man -t json "$(man -w ls)" ...` (the page may be `.gz`; `zcat` first),
then `tomet check` on the result.

## Findings (from `ls.1`; `tomet check` on the output fails with these)

1. `tomet-printer` prints a section's blocks on adjacent lines. Blank lines only go between
   TOP-LEVEL blocks (`document_to_tm_with_config`, `render_section` in
   `crates/tomet-format-printer/src/lib.rs`). Two paragraphs in one section read back as one
   (probe: 4 paragraphs -> 2), and a following `@links{`/`@strong[..]` line becomes inline.
   `test_section_blocks_blank_lines` pins the adjacent output, so it may be deliberate -- ask.
   Experiment (reverted): emitting `\n` between a section's children breaks that unit test and
   the `cheatsheet.printed` / `cheatsheet.pandoc.roundtrip` snapshots (refs need regenerating).
   Workaround in use: `cli_docs.rs` lays sections out flat.
2. Inline `Text` has no escape for `@name(..)`, `${..}`, `^n(..)` and the printer writes `Text`
   verbatim, so imported prose such as `@meta(x)` comes back as an element; `-lt: sort` style text
   read as an unknown connect `:sort`. Language-level gap. `cli_docs.rs::guard_triggers` wraps
   such words in code spans as a local workaround.
3. `from-pandoc` maps `DefinitionList` to `@links{ ("term")[def] }` (`from_pandoc.rs`, the
   `Block::DefinitionList` arm). `links` is not a std element and means "links" (HTML renders it as
   a list of anchors), so it is the wrong meaning and fails `tomet check`.

## BLOCKED on the author (do not decide or implement unprompted)

- Definition-list element: the author is designing a more general model that may subsume it. No `dl`.
  Leave the `DefinitionList` mapping alone until told.
- Whether to fix finding 1 in the printer (recommended by me, changes snapshot refs and one unit test).
- Finding 2 (escaping): a language decision.
- Root `.writ.tmt` `tmtroot-mirrors-the-root` says tmtroot mirrors `/` and "nothing else goes
  there"; `tmtroot/generated/` (holds `cli.tmt`) needs a sentence. Writs are the author's: propose, stop.
- `docs/README.tmt` map should list `docs/reference/` (generated English CLI reference). Proposal only.

## Possible next steps once unblocked

- [ ] Fix printer section separation (finding 1); regenerate refs with `TOMET_UPDATE_REF=1
      cargo test -p tomet-tests`; then drop the flat-layout workaround in `cli_docs.rs`.
- [ ] Re-run the `ls.1` pipeline, get `tomet check` clean, add a fixture-style test that does not
      need pandoc installed (feed a small pandoc JSON to `from_pandoc`).
- [ ] Decide `tomet from-man` entry vs. leaving the two-stage pipe (thin wrapper may not be worth it).
- [ ] Later: `clap_mangen` from `tomet` itself through the same path would make `cli-docs` redundant.
      Decide after using the pandoc route for a while.

When all of this is resolved: fold the decisions into doc comments / writs and delete this file.
