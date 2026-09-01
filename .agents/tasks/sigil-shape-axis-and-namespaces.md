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

## Open

- [ ] Does `${...}` expand inside a `+++` fence? Provisionally **no** —
      `default.config.tmt` stores `"${1}"` as a macro template that must
      survive parsing verbatim, which leaves little room for a different
      answer. Confirm and record.
- [ ] Identifier charset for element names. If names are ASCII-only, a
      non-ASCII `#タグ` never lexes as an element and the fall-back-to-text
      rule above becomes nearly unreachable. Decide before implementing
      the lookahead.

## Steps

- [ ] 1. Write the guard test first: parse a corpus with the builtin
      table and the external `elements:` map emptied, and assert the tree
      shape is identical to parsing with them populated. It fails today
      in four places (name-driven `{}`, `content:raw`, `format:`, external
      `elements:`). It is the acceptance criterion for steps 3-7.
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
