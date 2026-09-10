/**
 * Astro integration & Content Layer Loader for Tomet.
 *
 * Provides `tometLoader` for Astro 5 Content Collections, rendering `.tmt` documents
 * in-process via `@tomet/tomet-wasm` with incremental caching designed for up to 50,000+ files.
 */
import type { AstroIntegration } from 'astro';
import type { Loader } from 'astro/loaders';
import { type ProcessOptions, type ProcessedDoc, type TocItem } from '@tomet/tomet-wasm';
export type { ProcessOptions, ProcessedDoc, TocItem };
/**
 * Ensures the WebAssembly engine is loaded and initialized.
 */
export declare function ensureWasm(): Promise<void>;
/**
 * Render a single `.tmt` source string to an HTML body string in-process.
 */
export declare function renderTomet(source: string, options?: ProcessOptions): Promise<string>;
/**
 * Process a single `.tmt` source string to a `ProcessedDoc` in-process.
 */
export declare function processTomet(source: string, options?: ProcessOptions): Promise<ProcessedDoc>;
export interface TometLoaderOptions {
    /**
     * Base directory containing `.tmt` files.
     * Can be relative to the Astro project root or absolute.
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
     * Custom slug / entry ID generator.
     * By default, strips `.tmt` / `.tm` extension and uses the relative POSIX path.
     */
    generateId?: (params: {
        relPath: string;
        absPath: string;
    }) => string;
    /**
     * Custom filter to include or exclude paths.
     * Return `false` to skip a file.
     */
    filter?: (relPath: string) => boolean;
    /**
     * URL prefix prepended to resolved `ref:` links (default: `'/docs'`).
     */
    urlPrefix?: string;
    /**
     * URL prefix prepended to resolved image/asset `@embed` targets (default: `'/vault'`).
     */
    assetPrefix?: string;
    /**
     * Prefix prepended to `sourcePath` for each entry (default: `'docs'`).
     * Set to empty string `''` to use the relative path directly from `base`.
     */
    sourcePathPrefix?: string;
    /**
     * Optional path to the workspace configuration file (defaults to `default.config.tmt` under `base`).
     */
    configPath?: string;
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