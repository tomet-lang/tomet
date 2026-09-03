# Element sigil rework: shape axis, namespaces, `+++` raw bodies

`<T>` and `@name` were introduced to separate officially-defined elements
from user-defined ones, but `classify` matched
`Sigil::Type(name) | Sigil::At(Some(name))` into the same arm, so the
distinction carried no information. A design session on 2026-09-02
replaced the axis rather than repairing it.

**Status:** steps 1-7 and 9-11 are done. Steps 8 and 12 remain, plus the
namespace-binding work noted under step 7 and the open question at the
bottom. **A design session on 2026-09-03 retracted the shape axis
itself** — see "One sigil: `@`" below and steps 13-18.

The change is one breaking release, not several: the sigil swap, the
namespaces and the fence all rewrite the same documents, so they ship
together.

## Decisions

### One sigil: `@`. Position decides placement (2026-09-03)

The shape axis is removed. `#` stops being a block sigil; `@` is the only
element sigil. `#` stays as the heading marker and nothing else.

It carried no information — the same defect as the origin axis it
replaced, in a new spelling:

- **The parser never consulted it.** `is_inline_element_start`
  (`crates/tomet-syntax-parser/src/element.rs:21`) and
  `is_block_element_start` (`:47`) differ only in the glyph and in
  whether end-of-line is accepted; both then require `( [ { :` or a
  fence. Placement in the tree came from position, not from the sigil.
- **The vocabulary already holds the shape.** `required_shape`
  (`crates/tomet-semantics/src/kind.rs:229`) knows every builtin's shape,
  and `shape_mismatch` (`:253`) only checks the author's sigil back
  against that table.
- **For custom elements it constrains nothing** —
  `a_custom_element_may_take_either_shape` (`kind.rs:364`).

It also failed to prevent the accident it was introduced for. Measured
against `parse_document` with a temporary test, since deleted:

| source | blocks before this change |
| --- | --- |
| `@link(…)[Tomet] は軽量…です。` | **2** (Element + Paragraph) — the sentence is torn |
| `この言語の名前は` ⏎ `@link(…)[Tomet] といいます。` | **3** (Paragraph + Element + Paragraph) |
| `この言語の名前は` ⏎ `Tomet といいます。` | 1 (correct) |

Rows 1 and 2 come from `document.rs:76-79` (any line-start `@` element is
taken as a block) and `inline.rs:73-79` (a paragraph breaks when its
continuation line starts with an element).

**The rule.** An element is placed as a block when both hold, and is part
of the paragraph otherwise:

1. **it is in block context** — `parse_document`'s loop position:
   document start, after a blank line, or after a block closed. A
   paragraph's continuation line is not block context.
2. **it ends its line** — from the element's end, only inline whitespace
   and comments before the line break or EOF.

Neither consults the element vocabulary, so the step-1 invariant stands.

The group requirement moves from the sigil to the position too. In block
context a bare `@name` + end-of-line is an element (this is where `#`'s
end-of-line allowance lands); mid-paragraph an element still requires
`( [ { :` or a fence, which is what keeps `me@example.com` prose.

**Placement is recorded, not declared.** `Element` gains a `placement:
Placement` field that the parser derives from position. It is needed
because `Element.content` is `Option<Vec<Inline>>`, so a block element
sitting at a line start inside `[content]` (`inline.rs:159`) has nowhere
else to record that it was a block. Making `content` a `Vec<Block>` was
rejected for now: `.content` has 162 uses across 19 files.

Shape stays a semantic property. `required_shape` remains its single
source of truth, and validation compares it against *placement*, which
turns the report into a real error ("`heading` inside a paragraph")
instead of a spelling one ("`#em`").

`Block` / `Inline` in `tomet-syntax-ast/src/lib.rs:214,250` stay — they
express position in the tree, which is real. Only the surface declaration
goes.

`+++` still sits at the end of the element head; the sentence "every
block element line starts with `#`" now reads `@`.

### ~~Sigils encode shape, not origin~~ (retracted 2026-09-03)

Kept for the record. Superseded by the section above; `#name` is no
longer an element spelling.

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
      Rather than emptying the builtin table (a `const` array, and the
      parser never reads it or the external `elements:` map anyway), it
      uses hand-written source pairs differing in exactly one identifier
      the parser must not recognize, parses both, normalizes that
      identifier, and compares the trees. A corpus sweep was rejected:
      rewriting element names inside arbitrary Japanese prose also
      rewrites the same token where it occurs as plain text, so the trees
      would differ for reasons unrelated to vocabulary. **Passes.**
- [x] 2. AST. `Sigil::{Block(Name), Inline(Option<Name>), Bare, Dollar}`
      with `Name { namespace, name }`; `ElementValue::{Group(Vec<Entry>),
      Raw(String), Interp}` with `Entry::{Pair, Element}`. `as_data` /
      `as_children` are the computed views. Synthesized sigils remapped by
      shape (`#hr`, `#codeblock`, `#ol`, `#ul`, `@em`, `@strong`, `@mark`).
