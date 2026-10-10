# `codeblock` という旧名の残留を `raw` に統一

## 背景

`+++`フェンス撤廃タスクの調査中に見つかった、別件の命名残留。
コミット `e9b84b0 feat(syntax): unify codeblock and inline backticks into @raw`
でsigil名は`"raw"`に統一されたが、ファイル名・コメント・一部テストに
旧名`codeblock`が取り残されている。

**訂正**: 最初`heading.rs:86`のテストを「実コードが旧sigil名を使っていて
分類に引っかからない本物のバグかもしれない」と疑ったが、確認したところ
`non_heading_element_is_always_none`という**ネガティブテスト**で、
「heading以外の何らかのsigil」を示すために`"codeblock"`という文字列を
使っているだけ（どの非heading名でも成立するテスト）。機能的なバグではなく、
単に旧名が残っている cosmetic な残留。

## 確認済みの残留箇所

- `crates/tomet-syntax-parser/src/codeblock.rs` — ファイル名自体。
  内容はfenced code block (```) のパース。生成するsigilは既に`"raw"`
  (`codeblock.rs:48`)。
- `crates/tomet-syntax-parser/src/lib.rs:25` — モジュールdoc:
  "`codeblock` / `fence`: Fenced code blocks and raw verbatim fence spans (`+++`)."
- `crates/tomet-syntax-parser/src/lib.rs:34` — `mod codeblock;` 宣言。
- `crates/tomet-semantics/src/positional.rs:8` — コメント内で
  "`codeblock`, `embed` and `callout` had entries only" と旧名使用。
- `crates/tomet-semantics/src/lib.rs:17,21` — doc-comment内の例示
  "`codeblock`, `quote`, `table`, ..." 、"`<codeblock>(rust)` -> `{lang: "rust"}`"。
- `crates/tomet-semantics/src/kind.rs:113,156,270` — コメント内で
  "codeblock.rs"、"`@codeblock`"という旧名言及。
- `crates/tomet-semantics/src/elements/heading.rs:86` — テスト
  `non_heading_element_is_always_none`が`Sigil::named("codeblock")`を使用
  （機能上の問題はないが、紛らわしいので`"raw"`か別の非heading名に変える）。

`tomet-syntax-ast/src/cst_ast.rs:65`の`CstCodeBlock` / `CODE_BLOCK`という
CST側のノード種別名は、CSTが構文（backtick fence）そのものを指すものなので、
これは`codeblock`のままで妥当（sigil名の`raw`とは別の軸）。リネーム対象外。

## 進め方（着手時）

- [ ] `crates/tomet-syntax-parser/src/codeblock.rs` を `raw.rs` にリネーム
      （`jj file rename` or `mv` + `mod`宣言更新）。
- [ ] 上記のdoc-comment/コメント中の`codeblock`言及を`raw`に更新。
- [ ] `heading.rs:86`のテストのsigil名を`"codeblock"`以外（`"raw"`等）に変更。
- [ ] `CstCodeBlock`/`CODE_BLOCK`はCST構文ノード名として維持（リネーム対象外、
      区別の理由をどこかにコメントしておくと良いかもしれない）。
- [ ] `cargo build` / `just test` で確認。
- [ ] 着手時に`jj workspace add .agents/workspaces/rename-codeblock-to-raw`
      でワークスペースを作成。

## 現在の状態

未着手。調査結果のみ記録。
