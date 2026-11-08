# Task: Complete Tomet Syntax Design (One Container & Unified Declarations)

## Context & Goal
In the previous session, Stage 2 implementation (`one-container-2`) was started before the entire syntax design was fully settled, leaving 83 modified files in `.agents/workspaces/one-container` in a broken state.
Per user instruction:
1. Revert the incomplete Stage 2 code edits in `.agents/workspaces/one-container` back to the clean post-Stage-1b state (`tkktklmr 5503adb8`, where only `tmtroot/dev/design/one-container.tmt` is added).
2. Thoroughly review all notes (`.agents/notes/2026-11-13-syntax-conversation.record.md`, `transcript.md`), design docs (`tmtroot/dev/design/one-container.tmt`, `tmtroot/docs/spec/*.tmt`), and existing `.tmt` usage across the repository.
3. Complete the **entire syntax design** with the user—resolving all open questions and edge cases—before touching any Rust code.

## Progress & Steps
- [x] Inspect workspace state and existing notes/design documents
- [x] Revert incomplete Stage 2 code edits in `.agents/workspaces/one-container` (keep `tmtroot/dev/design/one-container.tmt`)
- [x] Analyze all remaining syntax questions and present a clear, complete design synthesis to the user
- [x] Save the 2026-11-14 syntax design conversation under `.agents/notes/2026-11-14-syntax-conversation/` (`transcript.md` and `record.md`)
- [ ] Finalize the remaining syntax decisions with the user and update `tmtroot/dev/design/one-container.tmt`
- [ ] Plan implementation stages only after syntax design is 100% locked in

## Summary of 2026-11-14 Decisions & Findings (see `.agents/notes/2026-11-14-syntax-conversation/record.md`)
1. **Unchanged Syntax**:
   - `#(id)` stays as ID connect sugar; `- |content` stays as multiline list item syntax.
   - Postfix `{ }` stays as sugar for `::meta(...)` on elements, headings, and list items (never used for structural/required properties like `version`).
   - Type seats (`<$version> 1.10`) deferred for later.
2. **Declarations & Keys**:
   - `::name(...)` is `::` + named tuple; head dot-paths (`::format.meta(...)`, `::field.author(...)`) are retired in favor of nested keys inside `(...)` (`::field(author: (...))`).
   - Dotted keys inside tuples (`url.wiki: (...)`) are kept as shorthand for `url: (wiki: (...))`; literal dotted keys are quoted (`"url.wiki"`).
   - Keys are strictly for traversal/addressing and must never carry the value's meaning (`::use(a: (version: 3))` is rejected).
3. **Three Unified Rules Reached at End of 2026-11-14 Session**:
   - **Rule 1 (Unnamed Tuples)**: `( ... )` is always a tuple regardless of item count (`()`, `(a)`, `(a, b, c)`), so `...list` always yields the tuple.
   - **Rule 2 (Values with Attributes)**: A value with attributes is written as a named tuple `a(version: 3)` or `string(required: true)`, accessed uniformly with `.` (`use.0.version`, `field.author.required`).
   - **Rule 3 (Single Merge Rule)**: All tuples merge by key (`"0"`, `"1"`, ... for positional items; `"key"` for named items). Two unnamed tuples at the same key merge recursively; colliding scalars or named tuples error by default.
