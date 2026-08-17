参照を辿って中身を取ってくる

TypedMark自身のファイル参照構文(@settings(file:...))を解決する「preprocessor/linker」層。typedmark-parserはI/Oをしてはいけない(Deterministic Static Parser Boundary)ので、ファイルを読んでパースする処理はここに置く。

#[ 実装済み ]

- ${id} / ${id.member} 参照解決(同一ドキュメント内のみ。{id:...}/(id:...)を持つ要素・見出し・リスト項目を探して、(args)と{value}をマージしたValueを返す。{value}が優先、なければ(args)にフォールバック)

// [ 未実装 ] file:で示した別ファイル内のid参照。関数呼び出し${function(...)}の評価はやらない、それはtypedmark-computeの仕事(ここが取ってきた値を渡す側)。
