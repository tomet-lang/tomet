# Expression language spec (tmtroot/spec/expressions)

Write the decisions from the 2026-10-17 design discussion into the spec. Spec only: no Rust changes.
The design is ahead of the implementation, so each file carries a status line and a
"現状の実装との差" section. Memory: project-tomet-expression-redesign holds the decision list.

Rules for this task:
- Only touch tmtroot/spec/expressions/*, tmtroot/spec/syntax/addressable.tmt,
  tmtroot/spec/features/blueprint.tmt, tmtroot/spec/features/data.tmt. The tree holds the user's own edits elsewhere.
- Syntax mentioned in prose goes inside backticks: a bare `${...}`, `@x` or `#x` in a paragraph is parsed.
- No commit unless asked. Fmt/check by path.

Where it lands when done: the spec files themselves; delete this file once the user has reviewed the diff.

## Steps
- [x] overview.expr.tmt
- [x] names.expr.tmt
- [x] address.expr.tmt
- [x] self.expr.tmt
- [x] include.expr.tmt
- [x] lambda.expr.tmt (added; not in the first plan)
- [x] types.expr.tmt (added after the library tidy: schema types as value types, methods, datetime format)
- [x] operators.expr.tmt (added: comparison, &&, ||, literals, grouping; not() stays a function)
- [x] extension.expr.tmt (added: only macros are user-extensible for now; future direction recorded)
- [x] filter.expr.tmt (Seq methods, missing keys)
- [x] macro.expr.tmt
- [x] func.expr.tmt (pure/impure, generate.*, vars.)
- [x] addressable.tmt: wording, bare ${intro}, stale template section
- [x] blueprint.tmt: generate.*, self.filename, vars.title
- [x] data.tmt: "マクロ$" wording
- [x] tomet check tmtroot/spec (OK 45), format --check on touched paths (OK; addressable.tmt's own table was
      already unaligned before this task and was left alone), export --check (up to date)

## Left for later
- Open items listed at the end of each expr file (Seq/Map text form, operators, library tidy, extension, file:/https: values).
- Rust side: nothing implemented. Each file's "現状の実装との差" is the work list.
- check-links reports 2 broken links in std.settings.element.tmt (docs/spec/...); not from this task.
