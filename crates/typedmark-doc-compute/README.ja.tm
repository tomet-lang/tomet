`${function(...)}`の関数評価と@metaの動的計算

typedmark-resolveが辿って取ってきた値(`${var}`で参照した要素の中身や、@metaのフィールドなど)に対して、関数を適用して計算・置換する層。例: @metaの数値フィールドを合計して表示する、複数idの内容を結合して表示する、など。

typedmark-parserと同じくI/Oはしない。参照解決はresolveの仕事、ここは「値が揃った後の計算」だけを担当する。

#[ 実装済み ]

- evaluate(doc, expr): Literalはそのまま、Identifier/MemberはResolveへ委譲、Callはargsを再帰評価してから組み込み関数を呼ぶ
- 組み込み関数: add / sub / mul / div / mod (全部2引数、数値のみ)。int同士はint、片方floatならfloat昇格、divは常にtrue division(float)、i64オーバーフロー時もfloatへ自動フォールバック(panicしない)

// [ 未実装 ] callee(呼び出す関数名)は裸のIdentifierのみ対応。a.b(x)のようなMemberをcalleeにする呼び出しはUnsupportedCalleeエラー。ユーザー定義関数、文字列/真偽値向けの関数は未設計。
typedmark-js
