# Expression language: implementation

Implement the design written in `tmtroot/spec/expressions/` (12 files, status "設計済み・未実装").
Each file's "現状の実装との差" section is the authoritative gap list; this file orders the work.
Decisions are in memory `project-tomet-expression-redesign` and in the spec itself.

Nothing here has been started. Commit only when the user asks. Use a jj workspace
(`jj workspace add .agents/workspaces/expr-impl`) once step 0 is settled; the spec files are
uncommitted in the default workspace, so land or copy them first.

Where it lands when done: the code and its `//!` docs; flip each spec file's "状態" line to
implemented and delete its "現状の実装との差" section as the work for it ships. Delete this file last.

## Layering (from `.writ.tmt` crate-layering)

- L1 `tomet-syntax-{lexer,parser,ast,cst,tree}`: grammar, AST, CST.
- L2 `tomet-semantics*` (compute, address): the evaluator. Pure: no I/O, no clock.
- L4 `tomet-config`: field `type:` declarations. L2 cannot read it.
- L5 `tomet-transform`, `tomet-vault`, `tomet-format*`, `tomet-convert*`: passes and the disk-free vault.
- L6 `tomet-load`, `tomet-workspace*`: reads the disk.
- L7 `apps/*`, `bindings/*`, `tests`.

Consequences: schema types and documents/directories must be supplied to the evaluator from L5/L6 as
already-built values; the evaluator never reaches for them.

## Step 0: settle before coding (needs the user)

- [x] DECIDED (user, 2026-10-17): a separate evaluation value type in `tomet-semantics-compute`, with
      conversions to/from `tomet_ast::Value`. Today `tomet_ast::Value` is both the `.tmt` data model
      and the evaluation result, and it is already stretched: `Call`, `Element` and `Blocks` are "only
      ever produced by `tomet-syntax-parser` reading source text, never by deserializing inbound data"
      (comment above the `Deserialize` impl in `tomet-syntax-ast/src/lib.rs`), and `truthy` in
      `functions.rs` needed arms for all three. The design adds Document, Directory, Function,
      DateTime on top, which belong to evaluation only. `Value::Blocks` is the natural payload of the
      fragment.
- [x] DECIDED (user, 2026-10-17): the resolver interface — how L2 asks for a document, a directory
      listing, a vault index — is a trait defined in `tomet-vault` (L5), implemented in
      `tomet-load` (L6), handed to the evaluator through the evaluation context.
- [x] DECIDED (user, 2026-10-17): the reserved-names table is one const table in
      `tomet-semantics-compute`, read by the lambda-parameter check, `check`, and the LSP.
- [x] PROVISIONAL (user: "たぶんそれでいい", 2026-10-17): the evaluation "time" (出力時 / 作成時) is a field
      on the evaluation context; `generate.*` fails with a dedicated error outside 作成時.
      Revisit if the field turns out awkward when Step 3 wires `tomet new`.

## Step 1: syntax (L1)

Files: `tomet-syntax-parser/src/interp.rs`, `tomet-syntax-ast/src/lib.rs` (`InterpExprKind`,
`Display`), `tomet-syntax-ast/src/cst_ast.rs`, `tomet-syntax-cst`.

- [ ] Literals `true` `false` `null` (reuse the `.tmt` value tokens).
- [ ] Grouping `( expr )`; distinguish from a lambda parameter list by the `=>` after `)`.
- [ ] Comparison `== != < <= > >=` (non-chaining), `&&`, `||`; precedence per `operators.expr.tmt`.
      Binary operators need surrounding whitespace (`a-b` stays one identifier). Lone `&` / `|` is an
      error with a hint.
- [ ] Lambda `x => expr`, `(a, b) => expr`; keep the `=>` token in one place (it may become `:>`).
- [ ] Postfix `#` with a document on the left: `doc#id`, `doc#(id)`, `doc#item(0)`; leading `#x`
      stays `self#x`. `.` after it is a member step.
- [ ] `addr(` raw argument: read to the matching `)`; quoted form too. The only special lexing rule.
      Fixes the `tmt:path` / named-argument (`name: expr`) clash.
- [ ] Remove bare-identifier-as-document-id from the evaluator side (Step 2), keep parsing it.
- [ ] Printer: `tomet-format-printer` and `InterpExpr` `Display` must round-trip all of the above;
      `tomet format` normalizes operator spacing.
- [ ] Parser tests in `crates/tomet-syntax-parser/src/tests/interpolation.rs`; a `||` inside a table
      cell and a `|`-initial line must not break table parsing.
- [ ] `tree-sitter-tomet` grammar (hand-maintained, highlighting only) and a new fixture under
      `tests/fixtures/` (corpus: add, do not edit existing). Run
      `cargo test -p tomet-tests -p tree-sitter-tomet`.
- [ ] `apps/tui/src/ui/highlight.rs`, editors' syntaxes if they list `$` forms.

## Step 2: evaluator and value types (L2)

Files: `tomet-semantics-compute/src/{lib,functions,error}.rs`, `tomet-semantics-address`.

- [ ] Evaluation value type and conversions (Step 0).
- [ ] Name resolution by position: call names -> builtin functions; value names -> reserved values
      (`self macro vault`, plus `vars generate` at 作成時); no chained fallback; unknown name is an
      error; bare `${intro}` and bare `${gh}` are errors; `macro.` required for macros.
