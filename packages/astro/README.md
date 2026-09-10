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

## Features & Architecture

- **In-process WebAssembly compilation**: Uses `@tomet/tomet-wasm` to parse and render `.tmt` files in-process without spawning CLI sub-processes.
- **Incremental Digest Caching**: Content digests combine file mtime, size, and workspace config mtime (`configKey`) to skip unchanged documents across builds.
- **Proximity-aware Link & Asset Resolution**: Automatically resolves `ref:` links and `@embed` image references against the scanned vault and asset map.
- **Rich Metadata & TOC**: Automatically surfaces document titles, headings (TOC), `@kind`, banners, thumbnails, tags, and custom metadata into `entry.data`.
- **Live Dev Mode Synchronization**: The dev watcher tracks `.tmt` files, images, assets, and workspace configuration changes with live vault index re-indexing.

## Loader Options

| Option | Type | Default | Description |
| --- | --- | --- | --- |
| `base` | `string` | *(required)* | Path to the directory containing `.tmt` documents (relative or absolute). |
| `advanced` | `boolean` | `true` | Enables auto-numbering headings and generating slug IDs. |
| `concurrency` | `number` | `32` | Maximum concurrent files to read and parse simultaneously. |
| `urlPrefix` | `string` | `'/docs'` | URL prefix prepended to resolved `ref:` note links. |
| `assetPrefix` | `string` | `'/vault'` | URL prefix prepended to resolved asset/image links. |
| `sourcePathPrefix` | `string` | `'docs'` | Prefix prepended to `sourcePath` (e.g. `'docs'` -> `docs/path/to/file.tmt`). |
| `configPath` | `string` | `default.config.tmt` | Path to the workspace configuration file. |
| `filter` | `(relPath: string) => boolean` | — | Predicate to include or exclude specific documents. |
| `generateId` | `(p: { relPath, absPath }) => string` | — | Custom entry ID generator (defaults to relative path without extension). |

## Entry Data (`TometEntryData`)

The loader injects parsed document metadata into Astro's `entry.data`:

```typescript
interface TometEntryData {
  title: string;          // From @meta{title}, fallback to first h1 or filename
  slug: string;           // Entry slug / ID
  section: string;        // Top-level folder name
  sourcePath: string;     // Source path (e.g. 'docs/intro.tmt')
  toc: TocItem[];         // Headings with id, level (2 or 3), and text
  meta: Record<string, unknown> | null; // Raw metadata object
  kind: string | null;    // Document kind (from @kind(...) or meta)
  isDataOnly: boolean;    // True if document produces no body HTML
  description?: string;   // Meta description or auto-extracted excerpt
  tags?: string[];        // Tags array normalized from metadata
  date?: string;          // Creation / publication date
  banner?: string;        // Resolved banner image URL
  bannerY?: number;       // Vertical banner position offset
  images?: string[];      // Resolved list of embedded images
  thumbnail?: string;     // First image, banner, or extracted img src
  [key: string]: unknown; // All custom properties declared in @meta are spread here
}
```

## Rendering in Astro Pages

In an Astro component (`src/pages/[...slug].astro`):

```astro
---
import { getCollection, render } from 'astro:content';

export async function getStaticPaths() {
  const docs = await getCollection('docs');
  return docs.map((entry) => ({
    params: { slug: entry.id },
    props: { entry },
  }));
}

const { entry } = Astro.props;
const { Content, headings } = await render(entry);
---

<article>
  <h1>{entry.data.title}</h1>
  {entry.data.banner && <img src={entry.data.banner} alt="" />}
  <div class="prose">
    <Fragment set:html={entry.rendered?.html} />
  </div>
</article>
```

## Development

```bash
npm install
npm run typecheck
npm test
npm run build
```

`node_modules/` and `dist/` are covered by the repository's root
`.gitignore`.

