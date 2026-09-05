# `@vocabulary` の最初のスライス

設計は `.agents/tasks/sigil-shape-axis-and-namespaces.md` の
「Namespaces, settled (2026-09-05)」、規範は `docs/spec/vocabulary.tmt`。

目標は 190 件の `UnknownElement` を落とし、`tomet check` がバリデータを
呼べるようにすること。そこまで行くと `singleton` が載る土台になる。

## 層の制約

`classify` は `tomet-semantics`（層 2）にいて I/O ができない。語彙は
ファイルから来るので、**読むのは resolver、判定するのは semantics**に割る。
`tomet-semantics-resolver` は既に `settings.rs` で `fs::read_to_string` を
しているので、語彙の読み込みはそこに置ける。

`validate_document` の doc comment は「never does I/O」を宣言している。
これを壊さない形にする —— 語彙を**引数で受け取る**。

## Steps

- [x] 1. `ElementKind` と `BUILTIN_KINDS` に 6 つ足す:
      `vocabulary` / `element` / `param` / `args` / `data` / `content`。
      `required_shape` は全部 Block。`is_directive` は `vocabulary` のみ。
      配列長 22 -> 28、`as_str` の往復テストが効く。
      **代償**: kind 語彙はこの 6 語を宣言できなくなる。`@use` で持ち込む
      名前空間は影を作れるので、そちらは自由。
- [x] 2. `tomet-semantics` に `Vocabulary` 型。名前 -> 宣言のマップ。
      純粋。I/O なし。`classify_name_in(name, &Vocabulary)`。
  step 1/2/4 で分かったこと:
  - 抽出は純粋なまま `tomet-semantics` に置けた。`Vocabulary::from_document`
    は `&Document` を取るだけで、ファイルを探すのは resolver の仕事。
    最初の計画より層がきれいに割れた。
  - `Bindings` が名前解決を持つ。bare は std -> kind の順、`@use` は常に
    完全修飾。`Bindings::declaration` は三つの軸を返す。
  - `.tomet/vocabularies/writ.vocabulary.tmt` が最初の実物。writ の自作要素は
    `@layers` と `@pure` の二つだけだった。
  - `@vocabulary` ヘッダが無い文書は語彙ではない（`@element` を持っていても）。
    `extract_blueprint_schema` が「`@kind` があれば blueprint」としていた
    失敗を繰り返さないため。テストで固定済み。

- [ ] 3. `tomet-semantics-resolver` に語彙の読み込み。
      `.tomet/vocabularies/<X>.vocabulary.tmt` を名前で引く
      (`blueprint_dir` と対称)。`@kind(X)` と `@use` から束縛を作る。
- [x] 4. `.tomet/vocabularies/writ.vocabulary.tmt` を本物として書く。
      `@layers` を宣言する。これが最初の実物になる。
- [ ] 5. `validate_document_with(doc, &Vocabulary)`。既存の
      `validate_document` は std のみのまま残す（純粋性の宣言を守る）。
- [ ] 6. `tomet check` が語彙を解決してバリデータを呼ぶ。
- [ ] 7. 190 件が落ちることを確認。落ちない分は何が残ったかを記録する。
