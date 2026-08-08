@meta(yaml){
  key: value
}

#[ What is typedmark? ]{ id:header1 }

- 人間工学的見やすさ。
- パーサーにも優しい。
- 拡張性を担保
- key:valueを重視
- 私は、研究目的。

##[
 なぜtypedmarkを使用するのか？
]
{ id:header1.2 }

非曖昧な構文
リンクと変数に特化。
統一された記法

<T> に (input) [area] {value} のうち必要なものを付ける。3つのグループは順序自由、同じ種類の括弧は1要素につき1回まで。
- <input>(name:email, type:email){required}
- <caution>[ ネストされた注意書き。 ]

組み込みの参照系は <T> の代わりに @ を使い、() の中のキー名（url / file / ref / meta など）から種別を推論する。名前を書かなくていい分、キーの意味は型ごとに一意に保つ。
- @(url:https://example.com)[Wiki]
- @[Wiki](url:https://example.com)  ※順序を変えても同じ意味
- @(file:assets/image.png)[Caption]
- @(ref:anotation1)

###[ [] のルール ]

- `[]`（content-span）は `<T>` / `@` / `#`(見出し) の直後に続くグループとしてのみ有効。単独で行頭に出てくる `[...]` は content-span ではなくただの文字列。
  → だから `Caution [ ... ]` のように裸の単語+`[]` は書かない。型を明示して `<caution>[ ... ]` にする。
  → リストの `-` はそもそも `[`/`]` を使わないので衝突しない。
- `{value}` の中で `tags: [a, b]` のような配列リテラルを書くのは可、`{}`の内側は別ネストなのでcontent-spanの規則とは無関係。

###[ vs json ]

key:value構造で張り合える。
tmtは、人間ファーストであり、改行ありの長文を挿入するのに長けている。
深いネストには弱い。タイムライン形式は得意なんじゃないかと考える。

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

#[ 残っている論点 ]

- 名前「TypedMark」を維持するか。当初はMarkdown互換の想定だったが、今の記法はもうMarkdownとほぼ関係ない。
- `@(meta:yaml){...}` の `meta:` というキー名がしっくりくるか（暫定）。
- `@` の推論キー（url / file / ref / meta）の一覧をどこかに正式なレジストリとして持つか。
