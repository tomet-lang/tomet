//! =[ cotomet ]
//!
//! The data language dual to tomet (`tmt` / `ctmt`): a lightweight, expressive
//! syntax for values, maps and sequences. This is the crate to depend on when
//! using cotomet from Rust.
//!
//! - Top-level serde support is provided by `serde_cotomet`: [`from_str`] and
//!   [`to_string`] read and write Rust types as cotomet.
//! - [`parse_file`] reads standalone `.ctmt` data files into a [`File`].
//! - [`eval`] evaluates `${macro...}` and `${var...}` expressions in `.ctmt`
//!   files when the optional `eval` feature is enabled.

use tomet_position::LineIndex;
use tomet_span::Span;

pub use serde_cotomet::{
    Error as SerdeError, Value as SerdeValue, from_str, from_value, print_value, to_string,
    to_value,
};
pub type File = tomet_ast::File<tomet_ast::Value>;
pub use tomet_ast::{Declaration, Value};
pub use tomet_ast_parser::{Error, Result, parse_value};

#[cfg(feature = "eval")]
pub use cotomet_eval as eval;

/// Parse a standalone cotomet file (`.ctmt`): declarations, then a map of
/// `key: value` entries.
///
/// Unquoted `${...}` expressions are rejected (this reader reads data only).
pub fn parse_file(src: &str) -> Result<File> {
    let file = tomet_ast_parser::parse_data_file(src)?;
    if let Some(span) = find_interp(&file.body) {
        let at = LineIndex::new(src).line_col(span.start);
        return Err(Error {
            message: "an expression `${...}` is not read: this reader reads data only".to_string(),
            line: at.line,
            column: at.column,
            offset: span.start,
        });
    }
    Ok(file)
}

/// Parse a standalone cotomet file (`.ctmt`) allowing unquoted `${...}` expressions
/// (e.g. for expression evaluation or formatting).
pub fn parse_file_with_expressions(src: &str) -> Result<File> {
    tomet_ast_parser::parse_data_file(src)
}

fn find_interp(v: &Value) -> Option<Span> {
    match v {
        Value::Interp(interp) => Some(interp.span),
        Value::Seq(items) => items.iter().find_map(find_interp),
        Value::Map(entries) => entries.iter().find_map(|(_, v)| find_interp(v)),
        Value::Call(_, args) => args.iter().find_map(find_interp),
        _ => None,
    }
}

pub mod syntax {
    pub use crate::{File, parse_file, parse_file_with_expressions};
    pub use tomet_ast::{Declaration, Value};
    pub use tomet_ast_parser::{Error, Result, parse_value};

