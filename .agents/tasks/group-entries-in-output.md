# `{...}` グループを出力形式のどこに置くか

**状態:** 半分だけ着手。実バグ 1 件を 2026-09-08 に修正（`ae6ba8c`）。
残りは設計判断で、作者の決定が要る。

## 前提：`{...}` は言語の中で唯一「データと要素が順番付きで混ざる」スロット

```rust
pub enum ElementValue { Group(Vec<Entry>), Raw(String), Interp(InterpExpr) }
pub enum Entry { Pair(String, Value), Element(Element) }
```

`@deck.card{ title: 混在, (a)[ 対と要素 ] }` は、`title: 混在` という対と
`(a)[ 対と要素 ]` という要素を、**書いた順で**ひとつの並びに持ちます。
`@links{ (1)[a] note: x (2)[b] }` が動くのはこの形のおかげで、語彙が
`@element(c){ @args{ @param(id){...} } }` と書けるのも同じ理由です。

問題はここです。**どの出力形式にも、そんなスロットはありません。**

- HTML の属性は文字列の対しか持てず、子要素は本文にしか置けない
- Pandoc の `Attr` は `(id, [class], [(key, value)])` で、値は文字列だけ
- CommonMark には属性という概念すら無い
- Typst は独自

だから writer は全員その場で答えを発明していて、**四人が四通りの答えを
出しています。しかも三人は「捨てる」と答えています。**

## 今どうなっているか（2026-09-08 実測）

`@deck.card` は非組み込み要素。`—` は出力に痕跡が残らないことを意味します。

| 入力 | html | to-md | to-typst | pandoc 往復 |
| --- | --- | --- | --- | --- |
| `(k: v)` args のみ | `data-k="v"` | — | — | `{ k: v }` に移動 |
| `{ id: x }` 対のみ | **—** | — | — | `{ id: x }` 保持 |
| `{ (a)[ y ] }` 要素のみ | `tm-children` の子 div | — | — | `[content]` へ移動、再パースで地の文 |
| `{ id: x }[ 本文 ]` | 本文のみ | 本文のみ | `// tomet:...` | 両方保持 |

読み取れること:

- **`{...}` の対は HTML から完全に消える。** `(args)` は `data-*` になるのに、
  `{value}` は属性にも本文にもならない。
- **to-md と to-typst は非組み込み要素のデータを丸ごと捨てる。**
  `to-md` は `<div data-tm-kind="deck.card"></div>` を出すだけ、`to-typst` は
  何も出さない（本文があるときだけコメント 1 行）。
- **pandoc だけがデータを保持する。** ただし `(args)` と `{value}` の区別は
  失われ、両方 `{...}` になって返る（これは仕様として記録済み、下記「対象外」）。

## 1. グループの要素が戻ってこない

`@deck.card{ title: 混在, (a)[ 対と要素 ] }` を Pandoc に通して戻すと:

```tmt
@deck.card[[対と要素]{ value: a }
]{ title: 混在 }
```

対 (`title: 混在`) は `{...}` に戻りますが、**要素が `[content]` に移動して
います**。Pandoc に「この `Div` は本文ではなくグループのエントリだった」と
言う手段が無いので、`to_pandoc` はグループの要素を本文ブロックとして書き
（`element_body_blocks`）、`from_pandoc` は戻す先を持ちません。

### 直した半分（`ae6ba8c`）

以前はさらに `Sigil::Bare` が失われ、**`bare` という名前の要素**になって
いました。`ElementKind::Bare::as_str()` が `"bare"` で、その文字列が
`tomet-<name>` クラスに乗るためです。`@bare` はどの語彙も宣言しないので、
**往復した文書は validate に通りませんでした。**

いまは予約キー `tomet-sigil: bare` で運びます。クラスではなくキーにしたのは
衝突を避けるためで、`@kind` が `bare` を宣言する語彙を名乗る文書では `@bare`
と無修飾で書けるので、`tomet-bare` は二つの意味を持ってしまいます。
`tomet-data` が既に予約キーの前例です。

`tests/src/snapshot.rs` の
`a_bare_entry_does_not_come_back_as_an_element_named_bare` が固定しています。

### 直っていない半分と、その代償

