//! What an expression takes from outside the file it is written in.
//!
//! An expression is read from its own bytes alone unless it names something the file does not
//! hold: a macro or a variable the file does not define, the vault, another document, where the
//! file sits, the type a vault gives a metadata field. [`reaches_outside`] finds those without
//! evaluating anything, which is what `::profile(standalone)` is checked by.
//!
//! The evaluator decides what each name does, so this module is the second reading of the same
//! facts and sits beside it: [`FUNCTIONS`](crate::FUNCTIONS) and the members of a document are
//! sorted here, and a test holds the sorting to the list of functions, so a function added there
//! has to be given a verdict here.

use tomet_ast::{Document, InterpExpr, InterpExprKind, Literal};
use tomet_span::Span;

use crate::address;

/// Functions that answer from another document.
const OTHER_DOCUMENT_FUNCTIONS: &[&str] = &["tmt", "ref", "include"];

/// Functions whose answer contains where the file sits: `link` writes `tmt:<path of the file>`.
const PLACEMENT_FUNCTIONS: &[&str] = &["link"];

/// Functions that answer from their arguments alone, kept so that every function has a verdict
/// (`addr` is read by the parser into [`InterpExprKind::Addr`] and is judged there). Nothing looks
/// at this list but the test that holds it to `FUNCTIONS`.
#[cfg(test)]
const INSIDE_FUNCTIONS: &[&str] = &[
    "add", "sub", "mul", "div", "mod", "not", "exists", "emoji", "unicode", "addr",
];

/// What an expression takes from outside the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reach {
    /// `vault`, however it is used.
    Vault,
    /// `macro.name` where the file has no `::macros` entry for `name`.
    Macro(String),
    /// `var.name` where the file has no `::vars` entry for `name`.
    Var(String),
    /// A call that answers from another document: `tmt`, `ref`, `include`.
    OtherDocument(&'static str),
    /// A value that contains where the file sits: `self.path`, `self.filename`, `link`.
    Placement(&'static str),
    /// `self.meta`, whose fields are typed by the vault's `::fields`.
    FieldType,
    /// An address written with `tmt:`, `ref:` or `dir:` on the left of `#`.
    Address(&'static str),
    /// `.addr()` on a string that is not written out, so where it points cannot be told.
    UnknownAddress,
}

impl std::fmt::Display for Reach {
    /// What it is that the file does not hold, as a noun phrase a report can build a sentence on.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Reach::Vault => write!(f, "the vault (`vault`)"),
            Reach::Macro(name) => {
                write!(f, "the macro `{name}`, which `::macros` does not define")
            }
            Reach::Var(name) => {
                write!(f, "the variable `{name}`, which `::vars` does not declare")
            }
            Reach::OtherDocument(function) => write!(f, "another document (`{function}(...)`)"),
            Reach::Placement("link") => write!(f, "a link, which writes where the file sits"),
            Reach::Placement(what) => write!(f, "where the file sits (`self.{what}`)"),
            Reach::FieldType => {
                write!(f, "the type the vault gives a metadata field (`self.meta`)")
            }
            Reach::Address(scheme) => {
                write!(
                    f,
                    "a document outside the file (an address with `{scheme}:`)"
                )
            }
            Reach::UnknownAddress => write!(
                f,
                "an address built at run time, which cannot be shown to stay inside the file"
            ),
        }
    }
}

/// One place an expression reaches outside, with where in the expression it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reaching {
    pub reach: Reach,
    pub span: Span,
}

/// The names a file defines for itself: what `macro.x` and `var.x` may use without the vault.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OwnNames {
    pub macros: Vec<String>,
    pub vars: Vec<String>,
}

impl OwnNames {
    /// The macros and variables `doc` itself declares.
    pub fn of_document(doc: &Document) -> OwnNames {
        OwnNames {
            macros: tomet_declaration::document_macros(doc)
                .into_iter()
                .map(|(name, _)| name)
                .collect(),
            vars: tomet_declaration::document_vars(doc)
                .into_iter()
                .map(|(name, _)| name)
                .collect(),
        }
    }
}

