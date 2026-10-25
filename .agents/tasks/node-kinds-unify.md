# One "any node" type instead of four

Raised after the `ast-element-split` task (2026-11-01). The author wants it done. Delete this file when done.
Destination of what is worth keeping: `tmtroot/dev/design/node-tree.tmt` (read it first; this overlaps `node-tree`), then the
`//!` of `tomet-syntax-tree`.
Related: `node-tree` (a different thing: Section/File/Folder as one tree; it is only a draft), `ast-body-view`.

## Facts

Four enums say "some node of a document", each with its own set of roles:

| Type | Where | Roles |
|---|---|---|
| `Node` | `tomet-syntax-tree` (`for_each_node`) | Block, Inline, Value, Entry |
| `Child` | `tomet-syntax-tree` (`for_each_descendant`, ...) | Element, Item |
| `Node` | `tomet-semantics-expr` (`nodes.rs`) | Element, Section, List, Connect, Declaration, Item |
| `Written` | `tomet-validator-core` | Element, Declaration |

The split added roles to each of them by hand (list item, bare entry, declaration, connect, hole). Every new role is
another arm in four places.

## Leans (assistant, not confirmed)

- The first one (`tomet_tree::Node`) is the primitive; the expression language and the validator want *a subset with
  an address / a name*. Derive those from it (a method, or a filter) instead of a parallel enum.
- Do not wait for `node-tree`: it is about Section/File/Folder; this is about what sits inside a document.

## Open

- [ ] Can the expression `Node` be `tomet_tree::Node` plus the owner it was reached from (it needs `Section`/`List`, which
      `for_each_node` does not hand out as one node today)?
- [ ] Should `Child` become a filtered view of `Node`?

## Steps

- [ ] Read all four definitions and every `match` on them; write down which role each arm needs.
- [ ] Decide the one type and what it carries (the role, the owner, the position it was reached from).
- [ ] Replace them one at a time, `Child` first (smallest).
- [ ] `cargo test --workspace`, `sisho check .`.
