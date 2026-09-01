# Element sigil rework: shape axis, namespaces, `+++` raw bodies

`<T>` and `@name` were introduced to separate officially-defined elements
from user-defined ones, but `classify` (`crates/tomet-semantics/src/kind.rs:144`)
matches `Sigil::Type(name) | Sigil::At(Some(name))` into the same arm, so
the distinction carries no information today. A design session on
2026-09-02 replaced the axis rather than repairing it. Every decision
below is settled; nothing has been implemented yet.

The change is one breaking release, not several: the sigil swap, the
namespaces and the fence all rewrite the same documents, so they ship
together.

## Decisions

### Sigils encode shape, not origin

- `@name` — inline element
- `#name` — block element
- `#[ ... ]` remains the heading; it is the name-omitted form of
  `#heading`, the same pattern the language already uses elsewhere.
  `##[ ... ]` is level 2.
- `<T>` is removed. The block-position directives that used `@`
  (`@meta`, `@config`, `@settings`, `@import`, `@links`) move to `#`.
  Inline `@link` / `@table`-style uses that were already the right shape
  do not change.
- Disambiguation after a run of `#` is one character of lookahead: `[`
  (optionally preceded by inline whitespace) means heading, an identifier
  means block element. A run longer than one `#` requires `[`.
  `is_heading_start` (`crates/tomet-syntax-parser/src/document.rs:109`)
  already implements the heading half; the identifier half is currently a
  fall-through to paragraph text, so the slot is free.
- `#name` NOT followed by one of `(`, `[`, `{`, `+++`, end-of-line falls
  back to literal text. This is error tolerance for prose, not a hashtag
  feature — hashtags are not being introduced. Note the consequence: a
  bare `#tag` alone on a line still lexes as an element and then fails as
  an unknown bare name (see namespaces), which is the intended report.
- `@[ ... ]` is removed. It currently parses and yields
  `ElementKind::Custom("at")`, a meaningless node, because the arg-key
  inference that once gave it meaning was retired.

### Namespaces encode origin

- Separator is `.`: `#deck.bookmark(name:foo)`, `@deck.ref(id:3)`.
  Chosen over `:` (collides with `key:value`, the `:(){}` connect syntax,
  and a possible future `::`) and `/` (reads as a path).
- Bare names are reserved for std. Custom elements MUST be namespaced.
- An unknown bare name is an error. It must no longer fall through to
  `ElementKind::Custom` — that silent fallback, not the sigil, is where
  the original ambiguity actually lived.
- Binding: `#import(file:./deck.tmt, as:deck)` — one arg group, not two.
- Shorthand is allowed while authoring (`@ref(id:3)`); `tomet fmt`
  expands it to the fully-qualified form on save, so what lands on disk
  and in a diff is always self-describing. Ambiguous shorthand (the same
  name reachable through two namespaces) is an error, never a silent pick.

### Raw and embedded bodies use a `+++` fence

Replaces both `(content:raw)[...]` and `(format:x){...}`.

    #memo+++
    don't forget: check [this] and [that]
    +++

    #meta(format:yaml)+++
    key: value
    +++

- The opener sits at the end of the element head, so every block element
  line still starts with `#`. The closer is a line containing only `+++`.
- Longer runs (`++++`) escape a body that contains a `+++` line, matching
  the backtick fence's rule.
- Unterminated runs to EOF, matching `parse_fenced_code_block`
  (`crates/tomet-syntax-parser/src/codeblock.rs`).
- Exclusive with `[content]` and `{value}`. No single-line form.
- `format:` survives as pure interpretation — which parser receives the
  opaque string — with no effect on lexing. This kills the early-exit bug
  in `embedded_format.rs`, which tracks brace depth and so terminates
  `#meta(format:yaml)` at an unquoted `}` inside legal YAML.
- ``` fences stay codeblock-only. `+++` was chosen over `---` (taken by
  thematic breaks, including `---[ Title ]---`), `***` (collides with
  emphasis) and `:::` (see the `.` rationale above).

### `{}` is always data

- Parse `{...}` uniformly into entries that are each a `key: value` pair
  or an element. Data-vs-children stops being a parse-time branch driven
  by the element name and becomes a view computed in `tomet-semantics`.
- `#links{ (1)[...] (2)[...] }` is otherwise unchanged. An earlier draft
  moved children to indentation; that was dropped, since it bought
  nothing and would have added a whole indentation spec.

