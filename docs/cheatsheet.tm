// [ NOT implemented ]
// @import(file:/settings.json)
// [ special-casing exists for "import" anywhere. Renders as a generic
//   element (`<div class="tm-element tm-import" data-file="...">`), same as
//   any other unrecognized `<T>`/`@name`. Kept commented so this file
//   doesn't look like a working example. ]
// @settings(format:json)

@meta(format:json){
  {
    "key": "value"
  }
}
@meta(format:yaml){
  key: value
  date: time
}
@meta(format:toml){
  key = "value"
}

// @links{}: real, implemented (`typedmark-renderer`'s `render_links_container`
// / `typedmark-markdown`'s `render_links_container`). Bare `(id)[content]`
// children only -- no type name needed, the container already supplies it.
// Renders as a `<dl>` definition list, each id becoming an `#link-<id>`
// anchor target other elements can `@(ref:<id>)` to.
@links {
  (1)[ 注釈その1 ]
  (anotation2)[ 注釈その2 ]
}

// [ NOT implemented ]
// @foot()[ 
//   :(id:asdf)[]
// ] 
// [ NOT implemented]
// @tag[]

@link[](url:https://)
@link[](file:/readme.md)
@link[](ref:1)

@[](url:https://)
@[](file:/readme.md)
@[](ref:2)

#[ heading ]{ id:1 }
##[ heading ]{ id:2 }
###[ heading ]{ id:3 }

<embed>[](url:https://)
<embed>[](file:/readme.md)
<embed>[](ref:3)

<codeblock>(lang:json)[
{sdfsdf}
]

<blockquote>[ 引用文 ]

// <callout>(caution)[
//
// ]

// <myfunc>()[]{}
// <foo>()[]{}

- aaaa
-. aaaa
-. aaaa

1. aaaa

**aaaa**
*aaaa*
==aaaa==
_aaaa_
`aaaa`

```sh

```

---

-----

---[ Title ]---

---Title---

----(💫)----

----[💫]----

// ---<icon>()---
// ===

// line comment
/* block comment */

| table | ----- |
|     1 |     2 |