- [ ] Member dispatch by receiver type: document, element, directory, Seq, Map, string, datetime.
- [ ] Operators: `== !=` (1 == 1.0 true, cross-type false), ordering (numbers, strings, datetimes),
      `&&` `||` short-circuit and Bool-only; `not(x)` Bool-only (no truthiness); `exists`.
- [ ] Remove functions `eq ne gt gte lt lte and or contains date time`; keep
      `add sub mul div mod not exists emoji unicode tmt ref`; add `addr include link`.
- [ ] Strictness: unknown input is an error (`date("hello")`, `uuid("bogus")`, `emoji("nope")`).
- [ ] `generate.now()` -> datetime, `generate.uuid()`; dedicated "作成時専用" error outside 作成時.
- [ ] Datetime value: ordering, `.format("YYYY-MM-DD HH:mm")` with tokens `YYYY YY MM DD HH mm ss`,
      other characters pass through, no offset conversion.
- [ ] Methods: Seq `.filter .sort .map .join .contains .len`; Map `.get .contains`;
      string `.contains`; document `.link()`; lambdas as values, evaluated in a local scope;
      lambda parameter may not reuse a reserved name; Null receiver is an error.
- [ ] Type mismatch errors carry enough to say what to use (`include(...)`, `.join(", ")`).
- [ ] Tests move with each function; the old `functions.rs` doc table is rewritten.

## Step 3: context and the single evaluation path (L5)

Files: `tomet-transform/src/{interp,macro_expand,blueprint,index_query}.rs`,
`tomet-vault/src/lib.rs` (`prepare`, `evaluation_vars`), `bindings/js/src/lib.rs`.

- [ ] Build `self` as a document value (`path filename meta kind`) in the vault; `.meta` typed from
      the config `type:` (`list` absent -> empty Seq, `datetime` -> datetime, others Null;
      undeclared key -> error; `.get(k, default)` for the rest).
- [ ] `vault` and directory values; `.docs` (recursive) from the lazy `VaultIndex`, via the resolver.
- [ ] One pass in `prepare` replacing `resolve_interpolations` + `expand_document_macros`
      + the blueprint evaluator; applies the position table in `overview.expr.tmt`
      (value / inline / block / list item) and the type-mismatch rule.
- [ ] References resolve against the source copy taken before any rewrite (already the shape; make
      it an explicit, tested invariant).
- [ ] Failure keeps the source spelling and reports (`Unresolved`); add the new reasons.
- [ ] JS binding follows the new entry point.

## Step 4: include (L5/L6)

- [ ] `include(doc)`, `include(doc#id)`; body only for a whole document (no `@meta @config @kind` etc.);
      a section id takes the whole section; value targets are an error.
- [ ] Heading level: auto one below the host heading; `level: n` absolute; `heading: false`.
- [ ] Evaluate the included content in its own file's context, then splice; cycle detection with the
      include stack, depth limit.
- [ ] Duplicate ids after splicing: diagnostic, later one dropped from the output.
- [ ] Rewrite `./` `../` link targets for the including file.
- [ ] Dependency tracking: `serve` reload and the `tomet-workspace-links` cache must learn that a
      document depends on the files it includes.

## Step 5: filter and the old forms

- [ ] Delete `index_query` special pass, `Query`, `by(...)`, string-keyed context, `known_paths`.
      `@kind(doc.index)` restriction removed; the vault index is built lazily on first `.docs`.
- [ ] Migrate `doc.index` and fixtures that use `${filter(...)}` to `vault.docs.filter(...)...`.
- [ ] `blueprint`: `generate.*`, `${self.filename}`, `${vars.title}`; `tomet new` passes `vars`.
- [ ] `type: uuid` field validation in `tomet check` (generation already exists in
      `tomet-format-field-utils`).

## Step 6: migration and tooling

Breaking changes and a rough count of files that use them today (`.tmt` / `.rs`):

| Old form | Where |
|---|---|
| bare macro call `$gh(...)` | 7 / 20 |
| `${filter(...)}` | 10 / 12 |
| `${uuid()}` `${date(...)}` `${time(...)}` | 2 / 8 |
| bare `${ident}` | 19 / 20 (includes blueprint `${title}`, `${filename}`) |
| `eq gt and or ...` as functions | 3 / 7 |

- [ ] A `tomet refactor` rule that rewrites `$name(` to `$macro.name(` for names in the macro table.
- [ ] Update the corpus under `tests/fixtures/` and `tests/ref/` where a test needs it
      (the corpus is otherwise frozen); see `tests/README.md`.
- [ ] `tomet check`: messages for type mismatch, 作成時専用, duplicate id after include, unknown name.
- [ ] LSP: `completion.rs` function list, `hover.rs`, reserved names, member completion by type.
- [ ] Regenerate `tomet api` docs and `tmtroot/generated`; `just docs`.

## Step 7: spec sync and close

- [ ] For each shipped area, flip the file's "状態" line and delete its "現状の実装との差" section.
- [ ] Remaining small open items stay in each file's "まだ決まっていないこと".
- [ ] Verify: `just test`, `just test-suite`, `just docs-check`, `just writ-check`.
- [ ] Fold anything worth keeping into doc comments; delete this file.

## Order and dependencies

0 -> 1 -> 2 -> 3 -> {4, 5} -> 6 -> 7. Steps 1 and 2 can be split into reviewable commits by area
(literals/operators, lambda, address + `addr`, value types). Step 3 is the point where behavior for
existing documents changes, so land it with the Step 6 migration for the same forms.
