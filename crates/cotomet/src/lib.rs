//! =[ cotomet ]
//!
//! The data language dual to tomet (`tmt` / `ctmt`): a lightweight, expressive
//! syntax for values, maps and sequences. This is the crate to depend on when
//! using cotomet from Rust. It holds no logic of its own.
//!
//! =[ Overview ]
//!
//! - [`syntax`] is the language: parser, printer and the spelling helpers
//!   from `cotomet-syntax`. Read its crate doc for what cotomet accepts. Without
//!   the `eval` feature, an unquoted expression `${...}` is an error.
//! - The top level is the serde side (`serde_cotomet`): [`from_str`] and
//!   [`to_string`] read and write Rust types as cotomet.
//! - [`eval`] evaluates `${macro...}` and `${var...}` expressions in `.ctmt`
//!   files when the optional `eval` feature is enabled.
//!
//! A crate that sits below this one in the workspace layering (the tomet
//! parser, the printers) depends on `cotomet-syntax` directly.

pub use cotomet_syntax as syntax;
pub use serde_cotomet::{
    Error, Result, Value, from_str, from_value, print_value, to_string, to_value,
};

#[cfg(feature = "eval")]
pub use cotomet_eval as eval;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unquoted_expression_errors_in_data_only() {
        let res = syntax::parse_file("a: ${macro.vocab(\"writ\", \"1.0.0\")}\n");
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
        let src = "::macros{ vocab: \"vocabularies/${1}/${1}@${2}.vocabulary.tmt\" }\na: ${macro.vocab(\"writ\", \"1.0.0\")}\n";
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
}
