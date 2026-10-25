//! Validation of a document's declarations apart from any host syntax.
//!
//! Checks the positional argument count, version slots, declared versions,
//! macros, and variable definitions.

use tomet_ast::Value;
use tomet_type::Type;

use crate::declaration::Declaration;
use crate::kind::{DeclKind, Host};
use crate::vars::{VarDecl, read_var_decls};

/// A problem found with a document's declarations.
///
/// Host-neutral and carries no span: a `.tmt` validator maps it back to
/// an element's span, and a `.ctmt` checker will do the same for a file header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// A version argument in a declaration that is not an integer of 1 or more.
    InvalidVersion {
        declaration: &'static str,
        found: String,
    },
    /// More arguments were supplied than the declaration takes positional slots for.
    TooManyArguments {
        declaration: &'static str,
        takes: usize,
    },
    /// An entry in a declaration's data or arguments that is malformed or invalid.
    InvalidDefinition {
        declaration: &'static str,
        name: String,
        reason: String,
    },
    /// A declaration written in a host that does not permit it.
    NotForThisHost { declaration: &'static str },
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Problem::InvalidVersion { declaration, found } => {
                write!(
                    f,
                    "the version in `::{declaration}(...)` must be an integer of 1 or more, found {found}"
                )
            }
            Problem::TooManyArguments { declaration, takes } => {
                write!(
                    f,
                    "`{declaration}` takes at most {takes} arguments: the name, then an optional version"
                )
            }
            Problem::InvalidDefinition {
                declaration,
                name,
                reason,
            } => write!(f, "`{name}` in `::{declaration}`: {reason}"),
            Problem::NotForThisHost { declaration } => {
                write!(f, "`::{declaration}` is not permitted in this host")
            }
        }
    }
}

