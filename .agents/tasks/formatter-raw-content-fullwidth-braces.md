# `format_source` changes raw content when a value group uses full-width braces

Found by the corpus-wide `does_not_change_the_parsed_document` test added
with the `tomet-tests` package (2026-09-01). Not a regression -- the old
version of that test only covered three hand-picked fixtures, so this was
never exercised.

## Repro

```text
<x>(content:raw)｛ id:1 ｝
[ 
  a
]
```

(Note the full-width `｛ ｝`, U+FF5B/U+FF5D, and the trailing space after
`[`.) `tomet format` strips that trailing space, so the raw content
changes and `parse(src) != parse(format(src))`.

With ASCII `{ }` the same input is preserved correctly, which is what
makes it a brace problem rather than a whitespace-rule problem.

## Cause

`tomet-parser` only accepts ASCII `{ }` for a value group. With full-width
braces the element does not parse the way it looks, so its `content:raw`
never takes effect and `collect_raw_spans` records no raw span covering
the `[ ... ]` body. `format_source`'s trailing-whitespace rule then treats
those lines as ordinary text.

## Open question: which side is wrong

- **Formatter.** `crates/tomet-format/src/lib.rs`'s module doc and
  `docs/develop/architecture.md` both state the no-semantic-change
  guarantee without qualification. Under that reading the formatter should
  hold the line even for input that did not parse the way it looks.
- **Fixture.** `tests/fixtures/examples/bookmark.tmt` is the only file
  hitting this, and its full-width braces look like a Japanese-IME typo.
  Fixing the fixture makes the symptom go away without addressing the
  guarantee.
- **Parser.** A third option: accept full-width braces, or reject the
  document loudly instead of silently reinterpreting it.

These are not mutually exclusive. Decide the guarantee's real scope first;
the fixture edit follows from that, not the other way round.

## Where it is recorded

`KNOWN_FORMAT_CHANGES_DOCUMENT` in `tests/src/roundtrip.rs` lists
`examples/bookmark.tmt` with the reasoning inline. The list is asserted to
match reality exactly, so once this is fixed the test fails until the
entry is removed.

## Steps

- [ ] 1. Decide the scope of the no-semantic-change guarantee
- [ ] 2. Fix accordingly (formatter, parser, and/or fixture)
- [ ] 3. Remove `examples/bookmark.tmt` from `KNOWN_FORMAT_CHANGES_DOCUMENT`
- [ ] 4. Reconcile the wording in `crates/tomet-format/src/lib.rs` and
      `docs/develop/architecture.md` with whatever was decided
