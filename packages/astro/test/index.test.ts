import { test, describe } from "node:test";
import assert from "node:assert/strict";
import {
  renderTomet,
  processTomet,
  declaredKind,
  resolveAssetUrl,
  tometLoader,
} from "../dist/index.js";

describe("tomet renderer in Astro integration", () => {
  test("renders basic tomet elements to HTML", async () => {
    const html = await renderTomet("=[ My Heading ]\nThis is paragraph.");
    assert.ok(html.includes("<h1"));
    assert.ok(html.includes("My Heading"));
    assert.ok(html.includes("<p>This is paragraph.</p>"));
  });

  test("processes tomet document with metadata and toc", async () => {
    const doc = await processTomet(
      '::meta(\n  title: "Test Title"\n  tags: (astro, tomet)\n  date: "2026-09-10"\n)\n\n=[ Heading 1 ]\nSome text\n\n==[ Heading 2 ]\nMore text\n\n===[ Heading 3 ]\nSub text',
    );
    assert.ok(doc.html.includes("Heading 1"));
    assert.equal(doc.title, "Test Title");
    assert.ok(doc.toc && doc.toc.length === 2);
    assert.equal(doc.toc[0].text, "Heading 2");
    assert.equal(doc.toc[0].level, 2);
    assert.equal(doc.toc[1].text, "Heading 3");
    assert.equal(doc.toc[1].level, 3);
  });

  test("hands values over in their JSON shape", async () => {
    const doc = await processTomet(
      '::meta(\n  title: "T"\n  tags: (a, b)\n)\n\nText',
    );
    // A named tuple is a plain object (a Map would fail this), an unnamed one an array.
    assert.deepStrictEqual(doc.meta, { title: "T", tags: ["a", "b"] });

    // Absent is null, not undefined.
    const bare = await processTomet("Text");
    assert.strictEqual(bare.title, null);
    assert.strictEqual(bare.meta, null);
  });

  test("leaves what is not data out of meta and keeps it in the header", async () => {
    const doc = await processTomet(
      '::meta(\n  title: "T"\n  icon: @doc.icon("star")\n  mode: auto\n)\n\nText',
    );
    // `meta` holds the entries that are data; the element and `auto` are not there, and it is no error.
    assert.deepStrictEqual(doc.meta, { title: "T" });
    // The header holds the declarations as the AST, so an application can read the icon.
    assert.equal(doc.header.length, 1);
    assert.equal(doc.header[0].name.name, "meta");
    const entries = doc.header[0].data.args?.entries ?? [];
    const icon = entries.find(
      (entry) =>
        entry.key !== null && "Name" in entry.key && entry.key.Name === "icon",
    );
    assert.ok(icon && "Element" in icon.value);
    assert.equal(icon.value.Element.name.namespace, "doc");
    assert.equal(icon.value.Element.name.name, "icon");

    // A `::meta` with nothing that is data is an empty object, no `::meta` at all is null.
    const onlyIcon = await processTomet('::meta(icon: @doc.icon("star"))\n\nText');
    assert.deepStrictEqual(onlyIcon.meta, {});
    assert.deepStrictEqual((await processTomet("Text")).header, []);
  });

  test("reads the kind from the ::kind declaration of the header", async () => {
    const guide = await processTomet("::kind(guide)\n\nText");
    assert.equal(declaredKind(guide.header), "guide");
    // Not a regex over the source: a `@kind(...)` line in the text is no declaration.
    const none = await processTomet("Text\n\n@kind(guide)\n");
    assert.equal(declaredKind(none.header), null);
    assert.equal(declaredKind((await processTomet("Text")).header), null);
  });
});

