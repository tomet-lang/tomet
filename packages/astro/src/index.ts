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
import type { Loader, LoaderContext } from "astro/loaders";
import { createHash } from "node:crypto";
import { existsSync, promises as fs } from "node:fs";
import { createRequire } from "node:module";
import {
  basename,
  dirname,
  isAbsolute,
  join,
  normalize,
  relative,
  resolve,
} from "node:path";
import { fileURLToPath } from "node:url";
import initWasm, {
  Vault,
  processFile,
  vaultRegistry,
  type Declaration,
  type Diagnostic,
  type ProcessOptions,
  type ProcessedFile,
  type TocItem,
  type VaultSources,
} from "@tomet/wasm";

export type { Diagnostic, ProcessOptions, ProcessedFile, TocItem };

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
export async function renderTomet(
  source: string,
  options?: ProcessOptions,
): Promise<string> {
  await ensureWasm();
  const res = processFile(source, options);
  return res.html;
}

/**
 * Process a single `.tmt` source string to a `ProcessedFile` in-process.
 */
export async function processTomet(
  source: string,
  options?: ProcessOptions,
): Promise<ProcessedFile> {
  await ensureWasm();
  return processFile(source, options);
}

/**
 * The kind a document declares with its `::kind(...)`, read from the declarations of
 * `ProcessedFile.header`: the text of the first unnamed entry of its arguments, or `null`
 * if the document declares none (or declares one that is not written as a text).
 */
export function declaredKind(header: readonly Declaration[]): string | null {
  const kind = header.find(
    (declaration) =>
      declaration.name.namespace === null && declaration.name.name === "kind",
  );
  const first = kind?.data.args?.entries.find((entry) => entry.key === null);
  const scalar =
    first !== undefined && "Scalar" in first.value ? first.value.Scalar : null;
  return scalar !== null && typeof scalar === "object" && "Text" in scalar
    ? scalar.Text
    : null;
}

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
  sourceUrl?: { file: string; dir: string };
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
export class TometLoaderError extends Error {
  readonly problems: readonly Problem[];

  constructor(collection: string, problems: readonly Problem[]) {
    const files = new Set(problems.map((p) => p.path)).size;
    const lines = problems.map((p) => `  ${describe(p)}`);
    super(
      `${problems.length} problem(s) in ${files} file(s) of the "${collection}" collection:\n${lines.join("\n")}`,
    );
    this.name = "TometLoaderError";
    this.problems = problems;
    // The message is the whole report; a stack through the loader adds only noise to it.
    this.stack = `${this.name}: ${this.message}`;
  }
}

function describe(problem: Problem): string {
  const at =
    problem.line === null
      ? problem.path
      : `${problem.path}:${problem.line}:${problem.column ?? 1}`;
  return `${at}: ${problem.severity} ${problem.code}: ${problem.message}`;
}

/**
 * Resolves a raw asset reference (e.g. `@link(ref:+hash.png)`, `+hash.png`, `https://...`)
 * to a browser-accessible URL.
 */
