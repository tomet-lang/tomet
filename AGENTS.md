# Tomet

## Project overview

Read the root `README.md` first (what the project is, directory layout,
build/run commands) -- it's currently sparse, fill it in as you learn
things worth putting there. Read `docs/develop/architecture.md` before
making any non-trivial change (the crate pipeline and its consumers, why
the grammar has two independent implementations, the apps/editor
integrations, known gaps like the AST carrying no span info). For where a
document belongs and what language it's written in, `docs/README.md` is
the map and `docs/develop/docs-guide.md` is the rulebook. Do not restate
those files' content here — extend them instead, and keep this pointer
short.

## Language

Write all code comments in English. Do not use Japanese in code.

For documentation the rule is scoped by audience, per
`docs/develop/docs-guide.md`: `docs/develop/` and `docs/design/` are
English (they sit alongside and cross-link with the English doc
comments); `docs/spec/`, `docs/guide/`, and `docs/examples/` are the
user-facing docs and are written in Japanese.

## Verifying changes

There's no standalone GUI app here to screenshot or click through — this
is a parser/CLI/LSP/editor-extensions project. Verify with `cargo build`/
`cargo test`/`cargo check` and by reading the code. For the editor
extensions (`editors/vscode`, `editors/zed`), this
environment can't reliably launch a real VS Code/Zed window either, so
verify those the same way: reading the code and their own tests, not by
launching the actual editor to click around.

For grammar changes specifically, also run
`cargo test -p tomet-tests -p tree-sitter-tomet`. `tomet-parser` is the
source of truth for the grammar; `tree-sitter-tomet`'s `grammar.js` is a
separate, hand-maintained approximation used only for editor syntax
highlighting, and it does not update itself when `tomet-parser` changes.
Drift between the two is caught by `tomet-tests`'s `corpus` target, which
runs the shared `.tmt` corpus through both implementations; the grammar's
own structural tests stay in `tree-sitter-tomet`. Cover the new construct
by adding a fixture under `tests/fixtures/` — that corpus is frozen and
does not pick up changes to `docs/` on its own.

Cross-crate tests live in the `tests/` package (`tomet-tests`), not in the
individual crates; see `tests/README.md` for what belongs there and how
the snapshot references work.

## Task tracking

Before starting implementation on any non-trivial task, create a new file under `.agents/tasks/` (one file per task, e.g. `.agents/tasks/<short-task-slug>.md`) that breaks the work into discrete steps. Update that file immediately after completing each step, marking it done. Keep it accurate and current so that if work is interrupted partway through, it can always be resumed from that file alone, without needing prior conversation context. Multiple task files may coexist under `.agents/tasks/` when several non-trivial tasks are in flight; do not let one task's file block or get overwritten by another's. Once all steps for a task are done and the task is complete, delete that task's file.
