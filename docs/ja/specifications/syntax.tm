@settings(file:docs/docs.settings.tm)
@meta{}

#[ 雛形 ]

```tm
@xxx(args)[content]{value}
@()[]{}
<T>()[]{}
// #[]{}
// ##[]{}
- (marker) content   // marker は key:value の args ではなく () や [] で囲んだ自由文字列
-. (marker) content
---[]---
// ${}             // 未実装
// $()              // 未実装
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

// 未実装のアイデア。`- () xxxxxx :{}` 自体は構文としては通るが、
// `:{}` に特別な意味は一切なく、単に marker が空文字でその後ろが
// ただの地の文になっているだけ。

```tm
// - () xxxxxx :{}
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