export function resolveAssetUrl(
  raw: unknown,
  fileRelDir: string,
  assetMap: Map<string, string>,
  allFilesSet: Set<string>,
  assetPrefix: string,
): string | null {
  if (!raw || typeof raw !== "string") return null;
  const s = raw.trim();
  if (!s) return null;

  // 1. External URL (or markdown link [caption](https://...))
  const urlMatch = s.match(/https?:\/\/[^\s)"]+/);
  if (urlMatch) return urlMatch[0];

  // 2. Extract asset target
  let target = s;
  const refMatch = s.match(/ref:@?"?([^")]+)"?/);
  if (refMatch) {
    target = refMatch[1];
  } else {
    const linkMatch = s.match(/@link\("?([^")]+)"?\)/);
    if (linkMatch) {
      target = linkMatch[1];
    }
  }

  target = target.replace(/^@/, "").trim();
  const targetBase = basename(target);
  const prefix = assetPrefix.replace(/\/$/, "");

  // 3. Check document's own folder first (e.g. dir/-/+hash.png or dir/+hash.png)
  const localDash = fileRelDir
    ? `${fileRelDir}/-/${targetBase}`
    : `-/${targetBase}`;
  if (allFilesSet.has(localDash)) {
    return `${prefix}/${localDash}`;
  }
  const localSame = fileRelDir ? `${fileRelDir}/${targetBase}` : targetBase;
  if (allFilesSet.has(localSame)) {
    return `${prefix}/${localSame}`;
  }

  // 4. Global asset map lookup
  if (assetMap.has(targetBase)) {
    const found = assetMap.get(targetBase)!;
    return `${prefix}/${found.replace(/^\//, "")}`;
  }

  // 5. Relative path check
  if (target.startsWith("./") || target.startsWith("../")) {
    const norm = normalize(join(fileRelDir, target)).replace(/\\/g, "/");
    return `${prefix}/${norm.replace(/^\//, "")}`;
  }

  return null;
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
  const workers = new Array(Math.min(limit, items.length))
    .fill(null)
    .map(async () => {
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
async function scanVault(
  dir: string,
  baseDir: string = dir,
): Promise<ScannedVault> {
  if (!existsSync(dir)) return { allFiles: [], docFiles: [] };
  const entries = await fs.readdir(dir, { withFileTypes: true });
  const allFiles: string[] = [];
  const docFiles: { absPath: string; relPath: string }[] = [];

  for (const entry of entries) {
    const name = entry.name;
    // Skip hidden files, system files, and dependencies
    if (name.startsWith(".") || name === "node_modules" || name === "dist")
      continue;

    const fullPath = join(dir, name);
    if (entry.isDirectory()) {
      const sub = await scanVault(fullPath, baseDir);
      allFiles.push(...sub.allFiles);
      docFiles.push(...sub.docFiles);
    } else if (entry.isFile()) {
      const relPath = relative(baseDir, fullPath).split("\\").join("/");
      allFiles.push(relPath);
      if (name.endsWith(".tmt") || name.endsWith(".tm")) {
        docFiles.push({ absPath: fullPath, relPath });
      }
    }
  }

  return { allFiles, docFiles };
}

/**
 * What identifies the renderer: a hash of the WebAssembly module. The version of the package says
 * nothing (it does not change from one revision of Tomet to the next), the bytes do.
 */
let rendererKey: Promise<string> | null = null;
function rendererIdentity(): Promise<string> {
  rendererKey ??= (async () => {
    const wasm = createRequire(import.meta.url).resolve("@tomet/wasm/wasm");
    return createHash("sha256")
      .update(await fs.readFile(wasm))
      .digest("hex");
  })();
  return rendererKey;
}

/** A line and column out of the `2:5: expected ')'` a document that does not parse is thrown as. */
function parseProblem(path: string, err: unknown): Problem {
  const text = (err as Error)?.message ?? String(err);
  const at = /^(\d+):(\d+): ([\s\S]*)$/.exec(text);
  return {
    path,
    line: at ? Number(at[1]) : null,
    column: at ? Number(at[2]) : null,
    severity: "error",
    code: "parse-error",
    message: at ? at[3] : text,
  };
}

/** The problems a document's diagnostics are, in the file they are in. */
function problemsOf(path: string, diagnostics: Diagnostic[]): Problem[] {
  return diagnostics.map((d) => ({
    path,
    line: d.start?.line ?? null,
    column: d.start?.column ?? null,
    severity: d.severity,
    code: d.code,
    message: d.message,
  }));
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
export function tometLoader(options: TometLoaderOptions): Loader {
  const {
    base,
    advanced = true,
    concurrency = 32,
    filter,
    urlPrefix = "/docs",
    assetPrefix = "/vault",
    sourcePathPrefix = "docs",
    vault: vaultConfig,
    basePath = "",
    sourceUrl,
  } = options;

  return {
    name: "@tomet/astro-loader",
    load: async ({
      collection,
      config,
      store,
      logger,
      generateDigest,
      watcher,
    }: LoaderContext) => {
      await ensureWasm();
      const strict = options.strict ?? watcher === undefined;

      const rootDir =
        config.root instanceof URL
          ? fileURLToPath(config.root)
          : String(config.root);
      const baseDir = isAbsolute(base) ? base : resolve(rootDir, base);
      const display = (absPath: string): string => {
        const rel = relative(rootDir, absPath).split("\\").join("/");
        return rel.startsWith("..") ? absPath : rel;
      };

      /** Everything a load is made from: made again whenever the files of the vault change. */
      type ReadyState = {
        ok: true;
        vault: Vault;
        vaultKey: string;
        files: Set<string>;
        docs: { absPath: string; relPath: string }[];
        assetMap: Map<string, string>;
        /** The files whose change changes the vault itself: its config and what it registers. */
        parts: string[];
      };

      type FailedState = {
        ok: false;
        files: Set<string>;
        /** The files whose change changes the vault itself: its config and what it registers. */
        parts: string[];
        /** Problems of the collection as a whole, not of a document. */
        problems: Problem[];
      };

      type State = ReadyState | FailedState;

      const configPath = vaultConfig
        ? isAbsolute(vaultConfig)
          ? vaultConfig
          : resolve(rootDir, vaultConfig)
        : join(baseDir, "vault.config.tmt");

      async function build(): Promise<State> {
        const problems: Problem[] = [];
        const collectionProblem = (path: string, code: string, message: string) =>
          problems.push({
            path,
            line: null,
            column: null,
            severity: "error",
            code,
            message,
          });

        const baseExists = existsSync(baseDir);
        if (!baseExists) {
          collectionProblem(
            display(baseDir),
            "missing-base",
            "the directory of the documents does not exist",
          );
        }
        const scanned = await scanVault(baseDir);
        const docs = filter
          ? scanned.docFiles.filter((f) => filter(f.relPath))
          : scanned.docFiles;
        if (docs.length === 0 && baseExists) {
          collectionProblem(
            display(baseDir),
            "no-documents",
            "no document of the directory is published: the collection would be empty",
          );
        }

        const sources: VaultSources = {
          files: scanned.allFiles,
          documents: docs.map((d) => d.relPath),
          urlPrefix,
          assetPrefix,
          basePath,
          sourceUrl,
        };
        const parts: string[] = [];
        const partsText: string[] = [];
        if (existsSync(configPath)) {
          parts.push(configPath);
          let text: string | undefined;
          try {
            text = await fs.readFile(configPath, "utf8");
          } catch (err) {
            collectionProblem(
              display(configPath),
              "unreadable",
              `cannot be read: ${(err as Error).message}`,
            );
          }
          if (text !== undefined) {
            sources.config = text;
            partsText.push(text);
            const configDir = dirname(configPath);
            try {
              const registry = vaultRegistry(text);
              const read = async (path: string): Promise<string | undefined> => {
                const file = resolve(configDir, path);
                parts.push(file);
                try {
                  const body = await fs.readFile(file, "utf8");
                  partsText.push(body);
                  return body;
                } catch (err) {
                  collectionProblem(
                    display(file),
                    "unreadable",
                    `registered by ${display(configPath)} but cannot be read: ${(err as Error).message}`,
                  );
                  return undefined;
                }
              };
              sources.vocabularies = {};
              for (const path of registry.vocabularies) {
                const body = await read(path);
                if (body !== undefined) sources.vocabularies[path] = body;
              }
              sources.dictionaries = {};
              for (const path of registry.dictionaries) {
                const body = await read(path);
                if (body !== undefined) sources.dictionaries[path] = body;
              }
              // A `.ctmt` blueprint is the schema of a data document, not of a `.tmt`.
              sources.blueprints = [];
              for (const path of registry.blueprints) {
                if (!path.endsWith(".ctmt")) {
                  const body = await read(path);
                  if (body !== undefined) sources.blueprints.push(body);
                }
              }
            } catch (err) {
              collectionProblem(
                display(configPath),
                "invalid-vault",
                (err as Error).message ?? String(err),
              );
            }
          }
        } else if (vaultConfig) {
          parts.push(configPath);
          collectionProblem(
            display(configPath),
            "missing-vault-config",
            "the vault config does not exist",
          );
        }

        const files = new Set(scanned.allFiles);
        if (problems.length > 0) {
          return { ok: false, files, parts, problems };
        }

        let vault: Vault;
        try {
          vault = new Vault(sources);
        } catch (err) {
          collectionProblem(
            display(configPath),
            "invalid-vault",
            (err as Error).message ?? String(err),
          );
          return { ok: false, files, parts, problems };
        }

        const assetMap = new Map<string, string>();
        for (const f of scanned.allFiles) {
          const b = basename(f);
          if (!assetMap.has(b)) assetMap.set(b, f);
        }

        // What an entry was rendered with besides its own text: the files of the vault (a link
        // depends on which exist, not on the bytes of the file it is in), what the vault is made
        // of, the renderer, and how it was asked to render.
        const vaultKey = createHash("sha256")
          .update(
            JSON.stringify({
              files: [...scanned.allFiles].sort(),
              documents: [...(sources.documents ?? [])].sort(),
              parts: partsText,
              renderer: await rendererIdentity(),
              advanced,
              urlPrefix,
              assetPrefix,
              basePath,
              sourceUrl,
              sourcePathPrefix,
            }),
          )
          .digest("hex");

        return {
          ok: true,
          vault,
          vaultKey,
          files,
          docs,
          assetMap,
          parts,
        };
      }

      let state = await build();

      /** Reads one document into the store and says what is wrong with it. */
      async function loadEntry(
        file: { absPath: string; relPath: string },
        current: ReadyState,
      ): Promise<Problem[]> {
        const id = file.relPath.replace(/\.(tmt|tm)$/, "");
        const path = display(file.absPath);
        const problem = (code: string, message: string): Problem[] => [
          { path, line: 1, column: 1, severity: "error", code, message },
        ];

        let content: string;
        try {
          content = await fs.readFile(file.absPath, "utf8");
        } catch (err) {
          store.delete(id);
          return problem("unreadable", (err as Error).message);
        }
        // A placeholder that says nothing is not a page: it is a document that is missing.
        if (content.trim() === "") {
          store.delete(id);
          return problem("empty-document", "the document is empty");
        }

        const digest = generateDigest({ content, vault: current.vaultKey });
        const existing = store.get(id);
        if (existing && existing.digest === digest) {
          // Unchanged: cache hit. What was found is still found.
          const found = (existing.data as TometEntryData).diagnostics ?? [];
          return problemsOf(path, found);
        }

        let processed: ProcessedFile;
        try {
          processed = current.vault.processFile(content, {
            advanced,
            numberHeadings: advanced,
            autoSlugHeadings: advanced,
            currentPath: file.relPath,
          });
        } catch (err) {
          // A document that does not parse has no page; one that did before must not stay.
          store.delete(id);
          return [parseProblem(path, err)];
        }

        // `meta` is the plain-data entries of `::meta(...)`: an object, or null without a `::meta`.
        // What is not data (`icon: @doc.icon(...)`) is in `processed.header`.
        const metaObj: Record<string, unknown> | null = processed.meta;

        const fallbackTitle = basename(file.relPath).replace(/\.(tmt|tm)$/, "");
        const title = processed.title ?? fallbackTitle;
        const section = file.relPath.split("/")[0] ?? "";

        const kind = declaredKind(processed.header);

        // Resolve banner and images
        const fileRelDir = dirname(file.relPath).replace(/\\/g, "/");

        let rawBanner = metaObj?.banner;
        if (Array.isArray(rawBanner)) rawBanner = rawBanner[0];
        const banner = resolveAssetUrl(
          rawBanner,
          fileRelDir,
          current.assetMap,
          current.files,
          assetPrefix,
        );

        const rawBannerY = metaObj?.["banner-y"] ?? metaObj?.bannerY;
        const bannerY =
          typeof rawBannerY === "number"
            ? rawBannerY
            : rawBannerY
              ? parseInt(String(rawBannerY), 10)
              : null;

        let rawImages: unknown[] = [];
        if (Array.isArray(metaObj?.images)) {
          rawImages = metaObj.images;
        } else if (typeof metaObj?.images === "string") {
          rawImages = [metaObj.images];
        } else if (metaObj?.image) {
          rawImages = Array.isArray(metaObj.image)
            ? metaObj.image
            : [metaObj.image];
        }

        const images = rawImages
          .map((img) =>
            resolveAssetUrl(
              img,
              fileRelDir,
              current.assetMap,
              current.files,
              assetPrefix,
            ),
          )
          .filter((u): u is string => Boolean(u));

        let thumbnail: string | null = images[0] ?? banner ?? null;
        if (!thumbnail && processed.html) {
          const match = processed.html.match(/<img[^>]+src=["']([^"']+)["']/i);
          if (match) {
            thumbnail = match[1];
          }
        }

        let description = (metaObj?.description as string) ?? null;
        if (!description && processed.html) {
          const pMatch = processed.html.match(/<p>([\s\S]*?)<\/p>/i);
          if (pMatch) {
            const rawSnippet = pMatch[1].replace(/<[^>]+>/g, "").trim();
            if (rawSnippet) {
              description =
                rawSnippet.length > 200
                  ? `${rawSnippet.slice(0, 197)}...`
                  : rawSnippet;
            }
          }
        }

        let tags: string[] | undefined;
        if (Array.isArray(metaObj?.tags)) {
          tags = metaObj.tags.map(String);
        } else if (typeof metaObj?.tags === "string") {
          tags = metaObj.tags
            .split(",")
            .map((t) => t.trim())
            .filter(Boolean);
        }

        const rawDate =
          metaObj?.date ?? metaObj?.created ?? metaObj?.publishDate;
        const date = rawDate ? String(rawDate) : null;

        const sourcePath = sourcePathPrefix
          ? `${sourcePathPrefix.replace(/\/$/, "")}/${file.relPath}`
          : file.relPath;

        const entryData: TometEntryData = {
          ...(metaObj ?? {}),
          title,
          slug: id,
          section,
          sourcePath,
          toc: processed.toc ?? [],
          meta: metaObj,
          kind,
          isDataOnly: processed.is_data_only,
          diagnostics: processed.diagnostics,
          description,
          tags,
          date,
          banner,
          bannerY,
          images,
          thumbnail,
        };

        store.set({
          id,
          data: entryData,
          body: processed.is_data_only ? content : "",
          rendered: {
            html: processed.html,
          },
          filePath: relative(rootDir, file.absPath).split("\\").join("/"),
          digest,
        });
        return problemsOf(path, processed.diagnostics);
      }

      /** Reports problems in the dev server: logged, never thrown. */
      const log = (problems: Problem[]) => {
        for (const p of problems) {
          (p.severity === "error" ? logger.error : logger.warn)(describe(p));
        }
      };

      async function loadAll(current: State): Promise<void> {
        if (!current.ok) {
          if (strict) throw new TometLoaderError(collection, current.problems);
          log(current.problems);
          return;
        }

        logger.info(`Reading Tomet documents in ${baseDir}...`);
        const untouched = new Set(store.keys());
        for (const file of current.docs) {
          untouched.delete(file.relPath.replace(/\.(tmt|tm)$/, ""));
        }
        const found = await mapConcurrent(current.docs, concurrency, (file) =>
          loadEntry(file, current),
        );
        // Entries of documents that are gone, or no longer published.
        for (const gone of untouched) store.delete(gone);

        const problems = found.flat().sort(
          (a, b) =>
            a.path.localeCompare(b.path) ||
            (a.line ?? 0) - (b.line ?? 0) ||
            (a.column ?? 0) - (b.column ?? 0),
        );
        logger.info(
          `Loaded ${store.keys().length} Tomet documents` +
            (problems.length > 0 ? ` with ${problems.length} problem(s).` : "."),
        );
        if (problems.length > 0) {
          if (strict) throw new TometLoaderError(collection, problems);
          log(problems);
        }
      }

      await loadAll(state);

      // Watcher for dev mode
      if (watcher) {
        watcher.add(baseDir);
        const watchedParts = new Set<string>();
        const syncWatchedParts = (parts: readonly string[]) => {
          for (const part of parts) {
            const abs = resolve(part);
            if (!watchedParts.has(abs)) {
              watchedParts.add(abs);
              watcher.add(part);
            }
          }
        };
        syncWatchedParts(state.parts);

        const isPart = (filePath: string) => {
          const abs = resolve(filePath);
          return state.parts.some((p) => resolve(p) === abs);
        };
        const isDoc = (path: string) =>
          path.endsWith(".tmt") || path.endsWith(".tm");
        const reload = async () => {
          try {
            state = await build();
            syncWatchedParts(state.parts);
            await loadAll(state);
          } catch (err) {
            logger.error(`Failed to reload Tomet documents: ${err}`);
          }
        };

        const handleChange = async (changedPath: string) => {
          if (isPart(changedPath)) {
            logger.info(
              "The vault config or a file it registers changed, reloading...",
            );
            return reload();
          }
          if (!changedPath.startsWith(baseDir)) return;
          const relPath = relative(baseDir, changedPath).split("\\").join("/");
          // A file that was not there changes what links can reach: the vault is made again.
          if (!state.files.has(relPath)) return reload();
          if (!state.ok) {
            log(state.problems);
            return;
          }
          if (!isDoc(changedPath)) return;
          const file = state.docs.find((d) => d.relPath === relPath);
          if (!file) return;
          log(await loadEntry(file, state));
        };

        const handleUnlink = async (deletedPath: string) => {
          if (isPart(deletedPath) || deletedPath.startsWith(baseDir)) {
            await reload();
          }
        };

        watcher.on("add", handleChange);
        watcher.on("change", handleChange);
        watcher.on("unlink", handleUnlink);
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
    name: "@tomet/astro",
    hooks: {},
  };
}