エントリは今も `[content]` に着地します。そこに `Sigil::Bare` の綴りは存在
しないので、printer は `[x]{ value: a }` と書き、**それは再パースすると要素に
なりません**。地の文の文字列になります。

```
修正前   @bare[ひとつめ]{ value: "1" }   パースは通る、validate に落ちる
修正後   [ひとつめ]{ value: "1" }        validate は通る、地の文になる
```

**無効な要素を、失われた要素と交換した**形です。どちらも同じ一つの穴の裏表で、
閉じられるのはこの節の残り半分だけです。

### 決めること

「この `Div` はグループのエントリだった」を Pandoc の語彙でどう言うか。

素案は予約クラス（`tomet-entry` など）＋ `from_pandoc` が本文ではなく
`{value}` に戻す、という形。ただし決めるべき点が三つあります。

- **順序をどう保つか。** `{...}` は対と要素の並び順を保持します
  (`Vec<Entry>`)。Pandoc 側では対が `Attr` に、要素が本文ブロックに分かれる
  ので、並びは分断されます。順序を捨てるのか、`tomet-data` のような
  exact-copy に順序ごと入れるのか。
- **どこまで往復させるか。** exact-copy に丸ごと入れれば往復は完全になりますが、
  そのぶん Pandoc を経由した他ツールからは読めない不透明な塊になります。
  Pandoc 経由の意味が薄れます。
- **printer 側も要る。** 仮に `from_pandoc` が `{value}` に戻せるように
  なっても、`Sigil::Bare` が `[content]` に現れる木は今でも作れてしまいます
  （from_pandoc 以外からも）。その木を printer が再パース可能な形で書けない
  のは別の穴です。

## 2. `{...}` の対はどこへ行くべきか

`(args)` には答えがあります — HTML では `data-*`、Pandoc では `Attr` の対。
`{value}` にはありません。

**そして「writer ごとに違う立場を取っている」のではなく、誰も立場を実装して
いません。**

`tomet-html` は本文に可視の span として出すつもりのコードを持っています
(`render_element_value`, `crates/tomet-convert-html/src/lib.rs:717`):

```rust
if let Some(data) = value.as_data() {
    let text = value_to_plain(&data);
    if !text.is_empty() {
        out.push_str("<span class=\"tm-value\">");
```

しかし `as_data()` は Group から**必ず** `Value::Map` を返し
(`tomet-syntax-ast/src/lib.rs:534`)、`value_to_plain` は Map に対して
空文字を返します (`:884`, `Value::Map(_) => String::new()`)。よって
`text.is_empty()` が常に真で、**この span は一度も出力されません**。
`tests/ref/` 全体を `tm-value` で grep しても 0 件です。

つまり「HTML は見せる、Pandoc は属性に入れる、という正当な立場の違い」では
なく、**HTML は見せるつもりで黙って落としている**。

### 決めること

`{...}` の対は、出力において何なのか。

- **データ** — HTML なら `data-*`、`(args)` と同じ棚。読者には見えない。
  `{}` は「常にデータ」だという uniform-group 以降の立場と一致する。
- **本文** — 読者に見せる。`render_element_value` が書こうとしていたこと。
  ただし `@deck.card{ id: x }` が `id: x` と表示されるのが望ましいかは疑問。
- **要素ごとに違う** — 語彙が宣言する。いちばん柔軟だが、`no-vocabulary` の
  外側（意味論）に判断を置く必要があり、writer が語彙を引くことになる。

決めたら `value_to_plain` の Map 分岐か、`render_element_value` の呼び出し側の
どちらかが必ず変わります。今はどちらも「Map は空」という一行で塞がっています。

## 対象外

往復で失われる他の二つは設計として受け入れ済みで、
`tomet-convert-pandoc/src/from_pandoc.rs` に記録があります。位置引数の
`(args)` が `{value: ...}` として返ること、`@meta` の数値が文字列として
返ること。どちらも Pandoc の `Attr` とメタデータ型に等価物が無いためです。

`to-md` と `to-typst` が非組み込み要素のデータを捨てる件は、この文書の
範囲外ですが**同じ問いの別の面**です。1 と 2 を決めたら、その決定を
この二つにも適用するか別途決めること。