### The invariant this is all for

**The parser must build the tree without consulting any element
vocabulary.** External files may add constraints or change rendering;
they must never change the tree.

Consequences for the external `elements:` map: `args` and `singleton`
stay (schema and validation), `types.*.style` stays (rendering),
`content` and `placement` are deleted — the fence and the sigil carry
them now, at the use site.

### Explicitly unchanged

Headings, lists, tables, `**strong**` / `*em*` / `==mark==` / `` `code` ``,
`---` breaks, `${...}`, comments, and the "at most one of each `(args)`
`[content]` `{value}`, any order" grouping rule.

## Resolved (2026-09-02, during planning)

- [x] **`${...}` does not expand inside a `+++` fence.** Confirmed against
      `default.config.tmt:7-22`, which stores 13 `"${1}"` macro templates
      inside `@config(format:json){...}`. They are consumed by literal
      `${N}` string splitting in `MacroPattern::from_template`
      (`crates/tomet-transform/src/macro_rewrite.rs:19-36`), so they must
      reach that function verbatim. This also preserves current behaviour:
      `${...}` is already not expanded in embedded-format bodies,
      `<codeblock>` bodies, or `content:raw` bodies.
- [x] **Element names are ASCII.** `ident := [A-Za-z_][A-Za-z0-9_-]*`,
      `name := ident ('.' ident)*`. `.` is demoted from an identifier
      character to the namespace separator. The repo has zero non-ASCII and
      zero dotted element names, so this costs no migration, and it matches
      `tree-sitter-tomet`'s existing `identifier` regex (`grammar.js:443`),
      so step 10 needs no charset work. Consequence: `#タグ` never lexes as
      an element and stays prose — the right default for the Japanese docs.
      Note `.` must stay legal in *map keys* (`default.config.tmt` has
      `"url.wiki"`; `tomet-semantics/src/config.rs:206-225` prefix-matches
      `macros.`), so keys and element names need separate lexers now.
- [x] **`ElementValue` becomes `Group(Vec<Entry>)` + `Raw(String)` +
      `Interp`**, with `Entry = Pair(String, Value) | Element(Element)`.
      Chosen over a `Group { data, children }` struct because the latter
      keeps the data/children split as fields instead of making it a view,
      and loses the source order of a mixed group — which would make the
      printer silently reorder. Non-map `{}` bodies (`{[1,2,3]}`, `{"str"}`,
      `{bare}`) become parse errors; the `+++` fence covers that case now.
      No corpus document and no test uses them. Note `parse_value`
      (`value.rs:12-20`), the data-only-document entry point `serde_tomet`
      uses, never touches `ElementValue` and is unaffected.
- [x] **Migration is a one-shot throwaway text tool**, not a legacy parse
      path and not a permanent `tomet migrate`. Nothing is published, so no
      external upgrade path is owed. It must still do real bracket matching
      (reuse `find_matching_bracket`/`skip_quoted` from `value.rs:113-214`)
      rather than regex, or `<memo>(content:raw)[don't [nest] this]`
      converts wrongly.
- [x] **Synthesized sigils remap by shape.** `Sigil::Type` is also the AST
      form of constructs whose *surface syntax* is unchanged —
      `em`/`strong`/`mark` (`inline.rs:292`), `hr` (`document.rs:59`,
      `heading.rs:163`), `codeblock` (`codeblock.rs:69`), `ol`/`ul`
      (`tomet-syntax-tree/src/element.rs:289`). They become `#hr`,
      `#codeblock`, `#ol`, `#ul`, `@em`, `@strong`, `@mark`. Step 2 below
      does not mention these; they are part of it.