/// Every place `expr` takes something from outside the file, in the order they are written.
///
/// Nothing is evaluated, so an expression that would fail is judged by what it names. A lambda
/// parameter is never outside (it is given by the expression's own receiver, which is walked), and
/// `self`, `#id` and `self.kind` are the file itself.
pub fn reaches_outside(expr: &InterpExpr, own: &OwnNames) -> Vec<Reaching> {
    let mut found = Vec::new();
    walk(expr, own, &mut found);
    found
}

fn walk(expr: &InterpExpr, own: &OwnNames, out: &mut Vec<Reaching>) {
    let mut push = |reach: Reach| {
        out.push(Reaching {
            reach,
            span: expr.span,
        })
    };
    match &expr.kind {
        InterpExprKind::Literal(_) | InterpExprKind::Id(_) => {}
        InterpExprKind::Identifier(name) => {
            if name == "vault" {
                push(Reach::Vault);
            }
        }
        InterpExprKind::Group(inner) => walk(inner, own, out),
        InterpExprKind::Lambda { body, .. } => walk(body, own, out),
        InterpExprKind::NamedArg { value, .. } => walk(value, own, out),
        InterpExprKind::Binary { lhs, rhs, .. } => {
            walk(lhs, own, out);
            walk(rhs, own, out);
        }
        InterpExprKind::DocId { doc, .. } => walk(doc, own, out),
        InterpExprKind::Addr { text, .. } => {
            if let Some(scheme) = address::outside_scheme(text) {
                push(Reach::Address(scheme));
            }
        }
        InterpExprKind::Member { object, member } => {
            match (name_of(object), member.as_str()) {
                (Some("var"), name) if !own.vars.iter().any(|v| v == name) => {
                    push(Reach::Var(name.to_string()));
                }
                (Some("macro"), name) if !own.macros.iter().any(|m| m == name) => {
                    push(Reach::Macro(name.to_string()));
                }
                (Some("self"), "path") => push(Reach::Placement("path")),
                (Some("self"), "filename") => push(Reach::Placement("filename")),
                (Some("self"), "meta") => push(Reach::FieldType),
                _ => {}
            }
            walk(object, own, out);
        }
        InterpExprKind::Call { callee, args } => {
            match &callee.kind {
                InterpExprKind::Identifier(function) => {
                    if let Some(function) = OTHER_DOCUMENT_FUNCTIONS.iter().find(|f| *f == function)
                    {
                        push(Reach::OtherDocument(function));
                    } else if let Some(function) =
                        PLACEMENT_FUNCTIONS.iter().find(|f| *f == function)
                    {
                        push(Reach::Placement(function));
                    }
                }
                InterpExprKind::Member { object, member } => {
                    match (name_of(object), member.as_str()) {
                        (Some("macro"), name) => {
                            if !own.macros.iter().any(|m| m == name) {
                                push(Reach::Macro(name.to_string()));
                            }
                        }
                        (_, "link") => {
                            push(Reach::Placement("link"));
                            walk(object, own, out);
                        }
                        (_, "addr") => match &object.kind {
                            InterpExprKind::Literal(Literal::String(text)) => {
                                if let Some(scheme) = address::outside_scheme(text) {
                                    push(Reach::Address(scheme));
                                }
                            }
                            _ => {
                                push(Reach::UnknownAddress);
                                walk(object, own, out);
                            }
                        },
                        _ => walk(object, own, out),
                    }
                }
                _ => walk(callee, own, out),
            }
            for arg in args {
                walk(arg, own, out);
            }
        }
    }
}

