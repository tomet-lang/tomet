# tomet-tests

The workspace's cross-crate test suite: the shared `.tmt` corpus, the
invariants that span more than one crate, and the end-to-end conversion
snapshots.

This package exists because the tests in it belong to no single crate.
Before it, four crates reached outside their own directory for the shared
corpus -- three via `include_str!("../../../fixtures/...")` and one via a
`.parent().unwrap()` walk up to the repo root, both flagged as a
fragility in `crates/README.dirs.tmt`.

**Layer-local tests stay in their own crate.** Only tests that need the
shared corpus, or that cross a crate boundary, live here.

## Layout

| Path | Contents |
| --- | --- |
| `fixtures/` | The `.tmt` corpus. Frozen -- see below |
| `ref/` | Committed reference output for the snapshot tests |
| `store/` | Actual output, written only on mismatch. Gitignored |
| `src/lib.rs` | Harness: corpus discovery, snapshot compare, tree-sitter helpers |
| `src/corpus.rs` | Every fixture parses; tree-sitter grammar drift cases |
| `src/roundtrip.rs` | Parser + formatter and parser + printer invariants |
| `src/snapshot.rs` | `.tmt` -> HTML / CommonMark / Typst / printed `.tmt` |

## Running

```bash
cargo test -p tomet-tests              # all of it
cargo test -p tomet-tests --test corpus    # one target
TOMET_UPDATE_REF=1 cargo test -p tomet-tests   # accept new snapshot output
```

When a snapshot fails, the actual output is written to `store/` at the
same relative path as its reference, so the two can be diffed directly.

## The references are a regression net, not a correctness claim

`ref/` was generated from the code's behavior at the time each entry was
added, so it pins today's output *including any bugs*. A failing snapshot
means "this changed", not "this broke". Read the diff, decide whether the
new output is better, and only then accept it.

Some references are legitimately empty: a config-only document such as
`test.config.tmt` renders to nothing in HTML/Typst, because `@meta` and
`@config` are invisible in output formats.

## `fixtures/` is deliberately not synced with `docs/`

Every file in `fixtures/` began as a copy of a document under `docs/`,
but the two are **not** kept in step, and that is the point. `docs/` holds
real, evolving documentation; if `cargo test` read those files directly,
editing prose would break unrelated code changes. So these copies are
frozen: they change only when a test needs them to.

The cost is that they drift, and drift silently. That is accepted, with
one rule to keep it from becoming invisible: **when a grammar change
lands, add a fixture covering the new construct here** rather than
expecting an existing file to have picked it up. A fixture that still
exercises retired syntax is not a bug -- regression coverage of the old
shape is worth keeping.

`docs/` gets its own separate check, `just docs-check`, which parses and
format-checks the live documents outside of `cargo test`.

## Known-exception lists

Two allow-lists record where current behavior falls short. Both are
asserted to match reality exactly, so a fixture that starts *or stops*
hitting the exception fails the suite rather than silently changing what
is covered:

- `KNOWN_UNPARSEABLE` (`src/lib.rs`) -- fixtures `tomet-parser` rejects.
- `KNOWN_FORMAT_CHANGES_DOCUMENT` (`src/roundtrip.rs`) -- fixtures where
  `format_source` changes the parsed document, which it promises not to.

Each entry carries the reason inline. They are defect records, not
permanent carve-outs.
