@settings(file:docs/docs.settings.tm)
@config(
  format:json
  /// [ (意図)[formatterが見る。視覚に長けたフォーマットであるために、
  ///   @[一行ずつ](file:tmt/examples/bookmark.tm)]や、@[エリアだけ改行](file:tmt/examples/scenario.tm)などをフォーマッタや出力が合わせるために記入したい。
  ///   だが、確かに、毎ファイル書くのはどうかとも思う。其の場合は、type(あるいは拡張子)で、別ファイルに宣言された設定を自動的に読み込むようにしたいかもしれない。 ]
  // style:structural // one_per_line 

  // export_type:[ commonmark, @import(file:path) ]
  // export_path:/readme.md
)

```
@meta(format:json){
  {
    "key": "value",
    "type": "person"
  }
}
@meta(format:yaml){
  key: value
  date: time
  type: character
}
@meta(format:toml){
  key = "value"
  type = "location"
}
// @meta(format:ini){
//   key = "value"
// }
// @meta(format:kdl){
//   key = "value"
// }
```

/// [ special-casing exists for "import" anywhere. Renders as a generic
///   element (`<div class="tm-element tm-import" data-file="...">`), same as
///   any other unrecognized `<T>`/`@name`. Kept commented so this file
///   doesn't look like a working example. ]
// @import(file:/settings.tm)

// @links{}: real, implemented (`typedmark-renderer`'s `render_links_container`
// / `typedmark-markdown`'s `render_links_container`). Bare `(id)[content]`
// children only -- no type name needed, the container already supplies it.
// Renders as a `<dl>` definition list, each id becoming an `#link-<id>`
// anchor target other elements can `@(ref:<id>)` to.
// @links {
//  (1)[ 注釈その1 ]
//  (anotation2)[ 注釈その2 ]
// }

// [ NOT implemented ]
// @foot()[ 
//   :(id:asdf)[]
// ] 

// [ NOT implemented ]
// @tag[]
// @[ ここには、英語しか入れれないということになる。 ]

// [ NOT implemented ]
// @(lang:sh)[
//   sudo whoami
//   ls --help
// ]

// [ これは、見にくくなるのでやりたくない。 ]
// @vars(foo:bool,a:int,b:int){foo:true, a:1, b:2}
// @var(foo:bool,a:int,b:int){foo:true, a:1, b:2}[ a + b ]
// @(foo:bool){foo:true}

// [ NOT implemented ]
// $(a.a) 変数で置換する。

@link[](url:https://)
@link[](file:/readme.md)
@link[](ref:1)

@[](url:https://) 
@[](file:/readme.md)
@[](ref:2)

#[ heading ]{ id:1 }
##[ heading ]{ id:2 }
###[ heading ]{ id:3 }

// <index>(){}
// <icon>()

<embed>[alt](url:https://)
<embed>[alt](file:/readme.md)
<embed>[alt](ref:3)

<codeblock>(lang:sh)[
sudo whoami
ls --help
]

// content:raw: opts any element's [content] into raw/verbatim text, same
// treatment as codeblock -- byte-safe against an embedded "]" and
// newline-preserving (no inline markup, no lazy-continuation folding).
<memo>(content:raw)[
  don't forget: check [this] and [that]
]

<blockquote>[ 引用文 ]

// <callout>(caution)[
//
// ]

// <myfunc>()[]{}
// <foo>()[]{}

- outline/list
-. number1
-. number2
- (x) check          {id:aaaaaa}
- ( ) none
- ( ) custom?
- (TODO) custom

1. aaaa

**aaaa**
*aaaa*
==aaaa==
_aaaa_
`aaaa`

```(lang:sh)

```

---

-----

---[ Title ]---

----[💫]----

// line comment
/* block comment */

/*
@table()[
[ title ][  sdfasdf   ][    fasdf    ][ sdffdsf ]
[ title ][ sdfddfasdf ][ fasddfdfdff ][ sdffdsf ]
[ title ][  sdfasdf   ][   fasdf     ][ sdffdsf ]
]{}
*/
