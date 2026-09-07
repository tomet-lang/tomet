# 次にやること

最終更新: 2026-09-08

## 片付いたもの

1. **Value DSL 移行** — `6531294 refactor(docs): four embedded bodies become
   the Value DSL they describe` でコミット済み。タスクファイルは畳んで削除済み。
2. **VS Code シンタックス** — `861e7d4 fix(vscode): one stray character stops
   repainting the rest of the file` でコミット済み。
3. **tomet-writ** — 2026-09-08 に `cd118f8..e49c24d` を push（15 コミット）。
   移動と設定の 3 コミットを追加した:
   - `da8ab09 docs: docs/ becomes tmtroot/` — `readme.tmt` は内容不変の移動、
     `.gitattributes` のパスも追随。空になった `docs/` は削除。
   - `958b6af fix(editorconfig): the extension is .tmt, not .tm`
   - `e49c24d fix(flake): the tomet input is fetched over ssh` — lock は
     tomet ノードの `url` 2 箇所だけ。作業ツリーにあった nixpkgs の巻き戻し
     （1788316716 → 1786384358）は取り込まず HEAD の値を維持した。
   - `tmtroot/agents.tmt`（0 バイト）は未追跡のまま手元に残してある。
   - `cargo test --workspace` は 35 passed。

## 残っている選択肢

**A. `tomet refactor --value-dsl` の対象拡張**
`normalize_meta_to_value_dsl`（`crates/tomet-transform/src/directive.rs:92`、
129 行）が `classify_std_lenient(el) == ElementKind::Meta` でしか動かないので
`@config` / `@settings` / `@deck.note` を変換できない。呼び出し元は
`crates/tomet-workspace/src/refactor.rs:65` の 1 箇所だけ。`materialize_raw_body`
は要素の種類を見ていないので、ガードを「`format:` を宣言した Raw フェンス body
を持つ要素」に広げるのが素直。前回の移行で手作業を強いられた箇所。

**B. `{...}` グループの出力先** — `.agents/tasks/group-entries-in-output.md`
前半（実バグ、単独で直せる）: `Sigil::Bare` が `to_pandoc.rs:128` と `:172` の
`format!("tomet-{}", kind.as_str())` を通って `tomet-bare` クラスになり、
戻ってきたとき `bare` という名前の要素になる。`@bare` は builtin ではないので
ラウンドトリップ後の文書が validate に通らない。参照は
`tests/ref/syntax/sigils.pandoc.roundtrip.tmt`。
後半（グループ entry が body に落ちる）は Pandoc に表現がなく、設計判断が要る。

**D. `|` 継続構文（設計中、実装未着手）**
`docs/design/ideas/syntax-continuation.tmt` に 2026-09-08 の会話を記録済み。
規則は「`|` は `[` が置ける位置に置け、`]` の代わりに `|` 行の連なりの終わりで
閉じる」の一つだけ。作者の合意は (a) `|content` == `[content]` /
(c) リストも例外にしない / (d) 裸の `|` は地の文、まで取れている。未決は列の規則、
formatter の綴り、名前付きスロットとの関係。実装の入口は `list.rs` が `(args)`
の直後に許す文字の集合に `'|'` を足すところ。

あわせて、消した仕様の一文と同じ主張が `crates/tomet-syntax-parser/src/list.rs`
のコメント 2 箇所とパーサの挙動（`parse_sugar_body` が一行で閉じる）に残っている。
`|` を入れるならそこも要る。

**C. `@tomet/astro`** — `packages/astro/`、骨組み 33 行で未追跡。README の
Prerequisites（`bindings/js` が名前で解決できない / `index.d.ts` が 1 破壊的
コミット遅れ / workspace 層が JS から届かない / `ref:` 未解決 / title ルール
なし / `DEFAULT_STYLE` が死んでいる）を外す作業が先に来る。重い。

@crate and @todo
