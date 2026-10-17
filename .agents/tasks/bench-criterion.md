# bench-criterion

Goal: criterion benchmarks for tomet, in a new workspace package `benches/` (`tomet-benches`, layer 7 -- the author
added `"benches"` to `crate-layering` in `.writ.tmt`; that one-line edit is the author's, not mine).

Why a new package and not `tomet-tests`: criterion and the `tomet-synth` git dependency would otherwise enter the test
package's dependency graph and be built by every `cargo test --workspace`. Benches are slow, noisy and optional; they
get their own package.

Inputs (all behind one small harness in `benches/src/lib.rs`):

- `tomet_tests::corpus()` -- the frozen in-repo fixtures (always available; reuse its discovery, do not reach into
  `tests/fixtures` by path).
- `tomet-synth` (git dev-dependency from tomet-lang/tomet-corpus) -- generated at 1 KB .. 1 MB per document and N
  documents, deterministic by seed.
- Optional real data: `TOMET_CORPUS_DIR` pointing at a tomet-corpus checkout (`data/wikipedia`, `data/aozora`,
  `data/handwritten`, `data/examples`). Absent => those groups are skipped, never an error.

Every input is parsed once with `parse_document(..).unwrap()` before it is measured, so a bench can never time an
error path by accident, and the generator's output is checked against the real parser here.

Steps:

- [x] Author edited `.writ.tmt` (layer 7 += "benches"); copied into this workspace.
- [x] `benches/` package (`tomet-benches`): Cargo.toml (criterion 0.8 without plotters/rayon), workspace member + `tomet-synth`/`criterion` in workspace.dependencies; `twrit check`: crate-layering ok (45 members); generated API doc `tmtroot/generated/crates/tomet-benches.tmt` added (only that file, the other 15 rust-api-docs differences predate this task).
- [x] Harness (`benches/src/lib.rs`): fixtures via `tomet_tests::parseable_corpus`, `synth_doc`, optional `TOMET_CORPUS_DIR`; every input is parse-checked first (synth output up to 1 MB verified against the real parser).
- [x] Bench `parse` (`benches/benches/parse.rs`): `-- --test` passes for all 22 benches; full runs recorded in the commits (beba4500, 89e104a8). 1 MB documents only with `TOMET_BENCH_LARGE=1` (kept opt-in; revisit now that parsing is linear).
- [x] Parse super-linearity: found with this bench, fixed and pushed (89e104a8, parse_document is linear now).
- [x] Verified: workspace tests ok (tomet-python excluded), `twrit check` crate-layering ok.
- [ ] Next benches (separate steps): format, validate/semantics, search, merge, convert.
- [ ] When all benches are in: decide where the remaining facts below live (doc comment / commit message), then delete this file and `jj workspace forget bench-criterion`. (The tomet-corpus task file was already deleted; its still-valid facts are below.)

Facts to keep for the next benches:

- `tomet format` rewrites `@link(ref:"X")[t]` to `@link[t](ref:"X")`. The tomet-corpus converters and `tomet-synth` both emit the non-canonical form, so a `format` bench over them never measures a no-op; if a no-op is wanted, format the input once first.
- Parsing is linear since 89e104a8; the old curve (and the `before` numbers) no longer apply. Re-measure before comparing anything to older figures.
- The benches run with plain `cargo` outside nix (`nix develop` needs untracked files in git); `CARGO_TARGET_DIR` can share the main tree`s target. `tomet-python` does not build outside nix (PyO3 vs Python 3.14).
