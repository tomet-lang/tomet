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
- [x] DECIDED (user, 2026-10-17), CORRECTED while starting Step 2: the resolver interface — how the evaluator
      asks for a document, a directory listing, a typed meta map — is a trait handed to the evaluator through
      the evaluation context. It must be defined in `tomet-semantics-compute` (L2): an L2 crate cannot name a
      trait defined in `tomet-vault` (L5). `tomet-vault` (L5, in-memory sources) and `tomet-load` (L6, disk)
      implement it. The clock and the random source (`generate.*`) are injected the same way (a `Generate`
      trait, with a system implementation), which also settles the open item about deterministic tests.
- [x] DECIDED (user, 2026-10-17): the reserved-names table is one const table in
      `tomet-semantics-compute`, read by the lambda-parameter check, `check`, and the LSP.
- [x] PROVISIONAL (user: "たぶんそれでいい", 2026-10-17): the evaluation "time" (出力時 / 作成時) is a field
      on the evaluation context; `generate.*` fails with a dedicated error outside 作成時.
      Revisit if the field turns out awkward when Step 3 wires `tomet new`.

## Step 1: syntax (L1)

Files: `tomet-syntax-parser/src/interp.rs`, `tomet-syntax-ast/src/lib.rs` (`InterpExprKind`,
`Display`), `tomet-syntax-ast/src/cst_ast.rs`, `tomet-syntax-cst`.

- [x] Literals `true` `false` `null` (reuse the `.tmt` value tokens). `nullable` and `true-ish` stay names.
- [x] Grouping `( expr )`; distinguished from a lambda parameter list by the `=>` after `)` (`lambda_head`). The group is kept as a node so printing gives the parentheses back.
- [x] Comparison `== != < <= > >=` (non-chaining), `&&`, `||`; precedence per `operators.expr.tmt`.
      Whitespace around them is optional (no operator symbol is an identifier character); the spec was
      corrected to say so. Lone `=`, `&`, `|` are errors that name the fix.
- [x] Lambda `x => expr`, `(a, b) => expr`. The `=>` token is read in one place (`lambda_head`; it may become `:>`).
- [x] Postfix `#` with a document on the left: `doc#id`, `doc#(id)`, `doc#item(0)` (`InterpExprKind::DocId`);
      the hash must touch what is on its left. Leading `#x` stays `Id` and means `self#x`. `.` and `(...)`
      after it are ordinary members and calls. Evaluation of `DocId` is Step 2 (`NotYetSupported` for now).
- [x] `addr(` raw argument: read to the matching `)` (parentheses nest); quoted form too (`InterpExprKind::Addr`,
      `raw` keeps the spelling). The only special lexing rule; fixes the `tmt:path` / named-argument clash.
      Works braceless (`$addr(...)`). The argument is a literal and is never evaluated: the spec had a
      contradiction (`addr(#note.prop.source)` read as raw text vs as an expression), resolved with the user
      by adding a string method `.addr()` for run-time strings (Step 2). The spec was updated.
- [ ] Remove bare-identifier-as-document-id from the evaluator side (Step 2), keep parsing it.
- [x] Printer: `tomet-format-printer` prints an expression through `InterpExpr` `Display`, which now covers all of
      the above and is a fixed point (tested). `tomet format` is whitespace-only and does not touch expression
      interiors; the spec said it would, and was corrected.
- [x] Parser tests in `crates/tomet-syntax-parser/src/tests/interpolation.rs` (18 new). A `||` inside a table
      cell parses. Still to check: the CST parser (`cst.rs` `parse_interp_expr`) reads `${...}` as opaque tokens up to
      the matching `}`, so a `}` inside a string literal or an `addr(...)` would end it early.
      `tomet check` (AST parser) handles `${"}"}`; the CST path (used by edit/LSP tooling) is unverified.
- [x] `tree-sitter-tomet` grammar (operators, group, lambda, `doc#id`, `addr(`, named args, member after a call),
      regenerated with the pinned `tree-sitter-cli@0.26.12` (`npx -y tree-sitter-cli@0.26.12 generate`, ABI 14 as
      before), queries extended. New fixture `tests/fixtures/syntax/expression.tmt` with its `tests/ref` files
      and `SYNTAX.md` rows. Known approximation: `addr(` stops at the first `)` here, nested parentheses are
      only handled by the real parser.
- [x] `apps/tui/src/ui/highlight.rs` fallback takes `${...}` as one span. The editors' own query files
      (`editors/{helix,neovim,zed}`) are separate, differ from `tree-sitter-tomet/queries`, and were not touched.

## Step 2: evaluator and value types (L2)