describe("resolveAssetUrl", () => {
  const assetMap = new Map<string, string>([
    ["diagram.png", "assets/diagram.png"],
    ["thumb.jpg", "images/thumb.jpg"],
  ]);
  const allFilesSet = new Set<string>([
    "assets/diagram.png",
    "images/thumb.jpg",
    "guide/-/local.png",
    "guide/pic.png",
  ]);
  const assetPrefix = "/vault";

  test("resolves direct http/https URLs", () => {
    const url = resolveAssetUrl(
      "https://example.com/cover.png",
      "guide",
      assetMap,
      allFilesSet,
      assetPrefix,
    );
    assert.equal(url, "https://example.com/cover.png");
  });

  test("resolves markdown style URLs", () => {
    const url = resolveAssetUrl(
      "[Cover](https://example.com/cover.png)",
      "guide",
      assetMap,
      allFilesSet,
      assetPrefix,
    );
    assert.equal(url, "https://example.com/cover.png");
  });

  test("resolves local dash asset in same folder", () => {
    const url = resolveAssetUrl(
      "local.png",
      "guide",
      assetMap,
      allFilesSet,
      assetPrefix,
    );
    assert.equal(url, "/vault/guide/-/local.png");
  });

  test("resolves same-folder asset", () => {
    const url = resolveAssetUrl(
      "pic.png",
      "guide",
      assetMap,
      allFilesSet,
      assetPrefix,
    );
    assert.equal(url, "/vault/guide/pic.png");
  });

  test("resolves global asset map lookup", () => {
    const url = resolveAssetUrl(
      "@link(ref:diagram.png)",
      "other",
      assetMap,
      allFilesSet,
      assetPrefix,
    );
    assert.equal(url, "/vault/assets/diagram.png");
  });

  test("resolves relative path", () => {
    const url = resolveAssetUrl(
      "./images/local.jpg",
      "posts",
      assetMap,
      allFilesSet,
      assetPrefix,
    );
    assert.equal(url, "/vault/posts/images/local.jpg");
  });
});

describe("tometLoader contract", () => {
  test("creates a loader with correct name", () => {
    const loader = tometLoader({ base: "./non-existent" });
    assert.equal(loader.name, "@tomet/astro-loader");
    assert.equal(typeof loader.load, "function");
  });
});

// ---------------------------------------------------------------------------------------------
// The loader on a real directory: what it publishes, and what it refuses to publish silently.
// ---------------------------------------------------------------------------------------------

import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { createHash } from "node:crypto";
import { TometLoaderError } from "../dist/index.js";

type Entry = {
  id: string;
  digest?: string;
  data: Record<string, any>;
  rendered?: { html: string };
};

/** The pieces of Astro's `LoaderContext` the loader uses. */
function context(root: string, store = new Map<string, Entry>(), dev = false) {
  const logged: { level: string; message: string }[] = [];
  const log = (level: string) => (message: string) => logged.push({ level, message });
  const handlers: Record<string, ((path: string) => unknown)[]> = {};
  const watcher = dev
    ? {
        add() {},
        on(event: string, handler: (path: string) => unknown) {
          (handlers[event] ??= []).push(handler);
        },
      }
    : undefined;
  return {
    store,
    logged,
    handlers,
    context: {
      collection: "docs",
      config: { root: pathToFileURL(`${root}/`) },
      logger: {
        info: log("info"),
        warn: log("warn"),
        error: log("error"),
        debug: log("debug"),
      },
      generateDigest: (data: unknown) =>
        createHash("sha256")
          .update(typeof data === "string" ? data : JSON.stringify(data))
          .digest("hex"),
      store: {
        get: (id: string) => store.get(id),
        set: (entry: Entry) => {
          store.set(entry.id, entry);
          return true;
        },
        delete: (id: string) => void store.delete(id),
        keys: () => [...store.keys()],
      },
      watcher,
    } as any,
  };
}

/** A directory of documents: `files` maps a path to its text. */
function site(files: Record<string, string>): string {
  const root = mkdtempSync(join(tmpdir(), "tomet-astro-"));
  for (const [path, text] of Object.entries(files)) {
    const file = join(root, "docs", path);
    mkdirSync(join(file, ".."), { recursive: true });
    writeFileSync(file, text);
  }
  return root;
}

