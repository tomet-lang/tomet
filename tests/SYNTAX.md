# 実装されている構文

**このファイルは生成物です。手で編集しないでください。**

出どころは `tests/src/syntax_report.rs` の表と `tomet-parser` そのもので、
`docs/spec/` ではありません。ここに載っているのは「言語がこう受け付ける
*べき*」ではなく「実装が今こう受け付ける」です。

```bash
cargo test -p tomet-tests --test syntax_report              # 検証
TOMET_UPDATE_REF=1 cargo test -p tomet-tests --test syntax_report  # 更新
```

文法を変えてこのファイルを更新し忘れると、テストが落ちます。

## `docs/spec/` との関係

`docs/spec/` は**規範的**（言語が何を受け付けるべきか）で、手書きです。
このファイルは**記述的**（実装が何を受け付けるか）で、生成物です。
両者が食い違っているとき、食い違いそのものが読み取るべき情報であって、
どちらかを黙って他方に合わせるべきではありません。

`AST` 欄は `Span` を省いた木の形です。`Span` は全ノードが持っていますが、
意味を持たないのでここには出しません。

## 目次

- [見出し](#見出し)
- [ブロック配置の要素](#ブロック配置の要素)
- [インライン配置の要素](#インライン配置の要素)
- [文字列に落ちる場合](#文字列に落ちる場合)
- [`+++` フェンス](#-フェンス)
- [`{...}` グループ](#-グループ)
- [`(args)` と値の文法](#args-と値の文法)
- [リスト](#リスト)
- [インライン記法](#インライン記法)
- [区切りとコードブロック](#区切りとコードブロック)
- [補間 `${...}`](#補間-)
- [コネクト `:`](#コネクト-)
- [コメント](#コメント)
- [撤去された構文](#撤去された構文)
- [綴りを失ったまま、代わりが未決のもの](#綴りを失ったまま、代わりが未決のもの)
- [紛らわしいが、これが正しい](#紛らわしいが、これが正しい)
- [受け付けない書き方](#受け付けない書き方)
- [仕様にあるが未実装](#仕様にあるが未実装)

## 見出し

### `#[ ... ]` は見出し専用のマーカー

```tmt
#[ タイトル ]
```

```
Block  @heading
  args    1
  content
    Text "タイトル"
```

### `#` の数がレベル

```tmt
##[ 節 ]
```

```
Block  @heading
  args    2
  content
    Text "節"
```

### 末尾の `{...}` は属性

```tmt
#[ タイトル ]{ id: intro }
```

```
Block  @heading
  args    1
  content
    Text "タイトル"
  group
    id: "intro"
```

### `:` を挟んでも同じ

```tmt
#[ タイトル ]:{ id: intro }
```

```
Block  @heading
  args    1
  content
    Text "タイトル"
  group
    id: "intro"
```

## ブロック配置の要素

### 行に要素しかなければブロック。グループがなくても行末で要素になる

```tmt
@memo
```

```
Block  @memo
```

検証:

```
unknown element `memo`: bare names are reserved for built-in elements; namespace it (`ns.memo`) or bind a namespace with `@import(file:..., as:ns)`
```

### `(args)` `[content]` `{value}` は各1個まで、順不同

```tmt
@memo(a: 1)[ 本文 ]{ b: 2 }
```

```
Block  @memo
  args    {a: 1}
  content
    Text "本文"
  group
    b: 2
```

検証:

```
unknown element `memo`: bare names are reserved for built-in elements; namespace it (`ns.memo`) or bind a namespace with `@import(file:..., as:ns)`
```

### 順序を入れ替えても同じ木になる

```tmt
@memo[ 本文 ](a: 1)
```

```
Block  @memo
  args    {a: 1}
  content
    Text "本文"
```

検証:

```
unknown element `memo`: bare names are reserved for built-in elements; namespace it (`ns.memo`) or bind a namespace with `@import(file:..., as:ns)`
```

### `.` 区切りの名前空間

```tmt
@deck.bookmark(name: foo)[ x ]
```

```
Block  @deck.bookmark
  args    {name: "foo"}
  content
    Text "x"
```

### 名前空間は多段でもよい

```tmt
@a.b.c(x: 1)
```

```
Block  @a.b.c
  args    {x: 1}
```

### 行頭にあっても、後ろに続きがあればブロックにならない。段落の書き出しとして読む

```tmt
@link(target: "https://example.com")[Tomet] は軽量マークアップ言語です。
```

```
Paragraph
  Inline @link
    args    {target: "https://example.com"}
    content
      Text "Tomet"
  Text " は軽量マークアップ言語です。"
```

### 折り返した行の先頭にある要素も地の文の一部。段落を切るのは空行

```tmt
この言語の名前は
@link(target: "https://example.com")[Tomet] といいます。
```

```
Paragraph
  Text "この言語の名前は "
  Inline @link
    args    {target: "https://example.com"}
    content
      Text "Tomet"
  Text " といいます。"
```

## インライン配置の要素

### 行の途中の要素は地の文の一部

```tmt
文中の @link(target: "https://example.com")[リンク] です。
```

```
Paragraph
  Text "文中の "
  Inline @link
    args    {target: "https://example.com"}
    content
      Text "リンク"
  Text " です。"
```

### 名前空間つき

```tmt
文中の @deck.badge(2)[印] です。
```

```
Paragraph
  Text "文中の "
  Inline @deck.badge
    args    2
    content
      Text "印"
  Text " です。"
```

## 文字列に落ちる場合

### `#` の後に `[` がなければ地の文

```tmt
# 見出しではない
```

```
Paragraph
  Text "# 見出しではない"
```

### `#` の後が `[` でなければ見出しにならない

```tmt
#memo(a: 1)
```

```
Paragraph
  Text "#memo(a: 1)"
```

### ASCII でない名前は要素にならない

```tmt
@タグ
```

```
Paragraph
  Text "@タグ"
```

### `@` の後にグループが続かなければ文字列

```tmt
連絡は me@example.com まで
```

```
Paragraph
  Text "連絡は me@example.com まで"
```

### 行中の `#` は普通の文字

```tmt
C# と F# の話
```

```
Paragraph
  Text "C# と F# の話"
```

### `<` はもうシジルではない

```tmt
型は Vec<T> と書く
```

```
Paragraph
  Text "型は Vec<T> と書く"
```

## `+++` フェンス

### 閉じる `+++` だけの行まで逐語。括弧も引用符もそのまま

```tmt
@memo+++
don't forget [this]
+++
```

```
Block  @memo
  raw     "don't forget [this]"
```

検証:

```
unknown element `memo`: bare names are reserved for built-in elements; namespace it (`ns.memo`) or bind a namespace with `@import(file:..., as:ns)`
```

### 本文に `+++` があるときは長い走りで囲む

```tmt
@memo++++
+++
まだ本文
++++
```

```
Block  @memo
  raw     "+++\nまだ本文"
```

検証:

```
unknown element `memo`: bare names are reserved for built-in elements; namespace it (`ns.memo`) or bind a namespace with `@import(file:..., as:ns)`
```

### 閉じないまま EOF に達したらそこで終わる

```tmt
@memo+++
閉じない
```

```
Block  @memo
  raw     "閉じない"
```

検証:

```
unknown element `memo`: bare names are reserved for built-in elements; namespace it (`ns.memo`) or bind a namespace with `@import(file:..., as:ns)`
```

### `${...}` は展開されず逐語で残る

```tmt
@config(format:json)+++
{"gh": "x/${1}"}
+++
```

```
Block  @config
  args    {format: "json"}
  raw     "{\"gh\": \"x/${1}\"}"
```

### 引用符の中の `}` で本文が途切れない

```tmt
@meta(format:yaml)+++
a: "}"
b: 1
+++
```

```
Block  @meta
  args    {format: "yaml"}
  raw     "a: \"}\"\nb: 1"
```

### `format:` は解釈だけを決め、字句解析には影響しない

```tmt
@zzz(format:yaml)+++
a: 1
+++
```

```
Block  @zzz
  args    {format: "yaml"}
  raw     "a: 1"
```

検証:

```
unknown element `zzz`: bare names are reserved for built-in elements; namespace it (`ns.zzz`) or bind a namespace with `@import(file:..., as:ns)`
```

## `{...}` グループ

### `key: value` の並び

```tmt
@memo{ a: 1, b: two }
```

```
Block  @memo
  group
    a: 1
    b: "two"
```

検証:

```
unknown element `memo`: bare names are reserved for built-in elements; namespace it (`ns.memo`) or bind a namespace with `@import(file:..., as:ns)`
```

### 要素の並び

```tmt
@links{
  (1)[ ひとつ ]
  (2)[ ふたつ ]
}
```

```
Block  @links
  group
    Bare
      args    1
      content
        Text "ひとつ"
    Bare
      args    2
      content
        Text "ふたつ"
```

### 対と要素の混在。並び順は保たれる

```tmt
@deck.card{ t: x, (a)[ y ], u: z }
```

```
Block  @deck.card
  group
    t: "x"
    Bare
      args    "a"
      content
        Text "y"
    u: "z"
```

### 空のグループ

```tmt
@memo{}
```

```
Block  @memo
  group   (空)
```

検証:

```
unknown element `memo`: bare names are reserved for built-in elements; namespace it (`ns.memo`) or bind a namespace with `@import(file:..., as:ns)`
```

## `(args)` と値の文法

### `key: value`

```tmt
@memo(a: 1, b: two)
```

```
Block  @memo
  args    {a: 1, b: "two"}
```

検証:

```
unknown element `memo`: bare names are reserved for built-in elements; namespace it (`ns.memo`) or bind a namespace with `@import(file:..., as:ns)`
```

### 位置引数（キーなし）

```tmt
@codeblock(rust)
```

```
Block  @codeblock
  args    "rust"
```

### 列

```tmt
@memo(xs: [1, 2, 3])
```

```
Block  @memo
  args    {xs: [1, 2, 3]}
```

検証:

```
unknown element `memo`: bare names are reserved for built-in elements; namespace it (`ns.memo`) or bind a namespace with `@import(file:..., as:ns)`
```

### 入れ子のマップ

```tmt
@memo(m: { x: 1 })
```

```
Block  @memo
  args    {m: {x: 1}}
```

検証:

```
unknown element `memo`: bare names are reserved for built-in elements; namespace it (`ns.memo`) or bind a namespace with `@import(file:..., as:ns)`
```

### 引用符つき文字列

```tmt
@memo(s: "a, b: c")
```

```
Block  @memo
  args    {s: "a, b: c"}
```

検証:

```
unknown element `memo`: bare names are reserved for built-in elements; namespace it (`ns.memo`) or bind a namespace with `@import(file:..., as:ns)`
```

### スカラーは型が推論される

```tmt
@memo(i: 1, f: 1.5, b: true, n: null)
```

```
Block  @memo
  args    {i: 1, f: 1.5, b: true, n: null}
```

検証:

```
unknown element `memo`: bare names are reserved for built-in elements; namespace it (`ns.memo`) or bind a namespace with `@import(file:..., as:ns)`
```

## リスト

### `-` が非順序、`-.` が順序

```tmt
- ひとつ
- ふたつ
```

```
Block  @ul
  group
    Bare
      content
        Text "ひとつ"
    Bare
      content
        Text "ふたつ"
```

### 順序つき

```tmt
-. ひとつ
-. ふたつ
```

```
Block  @ol
  group
    Bare
      content
        Text "ひとつ"
    Bare
      content
        Text "ふたつ"
```

### `--` は**ネストしない**。2行目は段落になる（未対応）

```tmt
- 親
-- 子
```

```
Block  @ul
  group
    Bare
      content
        Text "親"
Paragraph
  Text "-- 子"
```

### `(12:01)` は `{12: 1}` に**誤って**分解される。`:` が `key:value` として読まれるため

```tmt
- (12:01) 本文 {id: a}
```

```
Block  @ul
  group
    Bare
      args    {12: 1}
      content
        Text "本文"
      group
        id: "a"
```

### 末尾の `{...}` が `:` なしで項目に付く

```tmt
- 本文 {id: a}
```

```
Block  @ul
  group
    Bare
      content
        Text "本文"
      group
        id: "a"
```

## インライン記法

### 強調・太字・マーク・コード

```tmt
*em* と **strong** と ==mark== と `code`
```

```
Paragraph
  Inline @em
    content
      Text "em"
  Text " と "
  Inline @strong
    content
      Text "strong"
  Text " と "
  Inline @mark
    content
      Text "mark"
  Text " と `code`"
```

### 自動リンク

```tmt
見て https://example.com/x ください
```

```
Paragraph
  Text "見て "
  Inline @link
    args    {target: "https://example.com/x"}
  Text " ください"
```

## 区切りとコードブロック

### 区切り線

```tmt
---
```

```
Block  @hr
```

### 見出しつき区切り線

```tmt
---[ 章題 ]---
```

```
Block  @hr
  content
    Text "章題"
```

### ``` フェンス。中身は逐語

```tmt
```rust
let y = @T; *ptr
```
```

```
Block  @codeblock
  args    {lang: "rust"}
  content
    Text "let y = @T; *ptr"
```

## 補間 `${...}`

### 識別子

```tmt
${name}
```

```
Interp $
  interp  name
```

### メンバ参照

```tmt
${a.b.c}
```

```
Interp $
  interp  a.b.c
```

### 関数呼び出し

```tmt
${sum(1, 2)}
```

```
Interp $
  interp  sum(1, 2)
```

### `$name(...)` 形式

```tmt
$uuid()
```

```
Interp $
  interp  uuid()
```

## コネクト `:`

### `:{...}` は値をマージ

```tmt
@task[ A ]:{ id: t1 }
```

```
Block  @task
  content
    Text "A"
  group
    id: "t1"
```

検証:

```
unknown element `task`: bare names are reserved for built-in elements; namespace it (`ns.task`) or bind a namespace with `@import(file:..., as:ns)`
```

### `:(...)` は args をマージ

```tmt
@task(a: 1):(b: 2)
```

```
Block  @task
  args    {b: 2, a: 1}
```

検証:

```
unknown element `task`: bare names are reserved for built-in elements; namespace it (`ns.task`) or bind a namespace with `@import(file:..., as:ns)`
```

## コメント

### 行コメント

```tmt
// 消える
本文
```

```
Paragraph
  Text "本文"
```

### ブロックコメント

```tmt
本文 /* 消える */ の続き
```

```
Paragraph
  Text "本文 "
  Text " の続き"
```

## 撤去された構文

### `<T>` シジルは廃止。ただの文字列になる

```tmt
<memo>[ x ]
```

```
Paragraph
  Text "<memo>[ x ]"
```

> **パースは通る。** 上が実際の結果。

### 名前なしの `@` も撤去。`@(url:)` の推論は `1d0b7b2` の `infer.rs` 削除で消え `@link(target:)` に統一されたのに、 パーサだけが構文を受け付け続けていた

```tmt
@(url: "https://example.com")[リンク]
```

```
Paragraph
  Text "@(url: \""
  Inline @link
    args    {target: "https://example.com\")[リンク]"}
```

> **パースは通る。** 上が実際の結果。

### `@[ ... ]` も同じ

```tmt
@[ x ]
```

```
Paragraph
  Text "@[ x ]"
```

> **パースは通る。** 上が実際の結果。

### `#name` のブロックシジルも撤去。形はシジルではなく位置が決めるので、`#` は見出し専用に戻った

```tmt
#memo[ x ]
```

```
Paragraph
  Text "#memo[ x ]"
```

> **パースは通る。** 上が実際の結果。

## 綴りを失ったまま、代わりが未決のもの

### リモート接続。`<id:taskA>:{...}` と書いていたが `<T>` と共に 失われた。代わりの書き方は決まっていないので、実装も 受け付けない

```tmt
@id(taskA):{ priority: high }
```

```
Block  @id
  args    "taskA"
  group
    priority: "high"
```

検証:

```
unknown element `id`: bare names are reserved for built-in elements; namespace it (`ns.id`) or bind a namespace with `@import(file:..., as:ns)`
```

> **パースは通る。** 上が実際の結果。

## 紛らわしいが、これが正しい

### `(content:raw)` は普通の引数。`[...]` の解釈を変えない

```tmt
@memo(content:raw)[
1行目
2行目
]
```

```
Block  @memo
  args    {content: "raw"}
  content
    Text "1行目 2行目"
```

検証:

```
unknown element `memo`: bare names are reserved for built-in elements; namespace it (`ns.memo`) or bind a namespace with `@import(file:..., as:ns)`
```

> **パースは通る。** 上が実際の結果。

## 受け付けない書き方

### `{...}` に列は書けない。`+++` フェンスを使う

```tmt
@memo{[1, 2, 3]}
```

```
parse error: 1:6: a '{...}' group holds 'key: value' entries or elements; write a bare value in '(args)', or use a '+++' fence
```

### `{...}` に裸のスカラーも書けない

```tmt
@memo{hello}
```

```
parse error: 1:6: a '{...}' group holds 'key: value' entries or elements; write a bare value in '(args)', or use a '+++' fence
```

### 閉じない `[`

```tmt
@memo[ 閉じない
```

```
parse error: 2:1: unterminated, expected ']'
```

## 仕様にあるが未実装

### 入れ子の呼び出し（`docs/spec/builtin-functions.tmt`）

```tmt
${ref(id(asdf).contents(default))}
```

```
Interp $
  interp  ref(id(asdf).contents(default))
```

> **パースは通る。** 上が実際の結果。

### 正規表現リテラル（`docs/design/ideas/idea.tmt`）

```tmt
$regex(/*.svg/g)
```

```
parse error: 1:8: expected a value, identifier, or call in '${...}'
```

---

この一覧に載っていない構文は、実装されていないか、この表に追加し忘れて
いるかのどちらかです。後者を見つけたら `tests/src/syntax_report.rs` の
`CASES` に足してください。
