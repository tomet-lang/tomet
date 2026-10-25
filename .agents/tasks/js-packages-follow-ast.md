# The JS packages follow the AST

Left over from `ast-element-split` (done and deleted 2026-11-01; its stages landed on `main`). The AST no longer has `Sigil`:
a node's role is its type (`Element`, `Connect`, `Declaration`, `Interpolation`, `Hole`, `ListItem`, `Bare`; see the `//!` of
`tomet-syntax-ast`), and `Element.name` is mandatory. `bindings/js/index.d.ts` was rewritten to the serialized shape
(dumped with `serde_json`), but nothing in the JS side was checked with `tsc`. Nothing below is started.

Rules that apply: AGENTS.md (no backward compatibility is wanted; `.md` files are generated, edit the `.tmt`; no commit or push
without the author's word; jj with `--no-pager`; a workspace under `.agents/workspaces/` for non-trivial work).

## Open

- [ ] `bindings/js/index.d.ts`: check it against the real output. Install TypeScript and run `tsc --noEmit` over it, and compare
  one document's JSON (`tomet ast`, or `serde_json` on a `Document` with each role in it) with the types. Nobody has done this.
- [ ] `packages/react` (`src/types.ts`, `src/utils.ts`, `src/ElementRenderer.tsx`) and `packages/svelte` (`src/types.ts`,
  `src/utils.ts`, `src/ElementRenderer.svelte`): both read `element.sigil` as `{Type} | {At} | 'Bare' | 'Dollar' | string`.
  That shape stopped existing long before the split, and `Sigil` is gone altogether now. Rewrite them on `index.d.ts`: the role is
  which variant of `Block` / `Inline` / `Value` / `Entry` a node is; an element has `name`. `dist/*.d.ts` are build output of the
  same.
- [ ] `packages/astro/README.md` (around line 64): a table of the `Sigil` shape in three places; it quotes the old `index.d.ts`.
  Edit the source `.tmt` if there is one, not the generated `.md`.
- [ ] `tomet-syntax-ast`: `Group::from_children` has no caller left in the workspace. Delete it, or say why it stays.

## Not here

- `tmtroot/spec/syntax/hole.tmt` says tandr still reads patterns its own way. That is true until tandr's `587c1f61` is on its
  `main`; it is for the docs pass, not this task.

Delete this file when the entries are done.
