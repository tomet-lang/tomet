@settings(file:docs/docs.settings.tm)
@meta{}

#[ 雛形 ]

```tm
@xxx(args)[content]{value}
@()[]{}
<T>()[]{}
#[]{}
##[]{}
- (marker) content    // () は <T>(args) と同じ Value 文法。key:value も、
-. (marker) content   // positional(builtinキー "marker")によるキー省略も可能。
                      // 例: - (12:01) ... は {marker: "12:01"} に正規化される
                      // (typedmark-semantics::positional)。コロンを含む値は
                      // ("12:01") のように quote するとkey:value誤読を避けられる。
                      // - [x]/- [ ] のチェックボックス記法は廃止された。[] は
                      // リストマーカー直後で特別な意味を持たず、単なる地の文
                      // として扱われる。
---[]---
${xxx}                // 実装済み: `${id}` / `${a.b}` / `${sum(a, b)}` の構文解析のみ。
                      // 参照解決/関数評価はtypedmark-resolve/typedmark-computeの未実装分。
                      // 詳細: docs/reviews/2026-08-17-interpolation-syntax.md
// xxx
/*  */
```

#[ 派生 ]

##[ 柔軟性 ]

```tm
<T>()             // 括弧削減

<T>{}()[]         // 並び替え

@xxx () [] {}     // スペース(実際は空白の量に制限なし)
# []              // スペース

<T>(){}           // 改行
[]                // 改行
```

##[ 短縮 ]

組み込みの参照系は `<T>` の代わりに `@` を使い、`()` の中のキー名`(url / file / ref)`から種別を推論する。名前を書かなくていい分、キーの意味は型ごとに一意に保つ。
- `@(url:https://example.com)[Wiki]`
- `@[Wiki](url:https://example.com)`
- `@(file:assets/image.png)[Caption]`
- `@(ref:anotation1)`

##[ コネクト ]

実装済み。list item の内容が要素（`@name(...)`/`<T>`）で終わっていて、
かつその要素自身にも `{value}` を持たせたい場合、「直後の `{...}`/`(...)`
はどちらのものか」が曖昧になる。この曖昧さはコロンの有無で解消する:

- コロンなし `{...}`/`(...)` → 直前の要素自身のグループ（従来通り。
  `@meta(format:yaml) {...}` のように空白を挟んでもよい）
- コロンあり `:{...}`/`:(...)` → 要素ではなく、それを包んでいる
  list item 自身への「接続」。要素側は一切消費しない。

```tm
- @link(ref:x) {id:breakfast}    // {id:breakfast} は @link 自身の value
- @link(ref:x) :{id:breakfast}   // {id:breakfast} は list item 自身の attrs
- () xxxxxx :{}                  // 空map({})を item の attrs として明示的に接続
<id:asdf-asdf>:(){}              // トップレベル要素自身への接続(従来通り、list item を介さない)
```

#[ 対応Markdown記法 ]

```tm
---               // 線
*xxx*
**xxx**
==xxx==           // マーカー
_xxx_
`xxx`             // インライン記法保護(地の文として保護されるだけで<code>にはならない)
\`\`\`tm          // コードブロック
\`\`\`            // コードブロック
```

#[ DSL/軽量変数言語 ]

```tm
key:value,
key:["list1","list2"]
group:{key:value, key:value}

// group.key:value   // 未実装: `.` は識別子の一部として扱われるだけで、ネストへの自動展開はされない
// group{key:value}  // 未実装: `:` を省略した省略記法は未対応。group:{...} と書く必要がある
```

#[ 駄目な構文 ]

```
@xxx()(){}{}[][]    // 複数括弧(同じ種類のグループの重複)はエラー

@xxx()[]{}           // 全グループを明示的に空にした状態を意味し、
                     // グループ自体を省略した場合(args/content/valueがNone)とは区別される。
```
