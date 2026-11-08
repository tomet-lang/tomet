# oneline-profile-and-vault-separator

Follow-up of the format options (`paragraph.separator`, `map.align_values`, `map.max_key_width`),
committed as `9101d055` (`feat(format): choose how paragraphs are separated and align map values`).
That work's own file is `.agents/workspaces/format-layout-options/.agents/tasks/format-layout-options.md`
(the workspace is kept for now).

State: investigation only. Nothing of this file is implemented.

The catalog that holds the `[ ]` / `|` spelling and the separator is `.agents/tasks/syntax-style-catalog.md`.

## 1. `paragraph.separator: blank` in `vault.config.tmt`, some day

Not set now. `map.align_values: true` is set (uncommitted, in this workspace's `vault.config.tmt`).

Why not yet: with `blank` set for the vault, 7 files change that do not change otherwise
(`tmtroot/dict/tomet.dict.tmt`, `tmtroot/readme.tmt`, `tmtroot/spec/tome.tmt`,
`tmtroot/docs/moc/samples.tmt`, `tmtroot/dev/design/indegration.tmt`,
`tmtroot/dev/dropped/0003-square-bracket-list-literals.adr.tmt`,
`tmtroot/deprecated/dropped-comment.tmt`). The one that matters is the dictionary: it is
`::profile(oneline)` and written as `;@term(...)` lines, which `blank` would turn into blank-line
separated terms. (`semicolon` for the vault would change 105 files; not wanted either.)

What has to come first, either of:

- the dictionary says so itself: `::format(paragraph: { separator: semicolon })` in
  `tmtroot/dict/tomet.dict.tmt`. A document's own declarations lie over the vault's (tried, works); or
- the formatter treats `::profile(oneline)` as implying `semicolon` (section 2).

## 2. `::profile(oneline)`: what exists, what is possible

What exists today:

- `Profile::Oneline` (`tomet-semantics-declaration/src/profile.rs`) is a name. `tomet check` only
  validates that the name is a profile. Nothing else reads it: not the parser, not the formatter,
  not the printer.
- `tomet-validator-profile` checks `standalone` only. Its module doc says `oneline` "needs the lines
  of the source, which a parsed `File` does not carry". `spec/declarations/std.profile.declaration.tmt`
  lists the check and "the syntax it removes" under 未実装 ("derived from the real grammar").
- `tmtroot/dict/tomet.dict.tmt` is the only document that declares it. It is `;@term(...)` lines,
  `| ` continuation lines (`MARGIN` in the CST) and one blank line (after the header).
- `crates/tomet-syntax-ast-parser-oneline/` is an empty directory with no tracked file.

What the author is leaning to: no parser for it yet.

Possible, in this order (each needs the one before):

1. The definition. Settled by the author: `|` runs are inside `oneline` (they are what carries it).
   The dictionary is the acceptance test.
2. A check, `tomet-validator-profile`, from the source text, not the `File`: the CST
   (`tomet-syntax-cst-parser`) is lossless and has NEWLINE tokens and ranges. The rule is "no
   composite node holds a line break, except the ones that are lines of their own" (ROOT, DOCUMENT,
   SECTION, LIST, a `|` run). The list of removed syntax then falls out of the rule, generated the way
   `tests/src/syntax_report.rs` generates `tests/SYNTAX.md`, not kept by hand next to the grammar.
   `tomet check` already reads the source (`apps/cli/src/commands/check.rs`), so the text can be
   passed down. Layering of the new dependency: read the `.writ.tmt` first.
3. Writing it. Probed (parse both, compare `File`): the `[content]` of an element, on one line or
   several, is the same `File` as its `|` run (`@x[ one\ntwo ]` = `@x\n| one\n| two`; the dictionary's
   `;@term("v")[...]` = `;@term("v")\n| ...`). So respelling to `|` and to a `;` separator never
   changes the document: it is a choice of spelling, like `group_order` or `list.multiline.style`.
   This corrects an earlier claim of mine that a formatter cannot make a document oneline.
   Not covered: a bare multi-line paragraph (`one\ntwo`) has no `|` spelling that keeps the `File`
   (`@p\n| one\n| two` adds an element). How oneline writes such prose is open.
   Whose job: a print style first. The printer is where AST-born documents get their spelling
   (`tomet-convert-*` output, AST edits); `tomet-convert-standalone` is not the model (it changes the
   `File` and leaves printing to the command), because here the `File` stays. But the printer
   DROPS COMMENTS (probed: `a // note` and a line comment are gone after `document_to_tm`), so for
   a hand-written file the only comment-keeping route is the text formatter (it already has
   `File`-preserving respellings: `reorder.rs` swaps group order). The same rule would then live
   twice; decide whether that is accepted before writing either.
   The check (2.) is `tomet-validator-profile`'s `standalone_findings` counterpart, and is what both
   would be held to.
4. Last: a parser fast path that reads lines in parallel. A promise has to be checked before it is
   trusted, so after 2. `Dialect` (reads differently) is not the same thing as a profile (does not).

## Also found

- `tmtroot/spec/cli/format.tmt` names the config file `default.config.tmt` / `tomet.config.tmt`; the
  file that is looked up is `vault.config.tmt` (`tomet-load-config`, `VAULT_CONFIG_FILE`). Not fixed.
- Broken on `main` before the format options, found while checking them, untouched:
  `tomet spec --all --check` says `tmtroot/generated/cli.tmt` is out of date;
  `tomet export --check` cannot read 63 files (`tmtroot/generated/crates/*.tmt` use the old
  `::meta{...}` form); AGENTS.md names a `tomet api` subcommand that does not exist.
- `tmtroot/generated/config.tmt` was regenerated with `tomet spec config` for the new keys and so also
  changed two lines unrelated to them (`::fields` -> `::field`, `::vocabulary.ns()`): `main`'s file
  was stale. It is in `9101d055`.
