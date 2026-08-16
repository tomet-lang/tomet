@settings(file:docs/docs.settings.tm)
@meta{}

```tm
@meta(format:json)
@config(format:json)

@url(url:<url>)[ display_name ]
@file(file:<file>)[ display_name ]
@ref(ref:<ref>)
@(url:<url>)[ display_name ]     // 名前省略時は url/file/ref のキーから推論
@(file:<file>)[ display_name ]
@(ref:<ref>)

@links{
  (id1)[ note ]
  (id2)[ note ]
}

<embed>(url:<url>)[alt]          // src には url か file のみ使われる
<embed>(file:<file>)[alt]
<codeblock>(lang:<language>)[ <code> ]
<blockquote>[ <content> ]

- (marker:<string>) content
-. (marker:<string>) content

// comment
/* comments */
```

// 以下は未実装(構想段階):

```tm
// @config(
//   style:structural,           // 未実装: styleキーは読まれない
//   export_type:[ commonmark ], // 未実装: export_type/export_pathキーは読まれない
//   export_path:/readme.md
// )
// @import(file:path)            // 未実装: @importは未設計(docs/develop/architecture.mdの@import節を参照)

// @link(url:<url>)[ display_name ]  // 未実装: `@link`という明示名には特別な意味がない
                                      // (url/file/refとして特別扱いされるのは @url/@file/@ref か
                                      // 名前を省略した @(url:...) のみ)
// @tag{}                            // 未実装: 組み込みkindではない。書けるが汎用要素として扱われる

// <icon>(pack:lucide, name:<string>)  // 未実装: 組み込みkindではなく、アイコン表示の特別処理もない
// <index>()[ title ]                  // 未実装: 同上、目次生成などの特別処理はない
// <callout>(variant:info)[ <content> ] // 未実装: 同上、専用のスタイル分岐はない

// #(number:<uint>)[ heading ]  // 未実装: 見出しは (args) を持たない。連番機能もない

// ${<var>}                     // 未実装: 変数展開の仕組み自体が存在しない
// $(<var>)
```