/// The name an expression is, when it is a bare name (looking through parentheses).
fn name_of(expr: &InterpExpr) -> Option<&str> {
    match &expr.kind {
        InterpExprKind::Identifier(name) => Some(name),
        InterpExprKind::Group(inner) => name_of(inner),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FUNCTIONS;

    fn parse(source: &str) -> InterpExpr {
        tomet_parser::parse_interpolation_str(&format!("${{{source}}}"))
            .unwrap()
            .expr
    }

    fn reaches(source: &str) -> Vec<Reach> {
        let own = OwnNames {
            macros: vec!["mine".to_string()],
            vars: vec!["name".to_string()],
        };
        reaches_outside(&parse(source), &own)
            .into_iter()
            .map(|found| found.reach)
            .collect()
    }

    #[test]
    fn every_function_is_given_a_verdict_exactly_once() {
        let judged: Vec<&str> = OTHER_DOCUMENT_FUNCTIONS
            .iter()
            .chain(PLACEMENT_FUNCTIONS)
            .chain(INSIDE_FUNCTIONS)
            .copied()
            .collect();
        for (name, _) in FUNCTIONS {
            assert_eq!(
                judged.iter().filter(|j| *j == name).count(),
                1,
                "`{name}` is in FUNCTIONS and needs one verdict in outside.rs"
            );
        }
        assert_eq!(
            judged.len(),
            FUNCTIONS.len(),
            "a verdict for a function that does not exist"
        );
    }

    #[test]
    fn what_the_file_holds_is_not_outside() {
        for source in [
            "1",
            "add(1, 2)",
            "macro.mine(1)",
            "var.name",
            "self.kind",
            "self.item(0)",
            "#intro",
            "self#intro",
            "addr(#intro)",
            "\"#intro\".addr()",
            "(a, b) => add(a, b)",
            "exists(self.kind) && not(false)",
        ] {
            assert_eq!(reaches(source), vec![], "`{source}`");
        }
    }

    #[test]
    fn what_the_file_does_not_define_is_outside() {
        assert_eq!(
            reaches("macro.other(1)"),
            vec![Reach::Macro("other".into())]
        );
        assert_eq!(reaches("var.other"), vec![Reach::Var("other".into())]);
        assert_eq!(reaches("vault.docs"), vec![Reach::Vault]);
        assert_eq!(reaches("self.path"), vec![Reach::Placement("path")]);
        assert_eq!(reaches("self.filename"), vec![Reach::Placement("filename")]);
        assert_eq!(reaches("self.meta.created"), vec![Reach::FieldType]);
        assert_eq!(
            reaches("tmt(\"a.tmt\").kind"),
            vec![Reach::OtherDocument("tmt")]
        );
        assert_eq!(reaches("ref(\"note\")"), vec![Reach::OtherDocument("ref")]);
        assert_eq!(
            reaches("include(tmt(\"a.tmt\"))"),
            vec![Reach::OtherDocument("include"), Reach::OtherDocument("tmt")]
        );
        assert_eq!(reaches("link(#intro)"), vec![Reach::Placement("link")]);
        assert_eq!(reaches("self.link()"), vec![Reach::Placement("link")]);
        assert_eq!(reaches("#intro.link()"), vec![Reach::Placement("link")]);
    }

    #[test]
    fn an_address_is_outside_by_what_is_left_of_the_hash() {
        assert_eq!(
            reaches("addr(tmt:docs/a.tmt#intro)"),
            vec![Reach::Address("tmt")]
        );
        assert_eq!(reaches("addr(ref:note)"), vec![Reach::Address("ref")]);
        assert_eq!(reaches("\"dir:docs\".addr()"), vec![Reach::Address("dir")]);
        assert_eq!(reaches("var.name.addr()"), vec![Reach::UnknownAddress]);
    }

    #[test]
    fn a_reach_inside_an_argument_or_a_lambda_is_found() {
        assert_eq!(reaches("add(1, vault.docs)"), vec![Reach::Vault]);
        assert_eq!(
            reaches("f => macro.other(f)"),
            vec![Reach::Macro("other".into())]
        );
        assert_eq!(
            reaches("macro.mine(self.path)"),
            vec![Reach::Placement("path")]
        );
    }

    #[test]
    fn the_names_a_document_defines_are_read_from_the_document() {
        let doc = tomet_parser::parse_document(
            "::macros{ gh: \"https://x/${1}\" }\n::vars{ topic: { type: string } }\n",
        )
        .unwrap();
        let own = OwnNames::of_document(&doc);
        assert_eq!(own.macros, vec!["gh".to_string()]);
        assert_eq!(own.vars, vec!["topic".to_string()]);
    }
}