- [x] **`#` requires adjacency.** A `#` run immediately followed by an
      identifier (no space) is a block element; a `#` run + optional inline
      whitespace + `[` is a heading. The repo has 0 occurrences of `#ident`
      and 39 of `# word` (Markdown headings in `.tmt` READMEs, shell/YAML
      comments inside fences), so adjacency keeps every existing line prose.
      `@` keeps requiring a following group — that is what keeps
      `me@example.com` prose; `#` additionally accepts end-of-line.

## Correction to the analysis above

`parse_value_group` (`crates/tomet-syntax-parser/src/element.rs:264-292`)
is **not** name-driven. It branches on a `(` lookahead, so `@links{...}`
yields `Children` because of the `(`, not because the name is `links`.
Step 6 is therefore an independent cleanup, not something the invariant
requires — the step-1 guard passes with that branch left in place.

It stays in scope because it fixes a real bug: `@links{ note:x (1)[a] }`
parses today as `Data(Map([("note", String("x (1)[a]"))]))` — the element
is silently swallowed into a scalar string. (`@links{ (1)[a] note:x }`
errors instead.)

The places the parser *does* consult vocabulary are four, all retired by
steps 3 and 5: `is_format_target_element` and `local_format_key`
(`element.rs:238-251`), `is_verbatim_content` and `is_codeblock`
(`codeblock.rs:103-113`). A fifth, `is_config` (`element.rs:253-262`),
threads a document-wide `running_format` and goes with them.

## Steps

- [x] 1. Guard test — `tests/src/vocabulary.rs`, target `vocabulary`.
      Done. Rather than emptying the builtin table (a `const` array, and
      the parser never reads it or the external `elements:` map anyway),
      it uses hand-written source pairs differing in exactly one
      identifier the parser must not recognize, parses both, normalizes
      that identifier, and compares the trees. A corpus sweep was
      rejected: rewriting element names inside arbitrary Japanese prose
      also rewrites the same token where it occurs as plain text, so the
      trees would differ for reasons unrelated to vocabulary.
      Currently fails on all 5 cases — `<codeblock>`, `(content:raw)`,
      `(format:)`, `@meta`'s bare-string format shorthand, and
      `@config`'s document-wide `running_format`. Acceptance criterion
      for steps 3-7.
- [ ] 2. AST (`crates/tomet-syntax-ast/src/lib.rs`). Drop `Sigil::Type`;
      give the sigils a namespace slot and a `#` variant. Replace
      `ElementValue::{Data, Children}` with a uniform group plus a
      `Raw(String)` arm, keeping `Interp`. Update the `Sigil` matches in
      `tomet-transform/src/structural.rs:75`, `tomet-semantics/src/positional.rs`,
      `tomet-semantics-resolver/src/settings.rs` and
      `tomet-syntax-parser/src/element.rs:239,254`.
- [ ] 3. Parser: `#` as the block sigil, with the heading/element
      lookahead and the fall-back-to-text rule. Remove `<T>`.
- [ ] 4. Parser: namespaces, and `#import(file:..., as:...)`.
- [ ] 5. Parser: the `+++` fence. Retire `(content:raw)` and delete
      `embedded_format.rs`'s brace counting; `format:` becomes a
      post-parse dispatch.
- [ ] 6. Parser: uniform `{}`.
- [ ] 7. Semantics: unknown bare name becomes an error; data-vs-children
      becomes a view; shape-mismatch diagnostics (e.g. a block-only
      element written with `@`).
- [ ] 8. Trim the external `elements:` surface to `args` / `singleton` /
      `types.*.style` and reject the removed keys with a message pointing
      at the use-site replacement.
- [ ] 9. Formatter: namespace expansion on save, plus a migration pass
      (`<T>` to `#T`, the `@` directives to `#`, `(content:raw)[...]` and
      `(format:x){...}` to `+++`).
- [ ] 10. tree-sitter. Every construct is context-free after this work,
      so `grammar.js` can be faithful for the first time rather than an
      approximation that drifts.
- [ ] 11. Corpus fixtures under `tests/fixtures/` for each new construct,
      then `cargo test -p tomet-tests -p tree-sitter-tomet`.
- [ ] 12. Update `docs/` (Japanese, per `docs/develop/docs-guide.md`) and
      the editor extensions.
