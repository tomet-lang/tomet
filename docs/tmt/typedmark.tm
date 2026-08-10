// @config(format:json)
@meta(format:yaml){
  key: value
  date: time
}

<index>

#[ What is typedmark? ]{ id:header1 }  // 対応

-. 人間工学的見やすさ。
-. パーサーにも優しい。
-. 拡張性を担保
-. key:valueを重視
-. @link[私は、研究目的。](url:https://example.com)
-. 後方互換性
  - 知らない @xxx を見ても、パーサー全体を壊さない
-. 「とりあえずdivで包める」みたいな汎用コンテナをTypedMarkに大量導入すると、せっかくの@linkや@codeの意味が薄くなる。
-. Markdownの簡潔さ + HTMLのsemantic structure + ASTとしての厳格さ

##[
  なぜtypedmarkを使用するのか？
]
{ id:header1.2 }

非曖昧な構文
*リンク*と@[変数](file:/readme.md)に特化。
統一された記法

`<T>` に `(input) [area] {value}` のうち必要なものを付ける。3つのグループは@[順序自由](ref:2)、同じ種類の括弧は1要素につき1回まで。
- `<input>(name:email, type:email){required}`
- `<caution>[ ネストされた注意書き。 ]`

**組み込みの参照系**は <T> の代わりに @ を使い、() の中のキー名（url / file / ref / meta など）から種別を推論する。名前を書かなくていい分、キーの意味は型ごとに一意に保つ。
- @(url:https://example.com)[Wiki]
- @[Wiki](url:https://example.com)  ※順序を変えても同じ意味
- @(file:assets/image.png)[Caption]
- @(ref:anotation1)

###[ `[]` のルール ]

- `[]`（content-span）は `<T>` / `@` / `#`(見出し) の直後に続くグループとしてのみ有効。単独で行頭に出てくる `[...]` は content-span ではなくただの文字列。
  → だから `Caution [ ... ]` のように裸の単語+`[]` は書かない。型を明示して `<caution>[ ... ]` にする。
  → リストの `-` はそもそも `[`/`]` を使わないので衝突しない。
- `{value}` の中で `tags: [a, b]` のような配列リテラルを書くのは可、`{}`の内側は別ネストなのでcontent-spanの規則とは無関係。

###[ VS json ]

key:value構造で張り合える。
==tmtは、人間ファーストであり、改行ありの長文を挿入するのに長けている。==
深いネストには弱い。タイムライン形式は得意なんじゃないかと考える。

##[ VS html ]

実は案外構造は同じ。
`<p><a href="https://">Text</a></p>`
`<html>[<a>[ Text ]{url:https://}]`
ただしTypedmarkは、抽象表現が可能で、深い構造を好まないドキュメントで大いにメリットである。
`<p>[`
`  @[Text]{url:https://}`
`]`

#[ その他構想 ]
{
  id: header2
  cssclass: card
}

@links {
  (1)[ 注釈 ]
  (anotation1)[ 注釈 ]
}
※ `@links{}` の中の `(id)[content]` だけは型名省略OK。コンテナ自体が「これは注釈定義の並び」という型を与えているので、要素ごとに書く必要がない。

---[ Title ]---

<embed>[](url:https://static.wikia.nocookie.net/virtualyoutuber/images/b/b2/Ouro_Kronii_Portrait.png)
<embed>[](file:/readme.md)
<embed>[](ref:3)

<codeblock>(lang:shell)[
sudo unko
]

<blockquote>[ 引用文 ]

```sh

```

// line comment
/* block comment */

#[ 残っている論点 ]

- 名前「TypedMark」を維持するか。当初はMarkdown互換の想定だったが、今の記法はもうMarkdownとほぼ関係ない。
- `@` の推論キー（url / file / ref）の一覧は `typedmark_ast::INFERRED_AT_KEYS`/
  `infer_at_kind` に正式なレジストリとして持つ（決定・実装済み）。裸の `@(key:...)`
  推論は、`url`/`file`/`ref` をインライン記述で使うために文字数を減らす目的で
  導入したもので、「名前を省略できる」という一般的な便利機能ではない。
  `meta` はこのレジストリに含めない -- `@meta(tag){...}` は常にブロックレベルで
  簡潔に書く必要がないため、常に明示的な名前を書く（`@(meta:yaml){...}` という
  裸の書き方は廃止。書いても "meta" 種別としては推論されず、汎用の `at` 要素に
  フォールバックする）。
- `{...}` の中身は、`(input)` に `format` キー（`json`/`yaml`/`toml`）が
  あれば、TypedMark自身の軽量な `Value` 文法ではなく、本物のJSON/YAML/TOML
  ソースとしてそのまま `serde_json`/`serde_yaml`/`toml` クレートに渡して
  パースする（決定・実装済み）。これは `@meta` 専用の特別扱いではなく、
  どの要素でも同じ仕組みで動く汎用機構——`(format:...)` があるかどうかだけで
  決まる。`format` キーが無い、または未知の値の場合は従来どおり軽量文法に
  フォールバックする。
  `@meta` は今のところ `@meta(format:json){...}` のように明示的にキーを
  書く形だけを正式とする。以前あった `@meta(json){...}`（裸のタグ、位置引数
  的な書き方）は廃止した。将来的に糖衣構文として復活させる可能性はあるが、
  それは明示形が安定してから検討する。
  なお `{...}` の中身として渡るのは `{`/`}` そのものを除いた内側のテキストなので、
  JSONでオブジェクトを書きたい場合は自前の `{}` が別途必要
  （例: `@meta(format:json){ {"key": "value"} }`）。YAML/TOMLは
  `key: value`/`key = "value"` だけでそれぞれ単体で完結した文書になるため、
  この二重括弧は不要。

----[💫]----

- `@config(format:json|yaml|toml)` で、上記 `format` キーのドキュメント全体の
  デフォルト値を設定できる（決定・実装済み）。新しい文法は不要——`@config`は
  ただの `@name(input)` 要素で、`()`にキーを書く点も他の要素の設定
  （`<codeblock>(lang:...)` など）と同じ。`{}`ではなく`()`にしたのは、
  「これはデータではなく設定」という役割分担を守るため。
  意味は**出現順に効く・単一パス**：`@config(format:X)`より後に出てくる要素に
  効き、前にある要素には効かない（`@meta`が常にブロックレベルという慣習と
  同様、ドキュメント先頭に置く運用を想定。ドキュメント全体に順序非依存で効く
  二パス方式ではなく、あえて単純な逐次パースのままにした）。
  要素側の`format`キーは今までどおり最優先——キーが無ければドキュメントの
  デフォルトを継承し、キーはあるが未知の値（例:`format:none`）なら
  デフォルトが有効でも明示的にオプトアウトして軽量文法にフォールバックする
  （これは「未知の値は軽量文法にフォールバック」という既存の挙動をそのまま
  オプトアウトの手段に転用しただけで、新しい特別扱いは無い）。
  `@config`自身に`format`キーが無い場合は、実行中のデフォルトはそのまま
  変更しない（将来`format`以外のキーが`@config`に増えても、無関係に
  デフォルトを消さないため）。`@meta`と同じく`INFERRED_AT_KEYS`には含めず
  常に明示的に書く。出力（HTML/Markdown）には現れない（`@meta`と同じ扱い）。
- コードブロックは `<pre>` ではなく `<codeblock>` と呼ぶ（HTMLのタグ名を
  借りるより、CommonMark自身の用語に合わせる）。中身は `{value}` ではなく
  `[area]` に置く: `<codeblock>(lang:xxx)[code]`。`{}` は他の要素と同じく
  `id`/`cssclass` などの参照用メタデータのために空けておく
  （`<codeblock>(lang:rust){id:snippet1}[code]`）。`codeblock` の `[...]`
  は他の要素と違い、インライン文法（`*em*`・`` `code` ``・要素トリガー
  など）を一切通さない生テキストとして扱う——実コードに含まれる
  `*`/`<`/`@`などがマークアップとして誤解釈されないようにするため。
  （以前は「唯一の例外」だったが、下記の`area:raw`により他の要素にも
  同じ扱いを一般化できるようになった）。
- `[area]` content fidelity (decided/implemented). Two independent fixes,
  scoped separately because they solve different problems:
  1) Bracket-depth safety is now universal, not `codeblock`-only. Every
     `[area]` (and a heading's/titled-thematic-break's `[...]`) used to
     stop at the *first* literal `]`, so a bare "[ ]"/"[brackets]" inside
     otherwise-ordinary text truncated the rest of the area as corrupted
     trailing content. `parse_inline_seq`'s `Stop::Bracket` now tracks
     depth for unowned literal `[`/`]` (ones not already consumed by a
     nested element/`*em*`/backtick span), only ending the area at depth
     0. No opt-in, no syntax change -- applies to every element
     automatically.
  2) A local `area:raw` key in `(input)` opts a *specific* element's
     `[area]` into the same raw/verbatim treatment `codeblock` already
     had: no inline-markup interpretation, and (unlike bracket-depth
     safety) embedded newlines stay literal instead of collapsing to a
     space via `normalize_text`'s lazy-continuation folding. For
     freeform user text (a notes/memo field) where a source line break
     must mean a real line break, e.g. `<memo>(area:raw)[ ... ]`. Mirrors
     the `format` key's shape (a recognized tag value, unrecognized/
     absent falls back to the normal grammar) but is local-only with no
     `@config`-level document-wide default -- deliberately, so
     understanding one element's `[area]` never requires looking
     elsewhere in the document. Reuses `codeblock`'s bracket-depth
     matcher logic but *not* its quote-skipping: `find_matching_delimiter`
     skips `"`/`'`-quoted runs before counting depth, which is correct
     for real source code (unmatched quotes don't happen in valid code)
     but wrong for free-form prose, where an ordinary apostrophe
     (`don't`) would be misread as opening a quoted run and swallow the
     rest of the area. `area:raw` uses a separate, quote-agnostic
     depth-only matcher (`find_matching_bracket`) instead.
- `//`コメントの制約を緩和（決定・実装済み。範囲はRust側`typedmark-parser`
  のみ——tree-sitter文法は既知の別ギャップとして未対応のまま）。
  1) ブロックレベルの`//`/`/* */`はインデントしてよい（`#`やリストマーカー
  などブロック開始マーカー自体は引き続き行頭必須、コメントだけ緩めた）。
  2) 空白行が無くても、`//`/`/* */`は`#`やリストマーカーと同じく段落の
  lazy continuationを止める（`text\n// note\nmore`は「text」「more」の
  2段落＋コメントになる）。
  3) `()`/`{}`/`[]`の中でも`//`が効く。エントリ間・要素の先頭など「隙間」
  位置（`skip_ws_and_newlines`が動く場所）では常に安全に効く——`https://...`
  のような裸スカラーは`eat_scalar_raw`が一括で読み切るので誤爆しない。
  値の直後の同一行トレーリングコメント（`key: value // note`）は、
  「直前に空白があるときだけ`//`をコメント開始とみなす」境界規則
  （`*em*`/`_em_`と同じ発想）で対応——`https://`のように直前に空白が無い
  `//`はそのまま値の一部。裸スカラーがどうしても`//`から始まる必要がある
  場合（プロトコル相対URLなど）はクォート必須。`/* */`は`()`/`{}`/`[]`の
  中では今回未対応（要望が無かったため対象外）。

#[ 他 ]

- ふりがなが簡単
