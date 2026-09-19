= `\` は段落の継続トリガー

`display: どちらでも` の要素（`link` `embed` `file` `dir` `draft` `fixme`）が行だけで浮くと、周囲の改行が暗黙にブロックへ昇格し、意図せず複数のブロックに割れることがある。`\` は「この行は直前の段落に続く」と明示するトリガーで、行頭でも行末でも、どちらか片方だけで成立する。

#link("https://a.example")[a] #link("https://b.example")[b]

上と同じ木になる、行末で書く形。

#link("https://a.example")[a] #link("https://b.example")[b]

= 一度継ぎ目を越えれば、以降の行は `\` なしで続く

#link("https://a.example")[a] #link("https://b.example")[b] #link("https://c.example")[c]

= 冗長な `\` は黙って許容される

行頭・行末どちらにも、繰り返し書いてよい。

#link("https://a.example")[a] #link("https://b.example")[b] #link("https://c.example")[c]

= 既定は変わらない

`\` を書かなければ、今まで通り別々のブロックに割れる。 `docs/examples/dirs.tmt` のディレクトリ一覧のような、意図してこうなる用法を壊さないため。

#link("https://a.example")[a]

#link("https://b.example")[b]

= 見出しやリストなど、専用の綴りを持つ構文にも同じ規則が及ぶ

パーサーは種類を知らずに位置だけで配置を決めるので、`\` は既に解析済みのどんな `Block::Element`（見出し・リスト・水平線・フェンスコード……）でも同じように畳み込める——ただし `hr` は `required_shape` が常に `Block` なので、この形は `tomet-semantics::shape_mismatch` に引っかかる。稀にしか起きない、書いた人の誤用というだけの話。

text

#line(length: 100%) joined into the thematic break above

= 継ぐ先が無い `\` はダングリング

行頭の `\` の上に `Block::Element` が無い場合（文書の先頭、または空行の直後）と、行末の `\` の後に何も続かない場合は、パーサーは `\` を黙って消費して何もしない。構文エラーにはならない。

#link("https://a.example")[dangling-leading]

#link("https://a.example")[dangling-trailing]

