# @tomet/astro

An Astro integration for Tomet: it turns a directory of `.tmt` files into
Astro pages, with the bodies rendered by `tomet-convert-html` and the
metadata, slugs and links coming from the Rust workspace layer.

> **Status: active.** Provides `tometLoader` for Astro 5 Content Collections,
> compiling `.tmt` documents in-process via `@tomet/wasm` with
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

- **In-process WebAssembly compilation**: Uses `@tomet/wasm` to parse and render `.tmt` files in-process without spawning CLI sub-processes.
- **Incremental Digest Caching**: A document is rendered again unless the digest of its text, the files of the vault, the vault config and what it registers, the options, and the WebAssembly module that renders it are all as before. (File times say nothing: they are the same for every file in the Nix store.)
- **Link & Asset Resolution**: `ref:` notes, `file:`, `dir:` and `tmt:` paths and `@embed` are resolved against the files of the vault into the URL of a page, an asset, or the source of the project. A link that goes nowhere is a problem, never a dead `href`.
- **No silent skips**: see *Problems* below.
- **One vault per collection**: each collection reads in its own `Vault` of `@tomet/wasm`; two collections never see each other's files.
- **Rich Metadata & TOC**: Automatically surfaces document titles, headings (TOC), `::kind`, banners, thumbnails, tags, and custom metadata into `entry.data`.
- **Live Dev Mode Synchronization**: The dev watcher tracks `.tmt` files, images, assets, and the vault config; a file that is added or removed makes the vault again, since what links can reach has changed.

## Problems

A loader that skips what it cannot publish publishes a broken site, so this one does not skip. A *problem* is anything wrong with a document or with the collection: a document that does not parse, an empty one, a link that goes nowhere (a note, a path, an embed, a document the `filter` left out), an unknown element, an expression with no value, a dangling anchor, an `@draft`, a `base` that is not there, a `filter` that matches nothing. A problem has a file, a line and a column.

- In a build (`astro build`, `astro sync`), every problem is found before any is reported, and the load throws one `TometLoaderError` that lists all of them, one per line: `docs/spec/a.tmt:12:3: error unresolved-link: ...`. The build fails.
- In the dev server (`LoaderContext.watcher` is set) the problems are logged and the pages are still served, with what is wrong in `entry.data.diagnostics`.
- `strict` overrides which of the two it is.

What a site does not want to publish is left out with `filter`. A link to a document it left out is a problem too: the site has no page for it.

## Loader Options

| Option | Type | Default | Description |
| --- | --- | --- | --- |
| `base` | `string` | *(required)* | Path to the directory containing `.tmt` documents (relative or absolute). |
| `advanced` | `boolean` | `true` | Enables auto-numbering headings and generating slug IDs. |
| `concurrency` | `number` | `32` | Maximum concurrent files to read and parse simultaneously. |
| `urlPrefix` | `string` | `'/docs'` | Where the pages are: `/docs` gives `/docs/spec/syntax` for the entry `spec/syntax`. |
| `assetPrefix` | `string` | `'/vault'` | URL prefix prepended to resolved asset/image links. |
| `sourcePathPrefix` | `string` | `'docs'` | Prefix prepended to `sourcePath` (e.g. `'docs'` -> `docs/path/to/file.tmt`). |
| `vault` | `string` | `<base>/vault.config.tmt` | The vault config the documents are read with: macros, variables, and the vocabularies, dictionaries and blueprints it registers (read from its directory). Without one, `std` alone. |
| `basePath` | `string` | `''` | Where `base` is in the vault, from its root (`'tmtroot'`): paths in documents (`file:tmtroot/spec/x.tmt`) are written from the vault's root. |
| `sourceUrl` | `{ file: string; dir: string }` | — | Where the source of the project is, as URL templates with `{path}`: what a `file:` or `dir:` link has no page for goes there. Without it, such a link is a problem. |
| `strict` | `boolean` | `true` in a build, `false` in dev | Whether a problem fails the load. |
| `filter` | `(relPath: string) => boolean` | — | The documents of `base` that become entries. |

The id of an entry is the path of its document without the extension, always: it is the rule links are written on, so it cannot be changed.

## Entry Data (`TometEntryData`)

The loader injects parsed document metadata into Astro's `entry.data`:

```typescript
interface TometEntryData {
  title: string;          // From ::meta(title), fallback to the first h1, then the filename
  slug: string;           // Entry slug / ID
  section: string;        // Top-level folder name
  sourcePath: string;     // Source path (e.g. 'docs/intro.tmt')
  toc: TocItem[];         // Headings with id, level (2 or 3), and text
  meta: Record<string, unknown> | null; // The plain-data entries of `::meta(...)`; the rest is in `processFile`'s `header`
  kind: string | null;    // Document kind, from its `::kind(...)` declaration
  isDataOnly: boolean;    // True if document produces no body HTML
  diagnostics: Diagnostic[]; // What is wrong with the document or what it says about itself (see `@tomet/wasm`)
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

