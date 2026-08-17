参照を辿って中身を取ってくる

TypedMark自身のファイル参照構文(@settings(file:...))を解決する「preprocessor/linker」層。typedmark-parserはI/Oをしてはいけない(Deterministic Static Parser Boundary)ので、ファイルを読んでパースする処理はここに置く。

// [ 未実装 ] ${var}によるid参照(同一ファイル、あるいはfile:で示した別ファイル内の、指定idの要素を取ってくる)もここが担当する予定。関数呼び出し${function(...)}の評価はやらない、それはtypedmark-computeの仕事(ここが取ってきた値を渡す側)。
