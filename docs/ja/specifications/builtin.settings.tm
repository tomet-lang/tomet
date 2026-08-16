@meta{ type: "settings" }

/* この設定ファイル自体は構想段階のスキーマ定義で、実際にこれを読んで
   検証する consumer (typedmark-validator) はまだ存在しない
   (`cargo new` のままのスタブ)。args/content/values/placement/
   singleton といったキーはすべて将来のスキーマ言語の「案」であって、
   今のパーサー/レンダラーが解釈するものではない。

   ただしこの.tmファイル自体は正しいTypedMark構文で書けている必要が
   あるので、そこだけは現在の文法に合わせて直した:
   - キーは識別子(英数字/_/-/.)のみ書ける。`@meta`のような@付きキーや
     `"#"`のような記号のクォート済みキーはmapのキーにできない。
   - 列挙は `a | b | c` ではなく、実装済みの seq `[a, b, c]` を使う。
   - `key!` のような必須マーカー記法は無いので、
     `key:{ required:true }` のようにネストしたmapで表現する。
   - `{ target }` のようなコロン無しの裸識別子はmapのエントリとして
     書けない(常に `key: value` が必要)ので `{ target: string }` の
     ように書く。 */

@settings{
  // 実際に Sigil を持つ「要素」(typedmark_semantics::ElementKind)の一覧。
  // 名前で識別できるものだけがここに乗る -- <T>/@name で誰でも増やせる。
  elements: {
    settings: {　　　　　　　　　　　　　　　　　　　　// あとで、@を付けれるようにする。
      args: { target: { required: true } },
      values: "settings document",
      placement: head,
      singleton: true
    },
    config: {
      // 現状 args から実際に読まれるのは format キーだけ。
      // style / export_type / export_path はまだどのconsumerからも読まれない。
      args: {
        format: [ json, toml, yaml ]
      },
      placement: head,
      singleton: true
    },
    meta: {
      // meta の {value} は現状どのconsumerからも読まれない
      // (html/markdownどちらも素通りする)。type/idはここでは将来案。
      args: { format: [ json, toml, yaml ] },
      values: { type: string, id: uuid },
      placement: head,
      singleton: true
    },
    // `@link` という明示名には特別な意味は無い。実際に特別レンダリング
    // されるのは url/file/ref (と、名前省略時の @(url:...) 等のキー推論)。
    url: {
      args: { target: { required: true } },
      content: inline,
      placement: inline
    },
    file: {
      args: { target: { required: true } },
      content: inline,
      placement: inline
    },
    ref: {
      args: { target: { required: true } },
      content: inline,
      placement: inline
    },
    embed: {
      // src には file/url のみ使われる(refでは解決されない)
      args: { target: { required: true } },
      content: inline,
      placement: inline
    },
    hr: {
      content: inline,
      placement: block
    },
    em: {
      content: inline,
      placement: inline
    },
    strong: {
      content: inline,
      placement: inline
    },
    mark: {
      content: inline,
      placement: inline
    },
    codeblock: {
      args: { lang: language },
      content: raw,
      placement: block
    },
    blockquote: {
      content: raw,
      placement: block
    }
  },

  // 見出し/リスト/インラインコードのような構文プリミティブは Sigil を
  // 持たず classify() の対象にもならない -- <T>/@name のように誰でも
  // 追加できる「要素」ではないので、elements とは別枠にした。
  primitives: {
    heading: {
      placement: block,
      content: inline
    },
    list: {
      placement: block
    },
    list_ordered: {
      placement: block
    },
    thematic_break: {      // 3つ以上の '-'。実体は elements.hr と同じ
      placement: block
    },
    fenced_code: {         // 3つの backtick。実体は elements.codeblock と同じ
      args: { lang: language },
      placement: block,
      content: language
    },
    inline_code_span: {    // 1つの backtick。地の文保護のみで<code>にはならない
      placement: inline,
      content: raw
    }
  }
}