    pub fn parse_file_with<H>(_src: &str, _hook: &mut H) -> Result<File> {
        crate::parse_file_with_expressions(_src)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Value {
        Value::String(s.to_string())
    }

    fn map(entries: &[(&str, Value)]) -> Value {
        Value::Map(
            entries
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        )
    }

    fn declared(file: &File) -> Vec<(String, Option<Value>, Option<Value>)> {
        file.header
            .iter()
            .map(|d| {
                (
                    d.name.name.clone(),
                    d.data.args.clone(),
                    d.data.value.as_ref().map(|v| v.as_data()),
                )
            })
            .collect()
    }

    #[test]
    fn unquoted_expression_errors_in_data_only() {
        let res = parse_file("a: ${macro.vocab(\"writ\", \"1.0.0\")}\n");
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(
            err.message
                .contains("an expression `${...}` is not read: this reader reads data only"),
            "{}",
            err.message
        );
    }

    #[cfg(feature = "eval")]
    #[test]
    fn eval_feature_evaluates_expression() {
        let src = "::macros(vocab: \"vocabularies/${1}/${1}@${2}.vocabulary.tmt\")\na: ${macro.vocab(\"writ\", \"1.0.0\")}\n";
        let file = eval::eval_str(src, &[]).unwrap();
        let Value::Map(entries) = file.body else {
            panic!("expected map");
        };
        assert_eq!(
            entries[0],
            (
                "a".to_string(),
                Value::String("vocabularies/writ/writ@1.0.0.vocabulary.tmt".to_string())
            )
        );
    }

    #[test]
    fn a_file_may_open_with_declarations() {
        let f = parse_file(
            "// header\n::kind(catalog, 2)\n::use(deck)\n::meta(id: x, n: 1)\n\n\"a/b\": 1\nm: { k: v }\n",
        )
        .unwrap();
        assert_eq!(
            declared(&f),
            [
                (
                    "kind".to_string(),
                    Some(Value::Map(vec![
                        ("".to_string(), text("catalog")),
                        ("".to_string(), Value::Int(2)),
                    ])),
                    None
                ),
                ("use".to_string(), Some(text("deck")), None),
                (
                    "meta".to_string(),
                    Some(map(&[("id", text("x")), ("n", Value::Int(1))])),
                    None
                ),
            ]
        );
        assert_eq!(
            f.body,
            map(&[("a/b", Value::Int(1)), ("m", map(&[("k", text("v"))]))])
        );
    }

    #[test]
    fn a_declaration_may_have_both_groups_and_a_comment_after_it() {
        let f = parse_file("::x(a, b){ c: 1 } // note\n").unwrap();
        assert_eq!(
            declared(&f),
            [(
                "x".to_string(),
                Some(Value::Map(vec![
                    ("".to_string(), text("a")),
                    ("".to_string(), text("b")),
                ])),
                Some(map(&[("c", Value::Int(1))]))
            )]
        );
        let f = parse_file("::x{ c: 1 }(a)\n").unwrap();
        assert_eq!(f.header[0].data.args, Some(text("a")));
        // `()` is a declaration's primary group, empty: `::draft` alone does not parse.
        let f = parse_file("::draft()\n").unwrap();
        assert_eq!(
            declared(&f),
            [("draft".to_string(), Some(Value::Map(vec![])), None)]
        );
        assert!(parse_file("::draft\n").is_err());
    }

    #[test]
    fn a_declaration_reads_mixed_args_like_tmt() {
        let f = parse_file("::kind(note, version: 2)\n").unwrap();
        assert_eq!(
            f.header[0].data.args,
            Some(Value::Map(vec![
                ("".to_string(), text("note")),
                ("version".to_string(), Value::Int(2)),
            ]))
        );
    }

    #[test]
    fn a_file_with_only_a_header_has_an_empty_body() {
        let f = parse_file("::kind(catalog)\n").unwrap();
        assert_eq!(f.body, Value::Map(vec![]));
        assert_eq!(f.header.len(), 1);
    }

    #[test]
    fn a_declaration_records_where_it_starts() {
        let f = parse_file("// c\n::kind(a)\n  ::use(b)\n").unwrap();
        assert_eq!(f.header[0].span, Span::new(5, 14));
        assert_eq!(f.header[1].span, Span::new(17, 25));
    }

    #[test]
    fn a_declaration_span_ends_at_the_last_group_delimiter() {
        let src = "::kind(a)   // comment\nkey: 1\n";
        let f = parse_file(src).unwrap();
        assert_eq!(f.header[0].span, Span::new(0, 9));
        assert_eq!(
            &src[f.header[0].span.start..f.header[0].span.end],
            "::kind(a)"
        );
    }

    #[test]
    fn a_declaration_below_the_first_entry_is_an_error_that_says_where_it_goes() {
        let err = parse_file("a: 1\n::kind(x)\n").unwrap_err();
        assert!(err.message.contains("top of the file"), "{}", err.message);
        assert_eq!((err.line, err.column), (2, 1));
    }

    #[test]
    fn what_is_not_a_declaration_is_what_it_was() {
        assert!(parse_file("::(x)\n").is_err());
        assert!(parse_file("::\n").is_err());
        assert_eq!(
            parse_file("a: std::vec\n").unwrap().body,
            map(&[("a", text("std::vec"))])
        );
        assert!(parse_file("::kind(x)\nstray\n").is_err());
    }

    #[test]
    fn something_after_a_declaration_on_its_line_is_an_error() {
        assert!(parse_file("::kind(x) trailing\n").is_err());
        assert!(parse_file("::kind(x\n").is_err());
    }

    #[test]
    fn two_files_with_one_meaning_are_equal_whatever_their_offsets() {
        let a = parse_file("::kind(x)\na: 1\n").unwrap();
        let b = parse_file("\n\n::kind(x)   // c\n\na: 1\n").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn a_file_is_a_map_of_keyed_entries() {
        assert_eq!(
            parse_file("a: 1\n// note\n\"b/c\": list(x)\n")
                .unwrap()
                .body,
            map(&[("a", Value::Int(1)), ("b/c", Value::Seq(vec![text("x")]))])
        );
        assert_eq!(parse_file("").unwrap().body, Value::Map(vec![]));
        assert_eq!(
            parse_file("// only a comment\n").unwrap().body,
            Value::Map(vec![])
        );
        assert!(parse_file("a: 1\n").unwrap().header.is_empty());
    }
}
