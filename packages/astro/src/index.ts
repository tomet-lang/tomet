/**
 * Astro integration & Content Layer Loader for Tomet.
 *
 * Provides `tometLoader` for Astro 5 Content Collections, rendering `.tmt` documents
 * in-process via `@tomet/tomet-wasm` with incremental caching designed for up to 50,000+ files.
 */
import type { AstroIntegration } from 'astro';
import type { Loader, LoaderContext } from 'astro/loaders';
import { existsSync, promises as fs } from 'node:fs';
import { createRequire } from 'node:module';
import { basename, isAbsolute, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import initWasm, {
  processDocument,
  type ProcessOptions,
  type ProcessedDoc,
  type TocItem,
} from '@tomet/tomet-wasm';

export type { ProcessOptions, ProcessedDoc, TocItem };

let wasmReady: Promise<void> | null = null;

/**
 * Ensures the WebAssembly engine is loaded and initialized.
 */
export async function ensureWasm(): Promise<void> {
  if (wasmReady) return wasmReady;
  wasmReady = (async () => {
    try {
      // In bundlers (Vite/Astro) this will load the wasm via asset import / fetch
      await initWasm();
    } catch (_err) {
      // In Node.js server/CLI contexts where URL-based loading may fail, load bytes directly
      try {
        const require = createRequire(import.meta.url);
        const wasmPath = require.resolve('@tomet/tomet-wasm/wasm');
        const wasmBytes = await fs.readFile(wasmPath);
        await initWasm({ module_or_path: wasmBytes });
      } catch (nodeErr) {
        throw new Error(
          `Failed to initialize @tomet/tomet-wasm: ${(nodeErr as Error).message}`,
          { cause: nodeErr },
        );
      }
    }
  })();
  return wasmReady;
}

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
  generateId?: (params: { relPath: string; absPath: string }) => string;
  /**
   * Custom filter to include or exclude paths.
   * Return `false` to skip a file.
   */
  filter?: (relPath: string) => boolean;
}

export interface TometEntryData {
  title: string;
  slug: string;
  section: string;
  sourcePath: string;
  toc: TocItem[];
  meta: Record<string, unknown> | null;
  isDataOnly: boolean;
  [key: string]: unknown;
}

/**
 * Concurrency-limited promise pool runner.
 */
async function mapConcurrent<T, R>(
  items: T[],
  limit: number,
  fn: (item: T) => Promise<R>,
): Promise<R[]> {
  const results: R[] = new Array(items.length);
  let idx = 0;
  const workers = new Array(Math.min(limit, items.length)).fill(null).map(async () => {
    while (idx < items.length) {
      const current = idx++;
      results[current] = await fn(items[current]);
    }
  });
  await Promise.all(workers);
  return results;
}

/**
 * Fast recursive file finder skipping hidden files, `.git`, `node_modules`, and `.writ.tmt`.
 */
