# Multi-level nested `|` content

Split out of `.agents/tasks/conflict-element-and-content-model.md` during that
task's design discussion. Not started, not blocking that task.

## The problem

`tmtroot/docs/spec/syntax.tmt`'s "multiline" section sketches nested `|`
content, one extra `|` per nesting level:

```tmt
@tree
| @tree
| | @tree
| | | - outline
| | | - outline
| | @tree
| | | - outline
| | | - outline
@tree
| @tree
| | - outline
| | - outline
```

An element's own `|content` living *inside* another element's `|content`
needs every line to carry one marker per ancestor level it's still inside,
left to right (the innermost element's lines carry all its ancestors'
markers plus its own). That's a genuinely different problem from a single
level of `|content` gaining multi-paragraph support (see the sibling task's
"empty marker line ends a paragraph, not the whole run" fix) -- a single
level only ever tracks one active column; this needs tracking a *stack* of
simultaneously-active marker columns, one per nesting depth, and checking
all of them, in order, on every line.

Also unresolved, independent of the stacking issue: `tomet-syntax-parser`'s
`parse_inline_seq` (where `|`-content used to live) never dispatches on `=`
(section) or `-` (list) at all -- only backtick/autolink/comment/`@`/`$`/`^`/
delimiter are checked. So even a *single* level of `|content` cannot hold a
section or list today, only a paragraph or an `@`-element recognized via
`at_marked_line_start`. Whether `|content` should ever support `=`/`-` at
all (nested or not) hasn't been decided -- the `@tree` example above uses
`-` lists inside nested `|` content, but that example is from a sketch
section of the current spec, not a settled design.

## Why this is out of scope for the `@conflict` work

`@conflict`'s own spec (`tmtroot/docs/spec/elements/std-experimental/
conflict.tmt`) uses the bracket form (`a: [...]`, `b: [...]`) for block
content, not `|`. Nothing about promoting `@conflict` requires nested `|`
to work. Fixing the single-level empty-marker-line gap (in the sibling
task) is enough for every element discussed so far.

## Status

Completed (2026-10-08).
- Multi-level stacked column tracking implemented in `crates/tomet-syntax-parser/src/inline.rs` and `element.rs`.
  - `Stop::PipeRun` tracks `cols: &[usize]`.
  - `pipe_run_state` validates all ancestor and current columns, returns `End` on outer exits, and produces clear `"lines up with no content"` error on mismatch.
  - `split_softbreaks` folds all ancestor/current markers into `SoftBreak` span.
  - `parse_element_with_pipe_stack` and `parse_groups_with_pipe_stack` propagate active parent stacks to children.
- Block dispatch inside pipe content:
  - Lists (`-` and `1.`) are collected across lines with matching stack into `Block::Element(ul/ol)`.
  - Empty pipe lines correctly separate paragraphs at any depth.
  - Test parity verified against `[content]` in `tests/src/pipe.rs` with AST equality and printer expansion/round-trip.
