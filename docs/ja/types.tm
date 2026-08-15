#[ `<area>` ]

---[ `<area:default>` ]---

全てのブロックを入れることができる。

---[ `<area:raw>` ]---

改行やスペースを保持
```
あ あ
あ
```

---[ `<area:nest>` ]---

@foot[
  (id:<string>):[ <area> ]
  (id:<string>):[ <area> ]
  (id:<string>):[ <area> ]
  (id:<string>):[ <area> ]
  (id:<string>):[ <area> ]
  (id:<string>):[ <area> ]
]

#[ `<value>` ]

json、yaml、toml、kdlの中から選択可能。

#[ `<input>` ]

独自軽量言語のみ。

```tm
string:"string",
key:["list1","list2"]
group.number:1
group{key:value, key:value}
```
