= 補間の識別子は要素の識別子と同じ形

\`-\` を含められる。要素の引数で \`\@kind(node-graph)\` と書けるものが、補間の中でも同じように書ける。

`${ref(id(node-graph))}`

`${filename}`

`${vars.title}`

= 呼び出し

`${sum(1, 4)}`

`${sub(1, 2)}`

// tomet:memo
\`\$name(args)\` は本文の中にも置ける -- `${slug-of(title)}` のように

= 引数の中の識別子

`${ref(id(asdf-asdfa).contents(dafault))}`

`${outer(inner-name, "quoted-string", 42)}`

= \`-\` で始まるものは識別子ではない

先頭は英字か \`\_\` のみ。次の行はただの本文で、補間ではない。

- 箇条書きの \`-\` は今までどおり

