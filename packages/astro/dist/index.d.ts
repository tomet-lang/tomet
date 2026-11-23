/**
 * Astro integration & Content Layer Loader for Tomet.
 *
 * Provides `tometLoader` for Astro 5 Content Collections, rendering `.tmt` documents
 * in-process via `@tomet/wasm` with incremental caching designed for up to 50,000+ files.
 *
 * A loader that skips what it cannot publish publishes a broken site, so `tometLoader` does not skip.
 * Anything wrong with a document -- it does not parse, it is empty, a link goes nowhere, an element is
 * unknown, it still says `@draft` -- is a *problem*, and so is a collection or vault that cannot be
 * built (a missing base, no documents, a missing or unreadable vault config or part, an invalid vault).
 * In a build every problem is collected, with the file, line and column, and the build fails with all
 * of them at once; when the vault itself cannot be built, the build stops there instead of reading
 * documents against an incomplete vault. In the dev server problems are logged without taking the
 * server down.
 */
import type { AstroIntegration } from "astro";
import type { Loader } from "astro/loaders";
import { type Declaration, type Diagnostic, type ProcessOptions, type ProcessedFile, type TocItem } from "@tomet/wasm";
export type { Diagnostic, ProcessOptions, ProcessedFile, TocItem };
/**
 * Ensures the WebAssembly engine is loaded and initialized.
 */
export declare function ensureWasm(): Promise<void>;
/**
 * Render a single `.tmt` source string to an HTML body string in-process.
 */
export declare function renderTomet(source: string, options?: ProcessOptions): Promise<string>;
/**
 * Process a single `.tmt` source string to a `ProcessedFile` in-process.
 */
export declare function processTomet(source: string, options?: ProcessOptions): Promise<ProcessedFile>;
/**
 * The kind a document declares with its `::kind(...)`, read from the declarations of
 * `ProcessedFile.header`: the text of the first unnamed entry of its arguments, or `null`
 * if the document declares none (or declares one that is not written as a text).
 */
export declare function declaredKind(header: readonly Declaration[]): string | null;
export interface TometLoaderOptions {
    /**
     * Base directory containing `.tmt` files.
     * Can be relative to the Astro project root or absolute.
     *
     * The id of an entry is the path of its document from here without the extension (`spec/syntax`), and
     * the page of that entry is at `<urlPrefix>/<id>`: a link to a document is written on that rule.
     */
    base: string;
    /**
     * Whether to enable advanced heading numbering (`1`, `1.1`, ...) and auto-generated `id` slugs.
     * Defaults to `true`.
     */
    advanced?: boolean;
    /**
     * Maximum concurrent files to read and parse simultaneously (default: 32).
     * Prevents EMFILE and excessive memory spikes on large workspaces (50,000+ files).
     */
    concurrency?: number;
    /**
     * Custom filter to include or exclude paths: the documents of `base` that become entries. Return
     * `false` to leave one out. A document that is left out is not read, and a link to it is a problem:
     * the site has no page for it.
     */
    filter?: (relPath: string) => boolean;
    /**
     * URL prefix of the pages: `/docs` (the default) gives `/docs/spec/syntax`.
     */
    urlPrefix?: string;
    /**
     * URL prefix of the files that are not documents (images, ...): `/vault` (the default) gives
     * `/vault/img/x.png`. Serving them is the site's job.
     */
    assetPrefix?: string;
    /**
     * Prefix prepended to `sourcePath` for each entry (default: `'docs'`).
     * Set to empty string `''` to use the relative path directly from `base`.
     */
    sourcePathPrefix?: string;
    /**
     * The vault config (`vault.config.tmt`) the documents are read with: macros, variables, and the
     * vocabularies, dictionaries and blueprints it registers, which are read from the directory it is in.
     * Relative to the Astro project root, or absolute. Defaults to `vault.config.tmt` under `base`, if
     * there is one. Without a config, documents are read with `std` alone.
     */
    vault?: string;
    /**
     * Where `base` is in the vault, from the vault's root, when it is a part of it: `tmtroot` if `base` is
     * the `tmtroot/` of the vault. A path a document writes (`file:tmtroot/spec/x.tmt`) is written from
     * the vault's root, and this is what it is read from. Defaults to `''`: `base` is the vault.
     */
    basePath?: string;
    /**
     * Where the source of the project is on the web, as URL templates with `{path}` for the path from
     * the vault's root (percent-encoded). A `file:` or `dir:` link to something the site has no page for
     * goes there; without it, such a link is a problem.
     */
    sourceUrl?: {
        file: string;
        dir: string;
    };
    /**
     * Whether a problem fails the load. Defaults to `true` in a build and `false` in the dev server, where
     * problems are logged and the pages are still served.
     */
    strict?: boolean;
}
export interface TometEntryData {
    title: string;
    slug: string;
    section: string;
    sourcePath: string;
    toc: TocItem[];
    meta: Record<string, unknown> | null;
    kind: string | null;
    isDataOnly: boolean;
    /** What is wrong with the document, or what it says about itself (`@draft`): see `Diagnostic`. */
    diagnostics: Diagnostic[];
    description?: string | null;
    tags?: string[];
    date?: string | null;
    banner?: string | null;
    bannerY?: number | null;
    images?: string[];
    thumbnail?: string | null;
    [key: string]: unknown;
}
/**
 * Something wrong with a document, or with the collection it is in, and where.
 */
export interface Problem {
    /** The file, relative to the Astro project root when it is under it. */
    path: string;
    /** Counted from 1; `null` for a problem of the whole file or collection. */
    line: number | null;
    column: number | null;
    severity: "error" | "warning";
    /** What kind of problem: a `Diagnostic.code`, or `parse-error`, `empty-document`, `unreadable`, ... */
    code: string;
    message: string;
}
/**
 * What a strict load throws: every problem found, one per line, with where it is.
 */
export declare class TometLoaderError extends Error {
    readonly problems: readonly Problem[];
    constructor(collection: string, problems: readonly Problem[]);
}
/**
 * Resolves a raw asset reference (e.g. `@link(ref:+hash.png)`, `+hash.png`, `https://...`)
 * to a browser-accessible URL.
 */
export declare function resolveAssetUrl(raw: unknown, fileRelDir: string, assetMap: Map<string, string>, allFilesSet: Set<string>, assetPrefix: string): string | null;
/**
 * Astro 5 Content Layer Loader for Tomet documents.
 *
 * ```ts
 * // src/content.config.ts
 * import { defineCollection } from 'astro:content';
 * import { tometLoader } from '@tomet/astro';
 *
 * export const collections = {
 *   docs: defineCollection({
 *     loader: tometLoader({ base: '../tomet/docs' }),
 *   }),
 * };
 * ```
 *
 * =[ Problems ]
 *
 * Every problem is found before any is reported: a build fails once, with all of them
 * (`TometLoaderError`), each with its file, line and column. That includes a document that does not
 * parse, an empty one, an `@draft`, a link that goes nowhere, and an unknown element, and also a
 * `base` that is not there, a `filter` that matches nothing, or a vault config that is missing,
 * unreadable, or invalid. When the collection or vault itself cannot be built, the loader stops at
 * those problems rather than parsing documents against an incomplete vault. What a site does not want
 * to publish is left out with `filter`; nothing is skipped silently.
 */
export declare function tometLoader(options: TometLoaderOptions): Loader;
/** Options accepted by the Astro integration. */
export interface TometOptions {
}
/**
 * Registers the Tomet integration with Astro.
 */
export default function tomet(_options?: TometOptions): AstroIntegration;
//# sourceMappingURL=index.d.ts.map