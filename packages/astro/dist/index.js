import { existsSync, promises as fs } from 'node:fs';
import { basename, isAbsolute, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import initWasm, { processDocument, } from '@tomet/tomet-wasm';
let wasmReady = null;
/**
 * Ensures the WebAssembly engine is loaded and initialized.
 */
export async function ensureWasm() {
    if (wasmReady)
        return wasmReady;
    wasmReady = (async () => {
        await initWasm();
    })();
    return wasmReady;
}
/**
 * Render a single `.tmt` source string to an HTML body string in-process.
 */
export async function renderTomet(source, options) {
    await ensureWasm();
    const res = processDocument(source, options);
    return res.html;
}
/**
 * Process a single `.tmt` source string to a `ProcessedDoc` in-process.
 */
export async function processTomet(source, options) {
    await ensureWasm();
    return processDocument(source, options);
}
/**
 * Concurrency-limited promise pool runner.
 */
async function mapConcurrent(items, limit, fn) {
    const results = new Array(items.length);
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
 * Fast recursive vault scanner discovering both document files (.tmt/.tm)
 * and all static asset files (.png/.jpg/.svg/...) for link and embed resolution.
 */
async function scanVault(dir, baseDir = dir) {
    if (!existsSync(dir))
        return { allFiles: [], docFiles: [] };
    const entries = await fs.readdir(dir, { withFileTypes: true });
    const allFiles = [];
    const docFiles = [];
    for (const entry of entries) {
        const name = entry.name;
        // Skip hidden files, system files, and dependencies
        if (name.startsWith('.') || name === 'node_modules' || name === 'dist')
            continue;
        const fullPath = join(dir, name);
        if (entry.isDirectory()) {
            const sub = await scanVault(fullPath, baseDir);
            allFiles.push(...sub.allFiles);
            docFiles.push(...sub.docFiles);
        }
        else if (entry.isFile()) {
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
export function tometLoader(options) {
    const { base, advanced = true, concurrency = 32, generateId, filter, urlPrefix = '/docs', assetPrefix = '/vault', } = options;
    return {
        name: '@tomet/astro-loader',
        load: async ({ config, store, logger, generateDigest, watcher }) => {
            await ensureWasm();
            const rootDir = config.root instanceof URL ? fileURLToPath(config.root) : String(config.root);
            const baseDir = isAbsolute(base) ? base : resolve(rootDir, base);
            if (!existsSync(baseDir)) {
                logger.warn(`Tomet loader base directory not found: ${baseDir}`);
                return;
            }
            logger.info(`Scanning Tomet documents and assets in ${baseDir}...`);
            const scanned = await scanVault(baseDir);
            const allFiles = scanned.docFiles;
            const matchedFiles = filter ? allFiles.filter((f) => filter(f.relPath)) : allFiles;
            const vaultFiles = scanned.allFiles;
            const untouchedIds = new Set(store.keys());
            async function processFile(file) {
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
                    const processed = processDocument(content, {
                        advanced,
                        numberHeadings: advanced,
                        autoSlugHeadings: advanced,
                        vaultFiles,
                        currentPath: file.relPath,
                        urlPrefix,
                        assetPrefix,
                    });
                    const fallbackTitle = basename(file.relPath).replace(/\.(tmt|tm)$/, '');
                    const title = processed.title ?? fallbackTitle;
                    const section = file.relPath.split('/')[0] ?? '';
                    const entryData = {
                        title,
                        slug: id,
                        section,
                        sourcePath: `docs/${file.relPath}`,
                        toc: processed.toc ?? [],
                        meta: processed.meta ?? null,
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
                }
                catch (err) {
                    const msg = err?.message || String(err);
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
                const handleUpdate = async (changedPath) => {
                    if (!changedPath.endsWith('.tmt') && !changedPath.endsWith('.tm'))
                        return;
                    if (changedPath.startsWith(baseDir)) {
                        const relPath = relative(baseDir, changedPath).split('\\').join('/');
                        if (filter && !filter(relPath))
                            return;
                        await processFile({ absPath: changedPath, relPath });
                    }
                };
                const handleUnlink = async (deletedPath) => {
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
/**
 * Registers the Tomet integration with Astro.
 */
export default function tomet(_options = {}) {
    return {
        name: '@tomet/astro',
        hooks: {},
    };
}
//# sourceMappingURL=index.js.map