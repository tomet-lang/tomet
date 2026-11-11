= 行に座だけ（ブロック）

\<\$uuid\>

\<\$list(string, length: 6)\>

= 地の文の中

文書の id は \<\$uuid\> で、題は \<\$string(length: 40)\> と書く。

- リストの項目の座は \<\$int\> と \<\$date(default: auto)\> 。

_強調の中の \<\$string\> も_、*強い強調の中の \<\$int\> も*座。

= 見出しの座 \<\$string\>

= 値の位置

= 引数

\<\$string(length: 80)\>

\<\$nanoid(length: 8, prefix: "note-", default: auto)\>

\<\$list(list(string), length: 6)\>

\<\$list(string, length: 6)\>

= `>` を含む引数

\<\$string(prefix: "\>")\>

\<\$string(prefix: ")\>")\>

\<\$int(min: a \> b)\> と \<\$int((min \> 0), max: 9)\>

\<\$x\>y\> は座のあとに文字 `y>` が続く。\<\$x(a)\> \> \<\$y\> も同じ。

= 引数の有無

\<\$uuid\> と \<\$uuid()\> はどちらも座。

= 座にならないもの

\<\$\> \<\$5\> \<\$ x\> \<\$-x\> \<\$x.y\> \<\$x \>

\<`$x(a)` \> と \<\$x{a: 1}\> と \<`$x(a)`(b)\> も文字のまま。

\<\$uuid\> は座ではない。

