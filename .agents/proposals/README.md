# Proposed: every writ entry becomes `@rule(id){ ... }`

Author-owned files. These are proposals, not edits. Applying them is
three `cp`s:

```bash
cp .agents/proposals/root.writ.tmt   .writ.tmt
cp .agents/proposals/docs.writ.tmt   docs/.writ.tmt
cp .agents/proposals/parser.writ.tmt crates/tomet-syntax-parser/.writ.tmt
```

All twelve entries at once, deliberately. A half-migrated estate contains
entries nobody checks, which is the disease this is treating.

## What changed, and what did not

The prose is **byte-identical**. It was moved, never retyped. The whole
diff is:

- an `@rule(id){ ... }` element inserted under each `##[ ... ]` heading
- `@layers{...}` and `@pure{...}` deleted from where they floated at the
  top level, folded into their own rule's group as `layers:` / `pure:`

Nothing else. `diff` against the live files shows ten removed lines in
`.writ.tmt` and ten in the parser's, and those twenty are the two data
elements in their old position.

## Why the entry is one element

An entry was not a structure, it was a layout convention: `id / rule /
why / guard` were positions relative to two horizontal rules, and
`@layers` belonged to nothing. That is why one missing blank line before
`@layers{` deleted the workspace's layering rule in silence -- nothing
owned the declaration, so nothing noticed it was gone.

Inside a group, a dropped blank line cannot detach anything, and an
unclosed group is a parse error. The accident stops being possible rather
than becoming newly detectable.

## `guard:` is required

Four spellings, and the holder is the key:

```tmt
guard: { twrit: layers }                      # a kind twrit implements
guard: { runs: "just docs-check" }            # a runner this repo ships
guard: { test: "tests/src/vocabulary.rs" }    # a test here; twrit checks the path exists
guard: { none: "no mechanical check yet" }    # nobody, said out loud
```

`none:` takes a reason and must be written. A `@rule` with no `guard:` is
an error rather than a silent zero, because "nobody holds this" and
"somebody forgot" must not look alike -- the same disease as `ok` meaning
both "held" and "never read".

A guard is a pointer, never a program. It answers only *who guarantees
this right now*. An earlier draft put a shell line in the entry; that is
a Makefile wearing prose, and it would mean reading a `.writ.tmt` could
run arbitrary commands.

## Decisions still yours

1. **The headings are prose now.** They kept their current wording
   because it reads well, but `@rule(id)` is the name a tool is held to,
   so reword freely. `AGENTS.md` in `twrit` asks for exactly this: "a
   heading is prose and will be reworded".
2. **`.writ.tmt:20`'s stray `/tests/SYNTAX.md`.** Still there, now
   visibly dangling under the `crate-layering` prose. Drop it, make it an
   `@link(file:/tests/SYNTAX.md)`, or fold it into the Why.
3. **Two `none:` reasons were written for you** -- `blueprint-shape` and
   `explicit-form-first`. Both are one-line summaries of what their own
   Guard section already says at length. Reword if they undersell it.
4. **`kind-not-meta-type` uses `runs:`** for `tomet refactor --check
   --meta-kind`. Its Guard also records a known gap (unparseable files are
   skipped with a warning). The gap is prose; there is nowhere in the
   guard field to say "held, but with a hole".

## Applying this breaks `twrit check` until the tool follows

`twrit` looks for a top-level `@layers` element. After this it will not
find one, and will report `crate-layering: not declared` -- honestly, and
exiting 0. `parser-purity` the same.

That is the intended order: the document is the normative thing and the
tool follows it. See `writ-entry-shape.md` step 3 in the `tomet-writ`
repository.

## `.tomet/vocabularies/writ.vocabulary.tmt`

`@element(rule)` was added there so these files can be checked at all;
`tomet check` rejects an element no vocabulary declares. It is additive
and the live writs are unaffected.

`@element(layers)` and `@element(pure)` are still declared and are still
correct until these files are applied. Afterwards they are map keys
rather than elements, and their declarations should go -- one follow-up
edit, once `twrit` reads the new shape.

This is the point of the shape. A new rule kind used to mean a new
`@element`; now it is a new value of `guard: { twrit: X }`. The vocabulary
is written once.
