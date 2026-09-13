```
@meta(format:json)+++
{
  "key": "value",
  "type": "person"
}
+++
@meta(format:yaml)+++
key: value
date: time
type: character
+++
@meta(format:toml)+++
key = "value"
type = "location"
+++
// #meta(format:ini){
//   key = "value"
// }
// #meta(format:kdl){
//   key = "value"
// }
```

`${some_id}`

`${sum(a, b)}`

#link("https://")[https://]

#link("/readme.md")[/readme.md]

#link("1")[1]

\@\[\](url:https://) \@\[\](file:/readme.md) \@\[\](ref:2)

= heading

== heading

=== heading

#image("https://", alt: "alt")

#image("/readme.md", alt: "alt")

#image("3", alt: "alt")

```sh
sudo whoami ls --help
```

#quote(block: true)[引用文]

// tomet:card
分類を持たない、題名+本文の区画

- outline/list

1. number1
2. number2

- check
- none
- custom?
- custom

1. aaaa

*aaaa* _aaaa_ #highlight[aaaa] _aaaa_ \`aaaa\`

```(lang:sh)

```

#line(length: 100%)

#line(length: 100%)

Title
#line(length: 100%)

💫
#line(length: 100%)

#table(
  columns: 4,
  [*title*], [*sdfasdf*], [*fasdf*], [*sdffdsf*],
  [title], [sdfddfasdf], [fasddfdfdff], [sdffdsf],
  [title], [sdfasdf], [fasdf], [sdffdsf],
)

