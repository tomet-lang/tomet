# `:` をシジルにする — 残っている設計判断

**状態:** `:rule` は実装済み（MVP）。この文書はそのために作られた `:` 全体の
設計ログの生き残り — まだ決まっていない部分だけを残す。決まった部分・
実装された部分は各所のコードに折り込んだので、このファイルには残していない。

## 実装済み（このファイルではなくコードを見ること）

- `:` の基本規則（マージ vs 名前付きコネクトの位置による解決）:
  `crates/tomet-syntax-parser/src/element.rs` の `parse_groups`
  （`allow_colon_connect` の doc comment、コネクト分岐）
- `Element::connects: Vec<Element>`: `crates/tomet-syntax-ast/src/lib.rs`
- 閉じたコネクト名の表 (`ConnectMember`):
  `crates/tomet-semantics/src/connect_member.rs`
- `:rule` の検証: `crates/tomet-semantics-validator/src/{rule.rs,lib.rs}`
  （`check_rule_connects`/`check_one_rule`）
- `list(...)`/`enum(...)` 呼び出しリテラル (`Value::Call`):
  `crates/tomet-syntax-ast/src/lib.rs`。パーサは
  `crates/tomet-syntax-parser/src/value.rs` の `try_parse_call`
- 印字: `crates/tomet-format-printer/src/lib.rs` の `render_connects`
- `tree-sitter-tomet/grammar.js`: `connect`/`call` ルール
  （`_element_group`/`_entry_value` まわり）
- 仕様書: `docs/spec/syntax.tmt` の「名前付きコネクト」節、
  `tmtroot/docs/spec/connects.index.tmt`/`connects/rule.tmt`、
  `tmtroot/docs/spec/features/rule.tmt`

## 決まったこと

**`{...}`/`(...)` グループの中で `:` は使えない。** 確定。`@links{ (1)[a]
:{z: 3} }` は今後もエラーのまま — `parse_bare_element`・値グループ内の
`parse_element` の `allow_colon_connect: false` はこのまま変えない。値の中に
要素を埋め込む機能(`Value::Element`、別の設計ログで進行中)が入っても、この
制限には触れない。

## まだ決まっていないこと

**1. `:name(...):name(...)` の重複時の意味論。**

パーサは両方を `connects` に積むだけ（重複をエラーにしない）。
`:rule(...):rule(...)` や `:as(x):as(y)` を「複数あってよい」とするか
「エラー」とするかは、`:xxx` のメンバーが `rule` の一つしかない今は判断
材料が無い。次のメンバー（`:as` など）が決まってから判断する。

**2. `enum(...)` が具体的に何をするのか。**

`list(...)` と同じ呼び出し構文で書ける（パーサは呼び出し名を判断しない）が、
`enum` という名前に対応する意味論は何も無い。`:rule` の MVP は `list(...)`
しか使っていない。

**3. `allow:` に書いた名前自体を検証するか。**

`allow:list(ns.mycard)` の `ns.mycard` が実在する宣言済みの名前かどうかは
今は見ない（MVP の意図的なスコープ）。検証を足すなら
`tomet-semantics-validator::check_rule_connects` に手を入れる。

**4. blueprint のテンプレート本体に書いた `:rule` が、実体化後の文書でも
生き残って効き続けるのか。**

未検証。`tomet new` の実体化パスと `:rule` の相互作用は一度も試していない。
