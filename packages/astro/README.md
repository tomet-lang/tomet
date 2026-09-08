# @tomet/astro

An Astro integration for Tomet: it turns a directory of `.tmt` files into
Astro pages, with the bodies rendered by `tomet-convert-html` and the
metadata, slugs and links coming from the Rust workspace layer.

> **Status: active.** Provides `tometLoader` for Astro 5 Content Collections,
> compiling `.tmt` documents in-process via `@tomet/tomet-wasm` with
> incremental digest caching capable of scaling to 50,000+ files.

## Usage

Define a content collection in `src/content.config.ts`:

```typescript
import { defineCollection } from 'astro:content';
import { tometLoader } from '@tomet/astro';

export const collections = {
  docs: defineCollection({
    loader: tometLoader({
      base: '../docs',
      advanced: true, // auto heading numbers & slug IDs
    }),
  }),
};
```

## Why this exists

Tomet does not need a static site generator. `tomet-website` already
demonstrates the whole pattern in about 250 lines of `src/lib/docs.ts`:
walk the documents, render each one with the Rust converter, hand the HTML
fragment to Astro. Astro supplies the layout, routing, dev server and
build; Tomet supplies the parse and the render.

What is missing is not a generator but the layer directly under it. Every
site built this way has to redo the same work by hand: find the documents,
respect the workspace's ignore rules, decide each page's title, resolve
`ref:` links into hrefs, build the nav tree. That is the part worth having
once, and it is what this package is.

The scope boundary is deliberate:

- **In scope** — document discovery, metadata, slugs, the link graph, and
  a body-rendering entry point.
- **Out of scope** — themes, layouts, CSS, navigation components. Those
  belong to each site. `tomet-website` and a personal vault want opposite
  things there and neither should be shipped from here.

## Rule: no AST types are declared in this package

Types for the Tomet AST come from the binding. They are not re-declared
here, in any form, for any reason.

This is not a style preference. The AST is genuinely still moving --
`8a7c7db feat(syntax)!: sigils encode shape, namespaces encode origin`
and `e173ca0 fix!: remove the nameless @` are both recent -- and this
repository already carries three different generations of the same type:

| Location | `Sigil` |
| --- | --- |
| `crates/tomet-syntax-ast/src/lib.rs` (the truth) | `Named(Name) \| Bare \| Dollar` |
| `bindings/js/index.d.ts` | `'Bare' \| 'Angle' \| 'At' \| 'Dollar' \| ...` (9 variants) |
| `packages/react/src/types.ts` | `{Type: string} \| {At: string \| null} \| 'Bare' \| 'Dollar' \| string` |

The third is the oldest of the three -- a pre-rename shape, with a
trailing `| string` that suppresses the mismatch rather than fixing it,
and an `Inline` union that accepts two generations at once. A fourth
hand-written copy would make the situation worse, so there is not one
here.

This is also why the package lives in `packages/` rather than in its own
repository: a change to the binding and the change it forces here belong
in the same commit. Splitting it out later, once the AST settles, is easy;
re-merging it would not be.

## Prerequisites

Work that has to happen elsewhere in this repository before this package
can do its job. Roughly in dependency order.

1. **`@tomet/tomet-wasm` does not resolve.** `bindings/js/package.json`
   declares `"main": "tomet_js.js"` and `"types": "index.d.ts"`, but
   `tomet_js.js` is in `bindings/js/pkg/` and there is no `package.json`
   there. Nothing can depend on the binding by name today, which is why it
   is absent from this package's `dependencies`.

2. **`bindings/js/index.d.ts` is stale.** It still describes the
   nine-variant `Sigil` from `8a7c7db`, one breaking commit behind the
   Rust. The checked-in `pkg/` build should be checked at the same time.
   Generating these types from the Rust serde representation, rather than
   maintaining them by hand, is what stops this recurring.

3. **The workspace layer is not exposed to the binding.** All of it exists
   in Rust and none of it is reachable from JavaScript:

   | Rust | Needed for |
   | --- | --- |
   | `collect_tm_files_with_config` | document discovery honoring `workspace.ignore` |
   | `extract_metadata` | page metadata -- but it returns `BTreeMap<String, String>`, and a real workspace needs values, not strings (`topics` is a list of `@link`, `created` is a datetime) |
   | `collect_links` | the link graph, backlinks |
   | `WorkspaceIndex` / `FileTreeNode` | the nav tree |

   Walking the filesystem from Node instead would mean reimplementing the
   ignore rules in TypeScript, where they would drift from `.tmtconfig` --
   the same mistake as a second parser.

4. **`ref:` links have no resolution.** `render_link_element` in
   `crates/tomet-convert-html/src/lib.rs` emits
   `<a class="tm-ref" href="X">` with the raw note name as the href.
   Resolving that needs a name-to-path index, which is workspace state, so
   it cannot live in the converter without giving up its
   single-document, I/O-free shape. The `tm-ref` class is the hook: this
   package resolves those hrefs against the index after rendering, and the
   converter stays untouched.

5. **A title rule.** `tomet-website` scrapes the first `<h1>` with a
   regular expression because `meta_title` is private to the CLI crate.
   Once metadata is reachable the rule should be explicit --
   `@meta{title}`, then the first heading, then the filename. Filename
   matters more than it looks: in a large vault the filename usually *is*
   the title.

6. **The default stylesheet is dead.** Every selector in
   `DEFAULT_STYLE` (`crates/tomet-convert-html/src/lib.rs`) uses a `tmt-`
   prefix while the renderer emits `tm-`, so it matches nothing.
   `tomet-website/src/styles/global.css` is the only stylesheet that
   actually works against the real class names. Whether that gets
   upstreamed as a fixed `DEFAULT_STYLE` or offered here as an opt-in
   import is an open decision -- but per the scope boundary above, it is
   not a theme either way.

## Rendering does not need a subprocess

`toHtml` in the wasm binding calls `tomet_html::render_body`, so it
already returns a body fragment rather than a standalone page. Rendering
happens in-process.

This matters at scale. `tomet-website` shells out to the CLI once per
document, which is fine for a few dozen files; the vault this package is
being designed against has over twenty thousand, where one process per
document is not viable. The binding removes that cost entirely, and it
also removes the dependency on the CLI's `--body` flag.

## Development

```bash
npm install
npm run typecheck
npm run build
```

`node_modules/` and `dist/` are covered by the repository's root
`.gitignore`.
