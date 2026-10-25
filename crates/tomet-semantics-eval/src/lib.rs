//! =[ cotomet-eval ]
//!
//! Evaluation for cotomet (`.ctmt`) data files.
//!
//! Evaluates `${macro....}` and `${var....}` expressions within `.ctmt` data files
//! while enforcing data constraints: pure output phase, no vault access, and
//! reporting all expression errors with source line and column.
//!
//! =[ Architecture & Responsibilities ]
//!
//! - (Data-only evaluation)
//!   | Evaluates expressions in value positions of a `.ctmt` file, replacing
//!   | expressions with their evaluated data values (`Value::String`, `Value::Int`,
//!   | `Value::Float`, `Value::Bool`, `Value::Seq`, `Value::Map`, `Value::Null`).
//! - (No vault access)
//!   | Pure evaluation runs in `Phase::Output` with `NoResolver`. Address expressions
//!   | (`${tmt(...)}`, `${addr(...)}`, `${ref(...)}`, `${include(...)}`) and generator
//!   | functions (`generate.now`, `generate.uuid`) are rejected.
//! - (Header declarations)
//!   | Declarations `::macros` and `::vars` in the file header provide macro templates
//!   | and variable defaults. Caller-supplied variables override defaults.
//! - (Comprehensive error reporting)
//!   | Every evaluation error carries 1-based line, column, and AST span. Evaluation
//!   | walks the entire file and collects all errors rather than stopping at the first.
//!
//! =[ Not here ]
//!
//! - Address resolution: address checking in `.ctmt` requires a vault and is out of scope.
//! - Map-returning macros: a macro is a parameterized string template. A list comes from a
//!   variable declared `type: list`; `::vars` has no `map` type, so a map reaches `${var.name}`
//!   only as a value the caller passes for a declared name.
//! - Grammar parsing: expression parsing is delegated to `tomet-parser`'s `CotometValueHook`.

use std::collections::HashMap;
use std::fmt;

pub use cotomet_syntax::File;
use tomet_ast::{ElementValue, Sigil, Value};
use tomet_declaration::{DeclKind, Declaration, Host, Problem, check_declarations};
use tomet_expr::{DocRef, EvalContext, Generate, NoResolver, Phase};
use tomet_lexer::LineIndex;

/// What a `.ctmt` says of a document, a directory or an element: it has none to find.
const NO_VAULT: &str =
    "a .ctmt has no vault, so it cannot refer to a document, a directory or an element";

/// An evaluation or parse error located at a specific line and column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub message: String,
    pub line: usize,
    pub column: usize,
    pub span: tomet_ast::Span,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.column, self.message)
    }
}

impl std::error::Error for Error {}

/// A dummy generator for pure evaluation. In `Phase::Output`, `generate.*`
/// is rejected before calling the generator.
struct PureGenerate;

impl Generate for PureGenerate {
    fn now(&self) -> time::OffsetDateTime {
        time::OffsetDateTime::UNIX_EPOCH
    }
    fn uuid(&self) -> String {
        String::new()
    }
}

/// Evaluates a `.ctmt` source string, resolving `${...}` expressions.
///
/// Declarations `::macros` and `::vars` in the header provide macro templates
/// and variable defaults. Values in `vars` override declared variable defaults.
///
/// Returns the parsed [`File`] with expressions replaced by their evaluated data values,
/// or every encountered error with line and column.
pub fn eval_str(src: &str, vars: &[(String, Value)]) -> Result<File, Vec<Error>> {
    let file =
        cotomet_syntax::parse_file_with(src, &mut tomet_parser::CotometValueHook).map_err(|e| {
            vec![Error {
                message: e.message,
                line: e.line,
                column: e.column,
                span: tomet_ast::Span::new(e.offset, e.offset),
            }]
        })?;
    eval_file(file, src, vars)
}

