.tmドキュメントのschema/lint validation

typedmark-parserは文法(構文)としての妥当性しか見ない。その先の「意味として正しいか」——必須keyが揃っているか、型が合っているか、id重複がないか、未知の要素/属性がないか——をここで検査する。

値を計算したり(typedmark-compute)、参照を辿って中身を取ってきたり(typedmark-resolve)はしない。Documentを書き換える副作用もない。あくまで読み取り専用の検査結果(OK/エラー一覧)を返すだけ。
