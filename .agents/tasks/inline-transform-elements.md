# Task: `tomet_tree::transform_elements` を唯一の呼び出し元へインライン化して削除する

> Split out from: `.agents/tasks/tree-one-traversal.md`
> 完了時の折り畳み先（Destination）: 残すべき新規設計事項なし（API 削除のみ）。`twrit api .` / `just docs` で生成 API ドキュメントを更新し、検証完了後に本ファイルを削除すること。

---

## 背景・事実

- `crates/tomet-syntax-tree/src/walk.rs` の `pub fn transform_elements` は、`for_each_element_mut` を回して `if filter(el) && transform(el) { count += 1; }` しているだけの 13 行の薄いラッパーである。
- ワークスペース全体での呼び出し元は、`crates/tomet-transform/src/macro_rewrite.rs` の `transform_link_targets_with_macros` **1箇所のみ**。
- 同じ `tomet-transform` クレート内の他の変換（`structural.rs`, `links.rs`, `meta.rs`）はすでに直接 `for_each_element_mut` を使っているため、`macro_rewrite.rs` もそれに揃えて `transform_elements` を削除する。

> 注意: 並行タスク `.agents/tasks/unify-ast-file.md` が `walk.rs` と `macro_rewrite.rs` の引数型（`Document` → `File`）を変更するため、コンフリクトを避けるには `unify-ast-file` 完了後に実施するのが望ましい。

---

## 実行ステップ

- [ ] `crates/tomet-transform/src/macro_rewrite.rs` の `transform_link_targets_with_macros` を `tomet_tree::for_each_element_mut` の直接呼び出しに書き換える。
- [ ] `crates/tomet-syntax-tree/src/walk.rs` から `pub fn transform_elements` を削除する（他に参照がないことを確認）。
- [ ] `just docs`（または `twrit api .`）を実行し、`tmtroot/generated/crates/tomet-tree.tmt` 等の抽出 API ドキュメントを更新する。
- [ ] `just test`・`just docs-check`・`just writ-check` がすべて通ることを確認し、本タスクファイルを削除する。
