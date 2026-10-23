# tove -> motem

`tove` (Tomet Object & Value Expression) is renamed `motem`: the data language
dual to `tomet` (`tmt` / `mtm`, see the name memo in `tmtroot/spec/name.tmt`
once it lands). Standalone motem files get the extension `.mtm`.

Why now: `fs-catasto` will read its manifest (`catasto.mtm`) and lockfile with
this parser, pinned by git rev. Renaming before it depends on the crate keeps
the rename out of catasto's history.

Workspace: `.agents/workspaces/tove-to-motem` (based on `main`; the default
workspace holds the author's uncommitted work, which this task must not touch).

## Decisions

- Crate and package name: `motem`, directory `crates/motem`, version kept at 0.2.0.
- The acronym TOVE is dropped. The description reads "the data language dual to tomet".
- `.writ.tmt` is the author's file: this task proposes its edit, it does not make it.
- Historical notes are left alone: `tmtroot/dev/plans/roadmap.tmt` (being
  moved by the author's uncommitted work) and `tmtroot/deprecated/`.

## Phase 1: rename (this task)

- [x] Move `crates/tove` to `crates/motem`; rename the package and the `//!` docs.
- [x] Update `Cargo.toml` (workspace member, workspace dependency), `apps/cli`, `tomet-syntax-parser`.
- [x] Update `tove::` paths in `tomet-syntax-parser` (`error.rs`, `value.rs`) and `apps/cli` (`roundtrip.rs`, `cli.rs`).
- [x] Update prose: `bindings/python/src/lib.rs`, `tomet-semantics/src/data.rs`, `tmtroot/readme.tmt`, `tmtroot/spec/cli/roundtrip.tmt`.
- [x] `Cargo.lock` follows the rename.
- [x] Regenerate `README.md` and `tmtroot/generated/` from their sources; the stale `generated/crates/tove.tmt` goes away.
- [ ] Propose the `.writ.tmt` edit (lines naming `crates/tove`) and wait for the author.
- [ ] `just test`, then `just docs-check` once `.writ.tmt` is updated.

## Follow-ups (separate tasks)

- Quoted string keys in maps (`"a/b.tmt": value`): `parse_one_entry_with`
  accepts identifiers only; the printer must quote keys that are not identifiers.
- `.mtm` as a standalone file: top-level map body without braces, loader,
  `tomet roundtrip`, LSP, editors, tree-sitter.
- `tandr` (`Cargo.toml`, `tandr-core`) names `tove` directly; follow once tomet
  lands. Other repos only carry it in `Cargo.lock` and pick it up on a rev bump.
- `tmtroot/shelf.toml` is still TOML; same question as catasto's manifest.
- `fs-catasto`: read `catasto.mtm` / `catasto.lock` in motem, keyed by destination path.

## Where the knowledge goes

The `motem` crate's `//!` doc says what the language is and how it relates to
tomet. Delete this file when Phase 1 is merged.

## Progress notes

- Verified: `cargo test --workspace --exclude tomet-python` passes (105 binaries, 1240 tests). `tomet-python` cannot build in the dev shell (no Python 3 there); its only change is one doc-comment line.
- `just docs` ran: README.md and `tmtroot/generated/` regenerated, stale `generated/crates/tove.tmt` removed.
- `twrit check` fails on exactly one rename-related item: `crates/motem` is in no layer. Waiting on the author's `.writ.tmt` edit
  (`crates/tove` -> `crates/motem` at the layer-1 list, and the `@dir(crates/tove)` sentence).
- Failures that predate this task and are left alone: `file-io-doors` (`tomet-vault`), `document-placement` (41 files under `tmtroot/deprecated`).
- The dev shell's flake build cannot see untracked files (new `crates/motem/` before it is snapshotted/added); `direnv exec` fell back to the existing environment.
- Uncommitted work in the default workspace touches `Cargo.toml` and `tmtroot/dev/plans.tmt`; expect a rebase conflict in `Cargo.toml` (dependency names) when it is committed.
