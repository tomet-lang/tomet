# `\` 継続トリガー

**状態:** 実装完了、テスト全緑。

## 最終的な設計

- 既定は**孤立**。裸の要素が行だけで浮けば、空行の有無に関わらず単独の
  `Block::Element` になる（`docs/examples/dirs.tmt` の一覧が依存する挙動）。
- `\` は、その既定を上書きして直前のブロックへ明示的に繋ぐ**合流トリガー**。
  行頭でも行末でも、どちらか片方だけで成立する。継ぎ目は最初の一箇所だけで
  よく、一度段落に入れば以降の行は `\` なしで続く。行頭・行末どちらにも
  重ねて書いてよく、冗長な `\` は黙って許容される（黙って消費、literal
  text にはならない）。
- 継ぐ先が無い `\`（文書の先頭・空行の直後の行頭形、あるいは何も続かない
  行末形）は構文エラーにしない。パーサーはそれを消費するだけで先に進む。
- パーサーは種類を知らずに `\` を解釈する。直前のブロックが見出し・
  リスト・水平線・フェンスコードであっても同じように畳み込める。ただし
  それらの `required_shape` は常に `Block` なので、`tomet-semantics::
  shape_mismatch` が報告する——稀にしか起きない、明示的な誤用でしかない。
- `[content]` グループ内にも同じ規則が及ぶ（`Stop::Bracket`）。

### 既定を「合流」にしなかった理由(経緯の記録)

セッション中、一時的に「既定＝合流、`;`＝分離」という逆の極性を実装した
(git未コミットの中間状態)。しかし実例で洗い出した結果、孤立を望む
ケース(`dirs.tmt`・`scenario.tmt`・`node_graph.tmt`・見出し全般)の方が
合流を望むケース(badges 1例)より圧倒的に多く、既定を合流にしたのは
的外れだったと判断して元(`\`・既定は孤立)に戻した。記号を`;`にした
理由(「地の文でほとんど意味を持たない記号を選ぶ」)自体は合理的だったが、
それに引っ張られて極性まで一緒に反転させたのが誤りだった。

## 実装済みの変更

- `crates/tomet-syntax-parser/src/element.rs`: `LineEnd::{Bare,
  Continuation, No}`。`element_ends_line` は行末の `\` を検出する。
  `consume_trailing_continuation`/`leading_continuation` が消費を担う。
- `crates/tomet-syntax-parser/src/document.rs`: 先読み(`push_structural_or_
  join`/`paragraph_breaks_here` によるデフォルト合流判定)を全廃し、
  裸の要素・見出し・リスト・水平線・フェンスコードはすべて無条件に
  `Block::Element` として push。行頭の `\` は `blocks.last()` を pop して
  `continue_into_paragraph` へ、`LineEnd::Continuation` も同じ関数へ合流。
- `crates/tomet-syntax-parser/src/inline.rs`: `Stop::Paragraph` の継続
  判定から `;` 関連の分岐を除去。行頭・行末どちらの冗長な `\` も
  literal text にならないよう明示的に検出して除去する
  （`continue_into_paragraph` 自身が呼び出し直後の位置にある冗長な行頭
  `\` も同様に消費する——`Stop::Paragraph` の「改行を跨いだ後」の
  チェックでは、スキャン開始位置そのものにある `\` は捕まえられないため）。
  `Stop::Bracket` は行頭の `\` を検出して直前要素の `placement` を
  `Inline` に格上げするだけの単純な形に戻した
  （`bracket_line_joins_next`/`just_after_leading_separator` は削除）。
- `crates/tomet-semantics/src/kind.rs`: `shape_mismatch` から directive
  例外を撤去。`Placement::Inline` の `Block` 要求種類は常にエラー
  （`\` で明示的に誤用しない限りこの形は生成されないので、例外は不要）。
- `crates/tomet-format-printer/src/lib.rs`: `render_inlines` が、段落内で
  改行直後・かつ2番目以降の `@name(...)` 形の要素に `\` を再印字する
  （over-marking は安全なので、厳密な最小化はしない——Phase1計画の方針
  通り）。見出し/リストが合流した場合の糖衣保持の特別処理は維持。
- `tests/fixtures/syntax/continuation.tmt`・`tests/src/syntax_report.rs`・
  `tests/SYNTAX.md`・`docs/spec/syntax.tmt` の `##[ 継続 ]` 節、すべて
  `\` 設計に合わせて書き直し済み。
- `tmtroot/readme.tmt` の badges（本セッション最初の動機だった不具合）が
  `\` で正しく1行にまとまることを確認済み。

## 既知の限定事項(未解決、優先度低)

`\` で**前方に**見出し/リスト/水平線/フェンスコードを新規に取り込もうと
すると(例: `@link(a)\n\#[ Title ]`)、`Stop::Paragraph` のスキャナは
`#`糖衣を要素として構築する術を持たないため、literal text
"#[ Title ]" に化けてしまう(shape_mismatch にすら引っかからない)。
**後方に**(既に`Block::Element`としてパース済みのものを`\`でpopする方向)
は問題なく動く——`tests/fixtures/syntax/continuation.tmt` の実例も
この向きで書いてある。誰も実際には見出しを合流させたがらないので実害は
薄いが、直すなら `Stop::Paragraph` に構造的糖衣を認識させる作業になり、
まとまった追加作業。

## 完了したら

このファイルの内容は `docs/spec/syntax.tmt` の `##[ 継続 ]` 節と各コードの
doc comment に折り込み済み。上記の限定事項を解消するか、対応不要と
判断したら、このファイルは削除してよい。