- [x] 3. Parser: `#` block sigil, heading-vs-element lookahead, and the
      fall-back-to-text rule. `<T>` and `@[...]` removed. Element names are
      ASCII; map keys stay Unicode and may contain `.`.
      Also: a `#` element is recognized inside `[content]` at a line start,
      which is what lets `#references[` hold `#id(...)` entries.
- [x] 4. Parser: namespaces. `#import(file:..., as:ns)` **parses**, but
      the binding is not resolved yet — see step 7's remainder.
- [x] 5. Parser: the `+++` fence. `(content:raw)` and `(format:x){...}`
      retired; `embedded_format.rs` moved to `tomet-semantics/src/embedded.rs`
      as a post-parse dispatch, and `tomet-parser` dropped its
      serde_json/serde_yaml/toml dependencies.
- [x] 6. Parser: uniform `{}`. Fixes `#links{ note:x (1)[a] }` silently
      swallowing the element into a scalar string.
- [x] 7. Semantics: `classify` returns `Result<_, UnknownName>`;
      `classify_lenient` is the rendering fallback. `shape_mismatch`
      added, surfaced by the validator as `UnknownElement` /
      `ShapeMismatch`. The two divergent positional tables collapsed into
      one. `settings`, `import`, `references`, `id` joined `BUILTIN_KINDS`.
      **Remaining:** namespace binding resolution (`#import(as:)`),
      shorthand expansion, and ambiguous-shorthand errors.
- [ ] 8. Trim the external `elements:` surface to `args` / `singleton` /
      `types.*.style` and reject `content` / `placement` with a message
      pointing at the use-site replacement. Note `SettingsSchema` reads
      only `positional` today, so this is mostly docs plus rejection.
      `docs/docs.settings.tmt` uses both removed keys and must change.
- [x] 9. Formatter, printer and the one-shot migrator.
      `scripts/migrate-sigils.py` and `scripts/migrate-rust-strings.py`
      did their run and have been deleted, as planned.
      Codeblocks now print as ``` fences (a `#codeblock[...]` cannot round
      trip, since `[content]` is ordinary markup now).
      **Remaining:** namespace expansion on save.
- [x] 10. tree-sitter. `block_element`/`inline_element` replace
      `type_element`/`at_element`; `#`+name is one `block_sigil` token, at
      the same token precedence as `heading_marker` so match length decides
      between them. The `+++` fence is one external token covering opener,
      body and closer together, which is what keeps the scanner stateless.
- [x] 11. Corpus fixtures — `tests/fixtures/syntax/{sigils,fences}.tmt`.
      Both exception lists are now empty: `KNOWN_UNPARSEABLE`
      (`cheatsheet.tmt` parses) and `KNOWN_FORMAT_CHANGES_DOCUMENT`
      (`examples/bookmark.tmt` survives formatting — the fence removes the
      bracket-matching failure that caused it).
- [ ] 12. Update `docs/` (Japanese) and the editor extensions.

### Steps 13-18 — retracting the shape axis (2026-09-03)

- [x] 13. AST: `Sigil::{Block,Inline}` collapse to `Sigil::Named(Name)`;
      `Placement::{Block,Inline}` added and carried on `Element`.
      Synthesized elements get their placement at construction
      (`tomet-syntax-tree/src/element.rs`): `heading`/`hr`/`codeblock`/
      `ol`/`ul` are `Block`, `em`/`strong`/`mark` are `Inline`.
- [x] 14. Parser: one element-start predicate, with the end-of-line
      allowance scoped by position instead of by sigil. `#` keeps only
      `is_heading_start` (`document.rs:109`).
- [x] 15. Parser: apply the two-condition rule. Drop the element clauses
      from the paragraph stop set (`inline.rs:73-79`); take a line-start
      element as a block only when it ends its line, probing with a cursor
      copy the way `eat_list_marker_with_indent` (`list.rs:47-54`) does.
      Check whether the single-element promotion (`document.rs:129-139`)
      is still reachable.
- [x] 16. Semantics: `shape_mismatch` compares `required_shape` against
      `el.placement`; the validator's message becomes about placement, not
      spelling. Collapse the two-arm matches in `positional.rs:272` and
      `transform/structural.rs:81`.
- [x] 17. Printer and tree-sitter: one sigil in
      `format-printer/src/lib.rs:434-442`, and a `Placement::Block`
      element starts and ends its own line so it round trips. `grammar.js`
      drops `block_element`/`block_sigil`, and `heading_marker` regains
      sole ownership of `#`.