/// Evaluates a pre-parsed [`File`], resolving `${...}` expressions against `src`.
pub fn eval_file(file: File, src: &str, vars: &[(String, Value)]) -> Result<File, Vec<Error>> {
    let lines = LineIndex::new(src);
    let mut errors = Vec::new();

    let mut macros = HashMap::new();
    let mut var_decls = Vec::new();

    for decl in &file.header {
        let d = Declaration::from(decl);
        if matches!(d.kind(), Some(DeclKind::Macros | DeclKind::Vars)) {
            // An entry the shared check refuses would otherwise vanish, and a later
            // `${macro.x}` or `${var.x}` would complain of something unrelated.
            let at = lines.line_col(decl.span.start);
            for (_, problem) in check_declarations(std::slice::from_ref(&d), Host::Ctmt) {
                if matches!(problem, Problem::InvalidDefinition { .. }) {
                    errors.push(Error {
                        message: problem.to_string(),
                        line: at.line,
                        column: at.column,
                        span: decl.span,
                    });
                }
            }
        }
        match d.kind() {
            Some(DeclKind::Macros) => macros.extend(tomet_declaration::macros(&d)),
            Some(DeclKind::Vars) => var_decls.extend(
                tomet_declaration::read_var_decls(&d)
                    .into_iter()
                    .filter_map(|(name, read)| read.ok().map(|decl| (name, decl))),
            ),
            _ => {}
        }
    }

    let var_scope = match tomet_expr::var_scope(&var_decls, &DocRef(String::new()), vars) {
        Ok(scope) => scope,
        Err(e) => {
            errors.push(Error {
                message: e.to_string(),
                line: 1,
                column: 1,
                span: tomet_ast::Span::dummy(),
            });
            tomet_expr::VarScope::empty()
        }
    };

    let generate = PureGenerate;
    let cx = EvalContext {
        phase: Phase::Output,
        resolver: &NoResolver,
        generate: &generate,
        self_doc: DocRef(String::new()),
        macros: &macros,
        var: &var_scope.values,
        var_unset: &var_scope.unset,
        includes: &[],
        heading_level: 0,
        notes: Default::default(),
    };

    let body = walk_value(file.body, &lines, &cx, &mut errors);

    if errors.is_empty() {
        Ok(File {
            header: file.header,
            body,
        })
    } else {
        Err(errors)
    }
}

