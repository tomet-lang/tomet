# `.ctmt` declarations: what is left

Steps 1-3 are done and pushed (`feat(syntax)!`, `feat(declaration)!`, `feat(cli)` on top of
`refactor(declaration): check declarations apart from any host`): `tomet check` on a `.ctmt` hands its
header to the same `check_declarations` a `.tmt` gets (`Host::Ctmt`); a non-standard name such as
catasto's `::upstreams` is passed over; a repeated standard declaration is counted once, in
`tomet-declaration`, for both hosts. The decisions are in the crate `//!` of `tomet-declaration` and
`cotomet-syntax`, and in those commit messages. Delete this file when steps 4-5 are done, after folding
what is worth keeping into the crate `//!` and `tmtroot/spec/`.

## Decided (with the author, 2026-10-25)

- A `.ctmt` file has the declarations `kind version meta macros vars` (`Scope::Both`); the rest are
  `.tmt` only. `config`/`settings` may open to `.ctmt` later: a change of data in `Spec`, not of structure.
- Shape lives in `tomet-declaration`, meaning is injected by the host. No `rule` in `DeclKind`
  (`::rule` exists as a name in a design memo only; `:rule`, the connect, is another concept).
- Positional logic is shared (`cotomet_syntax::bind_positional`); declaration signatures live once, in
  `Spec`. A `.ctmt` has no body elements, so it needs no table of its own.
- Data-only `cotomet` is a cargo feature, not another language; address stays essential (call the
  existing logic, do not redevelop it).
- A declaration name that is not a standard one is passed over, not reported.
- No backward compatibility.

## Steps

- [x] 4. `.ctmt` `${macro.…}` / `${var.…}` evaluation and the data-only feature: done and pushed
  (`9a7bcd55`..`14bca04b`: `feat(syntax)!`, `feat(parser)`, `feat(expr)!`, `feat(eval)`, `feat(cli)`,
  `docs(api)`). `cotomet-eval` (`crates/tomet-semantics-eval`) behind `cotomet`'s `eval` feature (off by
  default); an unquoted `${...}` is an error for a reader with no hook. The decisions are in the `//!` of
  `cotomet-eval`, `cotomet-syntax`, `tomet-expr` and in those commit messages.
- [ ] 4a. **Spec pages that need a paragraph** (the spec was not touched by the agy work, because the
  author's spec edits were uncommitted in the default workspace):
  - `tmtroot/spec/syntax/cotomet.tmt`: expressions in a `.ctmt` (`${...}`, `$name(...)`), evaluated with
    the file's own `::macros` / `::vars`, `Phase::Output`, no vault; the `eval` cargo feature; an unquoted
    expression is an error where there is no hook, a quoted `"${x}"` is text; a declaration takes no
    expression and an expression is not a key; a list comes from a variable `type: list`, and `::vars`
    has no `map` type.
  - `tmtroot/spec/expressions/macro.expr.tmt`: a macro argument is an ordinary expression (a bare word is
    an unknown name); a call must match its template (`${1}`, `${2}` positional, `${name}` named; any
    other `${...}` in a template is text) or it is an error/warning; `unknown macro` replaces `unknown
    function`. **This is new for `.tmt` too**: a call with the wrong number of arguments warned nothing
    before.
- [ ] 4b. Address checking in `.ctmt` (`${tmt(...)}` / `${addr(...)}` / `${ref(...)}` against a vault's
  `Resolver`, existence and anchors). Taken out of step 4 by the author. Today these are errors that say a
  `.ctmt` has no vault.
- [ ] 5. `::rule`, and `:rule` on a cotomet value.

## Open question, not for this task alone

Diagnostics are appended pass by pass and are not sorted by position. If the order should follow the
source, sort all of them at the end of validation (every diagnostic's order would change).
