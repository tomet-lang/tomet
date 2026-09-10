import { test, describe } from 'node:test';
import assert from 'node:assert/strict';
import {
  renderTomet,
  processTomet,
  resolveAssetUrl,
  tometLoader,
} from '../dist/index.js';

describe('tomet renderer in Astro integration', () => {
  test('renders basic tomet elements to HTML', async () => {
    const html = await renderTomet('#[ My Heading ]\nThis is paragraph.');
    assert.ok(html.includes('<h1'));
    assert.ok(html.includes('My Heading'));
    assert.ok(html.includes('<p>This is paragraph.</p>'));
  });

  test('processes tomet document with metadata and toc', async () => {
    const doc = await processTomet(
      '@meta{\n  title: "Test Title"\n  tags: [astro, tomet]\n  date: "2026-09-10"\n}\n\n#[ Heading 1 ]\nSome text\n\n##[ Heading 2 ]\nMore text\n\n###[ Heading 3 ]\nSub text'
    );
    assert.ok(doc.html.includes('Heading 1'));
    assert.equal(doc.title, 'Test Title');
    assert.ok(doc.toc && doc.toc.length === 2);
    assert.equal(doc.toc[0].text, 'Heading 2');
    assert.equal(doc.toc[0].level, 2);
    assert.equal(doc.toc[1].text, 'Heading 3');
    assert.equal(doc.toc[1].level, 3);
  });
});

describe('resolveAssetUrl', () => {
  const assetMap = new Map<string, string>([
    ['diagram.png', 'assets/diagram.png'],
    ['thumb.jpg', 'images/thumb.jpg'],
  ]);
  const allFilesSet = new Set<string>([
    'assets/diagram.png',
    'images/thumb.jpg',
    'guide/-/local.png',
    'guide/pic.png',
  ]);
  const assetPrefix = '/vault';

  test('resolves direct http/https URLs', () => {
    const url = resolveAssetUrl(
      'https://example.com/cover.png',
      'guide',
      assetMap,
      allFilesSet,
      assetPrefix
    );
    assert.equal(url, 'https://example.com/cover.png');
  });

  test('resolves markdown style URLs', () => {
    const url = resolveAssetUrl(
      '[Cover](https://example.com/cover.png)',
      'guide',
      assetMap,
      allFilesSet,
      assetPrefix
    );
    assert.equal(url, 'https://example.com/cover.png');
  });

  test('resolves local dash asset in same folder', () => {
    const url = resolveAssetUrl(
      'local.png',
      'guide',
      assetMap,
      allFilesSet,
      assetPrefix
    );
    assert.equal(url, '/vault/guide/-/local.png');
  });

  test('resolves same-folder asset', () => {
    const url = resolveAssetUrl(
      'pic.png',
      'guide',
      assetMap,
      allFilesSet,
      assetPrefix
    );
    assert.equal(url, '/vault/guide/pic.png');
  });

  test('resolves global asset map lookup', () => {
    const url = resolveAssetUrl(
      '@link(ref:diagram.png)',
      'other',
      assetMap,
      allFilesSet,
      assetPrefix
    );
    assert.equal(url, '/vault/assets/diagram.png');
  });

  test('resolves relative path', () => {
    const url = resolveAssetUrl(
      './images/local.jpg',
      'posts',
      assetMap,
      allFilesSet,
      assetPrefix
    );
    assert.equal(url, '/vault/posts/images/local.jpg');
  });
});

describe('tometLoader contract', () => {
  test('creates a loader with correct name', () => {
    const loader = tometLoader({ base: './non-existent' });
    assert.equal(loader.name, '@tomet/astro-loader');
    assert.equal(typeof loader.load, 'function');
  });
});
