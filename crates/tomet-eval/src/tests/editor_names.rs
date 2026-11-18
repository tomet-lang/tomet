//! The names an editor offers.

use super::*;

/// Every name in [`FUNCTIONS`] is one the evaluator knows. Called with no
/// arguments it fails for some reason of its own, never for not existing.
#[test]
fn every_listed_function_is_one_the_evaluator_answers() {
    for (name, detail) in FUNCTIONS {
        assert!(!detail.is_empty(), "{name} has no description");
        if *name == "addr" {
            // Read by the parser: `addr()` is a syntax error, not a call.
            continue;
        }
        if let Err(EvalError::UnknownFunction(unknown)) = in_vault(&format!("{name}()")) {
            panic!("`{unknown}` is listed in FUNCTIONS but the evaluator does not know it");
        }
    }
}

/// Every name in [`RESERVED_NAMES`] starts an expression the evaluator reads.
#[test]
fn every_reserved_name_is_one_the_evaluator_answers() {
    for (name, detail) in RESERVED_NAMES {
        assert!(!detail.is_empty(), "{name} has no description");
        if let Err(EvalError::UnknownName(unknown)) = in_vault(name) {
            panic!("`{unknown}` is listed in RESERVED_NAMES but the evaluator does not know it");
        }
    }
}

#[test]
fn no_function_is_listed_twice() {
    let mut names: Vec<&str> = FUNCTIONS.iter().map(|(name, _)| *name).collect();
    names.sort_unstable();
    let before = names.len();
    names.dedup();
    assert_eq!(before, names.len());
}
