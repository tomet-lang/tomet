# TypedMark

## Project overview

Read the root `README.md` first (what the project is, directory layout,
build/run commands) -- it's currently sparse, fill it in as you learn
things worth putting there. Read `docs/architecture.md` before making any
non-trivial change (the crate pipeline and its consumers, why the grammar
has two independent implementations, the apps/editor integrations, known
gaps like the AST carrying no span info). Do not restate either file's
content here — extend them instead, and keep this pointer short.

## Language

Write all comments and documentation in English. Do not use Japanese.

## Verifying changes

There's no standalone GUI app here to screenshot or click through — this
is a parser/CLI/LSP/editor-extensions project. Verify with `cargo build`/
`cargo test`/`cargo check` and by reading the code. For the editor
extensions (`apps/integrations/vscode`, `apps/integrations/zed`), this
environment can't reliably launch a real VS Code/Zed window either, so
verify those the same way: reading the code and their own tests, not by
launching the actual editor to click around.

For grammar changes specifically, also run
`cargo test -p tree-sitter-typedmark`. `typedmark-parser` is the source of
truth for the grammar; `tree-sitter-typedmark`'s `grammar.js` is a
separate, hand-maintained approximation used only for editor syntax
highlighting, and it does not update itself when `typedmark-parser`
changes — that test is how drift between the two gets caught.

## Task tracking

Before starting implementation on any non-trivial task, create a new file under `.agents/tasks/` (one file per task, e.g. `.agents/tasks/<short-task-slug>.md`) that breaks the work into discrete steps. Update that file immediately after completing each step, marking it done. Keep it accurate and current so that if work is interrupted partway through, it can always be resumed from that file alone, without needing prior conversation context. Multiple task files may coexist under `.agents/tasks/` when several non-trivial tasks are in flight; do not let one task's file block or get overwritten by another's. Once all steps for a task are done and the task is complete, delete that task's file.