fn walk_value(value: Value, lines: &LineIndex, cx: &EvalContext, errors: &mut Vec<Error>) -> Value {
    match value {
        Value::Element(el) => {
            if matches!(el.sigil, Sigil::Dollar) {
                if let Some(ElementValue::Interp(expr)) = &el.value {
                    match tomet_expr::evaluate_data(expr, cx) {
                        Ok(data) => walk_value(data, lines, cx, errors),
                        Err(err) => {
                            let at = lines.line_col(el.span.start);
                            let message = match err {
                                tomet_expr::EvalError::NotData {
                                    is_vault_ref: true, ..
                                } => NO_VAULT.to_string(),
                                other => other.to_string(),
                            };
                            errors.push(Error {
                                message,
                                line: at.line,
                                column: at.column,
                                span: el.span,
                            });
                            Value::Element(el)
                        }
                    }
                } else {
                    Value::Element(el)
                }
            } else {
                Value::Element(el)
            }
        }
        Value::Map(entries) => {
            let mut new_entries = Vec::with_capacity(entries.len());
            for (k, v) in entries {
                let new_v = walk_value(v, lines, cx, errors);
                new_entries.push((k, new_v));
            }
            Value::Map(new_entries)
        }
        Value::Seq(items) => {
            let mut new_items = Vec::with_capacity(items.len());
            for item in items {
                new_items.push(walk_value(item, lines, cx, errors));
            }
            Value::Seq(new_items)
        }
        Value::Call(name, args) => {
            let mut new_args = Vec::with_capacity(args.len());
            for arg in args {
                new_args.push(walk_value(arg, lines, cx, errors));
            }
            Value::Call(name, new_args)
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macro_with_positional_and_named_arguments() {
        let src = r#"
::macros{
  vocab: "vocabularies/${1}/${1}@${2}.vocabulary.tmt"
  named: "hello ${who}"
}
pos: ${macro.vocab("writ", "1.0.0")}
nam: ${macro.named(who: "world")}
"#;
        let file = eval_str(src, &[]).unwrap();
        let Value::Map(entries) = file.body else {
            panic!("expected map");
        };
        assert_eq!(
            entries[0],
            (
                "pos".to_string(),
                Value::String("vocabularies/writ/writ@1.0.0.vocabulary.tmt".to_string())
            )
        );
        assert_eq!(
            entries[1],
            ("nam".to_string(), Value::String("hello world".to_string()))
        );
    }

    #[test]
    fn unknown_macro_error() {
        let src = "a: ${macro.nosuch(1)}\n";
        let errs = eval_str(src, &[]).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].line, 1);
        assert_eq!(errs[0].column, 4);
        assert_eq!(errs[0].message, "unknown macro `macro.nosuch`");
    }

    #[test]
    fn wrong_macro_arity_error() {
        let src = "::macros{ vocab: \"vocabularies/${1}/${1}@${2}.vocabulary.tmt\" }\na: ${macro.vocab(\"writ\")}\n";
        let errs = eval_str(src, &[]).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].line, 2);
        assert_eq!(errs[0].column, 4);
        assert_eq!(
            errs[0].message,
            "`macro.vocab` expects 2 argument(s), got 1"
        );
    }

    #[test]
    fn uncalled_macro_error() {
        let src = "::macros{ vocab: \"x\" }\na: ${macro.vocab}\n";
        let errs = eval_str(src, &[]).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].line, 2);
        assert_eq!(errs[0].column, 4);
        assert_eq!(
            errs[0].message,
            "the macro `vocab` has to be called: `macro.vocab(...)`"
        );
    }

    #[test]
    fn bare_word_macro_argument_is_unknown_name() {
        let src = "::macros{ vocab: \"x/${1}\" }\npath: ${macro.vocab(writ, \"1.0.0\")}\n";
        let errs = eval_str(src, &[]).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].line, 2);
        assert_eq!(errs[0].column, 7);
        assert_eq!(errs[0].message, "unknown name `writ`");
    }

    #[test]
    fn var_default() {
        let src = "::vars{ rev: { type: string, default: \"main\" } }\na: ${var.rev}\n";
        let file = eval_str(src, &[]).unwrap();
        let Value::Map(entries) = file.body else {
            panic!("expected map");
        };
        assert_eq!(
            entries[0],
            ("a".to_string(), Value::String("main".to_string()))
        );
    }

    #[test]
    fn var_that_holds_a_list_giving_a_list() {
        let src = "::vars{ tags: { type: list, default: list(\"a\", \"b\") } }\na: ${var.tags}\n";
        let file = eval_str(src, &[]).unwrap();
        let Value::Map(entries) = file.body else {
            panic!("expected map");
        };
        assert_eq!(
            entries[0],
            (
                "a".to_string(),
                Value::Seq(vec![
                    Value::String("a".to_string()),
                    Value::String("b".to_string())
                ])
            )
        );
    }

    #[test]
    fn a_map_comes_from_the_caller_not_from_a_declared_default() {
        // `::vars` has no `map` type: a declared default that is a map is refused...
        let src = "::vars{ cfg: { default: { host: \"localhost\" } } }\na: ${var.cfg}\n";
        let errs = eval_str(src, &[]).unwrap_err();
        assert!(
            errs[0].message.starts_with("`cfg` in `::vars`"),
            "{}",
            errs[0].message
        );

        // ...and a value the caller passes for a declared name is used as it is.
        let src = "::vars{ cfg: { type: string, default: \"none\" } }\na: ${var.cfg}\n";
        let passed = Value::Map(vec![(
            "host".to_string(),
            Value::String("localhost".to_string()),
        )]);
        let file = eval_str(src, &[("cfg".to_string(), passed.clone())]).unwrap();
        let Value::Map(entries) = file.body else {
            panic!("expected map");
        };
        assert_eq!(entries[0], ("a".to_string(), passed));
    }

    #[test]
    fn var_given_by_caller() {
        let src = "::vars{ rev: { type: string, default: \"main\" } }\na: ${var.rev}\n";
        let file = eval_str(
            src,
            &[("rev".to_string(), Value::String("feature".to_string()))],
        )
        .unwrap();
        let Value::Map(entries) = file.body else {
            panic!("expected map");
        };
        assert_eq!(
            entries[0],
            ("a".to_string(), Value::String("feature".to_string()))
        );
    }

    #[test]
    fn undeclared_var_access_and_caller_pass() {
        // Accessing undeclared var in expression
        let src = "a: ${var.undeclared}\n";
        let errs = eval_str(src, &[]).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].line, 1);
        assert_eq!(errs[0].column, 4);
        assert!(errs[0].message.contains("no member `undeclared`"));

        // Passing undeclared var from caller
        let src2 = "::vars{ ok: { default: 1 } }\na: 1\n";
        let errs2 = eval_str(
            src2,
            &[("unknown".to_string(), Value::String("x".to_string()))],
        )
        .unwrap_err();
        assert_eq!(errs2.len(), 1);
        assert!(
            errs2[0]
                .message
                .contains("no variable `unknown` is declared")
        );
    }

    #[test]
    fn unset_var_with_no_default_error() {
        let src = "::vars{ required: { type: string } }\na: ${var.required}\n";
        let errs = eval_str(src, &[]).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].line, 2);
        assert_eq!(errs[0].column, 4);
        assert!(
            errs[0]
                .message
                .contains("`var.required` is declared but has no value")
        );
    }

    #[test]
    fn built_in_add() {
        let src = "a: ${add(1, 2)}\n";
        let file = eval_str(src, &[]).unwrap();
        let Value::Map(entries) = file.body else {
            panic!("expected map");
        };
        assert_eq!(entries[0], ("a".to_string(), Value::Int(3)));
    }

    #[test]
    fn vault_functions_and_generate_as_errors() {
        let src = "a: ${tmt(\"a\")}\nb: ${generate.uuid()}\n";
        let errs = eval_str(src, &[]).unwrap_err();
        assert_eq!(errs.len(), 2);
        assert_eq!(errs[0].line, 1);
        assert_eq!(errs[0].column, 4);
        assert_eq!(errs[0].message, NO_VAULT);

        assert_eq!(errs[1].line, 2);
        assert_eq!(errs[1].column, 4);
        assert!(
            errs[1]
                .message
                .contains("can only be used when a document is being created")
        );
    }

    #[test]
    fn expression_nested_in_map_in_list() {
        let src = "items: list({ num: ${add(10, 20)} })\n";
        let file = eval_str(src, &[]).unwrap();
        let Value::Map(entries) = file.body else {
            panic!("expected map");
        };
        assert_eq!(
            entries[0],
            (
                "items".to_string(),
                Value::Seq(vec![Value::Map(vec![("num".to_string(), Value::Int(30))])])
            )
        );
    }

    #[test]
    fn quoted_expression_left_as_text() {
        let src = "a: \"${macro.vocab(\\\"writ\\\")}\"\n";
        let file = eval_str(src, &[]).unwrap();
        let Value::Map(entries) = file.body else {
            panic!("expected map");
        };
        assert_eq!(
            entries[0],
            (
                "a".to_string(),
                Value::String("${macro.vocab(\"writ\")}".to_string())
            )
        );
    }

    #[test]
    fn expression_in_key_and_declaration_errors() {
        let src_key = "${macro.vocab(\"writ\")}: 1\n";
        let errs_key = eval_str(src_key, &[]).unwrap_err();
        assert_eq!(errs_key.len(), 1);
        assert!(
            errs_key[0]
                .message
                .contains("an expression cannot be a key")
        );

        let src_decl = "::kind(${var.x})\n";
        let errs_decl = eval_str(src_decl, &[]).unwrap_err();
        assert_eq!(errs_decl.len(), 1);
        assert!(
            errs_decl[0]
                .message
                .contains("a declaration takes no expression")
        );
    }

    #[test]
    fn several_errors_in_one_file_all_reported() {
        let src = "a: ${macro.nosuch(1)}\nb: ${add(1)}\nc: ${tmt(\"x\")}\n";
        let errs = eval_str(src, &[]).unwrap_err();
        assert_eq!(errs.len(), 3);
        assert_eq!(errs[0].line, 1);
        assert_eq!(errs[0].column, 4);
        assert_eq!(errs[0].message, "unknown macro `macro.nosuch`");

        assert_eq!(errs[1].line, 2);
        assert_eq!(errs[1].column, 4);
        assert_eq!(errs[1].message, "`add` expects 2 argument(s), got 1");

        assert_eq!(errs[2].line, 3);
        assert_eq!(errs[2].column, 4);
        assert_eq!(errs[2].message, NO_VAULT);
    }

    #[test]
    fn vault_references_in_ctmt_say_no_vault() {
        let tmt_err = eval_str("a: ${tmt(\"a\")}\n", &[]).unwrap_err();
        assert_eq!(tmt_err[0].message, NO_VAULT);

        let ref_err = eval_str("a: ${ref(\"n\")}\n", &[]).unwrap_err();
        assert_eq!(
            ref_err[0].message,
            "note `n`: no vault to find the note `n` in"
        );

        let self_err = eval_str("a: ${self}\n", &[]).unwrap_err();
        assert_eq!(self_err[0].message, NO_VAULT);

        let vault_err = eval_str("a: ${vault.docs}\n", &[]).unwrap_err();
        assert_eq!(
            vault_err[0].message,
            "no vault to list the vault root's documents in"
        );

        let fn_err = eval_str("a: ${x => x}\n", &[]).unwrap_err();
        assert_eq!(fn_err[0].message, "a function cannot be stored as data");
    }

    #[test]
    fn bad_var_decl_is_reported_with_line_and_col() {
        let src = "::vars{ bad: 123 }\na: ${add(1, 2)}\n";
        let errs = eval_str(src, &[]).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].line, 1);
        assert_eq!(errs[0].column, 1);
        assert!(
            errs[0].message.starts_with("`bad` in `::vars`"),
            "{}",
            errs[0].message
        );
    }
}