Strategy: build the new evaluator beside the old one (`tomet-semantics-compute/src/eval/`), with its own
entry point and `EvalValue`, tested in isolation. The old `evaluate*` functions keep serving the existing
callers until Step 3 switches them, so no document changes meaning halfway and every sub-step leaves the
workspace green. Order: 2a skeleton (value, error, context, traits) -> 2b core (names, literals, operators,
lambdas, collection methods) -> 2c documents and addresses -> 2d datetime and `generate` -> 2e fragments,
`emoji`, `unicode` -> 2f macros and the phase check.

Files: `tomet-semantics-compute/src/{lib,functions,error}.rs`, `tomet-semantics-address`.

- [x] 2a Evaluation value type `EvalValue` and conversions to/from `tomet_ast::Value`, `EvalError`, `EvalContext`
      (`Phase`, `Resolver`, `Generate`), `Env`. `Node` holds an element or a section: the old address code only
      looked for `Element`s, so a heading's id was never found; the new type has to find both.
- [x] 2b Name resolution by position: call names -> builtin functions; value names -> lambda parameters, then
      reserved values (`self macro vault`, plus `vars generate` at 作成時); no chained fallback; unknown name is
      an error; `macro.` required for macros. `RESERVED_VALUES` is the one table (Step 0 decision).
- [x] 2c Member dispatch by receiver type: document (`path filename meta kind`, `.item(n)`), node (`prop id sigil
      connects content`, `.item(n)`, `.prop(key)`), directory (`docs`), Seq, Map, string. Datetime is 2d. A node has
      only its own members: the old "direct property access" fallback (`#intro.tags`) is gone, as the spec says.
- [x] 2c Addresses: `#id`, `doc#id`, `#item(n)` (an `item` id is positional only when called), `tmt(...)`,
      `ref(...)`, `addr(...)` and `"...".addr()`, with `file:` and `https:` reported as not supported. A node
      is an element or a section, found by id anywhere in the document. `MemoryResolver` (in-memory, untyped
      meta) serves the tests and later the vault's own resolver wraps the same idea. 17 tests.
- [x] 2b Operators: `== !=` (1 == 1.0 true, cross-type false), ordering (numbers, strings, datetimes),
      `&&` `||` short-circuit and Bool-only; `not(x)` Bool-only (no truthiness); `exists`.
- [x] 2e The new evaluator has only `add sub mul div mod not exists emoji unicode tmt ref link` as functions; `eq ne
      gt gte lt lte and or contains date time uuid` do not exist there (tested). The OLD evaluator still has them
      and is removed in Step 3 with its callers. `include` is Step 4.
- [ ] Strictness: unknown input is an error (`date("hello")`, `uuid("bogus")`, `emoji("nope")`).
- [x] 2b `generate.now()` -> datetime, `generate.uuid()` through the injected `Generate`; `PhaseOnly` error when
      rendering (it fires at the name `generate` or `vars`, before any call). The system clock is UTC; applying
      a field's configured offset belongs to the vault (Step 3).
- [x] 2d Datetime value: ordering and equality (the same instant at two offsets is equal), `.format` with `YYYY YY MM
      DD HH mm ss`, other characters pass through, no offset conversion. `parse_datetime` (RFC 3339 or a bare
      date as midnight UTC) and `format_datetime` are public for the vault to build typed meta with.
- [x] 2c String method `.addr()`: interprets a run-time string with the same rules as `addr(...)`; an
      uninterpretable string is `BadAddress`. (`addr(...)` itself takes only literals.)
- [x] 2e `link(x)` and `.link()` on a document or an element with an id build `@link(tmt:path[#id])` in a one-
      paragraph `Fragment`. `emoji` and `unicode` ported, with one emoji table shared with the old evaluator.
- [x] 2b Methods: Seq `.filter .sort .map .join .contains .len .item`; Map `.get .contains`; string `.contains`;
      lambdas as values with closures; a lambda parameter may not reuse a reserved name; a Null receiver is an
      error (`UnknownMember`). `.sort` puts items whose key is `Null` last, whichever way the rest are
      ordered (decided by the assistant, user delegated; spec updated), and checks the other keys are of one kind
      first so the comparison is a total order. (`.link()` is 2e, done.)
- [x] 2b Macro calls `macro.name(...)` (positional `${1}` and named `${key}` placeholders, scalars only).
- [x] 2b 46 tests in `eval/tests.rs`; `eval` is clippy-clean.
- [ ] (moved to Step 3) Type mismatch errors for a value placed where output goes: they must say what to use
      (`include(...)`, `.join(", ")`). The evaluator returns the value; the position rules live in the pass.
- [ ] (at the Step 3 cutover) Delete the old `evaluate*`, `EvaluationContext`, `functions.rs` and their tests; the
      crate `//!` doc already describes both evaluators and loses the older one then.

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
