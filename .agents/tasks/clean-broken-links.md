# Clean the broken links, so `sisho check` can run `tomet check-links`

`tomet check-links` is back (0.5.0, `feat(cli): bring back check-links`). `sisho check` does not run it yet because this
repository is red: **90 broken links** on 2026-10-24 (twrit has 2, sisho 0). When this repository is green, sisho adds the step
(see `sisho/.agents/tasks/check-links-in-check.md`). Until then `sisho tomet check-links .` is the way to run it.

## Measure

```
XDG_CACHE_HOME=$(mktemp -d) HOME=$XDG_CACHE_HOME tomet check-links .
```

The cache is `~/.cache/tomet/links.sqlite3`; pointing `XDG_CACHE_HOME` and `HOME` elsewhere keeps a measurement out of it.
`--json` gives the same as one object (`broken[].source/target/line/column`). Exit 1 while anything is broken or unreadable.

2026-10-24, 218 files, 302 links, 90 broken. By the first path component of the target:

| target starts with | count | what they are (guess, check each) |
|---|---|---|
| `docs/` | 45 | the old `docs/` tree (now `tmtroot/deprecated/docs/` or `tmtroot/spec/`) |
| `.tomet/` | 6 | `.tomet/` moved to `tmtroot/schema/` |
| `.` | 6 | relative paths |
| `schema/`, `generated/`, `dev/` | 2 each | paths that lost their `tmtroot/` |
| the rest | ~27 | not looked at |

## Steps

- [ ] Land the `tmtroot/` restructure in the default workspace first (it moves `plans/roadmap.tmt`, `.tomet/`, `docs/` and
      more): a good part of the 90 is the same stale paths it fixes. Measure again after it lands.
- [ ] For each remaining link: point it at where the target is now, or remove the link, or (for `deprecated/`) decide the
      history may keep a dead reference. Do not silence the check for a whole directory without writing why.
- [ ] A decision for `tmtroot/deprecated/`: its links are history. Either fix them like the rest, or tell the check to leave
      the directory alone (`workspace.ignore` / `unswept` in `default.config.tmt` would also hide its documents from
      `tomet check`, `format` and `export`; not wanted).
- [ ] `tomet check-links .` exits 0. Then do the sisho task.

Known and not a bug: `tomet-links` says references match "regardless of case and extension"; in fact it is an exact match of the
file name, or of the file name without the extension (`spec/cli/check-links.tmt`, "Where it differs").