async function failure(load: Promise<unknown>): Promise<TometLoaderError> {
  try {
    await load;
  } catch (err) {
    assert.ok(err instanceof TometLoaderError, String(err));
    return err;
  }
  assert.fail("the load did not fail");
}

describe("tometLoader on a directory", () => {
  const broken = {
    "good.tmt": "=[ Good ]\n\nFine.\n",
    "parse.tmt": "@foo(\n=[ x",
    "empty.tmt": "\n",
    "draft.tmt": "=[ Draft ]\n\n@draft(例)\n",
    "unknown.tmt": "=[ U ]\n\n@frobnicate[ hi ]\n",
    "links.tmt": "=[ L ]\n\n@link(ref:Nowhere)[x]\n",
  };

  test("a build fails with every problem at once, each with file, line and column", async () => {
    const root = site(broken);
    const { context: ctx } = context(root);
    const error = await failure(
      tometLoader({ base: "docs" }).load(ctx),
    );
    const lines = error.message.split("\n").slice(1);
    const has = (text: string) =>
      assert.ok(
        lines.some((l) => l.includes(text)),
        `${text}\n${error.message}`,
      );
    has("docs/parse.tmt:2:5: error parse-error: expected ')'");
    has("docs/empty.tmt:1:1: error empty-document");
    has("docs/draft.tmt:3:1: warning draft");
    has("docs/unknown.tmt:3:1: error unknown-element");
    has("docs/links.tmt:3:1: error unresolved-link");
    assert.equal(error.problems.length, 5);
    assert.ok(!error.message.includes("good.tmt"));
    rmSync(root, { recursive: true });
  });

  test("the dev server logs the same problems and keeps serving", async () => {
    const root = site(broken);
    const { context: ctx, logged, store } = context(root, new Map(), true);
    await tometLoader({ base: "docs" }).load(ctx);
    const problems = logged.filter((l) => l.level === "error" || l.level === "warn");
    assert.equal(problems.length, 5);
    // What parses is served, with what is wrong with it; what does not parse and what is empty is not.
    assert.deepEqual([...store.keys()].sort(), ["draft", "good", "links", "unknown"]);
    assert.equal(store.get("draft")!.data.diagnostics[0].code, "draft");
    rmSync(root, { recursive: true });
  });

  test("an empty collection and a missing directory are problems, not an empty site", async () => {
    const root = site({ "a.tmt": "=[ A ]\n\nx\n" });
    const none = await failure(
      tometLoader({ base: "docs", filter: () => false }).load(context(root).context),
    );
    assert.match(none.message, /no-documents/);
    const missing = await failure(
      tometLoader({ base: "nowhere" }).load(context(root).context),
    );
    assert.match(missing.message, /missing-base/);
    rmSync(root, { recursive: true });
  });

  test("a link to a document the filter left out is a problem", async () => {
    const root = site({
      "a.tmt": "=[ A ]\n\n@link(ref:Hidden)[hidden]\n\n@link(ref:b)[b]\n",
      "b.tmt": "=[ B ]\n\nText.\n",
      "Hidden.tmt": "=[ Hidden ]\n\nText.\n",
    });
    const error = await failure(
      tometLoader({ base: "docs", filter: (p) => p !== "Hidden.tmt" }).load(
        context(root).context,
      ),
    );
    assert.equal(error.problems.length, 1);
    assert.match(error.problems[0].message, /not published/);
    assert.equal(error.problems[0].line, 3);
    rmSync(root, { recursive: true });
  });

  test("a cache hit reports what was found before", async () => {
    const root = site({ "a.tmt": "=[ A ]\n\n@draft(x)\n" });
    const store = new Map<string, Entry>();
    const loader = tometLoader({ base: "docs", strict: false });
    await loader.load(context(root, store).context);
    const first = store.get("a")!.digest;
    // The same store, nothing changed: the entry is not rendered again, and is still not clean.
    const again = context(root, store);
    await loader.load(again.context);
    assert.equal(store.get("a")!.digest, first);
    assert.equal(again.logged.filter((l) => l.level === "warn").length, 1);
    const error = await failure(tometLoader({ base: "docs" }).load(context(root, store).context));
    assert.match(error.message, /docs\/a\.tmt:3:1: warning draft/);
    rmSync(root, { recursive: true });
  });

  test("a document is rendered again when a file it links to is gone", async () => {
    const root = site({
      "a.tmt": "=[ A ]\n\n@link(ref:b)[b]\n",
      "b.tmt": "=[ B ]\n\nText.\n",
    });
    const store = new Map<string, Entry>();
    await tometLoader({ base: "docs" }).load(context(root, store).context);
    assert.match(store.get("a")!.rendered!.html, /href="\/docs\/b"/);
    rmSync(join(root, "docs", "b.tmt"));
    const error = await failure(
      tometLoader({ base: "docs" }).load(context(root, store).context),
    );
    assert.match(error.message, /docs\/a\.tmt:3:1: error unresolved-link/);
    rmSync(root, { recursive: true });
  });

  test("two collections never see each other's files", async () => {
    const one = site({ "a.tmt": "=[ A ]\n\n@link(ref:Note)[n]\n", "Note.tmt": "=[ N ]\n\nx\n" });
    const two = site({ "a.tmt": "=[ A ]\n\n@link(ref:Note)[n]\n", "Other.tmt": "=[ O ]\n\nx\n" });
    const [ok, bad] = await Promise.allSettled([
      tometLoader({ base: "docs" }).load(context(one).context),
      tometLoader({ base: "docs" }).load(context(two).context),
    ]);
    assert.equal(ok.status, "fulfilled");
    assert.equal(bad.status, "rejected");
    rmSync(one, { recursive: true });
    rmSync(two, { recursive: true });
  });

  test("a file: link goes to the page, to the source URL, or is a problem", async () => {
    const root = site({
      "spec/a.tmt":
        "=[ A ]\n\n@link(file:tmtroot/spec/b.tmt#sec)[page]\n\n@link(file:crates/x/lib.rs)[source]\n\n@link(dir:tmtroot/spec)[dir]\n",
      "spec/b.tmt": "=[ B ]\n\n==[ Sec ]\n\nx\n",
    });
    const store = new Map<string, Entry>();
    const sourceUrl = {
      file: "https://git.example/blob/rev/{path}",
      dir: "https://git.example/tree/rev/{path}",
    };
    await tometLoader({ base: "docs", basePath: "tmtroot", sourceUrl }).load(
      context(root, store).context,
    );
    const html = store.get("spec/a")!.rendered!.html;
    assert.match(html, /href="\/docs\/spec\/b#sec"/);
    assert.match(html, /href="https:\/\/git\.example\/blob\/rev\/crates\/x\/lib\.rs"/);
    assert.match(html, /href="https:\/\/git\.example\/tree\/rev\/tmtroot\/spec"/);

    // Without a source URL, what has no page is a problem: never a raw path.
    const error = await failure(
      tometLoader({ base: "docs", basePath: "tmtroot" }).load(context(root).context),
    );
    assert.equal(error.problems.length, 2);
    assert.match(error.problems[0].message, /source URL/);
    rmSync(root, { recursive: true });
  });

  test("the vault config gives the documents its vocabularies and dictionaries", async () => {
    const root = site({
      "a.tmt": "=[ A ]\n\n@term(tomet#config)\n",
      "vault.config.tmt": "::dictionary(tomet: @file(dict.tmt))\n",
      "dict.tmt": '::dictionary(tomet)\n\n;@term("config")[ the settings ]#(config)\n',
    });
    const loader = (vault?: string) =>
      tometLoader({
        base: "docs",
        filter: (p) => p === "a.tmt",
        ...(vault ? { vault } : {}),
      });
    // The config is under `base` by default; with it the term is known.
    await loader().load(context(root).context);
    rmSync(join(root, "docs", "vault.config.tmt"));
    rmSync(join(root, "docs", "dict.tmt"));
    const error = await failure(loader().load(context(root).context));
    assert.match(error.message, /unresolved-term/);
    rmSync(root, { recursive: true });
  });
});
