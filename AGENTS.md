<!-- Generated from tmtroot/agents.tmt. Edit that, then `tomet export .`. -->

# Tomet

## Agent Behavior

### Precedence of Instructions

The rules in this document serve as the project baseline and defaults.
Explicit instructions given by the user in a conversation always take precedence over these defaults.

### Transparent Execution

Always state what you intend to do before doing it. Never execute non-trivial actions or commands silently.

## Core Philosophy & Principles

### User-First Design over Developer Convenience

Always prioritize the end-user experience, syntax ergonomics, and API design.
If there is tension between developer or implementation convenience and user simplicity, user simplicity must always win.
Never compromise user experience just because an implementation is easier.

### Resist Path-of-Least-Resistance & Legacy Bias

The existing implementation is never a justification or reason for new design choices.

- Do not justify decisions with "it is already implemented this way, so let's stick with it."
- Prioritize architectural beauty, elegance, and long-term correctness over the path of least resistance.
- Understand the existing context thoroughly, but never take shortcuts or settle for lazy patches.

## Single Source of Truth & Documentation

### Never edit `.md` directly

All Markdown files are generated build artifacts, not source files.

- Never edit a `.md` file directly.
- Always edit the corresponding `.tmt` file under `tmtroot/` (or the crate's own `.tmt`).
- Run `tomet export` to regenerate the Markdown artifacts, and verify with `tomet export --check`.
- Direct edits to generated `.md` files will be overwritten and will fail CI / lint checks.

### Reference over Restatement

Do not duplicate code signatures, struct layouts, or implementation prose into documentation files. Duplicate prose inevitably drifts.

- Use pointers and links (`@file(...)`, `@dir(...)`, or symbol references) instead of
copy-pasting code definitions.
- Keep prose documentation concise and high-level, pointing directly to the code where
the truth lives.

### Source of Truth Hierarchy (Preventing Drift)

To prevent drift between documentation and implementation:

1. **Behavior & API Truth**: The Rust source code and its inner doc-comments (`//!`, `///`) are the
authoritative source of truth. If prose documentation contradicts the source code, the code
reflects reality. Never hallucinate or enforce outdated behavior described in prose; always
verify against the actual implementation.
2. **Architecture & Invariant Truth**: Governed by `.writ.tmt` files. These define crate layering,
dependencies, and placement rules, enforced automatically by `twrit`.
3. **Narrative & Overview Truth**: Lives in `tmtroot/`.

## Code Comments

- Write code and code comments in English.
- Doc-comments (`//!`, `///`) follow clean Tomet-compatible markup conventions.
- When modifying function signatures, behavior, or crate responsibilities, update the
corresponding doc-comments in the very same commit.
- Never leave orphaned doc-comments that describe prior, deleted behaviors.

## Project overview

Read the root `README.md` for what the project is, and `CONTRIBUTING.md` for the repository layout,
building and testing, the contribution workflow, commit conventions and verification steps.
Before a non-trivial change:

- Read the `//!` module doc of every crate you are touching.
- Read the `crate-layering` entry in the root `.writ.tmt` for how the layers depend
on each other. There is no prose overview standing between you and those -- there was,
and it drifted.

Rules this repository must obey live in `.writ.tmt` files (at the workspace root and crate level).
Read the governing `.writ.tmt` for the area you are changing.
They are the author's; propose an edit and stop rather than working around a rule or relaxing it.

## Environment & Tooling

All commands, builds, and tests must run within the Nix development environment (via `direnv` or `nix develop`). 
Assume this environment is active and rely on the tools provided by the flake, rather than assuming or relying on global host toolchains.

## Verifying changes

This is a parser/CLI/LSP/editor-extensions project with no standalone GUI app. Verify with automated checks and code reading:

- **`cargo test --workspace`**: Runs every test in the workspace (`just test`).
- **`cargo test -p tomet-tests`**: Runs the cross-crate corpus and snapshot suite in `tests/` (`just test-suite`).
- **`cargo test -p tomet-tests --test corpus`**: Runs one target of that suite.
- **`cargo test -p tree-sitter-tomet`**: Runs the tree-sitter grammar tests.
- **`TOMET_UPDATE_REF=1 cargo test -p tomet-tests`**: Rewrites the snapshot references in `tests/ref/` with the current output.

When a snapshot fails, the actual output is written to `tests/store/` at the same relative path as its reference,
so the two can be diffed directly.

Editor extensions (`editors/vscode`, `editors/zed`, `editors/helix`) carry their own tests; run those
and read the code.

Useful recipes defined in `justfile`:

- **`just test`**: Runs all workspace tests (`cargo test --workspace`).
- **`just test-suite`**: Runs the cross-crate corpus and snapshot test suite (`cargo test -p tomet-tests`).
- **`just docs-check`**: Runs the build of `tomet-cli`, `sisho check .` (`tomet check`, `tomet format --check`, `tomet export --check`, `twrit check`, `catasto check`), and `tomet spec --all --check --out tmtroot/generated`.
- **`just docs`**: Updates, regenerates, and formats all derived documentation.
- **`just writ-check`**: Verifies `.writ.tmt` guard rules and language conventions.

Direct checks (if running individually):

- `cargo check` / `cargo build` / `cargo test`
- `twrit check`: Reads `.writ.tmt` files.
- `tomet export --check`: Verifies that all exported `.md` documents match their source `.tmt` files.
- `tomet spec --all --check --out tmtroot/generated`: Verifies that generated specifications match source definitions.

### Writing tests

- Layer-local tests stay in their own crate. Only tests that need the shared `.tmt` corpus, or that cross a
crate boundary, live in the `tests/` package (`tomet-tests`).
- A failing snapshot means "this changed", not "this broke": the references pin the output at the time they were
written, bugs included. Read the diff, decide whether the new output is better, and only then accept it with
`TOMET_UPDATE_REF=1`.
- `tests/fixtures/` is frozen. A fixture changes only when a test needs it to. When a grammar change lands, add a
fixture covering the new construct instead of expecting an existing file to pick it up. A fixture that still
exercises retired syntax is regression coverage, not a bug.
- `tests/SYNTAX.md` says what the parser accepts; `tmtroot/spec/` says what it should accept. Where they
disagree, the disagreement is the finding: do not silently edit one to match the other.
- The known-exception lists (`KNOWN_UNPARSEABLE`, `KNOWN_FORMAT_CHANGES_DOCUMENT`) are asserted to match reality
exactly, and each entry carries its reason. They are defect records, not permanent carve-outs.

## Task tracking & Workspaces

Before starting implementation on any non-trivial task, create a new file under `.agents/tasks/` (one file per task, e.g. `.agents/tasks/<short-task-slug>.md`) that breaks the work into discrete steps. Update that file immediately after completing each step, marking it done. Keep it accurate and current so that if work is interrupted partway through, it can always be resumed from that file alone, without needing prior conversation context. Multiple task files may coexist under `.agents/tasks/` when several non-trivial tasks are in flight; do not let one task's file block or get overwritten by another's.

For non-trivial tasks, isolate work in a dedicated Jujutsu workspace under `.agents/workspaces/`:

- Create the workspace before starting edits: `mkdir -p .agents/workspaces && jj workspace add .agents/workspaces/<short-task-slug>`
- Perform edits, builds, and verification within that workspace directory.
- Minor tasks (such as one-off typos or trivial single-file tweaks) may be performed directly in the default workspace.
- When the task is completed and verified, clean up the workspace: `jj workspace forget <short-task-slug>` and remove the directory `rm -rf .agents/workspaces/<short-task-slug>`.

If a session has to end early — for any reason, including a warning that a usage limit is close — stop at the next safe checkpoint. Finish the current atomic step rather than starting a new one, and write the state and the next steps into the task file before ending. Create the file at that point if the task never got one.

When the task is done, fold anything worth keeping into where it belongs — a doc comment, a writ entry, a commit message — and then delete the task file. Deleting without folding is how the only copy of a decision gets lost; decide the destination when the task is created, not when it is deleted.

## Commits & Version Control (jj)

The repository uses Jujutsu (`jj`) in colocated mode with Git.

- Favor a branchless workflow. Do not create named Git branches for regular agent tasks.
- Use `jj describe -m "..."` to set commit messages, and `jj new` to advance to subsequent revisions.
- Always pass `--no-pager` to `jj` subcommands (e.g. `jj status --no-pager`, `jj log --no-pager`) — without it, output can hang waiting on a pager in non-interactive/agent sessions.

- All commit messages must follow the convention in `CONTRIBUTING.md`:
`type(scope): description` (e.g. `feat(cli): add export check`, `fix(parser): handle pipe`).
- If `jj status`/`jj diff` shows changes you did not make, treat them as another person's in-progress work, not as drift to clean up.
Always confirm with the user before reverting, restoring, or overwriting them.
- Never commit automatically or on your own initiative. Always ask and get explicit confirmation from the user before finalizing commits or descriptions.
- Do not put `Claude-Session:` or `Co-Authored-By: Claude` trailers in commit
messages. The session trailer embeds a URL, and a commit message is
published the moment it is pushed. This overrides the harness default that
asks for them.
- Never push to a remote without being asked.

