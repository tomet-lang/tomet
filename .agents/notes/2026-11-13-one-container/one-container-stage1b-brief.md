# Brief: one-container, stage 1b -- remove the nameless entry of a `{ }` group

You are working in a jj workspace of the `tomet` repository. Your current directory is that workspace. Work only here.

## The order

The construct `(marker)[content]` written as an entry inside a `{ … }` group is removed from the language:

```
@memo(){
  (1)[ first ]
  (2)[ second ]
}
```

After this stage it does not parse. `{ }` is the meta of the thing on its left; it is not a place to list children.
This is the author's decision (2026-11-13). The design is `tmtroot/dev/design/one-container.tmt` (section 7); read it.

In the code the construct is `tomet_ast::Bare`, `Entry::Bare`, and everything that builds, walks, prints or converts
them. Only this construct goes. `Entry::Pair`, `Entry::Element`, `Entry::Hole`, `Group` and `Data` stay exactly as they
are: do not redesign them, do not rename them, do not touch `Value`.

Note that the word "bare" is used for unrelated things (a bare key, a bare section title, `FenceLines::Bare`, a bare
connect). Those are not this construct. Leave them alone.

## Rules

- Read first: `AGENTS.md`, `CONTRIBUTING.md`, the root `.writ.tmt` (`crate-layering`), `tests/README.md`, and the `//!`
  of every crate you touch.
- No backward compatibility: no shim, no deprecated alias, no "accept and warn". The construct is a parse error.
- The existing implementation is not a reason for anything. Do not keep a piece because it is there.
- Do not edit `.md` files (they are generated) and do not edit spec pages under `tmtroot/` (a later pass). If a
  generated file must change for the checks to pass, regenerate it with the project's own command and say so.
- `.writ.tmt` files are the author's: if a rule is in your way, stop and report; do not edit it.
- `tests/fixtures/` is a frozen corpus. This stage has the author's word to edit exactly one place in it: the
  `@memo(){ (1)[…] (2)[…] }` block of `tests/fixtures/syntax/sigils.tmt`. Remove that block (and a comment line that
  only introduces it). Touch no other fixture. Then regenerate the snapshot references the way `tests/README.md` says,
  and check that only references of `sigils` changed, and only by the removed block.
- `crates/tree-sitter-tomet/grammar.js`: if it has a rule for this construct, remove the rule and say so in your
  report. Do NOT run `tree-sitter generate`; the author regenerates `parser.c`. If a test fails only because
  `parser.c` is not regenerated, report it, do not work around it.
- Build with `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0` in the environment of every cargo command: the disk is
  5 GB and a default build fills it. If `cargo` is not on the path, run commands through `nix develop -c`.
- No commit, no `jj describe`, no `jj new`, no push, no `git` writes. Do not touch other workspaces or anything outside
  the current directory.
- Keep a progress file, `.agents/tasks/one-container-1b-progress.md`, updated after each step, so the work can be
  resumed from it.

## Where it is (starting points, not a complete list -- the compiler is the list)

`crates/tomet-syntax-ast` (`struct Bare`, `Entry::Bare`, the `Group` methods that match on it),
`crates/tomet-syntax-cst-parser` and `crates/tomet-syntax-cst` (the form that reads it; if a `SyntaxKind` exists only for
it, it goes), `crates/tomet-syntax-lowering/src/element.rs`, `crates/tomet-syntax-tree/src/walk.rs`,
`crates/tomet-printer` (`render_bare`, `value.rs`), `crates/tomet-transform/src/expressions.rs`,
`crates/tomet-stats`, `crates/tomet-convert-html/src/elements/mod.rs` (`render_bare`),
`crates/tomet-convert-pandoc` (`to_pandoc.rs`: `bare_to_block`, `BARE_ENTRY`; `from_pandoc.rs`: the branch that builds
`Bare` from a `Div` carrying that marker, and `push_bare_entries` -- with nothing writing the marker any more, the
reading branch has no input and goes too; if `ENTRY_KEY` is then used by nothing, it goes), `apps/lsp/src/folding.rs`,
`apps/cli/src/commands/spec/cli.rs`, `bindings/js/index.d.ts`, `packages/react`, `packages/svelte`, `packages/astro`
(types and renderers that name `Bare`; `packages/astro/README.md` is generated or hand-kept -- find out before editing),
`editors/vscode/syntaxes/tomet.tmLanguage.json` and `crates/tree-sitter-tomet/queries/highlights.scm` (only if they have
a pattern for this construct).

## The parse error

Writing the removed construct must give an error that says what to write instead, in the style of the existing parse
errors (look at how `tests/SYNTAX.md` records them). Add a test for the message next to the parser's other tests.
What to write instead is: a named element, `@name(…)[…]`, as an entry.

## Done means

1. This search finds nothing in `crates apps bindings packages` (outside `target`):
   `Entry::Bare|struct Bare\b|render_bare|bare_to_block|push_bare_entries|BARE_ENTRY`
2. `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 cargo test --workspace` passes. Paste the final summary lines of the
   real run into your report. If something fails, say which test and why; do not call a failure an improvement.
3. `jj diff --stat --no-pager` shows only files this brief accounts for. List any file you changed that the brief does
   not mention, with the reason.
4. Doc comments that described the construct are gone or rewritten in the same change; none describes something that no
   longer exists.

## Report

End with: what you removed (by crate), the snapshot references that changed and how, whether `grammar.js` changed, the
test summary lines, anything you were unsure about, and anything you did not do. "I do not know" and "I did not check"
are acceptable answers; a claim that turns out false is not.
