//! Type seats (`<$uuid(default: auto)>`): where they may be written, and whether each is a type.
//!
//! A type seat is the blueprint's: it says what goes there and where the value comes from, and a
//! document made from the blueprint has none. So one in any other document is an error, and one
//! in a blueprint is held to what it says -- the type it is written as has to be a type, and a
//! `default` written out has to be of that type -- before `tomet new` meets it.
//!
//! The seat is read as a blueprint's reader would read it but without the blueprint's
//! surroundings: nothing is evaluated against a vault, a variable or a macro. A seat that takes
//! anything from outside the text it is written in is judged when it is filled, where those are
//! known, and not here.

use std::collections::HashMap;

use tomet_ast::{File, TypeSeat};
use tomet_expr::{DocRef, EvalContext, EvalValue, NoResolver, OwnNames, evaluate_type};
use tomet_semantics_core::admits_default;

use crate::Diagnostic;

/// Reports every type seat of `doc` that is not where it may be or is not a type: outside a
/// blueprint, all of them; inside one, the seats whose type does not evaluate or whose written
/// `default` is not of the type.
pub fn check_type_seats(doc: &File, errors: &mut Vec<Diagnostic>) {
    let blueprint = tomet_declaration::is_blueprint(doc);
    tomet_tree::for_each_type_seat(doc, |seat| {
        if !blueprint {
            errors.push(Diagnostic::TypeSeatOutsideBlueprint {
                seat: seat.to_string(),
                span: seat.span,
            });
        } else if let Some(reason) = problem_with(seat) {
            errors.push(Diagnostic::InvalidTypeSeat {
                seat: seat.to_string(),
                reason,
                span: seat.span,
            });
        }
    });
}

/// What is wrong with the type `seat` is written as, or `None` when nothing is, or when it cannot
/// be told from the text alone.
fn problem_with(seat: &TypeSeat) -> Option<String> {
    if !tomet_expr::reaches_outside(&seat.expr, &OwnNames::default()).is_empty() {
        return None;
    }
    let cx = EvalContext {
        resolver: &NoResolver,
        self_doc: DocRef(String::new()),
        macros: &HashMap::new(),
        var: &EvalValue::Null,
        var_unset: &[],
        includes: &[],
        heading_level: 0,
        notes: Default::default(),
    };
    match evaluate_type(&seat.expr, &cx) {
        Err(error) => Some(error.to_string()),
        Ok(ty) => admits_default(&ty)
            .err()
            .map(|mismatch| format!("`default`: {mismatch}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_seats_in_blueprints_accept_list_defaults() {
        for src in [
            "::kind(blueprint)\n\n<$list(string, default: (\"a\", \"b\"))>\n",
            "::kind(blueprint)\n\n<$list(string, default: ())>\n",
            "::kind(blueprint)\n\n<$list(string, default: (\"a\",))>\n",
            "::kind(blueprint)\n\n<$list(string, default: (\"a\"))>\n",
        ] {
            let file = tomet_ast_parser::parse_file(src).unwrap();
            let mut errors = Vec::new();
            check_type_seats(&file, &mut errors);
            assert!(errors.is_empty(), "{src}: {errors:?}");
        }

        let bad_src = "::kind(blueprint)\n\n<$list(int, default: (1, \"b\"))>\n";
        let file = tomet_ast_parser::parse_file(bad_src).unwrap();
        let mut errors = Vec::new();
        check_type_seats(&file, &mut errors);
        assert!(!errors.is_empty(), "expected mismatch error for string in int list default");
    }
}
