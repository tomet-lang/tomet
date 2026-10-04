${macro.generated\_by(self.path)}

# A test that catches a converter silently flattening `Value::Blocks`

Not started -- recorded for later, not blocking anything.

## The incident this is from

While adding `@conflict` (see the now-deleted
`conflict-element-and-content-model.md`, folded into commit history),
every non-`.tmt` converter (`tomet-convert-{html,markdown,pandoc,
typst}`) turned out to silently destroy `a`/`b`'s content on export.
The generic "unknown element" fallback each converter has flattens
`(args)` into scalar attributes via `flatten_element_data`/
`value_to_json`; `Value::Blocks` (what `a`/`b` hold) has no scalar
projection by design, so it collapsed into an unreadable placeholder
string (`"<N block(s)>"`) with the real content gone.

`cargo test --workspace` stayed fully green through this -- every
unit test and snapshot test passed. The bug was found only by manually
running the real CLI (`tomet html`/`tomet to-pandoc`) by hand and
reading the output. Nothing in the automated suite would have caught
it, and nothing will catch the next one either: any future builtin or
custom-vocabulary element that puts `Value::Blocks` in `(args)` hits
the exact same silent-flatten path in all four converters, with no
test anywhere asserting against it.

## What to build

A test, parametrized over every entry in `tomet_semantics::
BUILTIN_KINDS` (or at least every one whose shape can plausibly carry
`Value::Blocks` -- `conflict` today, anything added later), asserting
that none of `tomet-convert-html`/`-markdown`/`-pandoc`/`-typst`'s
output for a document using that kind with `key: [...]` block-content
args contains the lossy placeholder shape (`"<" ... "block(s)>"` or
whatever the exact marker ends up being -- grep the current four
`value_to_json`/`scalar_string`-style fallbacks for the precise
strings, they may drift). Fails loudly the moment a kind's block
content silently disappears, instead of relying on a human noticing
during manual CLI testing.

Where this probably lives: `tests/src/` (`tomet-tests`), in the same
spirit as the existing snapshot/corpus-wide checks -- this is a
cross-crate invariant, not something any one converter crate can
assert about itself alone.

## Open questions for whoever picks this up

- Does this need a real fixture per kind, or can it build a minimal
  synthetic `Element` for each `BUILTIN_KINDS` entry and check it
  directly (faster, no fixture-file churn, but less representative of
  real documents)?
- Should the same test also guard the "exact copy" path
  (`EXACT_DATA_KEY`'s JSON), which has the identical lossy-placeholder
  problem for the same reason (see `flatten.rs`'s `value_to_json`) --
  or is that a separate, lower-priority gap since nothing reads it back
  today?
