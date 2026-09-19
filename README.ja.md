${macro.generated\_by(self.path)}

> [!warning]
> Tometは現在開発中であり、日々構文や仕様が大きく変わっています。
> 試す際には、バックアップを取り実運用をしないでください。

# Tomet

[![license](https://img.shields.io/github/license/tomet-lang/tomet?style=flat-square&color=brightgreen)](https://github.com/tomet-lang/tomet)
[![downloads](https://img.shields.io/github/downloads/tomet-lang/tomet/total?style=flat-square&color=orange)](https://github.com/tomet-lang/tomet/releases)
[![npm](https://img.shields.io/npm/v/@tomet/wasm?style=flat-square&color=blue)](https://www.npmjs.com/package/@tomet/wasm)

Tome to me!!

## What is tomet?

1. 人間工学的見やすさ。
2. パーサーにも優しい。
3. 拡張性を担保
4. key:valueを重視
5. 私は、研究目的。

## なぜtometを使用するのか？

1. 非曖昧な構文
2. リンクと変数に特化。
3. 統一された記法
4. ふりがなが簡単
5. リテラルを保持

> [!note]
> MDXやAstroコンポーネントのように、文書の中に型付きデータやコンポーネントを埋め込みたいエンジニア向け
> ObsidianのDataviewやBasesのように、メタデータとノートを高度に連携させたいKB作成向け

**vs json**

---

- key:value構造で張り合える。
- tmtは、人間ファーストであり、改行ありの長文を挿入するのに長けている。
- 深いネストには弱い。タイムライン形式は得意なんじゃないかと考える。

**vs markdown**

---

- key:valueをブロックに作成可能
- テンプレート機能
- 厳密
- Markdownのように簡潔にかけない。記号が多い

**vs xml/html**

---

- 文字が少ない
- 深いネストを作成しない

**vs typst**

---

- テキストファースト
- PDF無理
- HTML/Markdown、他の記法を組み込める。
- <mark>文書を美しく組版するための言語</mark>
- 意味を持った構造を記述し、それを様々な方法で消費できる言語
- データとしての読み書き可能。

**vs mdx**

---

- Javascript非依存

## 構造上の弱点

1. 視覚的ノイズとタイピング摩擦。記号の渋滞 (LSP機能を大切に)
2. Markdown非完全互換

# Supported Editors

- Helix
- Neovim
- VSCode
- Zed

**Community Support**

---

