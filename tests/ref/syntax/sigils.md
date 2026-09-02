# シジルは形を表す

\`@\` はインライン、\`#\` はブロック。名前空間が出自を表す。

# ブロック要素

<div data-tm-kind="memo">名前は \`#\` の直後に続く。空白があると本文になる</div>

<div data-tm-kind="deck.bookmark" data-name="foo" data-count="3">名前空間つき</div>

<div data-tm-kind="import" data-file="./deck.tmt" data-as="deck"></div>

# インライン要素

文中の [リンク](https://example.com) と

<div data-tm-kind="deck.badge">名前空間つきインライン</div>

は行の途中でも要素になる。

# 本文に落ちる \`#\`

# 見出しではない行。\`#\` の直後が識別子でなければただの文字。

これは #タグ ではない。ASCII でない名前は要素にならない。

# \`{}\` は常にデータ

**1**: ひとつめ
**2**: ふたつめ

<div data-tm-kind="deck.card"></div>

