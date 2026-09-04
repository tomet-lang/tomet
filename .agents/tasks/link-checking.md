# Make `tomet check-links` usable, and the vocabulary guard honest

Four items, all recorded in `.agents/tasks/writ-bootstrap.md`'s known gaps.
The link checker matters now because the plan is to make every `.md` an
export of a `.tmt` so their links come under it -- which is worth nothing
while the checker cannot see half the tree.

## A. The existence set is built from the wrong root

`check_vault` builds it with
`collect_all_paths_with_config(root, ...)` -- `root` being whatever is
being *checked*. When that is a single file, the set contains exactly that
file, so every link it makes points at something "missing":
`tomet check-links docs/README.tmt` reports all of its links broken.

What exists on disk does not depend on what you asked to check. The set
should come from `project_root`, which `check_vault` already takes.

- [ ] A1. Build `existing` from `project_root`.
- [ ] A2. Confirm single-file and directory mode now agree on the same
      file.

## B. Hidden files are skipped, in both directions

`collect_tm_files_with_config` and `collect_all_paths_with_config` both set
`.hidden(true)`, so a `.writ.tmt` is neither checked nor counted as
existing. The guard links inside the writs cannot be verified today.

`.writ.tmt` is the nameless form of `<name>.<kind>.tmt`; the leading dot is
a kind separator, not a request to be invisible.

- [ ] B1. `.hidden(false)` on both walks.
- [ ] B2. Check what that lets in. `.gitignore` covers `target/` and
      `.direnv/`, but confirm `.git/` itself is excluded -- measure the
      file count before and after, do not assume.
- [ ] B3. `tomet check-links` on the repo root should now see the three
      `.writ.tmt` files and verify `no-vocabulary`'s guard link.

## C. `docs/README.tmt`'s own links do not resolve

17 of 18 links under `docs/` are broken -- but this one is not a bug in the
checker. `check.rs`'s module doc records the author's rule: a target
starting with `./` or `../` is relative to the referencing file, and a bare
one like `fact/` is relative to the *project root*. So `file:fact/` looks
for `<repo>/fact/` and correctly finds nothing.

The map is written wrong, not the checker.

- [ ] C1. Rewrite the links in `docs/README.tmt` as `./fact/` etc.
      `./` rather than `docs/fact/`: they point at siblings, and the
      relative form survives the directory being moved.
- [ ] C2. `tomet check-links docs` reports zero broken.

## D. The vocabulary guard cannot fail

`tests/src/vocabulary.rs` parses two sources differing in one identifier
and compares the trees. Gut the comparison and it stays green forever. It
has no case asserting that it still detects a violation.

This is not hypothetical: the tree-sitter drift check in the same package
carried an empty marker (`text.contains("")` is always true) and could not
fail for as long as it existed.

- [ ] D1. Add a case that must fail: build two trees that genuinely differ
      and assert the comparison reports it. The point is to exercise the
      detector, not the parser.
- [ ] D2. Confirm by breaking the real comparison temporarily and watching
      the new case fail, then restore.

## Verify

- [ ] E. `cargo test --workspace`, `just docs-check`,
      `tomet check-links .`, and `twrit check` from `../tomet-writ`.
