# `Value` serializes five variants it cannot read back

Raised after the `ast-element-split` task (2026-11-01). The author wants it done. Delete this file when done.
Destination of what is worth keeping: the doc comment on `impl Deserialize for Value` and `bindings/js/index.d.ts`.

## Facts (`crates/tomet-syntax-ast/src/lib.rs`)

- `impl Serialize for Value` writes `Call` (`{call, args}`), `Interp`, `Hole`, `Element` and `Blocks`.
  `impl Deserialize for Value` has no arm for any of them (the comment says they are only produced by the parser).
- The JS / Java / Python bindings accept a `Document` JSON as input (`toHtml(doc)`, `printDocument(doc)`,
  `bindings/js/src/lib.rs` `from_value`). A document that holds `${...}`, `@x(icon: @doc.icon(..))` or `key: [..]`
  therefore serializes fine and fails to come back.
- `bindings/js/index.d.ts` now says so in the `Value` comment, but a reader still cannot tell a map from an element
  object by type.

## Open

- [ ] Make the five readable (tag them on the wire?) or make the bindings refuse a document that holds them, with a clear error?
- [ ] If tagged: that changes the JSON shape of `Value` again; do it once, with the version bump the JSON change of
      `ast-element-split` already needs.

## Steps

- [ ] Write a round-trip test: serialize a document holding each of the five, deserialize, compare.
- [ ] Decide with the author; implement; update `index.d.ts`.
