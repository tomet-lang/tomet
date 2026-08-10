@meta(format:yaml){
  key:value
}

##[ typedmark ]

- 拡張性
- ドキュメント
- 置換
- アイコン、ロゴ
- IDEサポート

---[ 拡張子 ]---

- tt
  - TypedText
- tmt
  - TypedMarkText
- tmd
  - TypedMarkDoc

##[ Crates ]

---[　フィルター検索対応　]---
- 公式でRustでもう、tagやidで絞り込む関数を作ってしまおうというもの。
- Markdownから移行してくる人の需要にクリティカルだろうし、
- 他言語で実装されるよりRustで実装したほうが速度と安全性も保証でき、Typedmarkの総合的な評価につながるのではないか？

---[ CLI ]---
- `--advanced`でserveした場合に、色々試せるように、設定をGUI経由でできるようにする（html出力はそのままで。）
  - urlを何らかの方法で見えるようにする。

##[ Cettila ]

- 統合
- 編集
