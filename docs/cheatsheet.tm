@import(file:/)

@meta(json){
  key = {
    value = 1,
    value2 = 2
  },
  key2 = 2
}
@meta(yaml){
  key:value
}
@meta(toml){
  [ toml ]
  version.workspace = true
  [[ toml ]]
  typedmark = "1"
}

@links()[
  @(parent)[]
  @(child)[]
]

@foot()[
  :(id:asdf)[]
  :(id:zxcv)[]
]

@link[](url:https://)
@link[](file:/)
@link[](ref:)

@[](url:https://)
@[](file:/)
@[](ref:)

<embed>[](url:https://)
<embed>[](file:/)
<embed>[](ref:)

#[ heading ]{}
##[ heading ]{}
###[ heading ]{}

- aaaa
-. aaaa
1. aaaa
**aaaa**
*aaaa*
==aaaa==
_aaaa_
`aaaa`
---
---------
---[]---
---()---

// line comment
/* block comment */
