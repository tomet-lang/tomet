@settings(path:../../docs.settings.tm)
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

@link(url:<url>, path:<path>, file:<file>, ref:<ref>)[ display_name ]
@tag{}

<icon>(pack:(lucide, ...), name:<string>)
<embed>(url:<url>, path:<path>, file:<file>, ref:<ref>)[alt]
<index>()[ title ]
<codeblock>(lang:<language>)[ <code> ]
<blockquote>[ <area> ]
<callout>(variant:(info, note, caution, ...))[ <area> ]

#(number:<uint>)[ heading ]

-.(number:<uint>)[ <area> ]
-(variant:(x, !, ?, TODO,...))[ <area> ]

---[ display ]---

${<var>}
$(<var>)

// comment
/* comments */
```
