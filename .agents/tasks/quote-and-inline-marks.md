# `blockquote` を `quote` にして、インラインの装飾を取り違えるのをやめる

**状態:** 未着手。方針は作者と合意済み（2026-09-08）。規範層の編集は作者の手が要る。

## きっかけ

`blockquote` という名前は Markdown / HTML から輸入したもので、**Tomet の中では
`block` が何とも対比していない**。

- CommonMark の引用構文は `>` だけ。インライン引用は存在せず、`"..."` はただの文字
- HTML には `<q>` があるので `<blockquote>` の `block` が意味を持つ
- Tomet には `<q>` に当たるものが無い

そして毎回 `block` と書くのが単純に苦。

## 1. `blockquote` を廃し、`quote` を足す

display は **どちらでも**。位置が決める、という Tomet の作法そのもの。

```tmt
@quote[ 引用 ]                 // 行を占有 → ブロック
これは @quote[ 引用 ] です。    // 地の文の中 → インライン
```

機械側は既に対応している。`required_shape`
(`crates/tomet-semantics/src/kind.rs:430`) が `None` を返せば「どちらでも」で、
`Link | Embed | Icon | File | Dir` と `Draft | Fixme` が既にそう。
`Quote => return None` の 1 行。

### 触るもの

`blockquote` は 105 箇所・20 ファイル超（`tests/ref/` を除く）。重いのは:

| 場所 | 内容 |
| --- | --- |
| `docs/spec/builtin-elements.tmt` | **規範層。作者のもの** |
| `docs/spec/builtin-settings.tmt` | `format.blockquote.always_newline` |
| `default.config.tmt` | 同上のキー |
| `crates/tomet-config` | `always_newline` フィールドの持ち主 |
| `crates/tomet-semantics/src/kind.rs` | `ElementKind::Blockquote`、`as_str`、`required_shape` |
| `crates/tomet-convert-{html,markdown,typst,pandoc}` | 4 つの writer |
| `crates/tomet-format-printer` | |
| `apps/lsp`、`apps/web/static/app.js` | |

実際に `@blockquote` を書いている文書は 5 つ:
`docs/guide/cheatsheet.tmt`（2 箇所）、`tests/fixtures/cheatsheet.tmt`、
`tests/fixtures/syntax/pipe.tmt`、`docs/spec/builtin-elements.tmt`、
`docs/design/ideas/idea.tmt`。

### 決めること

- **設定キーの名前。** `format.blockquote.always_newline` →
  `format.quote.always_newline` か。旧名を受け付ける移行期間を置くか。
- **インライン引用の出力。** HTML は `<q>`、Pandoc は `Quoted`、
  CommonMark には無い（引用符を文字として出すしかない）。Typst は要調査。
- **旧名の扱い。** `@blockquote` を書いた文書をどうするか。
  `tomet refactor` に一手足すのが素直だが、それ自体が判断。

## 2. `from_pandoc` がインラインの装飾を `@mark` に潰している

`crates/tomet-convert-pandoc/src/from_pandoc.rs`

```rust
Inline::Strikeout(inner) => inline_element("mark", ...)   // :327
Inline::SmallCaps(inner) => inline_element("mark", ...)   // :330
Inline::Quoted(_, inner) => inline_element("mark", ...)   // :332
```

対応要素が無い 3 つを、いちばん近い builtin に寄せている。`mark` は builtin
なので**文書は validate に通る**。だから誰も気づかない。実測:

```
Quoted(DoubleQuote, [Str "引用"])  →  @mark[引用]
```

`to_pandoc` に `Quoted` は無いので、出るときは `Span.mark` になり引用符も残らない。

### 決まったこと

- **`Quoted` → `quote`。** 1 が入れば素直に解決する。
- **`Strikeout` → `strikeout` を新設。** 意味を運ぶ（消した・済んだ・撤回した）。
  GFM の `~~x~~` があるので Markdown 取り込みで今も落ちている
  （`tomet-convert-markdown` は Strikethrough を扱っていない）。
  `==x==` → `mark` が既に非 CommonMark の糖衣なので、`~~x~~` → `strikeout` は
  同じ手。文字の群（`em` `strong` `mark` `icon`）に並ぶ。
- **`SmallCaps` は作らない。** 意味を運ばず、字の描き方の指定でしかない。
  Markdown に糖衣が無いので手で書く人がいない。素の内容に落とし、
  `from_pandoc.rs` のモジュール doc の損失一覧に 1 行足す。

## 順序

1. `quote` を足し `blockquote` を廃す（規範層の編集は作者）
2. `strikeout` を足す（`~~x~~` の糖衣込み）
3. `from_pandoc` の 3 分岐を直す（1 と 2 が入っていれば素直）
4. `tomet-convert-markdown` が `~~x~~` を取り込むようにする

3 は 1・2 が無くても部分的に直せる（`Quoted` を引用符の文字に落とすだけでも
`@mark` の嘘は消える）が、順番どおりにやるほうが手戻りが無い。
