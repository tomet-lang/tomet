# Where a `{...}` group belongs in an output format

Two unresolved questions left over from the Pandoc bridge (finished
2026-09-03), both about the same thing: a `{...}` group holds data *and*
can hold elements, and the writers disagree about where either half goes.

**Status:** not started.

## 1. A group entry loses its identity on the way back (a real bug)

`@deck.card{ title: 混在, (a)[ 対と要素が同じ並びに入る ] }` round-tripped
through Pandoc comes back as:

```tmt
@deck.card[@bare[対と要素が同じ並びに入る]{ value: a }
]{ title: 混在 }
```

Two things went wrong.

- The entry moved from the `{...}` group into `[content]`. Pandoc has no
  way to say "this `Div` was a group entry rather than part of the body",
  so `to_pandoc` renders group elements as body blocks
  (`element_body_blocks`) and `from_pandoc` has nothing to put them back
  with.
- It lost its `Sigil::Bare` and became an element *named* `bare`, because
  `ElementKind::Bare::as_str()` is `"bare"` and that string is what the
  `tomet-<name>` class carries. `@bare` is not a builtin, so **the
  round-tripped document does not validate.**

**The second half is fixed** (2026-09-08). `Sigil::Bare` travels under a
reserved attribute key, `tomet-sigil: bare`, rather than in the
`tomet-<name>` class -- a key and not another class because a class
collides: a document whose `@kind` names a vocabulary declaring `bare`
writes `@bare` unqualified. `a_bare_entry_does_not_come_back_as_an_element_named_bare`
in `tests/src/snapshot.rs` holds it, and the round-tripped fixture's
errors are now exactly the source fixture's.

**It made the first half visible rather than smaller.** The entry still
lands in `[content]`, where `Sigil::Bare` has no legal spelling, so the
printer writes `[x]{ value: a }` and *that does not re-parse as an
element* -- it comes back as the literal text `[x]{ value: a }`. Before,
the entry survived re-parsing as `@bare[x]` and failed validation
instead. So the round trip traded an invalid element for a lost one:

    before   @bare[ひとつめ]{ value: "1" }   parses, does not validate
    after    [ひとつめ]{ value: "1" }        validates, is text

Both are the same underlying gap -- a group entry has nowhere to be in
`[content]` -- and only the first half closes it.

The first half needs a representation Pandoc does not have: a reserved
class meaning "group entry", plus `from_pandoc` putting it back in
`{value}` rather than in the body, is the obvious shape but it is a real
design decision, not a patch.

Pinned by `pandoc_round_trip_is_stable`; see
`tests/ref/syntax/sigils.pandoc.roundtrip.tmt`.

## 2. Does `{value}` belong in the attributes or in the body?

The two writers answer differently today, and neither is obviously wrong:

- `tomet-pandoc` puts it in `Attr` (`to_pandoc::attr_of`). `{}` has been
  "always data" since the uniform-group change, and Pandoc's `Attr` is
  where data goes.
- `tomet-html` renders it as a visible span in the body
  (`render_element_value`, `crates/tomet-convert-html/src/lib.rs:668`),
  so `@deck.card{ id: x }` *shows* `id: x` to the reader.

They can legitimately differ -- HTML is a rendering, Pandoc's AST is an
interchange format -- but that should be a decision with a reason, not the
accident it currently is. Worth answering alongside 1, since both are
"what is a `{...}` group, structurally".

Note the HTML writer *also* puts data in attributes, via
`push_data_attrs`, but only from `(args)`. So in HTML `(args)` becomes
`data-*` and `{value}` becomes visible text, which is a third position
again.

## Not in scope

The other two round-trip losses are accepted by design and documented in
`tomet-convert-pandoc/src/from_pandoc.rs`: a positional `(args)` returning
as `{value: ...}`, and a number in `@meta` returning as a string. Both are
forced by Pandoc's `Attr` and metadata types having no equivalent.
