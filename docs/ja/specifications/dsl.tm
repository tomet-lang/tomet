`(args)` / `{value}` の中身、およびデータ専用の `.tm` ファイル全体で使われる、
軽量なキー・バリューDSL。`docs/ja/specifications/syntax.tm`の「DSL/軽量変数言語」節も参照。

```tm
key: "string"
key: value              // クォート無しの裸のスカラー(文字列/数値/真偽値/null)
key: 42
key: true
key: null
key: [ "list1", "list2" ]
key: { nested: value, key2: value2 }   // ネストは常に `:` の後に `{...}` を書く

// key.nested: value    // 未実装: `.` は識別子の一部として扱われるだけで、
                         // key:{nested:value} への自動展開はされない
// key{ nested: value } // 未実装: `:` を省略した省略記法は未対応
```
