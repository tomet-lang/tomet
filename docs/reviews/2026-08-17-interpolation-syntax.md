# `${...}` Interpolation Syntax Decision (2026-08-17)

`docs/develop/grammar-freeze.md` listed variable/function expansion
(`$()`/`${}`) among the anticipated-but-undesigned constructs still open
after the core grammar freeze. `docs/develop/idea.tm` (lines 8, 124-134)
carried unresolved brainstorming for this exact feature -- `$(var)` vs
`${var}` vs `{{var}}`, `$add(a, b)`, `${a % b}` -- without settling on a
sigil or a grammar. This records the decision and what's now implemented
vs. still open.

1. **Unified `${...}` sigil, not a competing `$func(...)` form.** A bare
   id reference (`${id}`) and a function call (`${sum(a, b)}`) both live
   inside `${...}`; there is no separate `$name(args)`-without-braces
   form. See `is_interp_start`/`parse_dollar_element` in
   `crates/typedmark-parser/src/document.rs`.

2. **`${...}` reuses `Element`, not a new `Inline` variant.**
   Structurally `${...}` is just a sigil with a mandatory `{value}`
   group -- the same shape as `@name{value}`, with no `(args)`/
   `[content]` groups in v1 -- so it's represented as `Sigil::Dollar`
   (a new, nameless `Sigil` variant; the path/call name lives in the
   expression itself, not the sigil) plus a new `ElementValue::Interp`
   variant carrying the parsed expression, and parses to an ordinary
   `Inline::Element`. This was corrected after an initial draft
   introduced a parallel `Inline::Interp`/`Interp` type instead, which
   needlessly duplicated `Element`'s existing `(args)[content]{value}`
   shape and its `span`/rendering plumbing, and broke every exhaustive
   match over `Inline` across `typedmark-html`/`typedmark-markdown`/the
   TUI printer for no benefit. Reusing `Element` means a standalone
   `${x}` line collapses to `Block::Element` exactly like a standalone
   `@meta{...}` does (`document.rs::parse_paragraph`'s one-element
   collapse) -- consistent with `@`/`<T>`, not a special case.

   One real representational limit this doesn't erase: `Element.args`/
   `ElementValue::Data` are `Value`-typed, and `Value` can't distinguish
   a bare identifier reference from a quoted string literal (both
   collapse to `Value::String`) or represent a nested `Call`/`Member` as
   an argument or object. That's why the expression itself still needs
   its own `InterpExpr` type (`crates/typedmark-ast/src/lib.rs`) rather
   than being flattened into `Value` too --
   `ElementValue::Interp(InterpExpr)`, not `ElementValue::Data(Value)`.

3. **`InterpExpr` is `{ kind: InterpExprKind, span: Span }`, and every
   node -- including nested `Call` args and `Member` objects -- carries
   its own span**, matching the "struct + trailing `span`" shape every
   other `typedmark-ast` node already uses (`Text`, `Heading`, `Element`,
   ...). `Call`'s `callee` and `Member`'s `object` are `Box<InterpExpr>`
   (not a bare `String` name), so postfix chains compose freely instead
   of needing two separate mechanisms: `a.b.c` (a pure dotted chain) is
   just nested `Member` with no `Call` in it; `b(x).id` (member access on
   a call's result) and `a.b(x)` (calling a member) both fall out of the
   same `Call { callee: Box<InterpExpr>, args }` / `Member { object:
   Box<InterpExpr>, member: String }` shapes. There's no separate flat
   `Path` variant -- an earlier draft had one (`Path(Vec<String>)`), but
   it couldn't represent `b(x).id`/`a.b(x)` at all (a `Call` could never
   appear inside a path chain), so it was dropped in favor of `Member`
   alone, which subsumes it.
   ```
   InterpExpr     := { kind: InterpExprKind, span }
   InterpExprKind := Identifier(String)
                    | Literal(Literal)
                    | Call { callee: Box<InterpExpr>, args: Vec<InterpExpr> }
                    | Member { object: Box<InterpExpr>, member: String }
   Literal        := Int(i64) | Float(f64) | String(String)
   ```
   `document.rs::parse_interp_expr` is a standard postfix-chain parser:
   parse a primary (`parse_interp_primary` -- an identifier or a
   literal), then loop consuming trailing `.member` (wrap in `Member`) or
   `(args)` (wrap in `Call`) until neither matches. See
   `parses_member_access_on_a_calls_result`/`parses_call_on_a_members_result`
   in `crates/typedmark-parser/src/lib.rs` for the composed cases. No
   infix operators yet -- `${a + b}`/`${a % b}` (from `idea.tm`'s
   brainstorming) remain open ideas, not rejected, just deferred. This is
   enough to cover both motivating use cases (displaying a referenced
   element by id, computing over `@meta` fields via a function call)
   without needing operator precedence.

4. **No newlines inside `${...}`.** `parse_dollar_element` only skips
   inline whitespace (`skip_inline_ws`), never `skip_ws_and_newlines`,
   so `${...}` stays a strictly single-line construct. It's also
   deliberately absent from `Stop::Paragraph`'s lazy-continuation
   block-trigger disjunction (unlike `<T>`/`@name`, which always end a
   paragraph): a `${...}` embedded mid-prose with text around it on the
   same or a following line stays inline content, not a paragraph
   break. See `interp_trigger_on_next_line_does_not_end_paragraph` in
   `crates/typedmark-parser/src/lib.rs`.

5. **Evaluation is out of scope for this decision.** `InterpExpr` is an
   unresolved syntax tree; looking up an `Identifier`/`Member`'s
   referenced element (`typedmark-resolve`'s job) or calling a `Call`'s
   named function (`typedmark-compute`'s job) is separate, not-yet-built
   work. Both crates currently have no code that consumes `Element`/
   `ElementValue::Interp`. `typedmark-html`/`typedmark-markdown`/the TUI
   printer each independently re-render an unresolved `${...}` back to
   display/source text in the meantime (not shared via `typedmark-ast`
   -- rendering `Element`/`Value` back to text is already each
   consumer's own job everywhere else in this codebase, e.g.
   `render_value_inner` in the TUI printer vs. `value_to_plain` in
   `typedmark-html`).

`docs/develop/idea.tm` trimmed to point at this doc for the settled
sigil/grammar-shape questions, while keeping the still-open
operator-precedence ideas (`${a % b}`, `$regex(...)`, `$math()`) as open
notes rather than deleting them. `docs/develop/grammar-freeze.md`'s
"does not exist yet" list updated: `${}` removed (now exists, this
doc), `$()` removed outright (rejected as a competing sigil, not merely
deferred).
