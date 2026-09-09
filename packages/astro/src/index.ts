/**
 * Astro integration & Content Layer Loader for Tomet.
 *
 * Provides `tometLoader` for Astro 5 Content Collections, rendering `.tmt` documents
 * in-process via `@tomet/tomet-wasm` with incremental caching designed for up to 50,000+ files.
 */
import type { AstroIntegration } from 'astro';
import type { Loader, LoaderContext } from 'astro/loaders';
import { existsSync, promises as fs } from 'node:fs';
import { basename, isAbsolute, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import initWasm, {
  processDocument,
  setVaultFiles,
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
    await initWasm();
  })();
  return wasmReady;
}

/**
 * Render a single `.tmt` source string to an HTML body string in-process.
 */
export async function renderTomet(source: string, options?: ProcessOptions): Promise<string> {
  await ensureWasm();
  const res = processDocument(source, options);
  return res.html;
}

/**
 * Process a single `.tmt` source string to a `ProcessedDoc` in-process.
 */
export async function processTomet(source: string, options?: ProcessOptions): Promise<ProcessedDoc> {
  await ensureWasm();
  return processDocument(source, options);
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
  /**
   * URL prefix prepended to resolved `ref:` links (default: `'/docs'`).
   */
  urlPrefix?: string;
  /**
   * URL prefix prepended to resolved image/asset `@embed` targets (default: `'/vault'`).
   */
  assetPrefix?: string;
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

interface ScannedVault {
  allFiles: string[];
  docFiles: { absPath: string; relPath: string }[];
}

/**
 * Fast recursive vault scanner discovering both document files (.tmt/.tm)
 * and all static asset files (.png/.jpg/.svg/...) for link and embed resolution.
 */
async function scanVault(dir: string, baseDir: string = dir): Promise<ScannedVault> {
  if (!existsSync(dir)) return { allFiles: [], docFiles: [] };
  const entries = await fs.readdir(dir, { withFileTypes: true });
  const allFiles: string[] = [];
  const docFiles: { absPath: string; relPath: string }[] = [];

  for (const entry of entries) {
    const name = entry.name;
    // Skip hidden files, system files, and dependencies
    if (name.startsWith('.') || name === 'node_modules' || name === 'dist') continue;

    const fullPath = join(dir, name);
    if (entry.isDirectory()) {
      const sub = await scanVault(fullPath, baseDir);
      allFiles.push(...sub.allFiles);
      docFiles.push(...sub.docFiles);
    } else if (entry.isFile()) {
      const relPath = relative(baseDir, fullPath).split('\\').join('/');
      allFiles.push(relPath);
      if (name.endsWith('.tmt') || name.endsWith('.tm')) {
        docFiles.push({ absPath: fullPath, relPath });
      }
    }
  }

  return { allFiles, docFiles };
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
    urlPrefix = '/docs',
    assetPrefix = '/vault',
    configPath,
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

      // Read optional workspace default.config.tmt
      const resolvedConfigPath = configPath
        ? (isAbsolute(configPath) ? configPath : resolve(rootDir, configPath))
        : join(baseDir, 'default.config.tmt');
      const workspaceConfig = existsSync(resolvedConfigPath)
        ? await fs.readFile(resolvedConfigPath, 'utf8')
        : undefined;
      const configStat = workspaceConfig ? await fs.stat(resolvedConfigPath).catch(() => null) : null;
      const configKey = configStat ? `${configStat.mtimeMs}` : '';

      logger.info(`Scanning Tomet documents and assets in ${baseDir}...`);
      const scanned = await scanVault(baseDir);
      const allFiles = scanned.docFiles;
      const matchedFiles = filter ? allFiles.filter((f) => filter(f.relPath)) : allFiles;

      // Pre-build link resolution index once in Wasm memory
      logger.info(`Building link index for ${scanned.allFiles.length} files...`);
      setVaultFiles(scanned.allFiles);

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

          // Generate digest using mtime, size, and workspace config mtime
          const digest = generateDigest(`${stat.mtimeMs}:${stat.size}:${configKey}`);
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
            currentPath: file.relPath,
            urlPrefix,
            assetPrefix,
            config: workspaceConfig,
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
          const msg = (err as Error)?.message || String(err);
          logger.error(`Failed to process Tomet file ${file.absPath}: ${msg}`);
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
