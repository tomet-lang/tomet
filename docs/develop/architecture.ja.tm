#[ `@heading(level:)[]{}` ]

- `#[]{}`

#[ `@link(target:"scheme:address")` ]

- target
-- "tm:tmfile"
-- "https://example.com"


---[ `@link(target:tm("tmfile"))` ]---

- tm("tmfile")が安全に"tm:tmfile"に展開。

---[ `@link(tm:tmfile)` ]---

- targetを省略した記法。最初がtarget:と推論される。これは、argsの中で、targetのみ。

#[ その他 ]

```(tm)
```
---(lang:tm)[
  content
]---
---[]---
