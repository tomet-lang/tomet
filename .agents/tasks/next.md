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

## パーサの欠陥 — `|` の作業中に出てきたもの

`|` を入れる過程で、シジルの認識まわりに構造的な欠陥が 4 つ見つかった。
一つずつ潰す。それぞれタスクファイルに測定結果ごと切り出してあるので、
中断しても調べ直しは要らない。

依存順に並べる。

1. `colon-as-sigil.md` — **設計未決。ここが先。** `:` をシジルとして扱う。
   決まると 2 の集合から `:` が消えて純粋になる。
2. ~~`sigil-follow-set.md`~~ — **完了（2026-09-08）**。`46de207` で
   `opens_group` に一本化し `every_sigil_takes_every_group_opener` を追加、
   `76bccea` で `@name:` を family の書き位置だけに狭めた。棄却案（述語を
   消して投機的パース）の理由は `opens_group` の doc comment に残した。
   `-` と `-.` が `(` を取らない件だけ `KNOWN_GAPS` に残っている（→ 3 の 2a）。
3. ~~`list-recognition.md`~~ — **完了（2026-09-08）**。`6d8e295` で
   `ListMarker` に `full_form` を載せて 3 → 2（挙動不変、関数も 4 → 2）、
   `421b396` で `(` を marker の開き括弧に加えた。`-(x)` が項目になり、
   `- ( )` が末尾空白の有無によらずマーカーになる。
   下敷きにあった「full-form 分岐が `parse_groups` の読んだ `args` を
   捨てていた」バグも一緒に直した。`KNOWN_GAPS` は空。
4. `duplicate-group-silently-dropped.md` — `@memo(a:1)(b:2)` が仕様どおりの
   エラーにならず、黙って地の文に落ちてブロックからインラインに降格する。
   規範層の話なので作者の決定が要る。
5. `bare-entry-value-group.md` — `{...}` の中で裸のエントリだけ `{value}` を
   取れない。`-[ x ]` と同種で、2 の構造の 4 例目。`colon-as-sigil.md` の
   規則検証がここで止まっているので、2 と一緒に片付く可能性がある。

いずれも「同じ構文が複数箇所で認識され、片方だけ更新されていない」という
同じ形をしている。2 がその構造そのものを扱う。

## そのほかの選択肢

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

**D. `|` 継続構文 — 実装済み（2026-09-08）**
`181b542` `-[ x ]` の非対称除去 / `0fc9e4b` パーサとテスト /
`1771a1f` tree-sitter と fixture。設計は
`docs/design/ideas/syntax-continuation.tmt`。

残っているもの:

- **連結と空白の意味論（先送り中）。** `|` は `[content]` から継承するだけで
  何も決めていない。テーブルの行が AST に無く、行を分けているのが `]` と `[`
  の間の空白だという弱さも一緒に継承している。テーブルを汎用化するときに
  両方まとめて決める。等価性は `tests/src/pipe.rs` が固定しているので、
  片方だけ直れば落ちる。
- **列の規則の帰結。** 開いた `|` の列に揃えるので、`@blockquote(A)| x` の
  続きは列 15 に立つ。複数行にするなら `|` を次の行で開く形が自然な綴りに
  なる。狙い通りか未確認。
- **printer の往復バグ（既存、`|` と無関係）。** `@references[` が最初の
  ブロック子要素を開き括弧と同じ行に置き、読み直すと Inline になる。
  括弧形でも壊れる。`tests/src/pipe.rs` の該当テストがそこを避けている。
- **tree-sitter は `|` の中のテーブル行を読めない。** 裸の `]` にトークンが
  無く、外部スキャナが要る。`KNOWN_TS_ERRORS` の `syntax/pipe-table.tmt`。
  `examples/dirs.tmt` などと同じ壁。

**C. `@tomet/astro`** — `packages/astro/`、骨組み 33 行で未追跡。README の
Prerequisites（`bindings/js` が名前で解決できない / `index.d.ts` が 1 破壊的
コミット遅れ / workspace 層が JS から届かない / `ref:` 未解決 / title ルール
なし / `DEFAULT_STYLE` が死んでいる）を外す作業が先に来る。重い。

@crate and @todo
