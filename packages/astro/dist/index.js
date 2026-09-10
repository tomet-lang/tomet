import { existsSync, promises as fs } from 'node:fs';
import { basename, dirname, isAbsolute, join, normalize, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import initWasm, { processDocument, setVaultFiles, } from '@tomet/tomet-wasm';
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
 * Resolves a raw asset reference (e.g. `@link(ref:+hash.png)`, `+hash.png`, `https://...`)
 * to a browser-accessible URL.
 */
export function resolveAssetUrl(raw, fileRelDir, assetMap, allFilesSet, assetPrefix) {
    if (!raw || typeof raw !== 'string')
        return null;
    const s = raw.trim();
    if (!s)
        return null;
    // 1. External URL (or markdown link [caption](https://...))
    const urlMatch = s.match(/https?:\/\/[^\s)"]+/);
    if (urlMatch)
        return urlMatch[0];
    // 2. Extract asset target
    let target = s;
    const refMatch = s.match(/ref:@?"?([^")]+)"?/);
    if (refMatch) {
        target = refMatch[1];
    }
    else {
        const linkMatch = s.match(/@link\("?([^")]+)"?\)/);
        if (linkMatch) {
            target = linkMatch[1];
        }
    }
    target = target.replace(/^@/, '').trim();
    const targetBase = basename(target);
    const prefix = assetPrefix.replace(/\/$/, '');
    // 3. Check document's own folder first (e.g. dir/-/+hash.png or dir/+hash.png)
    const localDash = fileRelDir ? `${fileRelDir}/-/${targetBase}` : `-/${targetBase}`;
    if (allFilesSet.has(localDash)) {
        return `${prefix}/${localDash}`;
    }
    const localSame = fileRelDir ? `${fileRelDir}/${targetBase}` : targetBase;
    if (allFilesSet.has(localSame)) {
        return `${prefix}/${localSame}`;
    }
    // 4. Global asset map lookup
    if (assetMap.has(targetBase)) {
        const found = assetMap.get(targetBase);
        return `${prefix}/${found.replace(/^\//, '')}`;
    }
    // 5. Relative path check
    if (target.startsWith('./') || target.startsWith('../')) {
        const norm = normalize(join(fileRelDir, target)).replace(/\\/g, '/');
        return `${prefix}/${norm.replace(/^\//, '')}`;
    }
    return null;
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
    const { base, advanced = true, concurrency = 32, generateId, filter, urlPrefix = '/docs', assetPrefix = '/vault', sourcePathPrefix = 'docs', configPath, } = options;
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
            // Read optional workspace default.config.tmt
            const resolvedConfigPath = configPath
                ? (isAbsolute(configPath) ? configPath : resolve(rootDir, configPath))
                : join(baseDir, 'default.config.tmt');
            let workspaceConfig = existsSync(resolvedConfigPath)
                ? await fs.readFile(resolvedConfigPath, 'utf8')
                : undefined;
            const configStat = workspaceConfig ? await fs.stat(resolvedConfigPath).catch(() => null) : null;
            let configKey = configStat ? `${configStat.mtimeMs}` : '';
            logger.info(`Scanning Tomet documents and assets in ${baseDir}...`);
            const scanned = await scanVault(baseDir);
            const allFiles = scanned.docFiles;
            const matchedFiles = filter ? allFiles.filter((f) => filter(f.relPath)) : allFiles;
            // Pre-build link resolution index once in Wasm memory
            logger.info(`Building link index for ${scanned.allFiles.length} files...`);
            setVaultFiles(scanned.allFiles);
            const allFilesSet = new Set(scanned.allFiles);
            const assetMap = new Map();
            for (const f of scanned.allFiles) {
                const b = basename(f);
                if (!assetMap.has(b)) {
                    assetMap.set(b, f);
                }
            }
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
                    // Generate digest using mtime, size, and workspace config mtime
                    const digest = generateDigest(`${stat.mtimeMs}:${stat.size}:${configKey}:v5`);
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
                        currentPath: file.relPath,
                        urlPrefix,
                        assetPrefix,
                        config: workspaceConfig,
                    });
                    let metaObj = null;
                    if (processed.meta) {
                        if (typeof processed.meta.forEach === 'function') {
                            metaObj = {};
                            processed.meta.forEach((v, k) => {
                                if (v !== undefined)
                                    metaObj[k] = v;
                            });
                        }
                        else if (typeof processed.meta === 'object') {
                            metaObj = processed.meta;
                        }
                    }
                    const fallbackTitle = basename(file.relPath).replace(/\.(tmt|tm)$/, '');
                    const title = metaObj?.title ?? fallbackTitle;
                    const section = file.relPath.split('/')[0] ?? '';
                    const kindMatch = content.match(/^@kind\(([^)]*)\)/m);
                    const kind = processed.kind ?? (kindMatch ? kindMatch[1].trim() : null);
                    // Resolve banner and images
                    const fileRelDir = dirname(file.relPath).replace(/\\/g, '/');
                    let rawBanner = metaObj?.banner;
                    if (Array.isArray(rawBanner))
                        rawBanner = rawBanner[0];
                    const banner = resolveAssetUrl(rawBanner, fileRelDir, assetMap, allFilesSet, assetPrefix);
                    const rawBannerY = metaObj?.['banner-y'] ?? metaObj?.bannerY;
                    const bannerY = typeof rawBannerY === 'number'
                        ? rawBannerY
                        : rawBannerY
                            ? parseInt(String(rawBannerY), 10)
                            : null;
                    let rawImages = [];
                    if (Array.isArray(metaObj?.images)) {
                        rawImages = metaObj.images;
                    }
                    else if (typeof metaObj?.images === 'string') {
                        rawImages = [metaObj.images];
                    }
                    else if (metaObj?.image) {
                        rawImages = Array.isArray(metaObj.image) ? metaObj.image : [metaObj.image];
                    }
                    const images = rawImages
                        .map((img) => resolveAssetUrl(img, fileRelDir, assetMap, allFilesSet, assetPrefix))
                        .filter((u) => Boolean(u));
                    let thumbnail = images[0] ?? banner ?? null;
                    if (!thumbnail && processed.html) {
                        const match = processed.html.match(/<img[^>]+src=["']([^"']+)["']/i);
                        if (match) {
                            thumbnail = match[1];
                        }
                    }
                    let description = metaObj?.description ?? null;
                    if (!description && processed.html) {
                        const pMatch = processed.html.match(/<p>([\s\S]*?)<\/p>/i);
                        if (pMatch) {
                            const rawSnippet = pMatch[1].replace(/<[^>]+>/g, '').trim();
                            if (rawSnippet) {
                                description = rawSnippet.length > 200 ? `${rawSnippet.slice(0, 197)}...` : rawSnippet;
                            }
                        }
                    }
                    let tags;
                    if (Array.isArray(metaObj?.tags)) {
                        tags = metaObj.tags.map(String);
                    }
                    else if (typeof metaObj?.tags === 'string') {
                        tags = metaObj.tags.split(',').map((t) => t.trim()).filter(Boolean);
                    }
                    const rawDate = metaObj?.date ?? metaObj?.created ?? metaObj?.publishDate;
                    const date = rawDate ? String(rawDate) : null;
                    const sourcePath = sourcePathPrefix
                        ? `${sourcePathPrefix.replace(/\/$/, '')}/${file.relPath}`
                        : file.relPath;
                    const entryData = {
                        ...(metaObj ?? {}),
                        title,
                        slug: id,
                        section,
                        sourcePath,
                        toc: processed.toc ?? [],
                        meta: metaObj,
                        kind,
                        isDataOnly: processed.is_data_only,
                        description,
                        tags,
                        date,
                        banner,
                        bannerY,
                        images,
                        thumbnail,
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
                if (workspaceConfig && existsSync(resolvedConfigPath)) {
                    watcher.add(resolvedConfigPath);
                }
                const handleUpdate = async (changedPath) => {
                    if (resolve(changedPath) === resolve(resolvedConfigPath)) {
                        logger.info('Workspace config changed, reloading Tomet files...');
                        try {
                            workspaceConfig = await fs.readFile(resolvedConfigPath, 'utf8');
                            const newStat = await fs.stat(resolvedConfigPath).catch(() => null);
                            configKey = newStat ? `${newStat.mtimeMs}` : '';
                            await mapConcurrent(matchedFiles, concurrency, processFile);
                        }
                        catch (err) {
                            logger.error(`Failed to reload config ${resolvedConfigPath}: ${err}`);
                        }
                        return;
                    }
                    if (changedPath.startsWith(baseDir)) {
                        const relPath = relative(baseDir, changedPath).split('\\').join('/');
                        // Maintain vault asset and file index
                        if (!allFilesSet.has(relPath)) {
                            allFilesSet.add(relPath);
                            scanned.allFiles.push(relPath);
                            assetMap.set(basename(relPath), relPath);
                            setVaultFiles(scanned.allFiles);
                        }
                        if (changedPath.endsWith('.tmt') && !changedPath.endsWith('.tm'))
                            return;
                        if (changedPath.endsWith('.tmt') || changedPath.endsWith('.tm')) {
                            if (filter && !filter(relPath))
                                return;
                            await processFile({ absPath: changedPath, relPath });
                        }
                    }
                };
                const handleUnlink = async (deletedPath) => {
                    if (deletedPath.startsWith(baseDir)) {
                        const relPath = relative(baseDir, deletedPath).split('\\').join('/');
                        if (allFilesSet.has(relPath)) {
                            allFilesSet.delete(relPath);
                            const idx = scanned.allFiles.indexOf(relPath);
                            if (idx !== -1)
                                scanned.allFiles.splice(idx, 1);
                            assetMap.delete(basename(relPath));
                            setVaultFiles(scanned.allFiles);
                        }
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