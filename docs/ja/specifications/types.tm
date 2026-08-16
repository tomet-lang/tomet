#[ `<content>` ]

---[ `<content:default>` ]---

全てのブロックを入れることができる。

---[ `<content:raw>` ]---

改行やスペースを保持
```
あ あ
あ
```

---[ `<content:nest>` ]---

@foot[
  (id:<string>):[ <content> ]
  (id:<string>):[ <content> ]
  (id:<string>):[ <content> ]
  (id:<string>):[ <content> ]
  (id:<string>):[ <content> ]
  (id:<string>):[ <content> ]
]

#[ `<value>` ]

json、yaml、toml、kdlの中から選択可能。

#[ `<args>` ]

独自軽量言語のみ。

```tm
string:"string",
key:["list1","list2"]
group.number:1
group{key:value, key:value}
```