async function findTmtFiles(dir: string, baseDir: string = dir): Promise<{ absPath: string; relPath: string }[]> {
  if (!existsSync(dir)) return [];
  const entries = await fs.readdir(dir, { withFileTypes: true });
  const files: { absPath: string; relPath: string }[] = [];

  for (const entry of entries) {
    const name = entry.name;
    // Skip hidden files, system files, and rules
    if (name.startsWith('.') || name === 'node_modules') continue;

    const fullPath = join(dir, name);
    if (entry.isDirectory()) {
      files.push(...(await findTmtFiles(fullPath, baseDir)));
    } else if (entry.isFile() && (name.endsWith('.tmt') || name.endsWith('.tm'))) {
      const relPath = relative(baseDir, fullPath).split('\\').join('/');
      files.push({ absPath: fullPath, relPath });
    }
  }

  return files;
}

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
export function tometLoader(options: TometLoaderOptions): Loader {
  const {
    base,
    advanced = true,
    concurrency = 32,
    generateId,
    filter,
  } = options;

  return {
    name: '@tomet/astro-loader',
    load: async ({ config, store, logger, generateDigest, watcher }: LoaderContext) => {
      await ensureWasm();

      const rootDir = config.root instanceof URL ? fileURLToPath(config.root) : String(config.root);
      const baseDir = isAbsolute(base) ? base : resolve(rootDir, base);
      if (!existsSync(baseDir)) {
        logger.warn(`Tomet loader base directory not found: ${baseDir}`);
        return;
      }

      logger.info(`Scanning Tomet documents in ${baseDir}...`);
      const allFiles = await findTmtFiles(baseDir);
      const matchedFiles = filter ? allFiles.filter((f) => filter(f.relPath)) : allFiles;

      const untouchedIds = new Set(store.keys());

      async function processFile(file: { absPath: string; relPath: string }) {
        const id = generateId
          ? generateId(file)
          : file.relPath.replace(/\.(tmt|tm)$/, '');

        untouchedIds.delete(id);

        try {
          const stat = await fs.stat(file.absPath);
          // Skip empty placeholder files
          if (stat.size === 0) {
            store.delete(id);
            return;
          }

          // Generate digest using mtime and size for instant caching
          const digest = generateDigest(`${stat.mtimeMs}:${stat.size}`);
          const existing = store.get(id);

          if (existing && existing.digest === digest) {
            // Unchanged: cache hit!
            return;
          }

          const content = await fs.readFile(file.absPath, 'utf8');
          const processed: ProcessedDoc = processDocument(content, {
            advanced,
            numberHeadings: advanced,
            autoSlugHeadings: advanced,
          });

          const fallbackTitle = basename(file.relPath).replace(/\.(tmt|tm)$/, '');
          const title = processed.title ?? fallbackTitle;
          const section = file.relPath.split('/')[0] ?? '';

          const entryData: TometEntryData = {
            title,
            slug: id,
            section,
            sourcePath: `docs/${file.relPath}`,
            toc: processed.toc ?? [],
            meta: (processed.meta as Record<string, unknown>) ?? null,
            isDataOnly: processed.is_data_only,
          };

          const relToRoot = relative(rootDir, file.absPath).split('\\').join('/');

          store.set({
            id,
            data: entryData,
            body: processed.is_data_only ? content : '',
            rendered: {
              html: processed.html,
            },
            filePath: relToRoot,
            digest,
          });
        } catch (err) {
          logger.error(`Failed to process Tomet file ${file.absPath}: ${(err as Error).message}`);
        }
      }

      // Process in batches
      await mapConcurrent(matchedFiles, concurrency, processFile);

      // Clean up deleted entries
      for (const deletedId of untouchedIds) {
        store.delete(deletedId);
      }

      logger.info(`Loaded ${matchedFiles.length} Tomet documents.`);

      // Watcher for dev mode
      if (watcher) {
        watcher.add(baseDir);

        const handleUpdate = async (changedPath: string) => {
          if (!changedPath.endsWith('.tmt') && !changedPath.endsWith('.tm')) return;
          if (changedPath.startsWith(baseDir)) {
            const relPath = relative(baseDir, changedPath).split('\\').join('/');
            if (filter && !filter(relPath)) return;
            await processFile({ absPath: changedPath, relPath });
          }
        };

        const handleUnlink = async (deletedPath: string) => {
          if (deletedPath.startsWith(baseDir)) {
            const relPath = relative(baseDir, deletedPath).split('\\').join('/');
            const id = generateId
              ? generateId({ relPath, absPath: deletedPath })
              : relPath.replace(/\.(tmt|tm)$/, '');
            store.delete(id);
          }
        };

        watcher.on('add', handleUpdate);
        watcher.on('change', handleUpdate);
        watcher.on('unlink', handleUnlink);
      }
    },
  };
}

/** Options accepted by the Astro integration. */
export interface TometOptions {}

/**
 * Registers the Tomet integration with Astro.
 */
export default function tomet(_options: TometOptions = {}): AstroIntegration {
  return {
    name: '@tomet/astro',
    hooks: {},
  };
}