/// A version value the way a reader wrote it: strings quoted, floats with their
/// point (`1.0`, not `1`).
fn version_as_written(value: &Value) -> String {
    match value {
        Value::String(s) => format!("{s:?}"),
        Value::Float(f) => format!("{f:?}"),
        Value::Number(text) => text.clone(),
        Value::Int(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        _ => "a value that is not a number".to_string(),
    }
}

/// Checks `declarations` for validity within `host`.
///
/// Returns findings paired with the index into `declarations` where each was found.
/// Non-standard declarations are ignored.
pub fn check_declarations(declarations: &[Declaration], host: Host) -> Vec<(usize, Problem)> {
    let mut problems = Vec::new();

    for (idx, decl) in declarations.iter().enumerate() {
        let Some(kind) = decl.kind() else { continue };
        let spec = kind.spec();

        if !spec.scope.allows(host) {
            problems.push((
                idx,
                Problem::NotForThisHost {
                    declaration: spec.name,
                },
            ));
            continue;
        }

        match kind {
            DeclKind::Kind | DeclKind::Use => {
                check_version_slot(idx, decl, spec.name, &mut problems);
            }
            DeclKind::Version | DeclKind::Vocabulary | DeclKind::Blueprint => {
                check_declared_version(idx, decl, kind, spec.name, &mut problems);
            }
            DeclKind::Macros => {
                check_macros(idx, decl, spec.name, &mut problems);
            }
            DeclKind::Vars => {
                check_vars(idx, decl, spec.name, &mut problems);
            }
            _ => {}
        }
    }

    problems
}

fn check_version_slot(
    idx: usize,
    decl: &Declaration,
    name: &'static str,
    problems: &mut Vec<(usize, Problem)>,
) {
    const TAKES: usize = 2;

    let Some(Value::Map(entries)) = decl.bound_args() else {
        return;
    };

    if entries.iter().any(|(key, _)| key.is_empty()) {
        problems.push((
            idx,
            Problem::TooManyArguments {
                declaration: name,
                takes: TAKES,
            },
        ));
    }

    if let Some((_, version)) = entries.iter().find(|(key, _)| key == "version")
        && !matches!(version, Value::Int(n) if *n >= 1)
    {
        problems.push((
            idx,
            Problem::InvalidVersion {
                declaration: name,
                found: version_as_written(version),
            },
        ));
    }
}

/// The version a declaration wrote, where its kind keeps it: `::version(1.10)` takes it as an
/// argument (or in its data), `::vocabulary` and `::blueprint` as `version:` in their data.
fn written_version(decl: &Declaration, kind: DeclKind) -> Option<Value> {
    let in_data = || match &decl.data {
        Some(Value::Map(entries)) => entries
            .iter()
            .find(|(key, _)| key == "version")
            .map(|(_, v)| v.clone()),
        _ => None,
    };
    match kind {
        DeclKind::Version => decl
            .bound_args()
            .and_then(|args| match args {
                Value::Map(entries) => entries
                    .into_iter()
                    .find(|(key, _)| key == "version")
                    .map(|(_, v)| v),
                other => Some(other),
            })
            .or_else(in_data),
        DeclKind::Vocabulary | DeclKind::Blueprint => in_data(),
        _ => None,
    }
}

fn check_declared_version(
    idx: usize,
    decl: &Declaration,
    kind: DeclKind,
    name: &'static str,
    problems: &mut Vec<(usize, Problem)>,
) {
    let Some(written) = written_version(decl, kind) else {
        return;
    };
    if matches!(&written, Value::Element(el) if matches!(el.sigil, tomet_ast::Sigil::Dollar)) {
        return;
    }
    if let Err(mismatch) = Type::Version.admits(&written) {
        problems.push((
            idx,
            Problem::InvalidDefinition {
                declaration: name,
                name: "version".to_string(),
                reason: mismatch.to_string(),
            },
        ));
    }
}

fn check_macros(
    idx: usize,
    decl: &Declaration,
    name: &'static str,
    problems: &mut Vec<(usize, Problem)>,
) {
    let entries = [decl.bound_args(), decl.data.clone()];
    for entries in entries.into_iter().flatten() {
        let Value::Map(entries) = entries else {
            continue;
        };
        for (macro_name, template) in entries {
            if !matches!(template, Value::String(_)) {
                problems.push((
                    idx,
                    Problem::InvalidDefinition {
                        declaration: name,
                        name: macro_name,
                        reason: "a macro is a string template, such as `\"https://x/${1}\"`"
                            .to_string(),
                    },
                ));
            }
        }
    }
}

fn check_vars(
    idx: usize,
    decl: &Declaration,
    name: &'static str,
    problems: &mut Vec<(usize, Problem)>,
) {
    for (var_name, var_decl) in read_var_decls(decl) {
        let reason = match var_decl {
            Err(reason) => Some(reason),
            Ok(v) => var_problem(&v),
        };
        if let Some(reason) = reason {
            problems.push((
                idx,
                Problem::InvalidDefinition {
                    declaration: name,
                    name: var_name,
                    reason,
                },
            ));
        }
    }
}

fn var_problem(decl: &VarDecl) -> Option<String> {
    let default = decl.default.as_ref()?;
    decl.ty
        .admits(default)
        .err()
        .map(|err| format!("`default` is not a value of `type: {}`: {err}", decl.ty))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declaration::declarations;
    use tomet_ast::Document;

    fn parse(src: &str) -> Document {
        tomet_parser::parse_document(src).expect("parses")
    }

    #[test]
    fn version_slot_valid_and_invalid() {
        let good_kind = Declaration::new(
            "kind",
            Some(Value::Map(vec![
                ("kind".into(), Value::String("note".into())),
                ("version".into(), Value::Int(1)),
            ])),
            None,
        );
        let problems = check_declarations(&[good_kind], Host::Tmt);
        assert!(problems.is_empty());

        let bad_version = Declaration::new(
            "kind",
            Some(Value::Map(vec![
                ("kind".into(), Value::String("note".into())),
                ("version".into(), Value::Float(1.0)),
            ])),
            None,
        );
        let problems = check_declarations(&[bad_version], Host::Tmt);
        assert_eq!(problems.len(), 1);
        assert_eq!(
            problems[0].1,
            Problem::InvalidVersion {
                declaration: "kind",
                found: "1.0".into(),
            }
        );
        assert_eq!(
            problems[0].1.to_string(),
            "the version in `::kind(...)` must be an integer of 1 or more, found 1.0"
        );

        let too_many_args = Declaration::new(
            "use",
            Some(Value::Map(vec![
                ("target".into(), Value::String("deck".into())),
                ("version".into(), Value::Int(1)),
                ("".into(), Value::Int(2)),
            ])),
            None,
        );
        let problems = check_declarations(&[too_many_args], Host::Tmt);
        assert_eq!(problems.len(), 1);
        assert_eq!(
            problems[0].1,
            Problem::TooManyArguments {
                declaration: "use",
                takes: 2,
            }
        );
        assert_eq!(
            problems[0].1.to_string(),
            "`use` takes at most 2 arguments: the name, then an optional version"
        );
    }

    #[test]
    fn declared_versions_held_to_version_type() {
        let good_ver = Declaration::new(
            "version",
            Some(Value::Map(vec![("version".into(), Value::Float(1.10))])),
            None,
        );
        assert!(check_declarations(&[good_ver], Host::Tmt).is_empty());

        let bad_ver = Declaration::new(
            "version",
            Some(Value::Map(vec![(
                "version".into(),
                Value::String("1.0-beta".into()),
            )])),
            None,
        );
        let problems = check_declarations(&[bad_ver], Host::Tmt);
        assert_eq!(problems.len(), 1);
        assert!(matches!(
            &problems[0].1,
            Problem::InvalidDefinition { declaration: "version", name, .. } if name == "version"
        ));
    }

    #[test]
    fn declared_versions_in_vocabulary_and_blueprint() {
        for name in ["vocabulary", "blueprint"] {
            let bad = Declaration::new(
                name,
                None,
                Some(Value::Map(vec![(
                    "version".into(),
                    Value::String("abc".into()),
                )])),
            );
            let problems = check_declarations(&[bad], Host::Tmt);
            assert_eq!(problems.len(), 1);
            assert_eq!(
                problems[0].1,
                Problem::InvalidDefinition {
                    declaration: name,
                    name: "version".into(),
                    reason: "`abc` is not a version such as `1.10.0`".into(),
                }
            );

            let good = Declaration::new(
                name,
                None,
                Some(Value::Map(vec![("version".into(), Value::Float(1.10))])),
            );
            assert!(check_declarations(&[good], Host::Tmt).is_empty());
        }
    }

    #[test]
    fn unevaluated_expressions_in_version_are_skipped() {
        for src in [
            "::version(${expr.ver})\n",
            "::vocabulary(deck){ version: ${expr.ver} }\n",
            "::blueprint(note){ version: ${expr.ver} }\n",
        ] {
            let doc = parse(src);
            let decls: Vec<_> = declarations(&doc)
                .into_iter()
                .map(|f| f.declaration)
                .collect();
            assert!(check_declarations(&decls, Host::Tmt).is_empty(), "{src}");
        }
    }

    #[test]
    fn macros_and_vars_checks() {
        let bad_macro = Declaration::new(
            "macros",
            None,
            Some(Value::Map(vec![("gh".into(), Value::Int(42))])),
        );
        let problems = check_declarations(&[bad_macro], Host::Tmt);
        assert_eq!(problems.len(), 1);
        assert_eq!(
            problems[0].1,
            Problem::InvalidDefinition {
                declaration: "macros",
                name: "gh".into(),
                reason: "a macro is a string template, such as `\"https://x/${1}\"`".into(),
            }
        );

        let bad_var = Declaration::new(
            "vars",
            None,
            Some(Value::Map(vec![(
                "n".into(),
                Value::Map(vec![
                    ("type".into(), Value::String("int".into())),
                    ("default".into(), Value::String("3".into())),
                ]),
            )])),
        );
        let problems = check_declarations(&[bad_var], Host::Tmt);
        assert_eq!(problems.len(), 1);
        assert_eq!(
            problems[0].1,
            Problem::InvalidDefinition {
                declaration: "vars",
                name: "n".into(),
                reason: "`default` is not a value of `type: int`: `3` is not an integer".into(),
            }
        );
    }

    #[test]
    fn parsed_document_checked_through_declarations() {
        let doc = parse("::kind(note, 1.0)\n::use(deck, 1, 2)\n");
        let found = declarations(&doc);
        let decls: Vec<_> = found.into_iter().map(|f| f.declaration).collect();
        let problems = check_declarations(&decls, Host::Tmt);
        assert_eq!(problems.len(), 2);
        assert_eq!(
            problems[0],
            (
                0,
                Problem::InvalidVersion {
                    declaration: "kind",
                    found: "1.0".into(),
                }
            )
        );
        assert_eq!(
            problems[1],
            (
                1,
                Problem::TooManyArguments {
                    declaration: "use",
                    takes: 2,
                }
            )
        );
    }

    #[test]
    fn scope_checks_for_host() {
        let use_decl = Declaration::new(
            "use",
            Some(Value::Map(vec![(
                "target".into(),
                Value::String("deck".into()),
            )])),
            None,
        );
        // `use` is Scope::Tmt
        assert!(check_declarations(std::slice::from_ref(&use_decl), Host::Tmt).is_empty());
        let problems = check_declarations(&[use_decl], Host::Ctmt);
        assert_eq!(problems.len(), 1);
        assert_eq!(
            problems[0].1,
            Problem::NotForThisHost { declaration: "use" }
        );
    }
}