- [x] 18. Rewrite the corpus and `docs/` (342 line-start `#name`
      occurrences across 62 files), regenerate `tests/SYNTAX.md`, and add
      the regression cases from the table above. They live in the report
      itself, which is CI-verified: the torn-sentence case, the
      continuation-line case, `@memo` alone on its line, and
      `me@example.com`.

### Fallout worth knowing about

`tests/fixtures/examples/bookmark.tmt` now prints as one long paragraph
instead of alternating element/paragraph blocks. Its elements are written
`@bookmark(...)｛ ... ｝` with **fullwidth** braces, so the element head
ends at `)` and the `｛...｝` text follows it on the same line -- the
element does not end its line, so it is inline, and with element triggers
no longer breaking a paragraph the whole run lazily continues into one.

The document was already parsing wrong before this change (the old
reference shows the same `｛...｝` split off as loose prose); it is wrong
differently now. No other corpus document changed shape -- the rest are
pure `#`->`@` renames, because a normal `@name(...)[...]` line ends its
own line and stays a block.

**Update (2026-09-03).** The fullwidth braces were fixed in the fixture
and it now reads `@bookmark(...){ ... }+++ ... +++`, which hits a
different wall: **`{value}` and a `+++` fence are exclusive** (the rule
recorded above, enforced at
`crates/tomet-syntax-parser/src/element.rs:201` via
`el.value.is_none() && el.content.is_none()`). Taking the `{value}` group
fills `el.value`, so the following `+++` is not read as a fence, the
element cannot end its line, and the rest of the document flows into one
paragraph.

So the language has no spelling for "an element with attributes *and* a
verbatim body", which is exactly what `bookmark` wants. Deferred by
decision -- it is one example document, not a blocker. The bookmark
snapshot references under `tests/ref/` therefore **record output that is
known to be wrong**; they are descriptive, not blessed. When the
exclusivity is revisited, regenerate them rather than diffing against
them.

Options, if it is picked up: move the attributes into `(args)` (works
today), or let `{value}` and `+++` coexist by giving the raw body its own
slot instead of sharing `ElementValue::value`.

## 実装の現況は `tests/SYNTAX.md` を見る

パーサが今どの構文を受け付けるかは `tests/SYNTAX.md` が生成物として持って
いる（`tests/src/syntax_report.rs` の表 + `tomet-parser` から生成、CIで検証）。
`docs/spec/` は規範、`SYNTAX.md` は記述。食い違いは黙って揃えず、差分として
読むこと。

その一覧が実際に見つけたもの（対応済み）:

- [x] **名前なし `@` を撤去。** `@(url:...)` の推論は `1d0b7b2` の
      `infer.rs` 削除で消え `@link(target:)` に統一されていたのに、
      パーサだけが構文を受け付け続け、`classify` は意味のない
      `Custom("at")` を返していた。`Sigil::Inline(Name)` にして、
      名前なしを表現できなくした。自動リンクも `@link(target:)` を
      作るよう変更。
- [x] **`#id(taskA)` を撤去。** `<T>` 消滅で `<id:taskA>` が綴りを
      失ったとき、私が独断で `#id(...)` という綴りを発明していた。
      依頼されていないので戻した。`parse_remote_connection_element` は
      `None` を返すだけになり、リモート接続には**現在綴りがない**。

## 未決 — リモート接続の綴り

`<id:taskA>:{ priority: high }` は `85b73c8` で入った機能だが、`<T>` の
撤去で書けなくなった。代わりの綴りは決めていない。resolver 側のロジック
（`#references[...]` を走査して対象要素に属性を配る）は残してあるが、
`RemoteConnection` を作るものが何もないので機能していない。

- 綴りを決めて `parse_remote_connection_element` を書き直す
- あるいは機能ごと削除する

## Open — needs a decision

**Which namespace do the docs' own custom elements take?**

Bare names are reserved for `BUILTIN_KINDS`, so every custom element in
`docs/` is now an `UnknownElement` *validation* error (they still parse).
By frequency: `bookmark` (27), `line` (12), `callout` (7), `node`,
`timestamp`, `index`, `dirs`, `memo`, `foot`, `tag`. Many occurrences are
inside code fences as examples, but the example documents under
`docs/examples/` use them for real.

They need a namespace plus an `#import(file:..., as:ns)` binding — e.g.
`#import(file:docs.settings.tmt, as:docs)` and then `#docs.bookmark(...)`.
The namespace name is an editorial choice about the docs, so it is not
being guessed here. This blocks finishing steps 11 and 12.

## Known-unrelated failures

`docs/spec/builtin-functions.tmt` and `docs/design/ideas/idea.tmt` do not
parse, on `${...}` constructs that are documented but unimplemented
(`${ref(id(x).contents(y))}`, `$regex(/*.svg/g)`). Pre-existing;
`builtin-functions.tmt` was never touched by this work.
