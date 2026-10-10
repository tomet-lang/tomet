# `|content` と `[content]`/ROOT のブロック文法の非対称性

## 背景

`+++`フェンス撤廃タスク(`remove-legacy-embedded-format.md`)の調査中に見つかった、
既存の設計ギャップ。別タスクとして切り出す。

## 調査済みの事実

ブロックレベルの文法（section, list, element, fenced code, thematic break,
paragraph）は3つの場所で解釈されるが、実装の対称性がバラバラ:

1. **ROOT / `[content]`（ASTパーサー, `document.rs`）** — 対称。
   `parse_block_seq`という単一関数を`parse_document`(`BlockStop::Eof`)と
   `element::parse_content`(`[...]`, `BlockStop::Bracket(']')`)の両方が呼ぶ。
   doc-comment(`document.rs:47-53`)にも明記されている。
   backtick fence (```) はここで`is_fenced_code_block_start`/
   `parse_fenced_code_block`(`codeblock.rs`)として処理され、`[...]`内でも
   ROOT直下と同じく動く。fenced code blockは内部的にsigil `"raw"`の
   `Element`として合成される(`codeblock.rs:48`)。

2. **`|content`（ASTパーサー, `element.rs::parse_pipe_content`）** — 非対称。
   `parse_block_seq`を呼ばず、**別実装**。element・list・paragraphしか扱わず、
   `is_section_start`も`is_fenced_code_block_start`も呼んでいない。
   つまり`|`の中では現状、sectionもbacktick fenceも書けない。
   どこにも「なぜ`|`だけ制限するか」を説明するコメントはなく、
   `parse_block_seq`抽出時に`|`側まで一緒に統合されなかった、歴史的な
   共通化漏れに見える（意図的な設計上の制約という形跡なし）。

3. **CST（`tomet-syntax-parser/src/cst.rs`、lossless concrete syntax tree,
   editor tooling用）** — `[...]`も`|`も両方inline止まり。
   `parse_content_group`(`cst.rs:724`)は`parse_inline(InlineStop::Bracket)`
   しか呼ばず、ROOT/SECTIONが使う`parse_block`(section/list/code-block対応)
   を再利用していない。ASTより制限的で、ASTでは許される構造
   （`[...]`内のネストしたsectionやcode-fence）をCSTだけ取り逃す。
   CSTを参照するエディタ機能(LSP等)がある場合、実際にパースできる構造を
   ハイライト/フォールディングできない、という実害が出る可能性がある。

## 未解決の問い（決めるのはユーザー）

- `|content`にsection/code-fenceを許すべきか？ 許すなら`parse_pipe_content`を
  `parse_block_seq`と統合（ネストしたマーカー列の追跡をどう共存させるかが課題）。
  許さないなら、その制限を明文化したdoc-commentを書く。
- CSTの`parse_content_group`/`|`処理をASTと同じ再帰文法に揃えるべきか？
  揃えるなら、CST側に`parse_block`相当の再帰呼び出しを追加する必要がある。
  CSTを実際に使っている箇所（LSP、format-printerなど）を先に特定し、
  この非対称性が実際に機能的な欠落を引き起こしているか確認する。

## 次のステップ（着手時）

- [ ] CSTの消費者（LSP, tomet-format-printer等）を洗い出し、
      非対称性が実害を伴うか確認。
- [ ] `parse_pipe_content`をどこまで`parse_block_seq`と統合できるか調査
      （`current_stack`によるネストマーカー追跡との整合性）。
- [ ] ユーザーに方針（許容/明文化で現状維持）を確認してから実装。
- [ ] 着手時に`jj workspace add .agents/workspaces/pipe-content-block-parity`
      でワークスペースを作成。

## 現在の状態

未着手。調査結果のみ記録。
