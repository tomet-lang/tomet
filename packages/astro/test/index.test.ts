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
