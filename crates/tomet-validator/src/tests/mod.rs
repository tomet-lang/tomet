//! The validator's unit tests, by topic.

mod declarations;
mod elements;
mod fields;
mod ids;
mod links;
mod preamble;
mod rules;
mod standalone;
mod type_seats;
mod typed_arguments;
mod versions;
mod warnings;

use super::*;
use tomet_ast::File;
use tomet_scalar::Scalar;

fn parse(source: &str) -> File {
    tomet_ast_parser::parse_file(source).expect("valid Tomet source")
}

/// These test `check_rule_connects` directly rather than through
/// `validate_document`, so the assertions stay about `::rule` alone --
/// nesting a block-shaped element (`@card`/`@heading`) inside another
/// element's `[content]` also trips the unrelated, pre-existing
/// `ShapeMismatch` check (inline position vs. block-required kind),
/// which is real but has nothing to do with what these tests check.
fn rule_errors(doc: &File, bindings: &Bindings) -> Vec<Diagnostic> {
    let mut errors = Vec::new();
    check_rule_connects(doc, bindings, &mut errors);
    errors
}

/// A vocabulary with a declared parameter list.
fn deck_with_params() -> Bindings {
    let vocab = tomet_semantics::Vocabulary::from_file(&parse(
        r#"::kind(vocabulary)
::vocabulary(deck)

::element(
  card: (
    args: (
      id: (
        positional: true
        required: true
        about: | 通し番号。
      )
      tags: (
        about: | タグ。
      )
    )
    about: | カード。
  )
  plain: (
    about: | 宣言なし。
  )
)
"#,
    ))
    .expect("a vocabulary");
    Bindings::for_document(&parse("::kind(deck)\n"), [vocab])
}

/// A document with `deck` in scope.
///
/// A namespaced name passes only when its namespace is bound
/// (`Bindings`), so the namespace has to actually be in scope here, and
/// saying so is what keeps these tests about duplicate ids.
fn with_deck() -> Bindings {
    let vocab = tomet_ast_parser::parse_file(
        "::kind(vocabulary)\n::vocabulary(deck)\n\n::element(task: (), ref: ())\n",
    )
    .expect("vocabulary parses");
    Bindings {
        used: [(
            "deck".to_string(),
            tomet_semantics::Vocabulary::from_file(&vocab).expect("has a header"),
        )]
        .into_iter()
        .collect(),
        ..Bindings::default()
    }
}

fn field_diagnostics(source: &str, declared: &[(&str, &str)]) -> Vec<Diagnostic> {
    let types = declared
        .iter()
        .map(|(field, field_type)| (field.to_string(), field_type.to_string()))
        .collect();
    let mut errors = Vec::new();
    check_field_types(&parse(source), &types, &mut errors);
    errors
}

const UUID: &str = "123e4567-e89b-42d3-a456-426614174000";

fn version_problems(src: &str) -> Vec<String> {
    validate_document(&parse(src))
        .iter()
        .filter(|e| matches!(e, Diagnostic::InvalidDefinition { name, .. } if name == "version"))
        .map(|e| e.to_string())
        .collect()
}

fn argument_problems(src: &str) -> Vec<String> {
    validate_document_with(&parse(&format!("::kind(deck)\n\n{src}\n")), &typed_deck())
        .iter()
        .filter(|e| matches!(e, Diagnostic::InvalidArgument { .. }))
        .map(|e| e.to_string())
        .collect()
}

const TYPED_DECK: &str = r#"::kind(vocabulary)
::vocabulary(deck)

::element(
  entry: (
    args: (
      id: (type: uint, positional: true)
      label: (type: string)
      rev: (type: version)
      done: (type: bool)
      owner: (type: person)
      tags: (type: list(string))
      unsupported: (type: set(string))
    )
  )
)
"#;

fn typed_deck() -> Bindings {
    let vocab = tomet_semantics::Vocabulary::from_file(&parse(TYPED_DECK)).expect("a vocabulary");
    Bindings::for_document(&parse("::kind(deck)\n"), [vocab])
}
