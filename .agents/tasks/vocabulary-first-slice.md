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

- [x] 3. `tomet-semantics-resolver` に語彙の読み込み。
      **慣習探索は却下された（著者の判断）。** vault が設定の
      `vocabularies` にパスを並べ、各ファイルが `@vocabulary(ns)` で
      自分の名前を名乗る。Cargo の `[workspace] members` の形。
      名前 -> パスのマップは重複（名前がキー・パス・ファイル内の三箇所）。
      理由: 慣習探索は**当たらなかったことが見えない**。`@kind(writ)` に
      対してファイル名が `writ.vocabluary.tmt` だと、エラーは使用箇所で
      N 回出て、原因の場所では出ない。宣言リストなら一度、書いた場所で出る。
      blueprint も同じ形に揃え済み（下記）。
      `PrinterConfig.vocabularies` は入った。読み込みと `Bindings` の
      組み立てが残り。
- [x] 4. `.tomet/vocabularies/writ.vocabulary.tmt` を本物として書く。
      `@layers` を宣言する。これが最初の実物になる。
- [x] 5. `validate_document_with(doc, &Vocabulary)`。既存の
      `validate_document` は std のみのまま残す（純粋性の宣言を守る）。
  step 3/5 で分かったこと:
  - `crate-layering` が設計を直した。`load_vocabularies` に `PrinterConfig`
    を渡そうとしたら層 2 -> 層 3 の上向き辺になり、ビルドが止まった。
    パスのリストを受け取る形にしたら、resolver が config を知る必要が
    そもそも無かった。
  - `@import` を `@use` / `@include` に分割。`BUILTIN_KINDS` 22 -> 29。
    これを捕まえたのは `directives_have_no_markdown_output` で、
    `import` が `Custom` に戻った瞬間 `<div>` が復活した。
  - **挙動が一つ厳しくなった。** 以前は名前空間つきの名前が無条件で
    `Custom` になっていた（`classify_name` の最初の分岐）。`Bindings` は
    束縛されていない名前空間を拒否する。これが塞ぎたかった穴そのものだが、
    `validate_document`（束縛なし）を使う側 —— js/java/python バインディング
    と `apps/web` —— は、名前空間つき要素を全部 unknown と報告するように
    なる。ファイルを読めない実行環境にどう語彙を渡すかは未解決。
  - `ShapeMismatch` のメッセージが撤回済みの設計を引きずっていた。
    「use `#link`」—— `#` が要素シジルだった頃の助言で、今は従いようがない。
    位置で決まると言うように直した。

- [x] 6. `tomet check` が語彙を解決してバリデータを呼ぶ。
      **配線は書けているが未コミット**（`$SCRATCH/check-wired.rs`）。
      繋ぐと `docs-check` が 10 ファイルで赤くなる。内訳は下記 step 7。
- [x] 7. 190 件が落ちることを確認。落ちない分は何が残ったかを記録する。
      配線した状態で計測済み: 追跡下の `.tmt` 139 件中 90 件が通り 49 件が落ちる。
      docs/ に残る未知の名前（件数順）:
      `bookmark` 27 / `line` 12 / `file` 6 / `timestamp` 4 / `node` 4 /
      `tag` 2 / `ref` 2 / `index` 2 / `dirs` 2 / `callout` 2 / `memo` 1 /
      `import` 1。
      **これは著者の編集判断待ち** —— どれが kind 付きでどれが共有か、
      共有語彙を何と呼ぶか。task file `sigil-shape-axis-and-namespaces.md`
      の「Open — the editorial half only」がその項目。
      別枠で `ShapeMismatch` が数件: `@link` と `@icon` が行に単独で置かれて
      いる。`required_shape(Link) = Inline` が厳しすぎるのではないか、
      という設計上の問いが残っている（下記）。

  step 6/7 の結果: `docs/` は全部通る。`tomet check` はバリデータを呼ぶ。
  リポジトリ全体では 105 通過 / 34 不通過で、不通過は全部 `tests/` の
  凍結フィクスチャ（`workspace.ignore` が既に他の guard から外している）と
  `tmtroot/readme.tmt`（下記）。

## 決着したもの

1. **docs/ の自作要素**: kind ごとの語彙 6 つ。kind と要素が一対一だった。
   `bookmark` / `dirs` / `node-graph` / `scenario` / `video` が実要素を宣言し、
   `idea` は `open: true` で宣言ゼロ。
2. **`required_shape(Link)` は `None` に**（著者の判断）。link/embed/icon は
   行に単独で立ちうる。`shape_mismatch` が捕まえるべきは「段落の中の
   `heading`」のような構造の誤りで、単独行のリンクは何も壊していない。
   LSP の `shape_of` は「ブロックとして置いて怒られるか」で形を逆算していて
   「どちらでも可」を表現できなかったので、`required_shape` を公開して
   直接引くようにした。

## 残っている決定

3. **ファイルを読めない実行環境に語彙をどう渡すか。**
   js/java/python バインディングと `apps/web` は `validate_document` を
   呼んでいて、名前空間つき要素を全部 unknown と報告するようになる。
4. ~~**`callout` を `std` に入れるか。**~~ 入れた（著者の判断）。 `tmtroot/readme.tmt` が唯一の
   未解決ファイルで、`@callout(note)[...]` を使っている。
   `docs/spec/builtin-elements.tmt` は「組み込み kind ではない」と書いているが、
   実装は八箇所で特別扱いしている ——
   `positional.rs`（`variant` を位置引数に）、`convert-typst`、
   `convert-markdown` の export と **import**（`> [!note]` から
   `Sigil::named("callout")` を作る）、`format-printer` の style、
   `tomet-config` の二箇所、LSP。
   つまり Markdown を往復させると、検証が拒否する要素が生成される。
   `BUILTIN_KINDS` だけが知らない状態だった。
   三つの writer は `kind.as_str()` にマッチしていて、しかも `heading` や
   `codeblock` や `link` と同じ arm の並びにいたので、一覧に足しても
   何も壊れなかった。壊れたのは LSP のテスト二つで、どちらも
   「callout は組み込みではないから補完に出ない」を固定していた。
   対照として `memo` には特別扱いが一つも無く、本当に組み込みではない。
   これで `tests/` の凍結フィクスチャと意図的に不正な
   `invalid_syntax.tmt` を除く**全ての文書が通る**（115 / 145）。
