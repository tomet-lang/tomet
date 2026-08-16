---[ `(args)` ]---

```(lang:tm)
format: json                   // json / toml / yaml のいずれか。{value}の中身をそのフォーマットの
                               // 生ソースとして扱う。@config(format:...)で以降のデフォルトにもでき

url: <url>                     // @(url:...) / @url(url:...) として推論・特別レンダリングされる
file: <file>                   // @(file:...) / @file(file:...) として推論・特別レンダリングされる
ref: <ref>                     // @(ref:...) / @ref(ref:...) として推論・特別レンダリングされる
                                // (対応する @links の id が実在するかの検証はまだ無い)

// format: kdl                 // 未実装: kdlはembedded formatとして認識されない
// path:(<path>,<path>#<var>)  // 未実装: path というキーには何の特別な意味もない
// wiki:(<wiki>,<wiki>#<var>)  // 未実装: wikilink解決の仕組みは無い
// url:(<url>#<var>)           // 未実装: `#var` のようなフラグメント/変数展開の仕組みは無い
```

---[ `{var}` ]---

```
id: xxxx-xxxx,        // 見出し・リスト項目・codeblockのidに使われる(html: id属性)
cssclass: "name"      // 同上、cssclassに使われる(html: class属性)。値は単一の文字列

// type: @settings    // 未実装: typeという特別なキーはどのconsumerからも読まれない
```

---[ special ]---

```
// @this               // 未実装: `@this` に特別な意味は無い
```
