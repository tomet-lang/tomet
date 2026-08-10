@import(file:/)
@meta(json){
  { 
    "key": "value" 
  }
}
@meta(yaml){
  key: value
  date: time
}
@meta(toml){
  key = "value"
}

// @links()[
//   @(parent)[]
//   @(child)[]
// ]

// @foot()[
//   :(id:asdf)[]
//   :(id:zxcv)[]
// ]

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

// <code>(lang:sh)[]
// <codeblock>
// <execute>(lang)[]
<pre>(lang:json){
{sdfsdf}
}

<myfunc>()[]{}
<foo>()[]{}

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
----(💫)----

---<icon>()---

// line comment
/* block comment */

| table | ----- |
|     1 |     2 |
