# Cettila

## Project overview

Read the root `README.md` first (what the project is, directory layout,
build/run commands). Read `docs/architecture.md` before making any
non-trivial change (view-crate pattern, pane/workspace system, rendering
tiers, config/persistence, the CLI). Do not restate either file's content
here — extend them instead, and keep this pointer short.

## Language

Write all comments and documentation in English. Do not use Japanese.

## Verifying changes

Do not debug or verify changes by taking screenshots (e.g. via Xvfb/Wayland screenshot tools). This environment does not reliably support it. Verify with `cargo build`/`cargo check` and by reading the code instead.

Do not use X11 (xdotool, wmctrl, xwininfo, forcing `QT_QPA_PLATFORM=xcb`, or any other X11-based automation/inspection) to drive, resize, or inspect the running app. Do not launch the actual GUI app to click around or simulate input.

## Task tracking

Before starting implementation on any non-trivial task, create a new file under `.agents/tasks/` (one file per task, e.g. `.agents/tasks/<short-task-slug>.md`) that breaks the work into discrete steps. Update that file immediately after completing each step, marking it done. Keep it accurate and current so that if work is interrupted partway through, it can always be resumed from that file alone, without needing prior conversation context. Multiple task files may coexist under `.agents/tasks/` when several non-trivial tasks are in flight; do not let one task's file block or get overwritten by another's. Once all steps for a task are done and the task is complete, delete that task's file.
