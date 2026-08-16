#[ `<content>` ]

---[ `<content:default>` ]---

全てのブロックを入れることができる。

---[ `<content:raw>` ]---

`(content:raw)` を args に指定すると改行やスペースを保持する生テキストになる
(`<codeblock>` は常にこれと同じ扱い)。

```
<memo>(content:raw)[
あ あ
あ
]
```

---[ `<content:nest>` ]---

// 未実装のアイデア。実際に近い機能は `{value}` 側の bare children パターンで、
// `[...]` ではなく `{...}` を使い、`(id):[ ]` のようなコロンも書かない。

```
// @foot[
//   (id:<string>):[ <content> ]
//   (id:<string>):[ <content> ]
// ]

@links{
  (id1)[ content ]
  (id2)[ content ]
}
```

#[ `<value>` ]

json、yaml、toml の中から選択可能。

// kdl は未実装(embedded formatとして認識されない)。

#[ `<args>` ]

独自軽量言語のみ。

```tm
string:"string",
key:["list1","list2"]
group:{key:value, key:value}

// group.number:1              // 未実装: ネストへの自動展開はされない(識別子の一部として扱われる)
// group{key:value, key:value} // 未実装: `:` を省略した省略記法は未対応
```
