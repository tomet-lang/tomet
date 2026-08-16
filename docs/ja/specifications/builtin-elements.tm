@settings(file:docs/docs.settings.tm)
@meta{

}

```tm
@config(
)
@meta(
  // file value
  type:<string>,
  // global value
  format:(json, toml, yaml, kdl)
)
@config(
  format:json
  style:structural // one_per_line 
  export_type:[ commonmark, @import(file:path) ]
  export_path:/readme.md
)

@link(url:<url>, path:<path>, file:<file>, ref:<ref>)[ display_name ]
@tag{}

<icon>(pack:(lucide, ...), name:<string>)
<embed>(url:<url>, path:<path>, file:<file>, ref:<ref>)[alt]
<index>()[ title ]
<codeblock>(lang:<language>)[ <code> ]
<blockquote>[ <content> ]
<callout>(variant:(info, note, caution, ...))[ <content> ]

#(number:<uint>)[ heading ]

-.(number:<uint>)[ <content> ]
-(variant:(x, !, ?, TODO,...))[ <content> ]

---[ display ]---

${<var>}
$(<var>)

// comment
/* comments */
```
