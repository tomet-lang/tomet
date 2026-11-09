//! Validation of a document's declarations apart from any host syntax.
//!
//! Checks the positional argument count, version slots, declared versions,
//! macros, and variable definitions, and that a declaration is written in the [`Shape`] it has:
//! `(...)` alone, or `(...)` with a `{...}` body only for the one that reads one (`::blueprint`). A plural spelling the
//! singular declarations replaced (`::fields`) is reported with the name to write instead.

use tomet_ast::Value;
use tomet_semantics_core::admits::admits;
use tomet_type::Type;

use crate::declaration::{Declaration, EntriesError};
use crate::kind::{Cardinality, DeclKind, Host, Shape};
use crate::profile::Profile;
use crate::read::profile_names;
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
    /// A declaration that may appear at most once was written more than once.
    ///
    /// The shared check is the one place standard declarations are counted for both
    /// `.tmt` and `.ctmt` hosts; `check_singletons_and_regions` in the validator counts
    /// only user-vocabulary declarations. `first` is the index of the first occurrence.
    Repeated {
        declaration: &'static str,
        first: usize,
    },
    /// `id` written in `::meta(...)`. `::meta` holds free attributes; what the language itself
    /// reads about a document, its identity, is the declaration `::id(...)`.
    MetaIdMoved,
    /// A `{...}` written on a declaration that has no body: `::meta(){ a: 1 }`. Its entries go in
    /// the `(...)`: `::meta(a: 1)`. Only the header of `::blueprint` takes one. A `::dictionary` does not:
    /// its terms are `;@term("word")[meaning]` blocks of the document body.
    UnexpectedData { declaration: &'static str },
    /// The `(...)` of a declaration of entries is something else: `::macro(gh)`, or a bare value
    /// among the entries, as in `::meta(a: 1, 2)`. A value goes with a name in the head:
    /// `::macro.gh("...")`.
    ExpectedEntries { declaration: &'static str },
    /// A path written after the name of a declaration that has none: `::kind.version(2)`. Its
    /// arguments are in the parentheses, `::kind(config, 2)`.
    UnexpectedPath { declaration: &'static str },
    /// A `[...]` that has nothing to be the prose of, or that clashes with the value.
    UnexpectedProse {
        declaration: &'static str,
        reason: &'static str,
    },
    /// A plural spelling that the singular declaration replaced: `::fields` is `::field`.
    Retired {
        written: String,
        instead: &'static str,
    },
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
            Problem::Repeated { declaration, .. } => {
                write!(f, "`{declaration}` may appear once in a document")
            }
            Problem::MetaIdMoved => {
                write!(
                    f,
                    "`id` is not written in `::meta`; the identity of a document is `::id(...)`"
                )
            }
            Problem::UnexpectedData {
                declaration: "dictionary",
            } => write!(
                f,
                "`::dictionary` takes no `{{...}}`; its terms are blocks of the document body, \
                 `;@term(\"word\")[meaning]#(id)`"
            ),
            Problem::UnexpectedData { declaration } => {
                write!(
                    f,
                    "`::{declaration}` takes no `{{...}}`; write its entries in the parentheses, \
                     `::{declaration}(name: value)`"
                )
            }
            Problem::ExpectedEntries { declaration } => {
                write!(
                    f,
                    "`::{declaration}(...)` holds `name: value` entries; found a value without a name. \
                     Name it in the head, `::{declaration}.name(value)`"
                )
            }
            Problem::UnexpectedPath { declaration } => {
                write!(
                    f,
                    "`::{declaration}` takes its arguments in the parentheses and has no path after its name"
                )
            }
            Problem::UnexpectedProse {
                declaration,
                reason,
            } => write!(f, "`::{declaration}`: {reason}"),
            Problem::Retired { written, instead } => {
                write!(
                    f,
                    "`::{written}` is retired; write `::{instead}.<name>(...)` for each entry, or \
                     `::{instead}(<name>: ...)` to group them"
                )
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
    let mut seen_once = std::collections::HashMap::new();

    for (idx, decl) in declarations.iter().enumerate() {
        let Some(kind) = decl.kind() else {
            if let Some(instead) = DeclKind::retired(&decl.name) {
                problems.push((
                    idx,
                    Problem::Retired {
                        written: decl.name.clone(),
                        instead: instead.name(),
                    },
                ));
            }
            continue;
        };
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

        // What is written once: the declarations a document says about itself, and the header of
        // one that has a path-less form of its own (`::blueprint(daily)`, as against the
        // registrations `::blueprint.daily(@file(...))`, of which there are many).
        if spec.cardinality == Cardinality::Once || decl.is_own_header() {
            if let Some(&first_idx) = seen_once.get(&kind) {
                problems.push((
                    idx,
                    Problem::Repeated {
                        declaration: spec.name,
                        first: first_idx,
                    },
                ));
            } else {
                seen_once.insert(kind, idx);
            }
        }

        let has_path = !decl.path.is_empty();
        if spec.shape == Shape::Args && has_path && !spec.own_header {
            problems.push((
                idx,
                Problem::UnexpectedPath {
                    declaration: spec.name,
                },
            ));
        }
        if (spec.shape != Shape::ArgsAndData || has_path) && decl.data.is_some() {
            problems.push((
                idx,
                Problem::UnexpectedData {
                    declaration: spec.name,
                },
            ));
        }
        if spec.shape == Shape::Entries || has_path {
            if let Err(error) = decl.entries() {
                problems.push((idx, entries_problem(spec.name, error)));
            }
        } else if decl.prose.is_some() {
            problems.push((
                idx,
                entries_problem(spec.name, EntriesError::UnexpectedProse),
            ));
        }

        match kind {
            DeclKind::Kind | DeclKind::Use => {
                check_version_slot(idx, decl, spec.name, &mut problems);
            }
            DeclKind::Version | DeclKind::Vocabulary | DeclKind::Blueprint => {
                check_declared_version(idx, decl, kind, spec.name, &mut problems);
            }
            DeclKind::Profile => {
                check_profile(idx, decl, spec.name, &mut problems);
            }
            DeclKind::Macro => {
                check_macros(idx, decl, spec.name, &mut problems);
            }
            DeclKind::Var => {
                check_vars(idx, decl, spec.name, &mut problems);
            }
            DeclKind::Field => {
                check_fields(idx, decl, spec.name, &mut problems);
            }
            DeclKind::Meta => {
                if decl
                    .entries()
                    .is_ok_and(|entries| entries.iter().any(|(key, _)| key == "id"))
                {
                    problems.push((idx, Problem::MetaIdMoved));
                }
            }
            _ => {}
        }
    }

    problems
}

/// What is wrong with a declaration that is not written as entries.
fn entries_problem(declaration: &'static str, error: EntriesError) -> Problem {
    match error {
        EntriesError::ValueWithoutName => Problem::ExpectedEntries { declaration },
        EntriesError::UnexpectedProse => Problem::UnexpectedProse {
            declaration,
            reason: "there is no `[...]` here: it goes after a head that names what it describes \
                     (`::field.author(...)[ about ]`), and only a field, a variable, an element or \
                     a macro takes one",
        },
        EntriesError::ProseAndValue => Problem::UnexpectedProse {
            declaration,
            reason: "a macro is either a string argument or a `[...]`, not both",
        },
    }
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
/// argument, `::vocabulary.ns(version: "1.0.0")` as a field of its entry, and `::blueprint` as
/// `version:` in its `{...}` body.
fn written_version(decl: &Declaration, kind: DeclKind) -> Option<Value> {
    match kind {
        DeclKind::Version => decl.bound_args().and_then(|args| match args {
            Value::Map(entries) => entries
                .into_iter()
                .find(|(key, _)| key == "version")
                .map(|(_, v)| v),
            other => Some(other),
        }),
        DeclKind::Vocabulary => decl.entries().ok()?.into_iter().find_map(|(_, entry)| {
            let Value::Map(fields) = entry else {
                return None;
            };
            fields
                .into_iter()
                .find(|(key, _)| key == "version")
                .map(|(_, v)| v)
        }),
        DeclKind::Blueprint => match &decl.data {
            Some(Value::Map(entries)) => entries
                .iter()
                .find(|(key, _)| key == "version")
                .map(|(_, v)| v.clone()),
            _ => None,
        },
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
    if matches!(&written, Value::Interp(_)) {
        return;
    }
    if let Err(mismatch) = admits(&Type::Version, &written) {
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

/// A `::profile` names at least one profile, and every name is one. A promise nobody can check is
/// reported, not ignored.
fn check_profile(
    idx: usize,
    decl: &Declaration,
    name: &'static str,
    problems: &mut Vec<(usize, Problem)>,
) {
    let names = profile_names(decl);
    if names.is_empty() {
        problems.push((
            idx,
            Problem::InvalidDefinition {
                declaration: name,
                name: name.to_string(),
                reason: "names at least one profile, such as `standalone`".to_string(),
            },
        ));
    }
    for written in names {
        if Profile::from_name(&written).is_none() {
            let known: Vec<String> = Profile::ALL.iter().map(|p| format!("`{p}`")).collect();
            problems.push((
                idx,
                Problem::InvalidDefinition {
                    declaration: name,
                    name: written,
                    reason: format!("not a profile; the profiles are {}", known.join(", ")),
                },
            ));
        }
    }
}

fn check_macros(
    idx: usize,
    decl: &Declaration,
    name: &'static str,
    problems: &mut Vec<(usize, Problem)>,
) {
    for (macro_name, template) in crate::read::entries(decl) {
        if let Some(reason) = macro_problem(&template) {
            problems.push((
                idx,
                Problem::InvalidDefinition {
                    declaration: name,
                    name: macro_name,
                    reason,
                },
            ));
        }
    }
}

/// Why `template` cannot be a macro's, or `None` if it can. A string is a text macro. A `[ ... ]`
/// is a block macro, whose only interpolations are the places arguments go.
fn macro_problem(template: &Value) -> Option<String> {
    match template {
        Value::String(_) => None,
        Value::Blocks(_) => {
            let expressions = crate::expressions_in_body(template);
            if expressions.is_empty() {
                None
            } else {
                let written: Vec<String> =
                    expressions.iter().map(|e| format!("`${{{e}}}`")).collect();
                Some(format!(
                    "in a block macro `${{...}}` is where an argument goes (`${{1}}`, `${{name}}`), not an \
                     expression; found {}",
                    written.join(", ")
                ))
            }
        }
        _ => Some(
            "a macro is a string template, such as `\"https://x/${1}\"`, or block content, such as \
             `[ @card[ ${1} ] ]`"
                .to_string(),
        ),
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
    admits(&decl.ty, default)
        .err()
        .map(|err| format!("`default` is not a value of `type: {}`: {err}", decl.ty))
}

fn check_fields(
    idx: usize,
    decl: &Declaration,
    name: &'static str,
    problems: &mut Vec<(usize, Problem)>,
) {
    for (field_name, field_def) in &crate::read::entries(decl) {
        let Value::Map(props) = field_def else {
            continue;
        };
        for (k, v) in props {
            if k == "type" {
                if let Err(e) = tomet_semantics_core::parse_type_spec(v) {
                    problems.push((
                        idx,
                        Problem::InvalidDefinition {
                            declaration: name,
                            name: field_name.clone(),
                            reason: e.to_string(),
                        },
                    ));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declaration::declarations;
    use tomet_ast::File;

    fn parse(src: &str) -> File {
        tomet_ast_parser::parse_file(src).expect("parses")
    }

    fn problems_of(src: &str) -> Vec<Problem> {
        let doc = parse(src);
        let decls: Vec<_> = declarations(&doc)
            .into_iter()
            .map(|found| found.declaration)
            .collect();
        check_declarations(&decls, Host::Tmt)
            .into_iter()
            .map(|(_, problem)| problem)
            .collect()
    }

    #[test]
    fn a_profile_must_name_profiles_that_exist() {
        assert!(problems_of("::profile(oneline, standalone)\n").is_empty());
        assert!(problems_of("::profile(standalone)\n").is_empty());

        let found = problems_of("::profile(oneline, pure)\n");
        assert_eq!(
            found,
            [Problem::InvalidDefinition {
                declaration: "profile",
                name: "pure".into(),
                reason: "not a profile; the profiles are `oneline`, `standalone`".into(),
            }]
        );
        assert_eq!(
            found[0].to_string(),
            "`pure` in `::profile`: not a profile; the profiles are `oneline`, `standalone`"
        );

        // Naming none is not a promise either.
        assert_eq!(problems_of("::profile()\n").len(), 1);
    }

    #[test]
    fn a_second_profile_declaration_is_reported_and_a_ctmt_file_is_not_given_one() {
        let found = problems_of("::profile(oneline)\n::profile(standalone)\n");
        assert_eq!(
            found,
            [Problem::Repeated {
                declaration: "profile",
                first: 0
            }]
        );

        let doc = parse("::profile(standalone)\n");
        let decls: Vec<_> = declarations(&doc)
            .into_iter()
            .map(|found| found.declaration)
            .collect();
        assert_eq!(
            check_declarations(&decls, Host::Ctmt)
                .into_iter()
                .map(|(_, problem)| problem)
                .collect::<Vec<_>>(),
            [Problem::NotForThisHost {
                declaration: "profile"
            }]
        );
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

        let good_prerelease = Declaration::new(
            "version",
            Some(Value::Map(vec![(
                "version".into(),
                Value::String("1.0-beta".into()),
            )])),
            None,
        );
        assert!(check_declarations(&[good_prerelease], Host::Tmt).is_empty());

        let bad_ver = Declaration::new(
            "version",
            Some(Value::Map(vec![(
                "version".into(),
                Value::String("1.0.0_beta".into()),
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
        let bad = |name: &str, version: &str| {
            let src = match name {
                "vocabulary" => format!("::vocabulary.deck(version: {version})\n"),
                _ => format!("::blueprint(note){{ version: {version} }}\n"),
            };
            problems_of(&src)
        };
        for name in ["vocabulary", "blueprint"] {
            assert_eq!(
                bad(name, "abc"),
                [Problem::InvalidDefinition {
                    declaration: if name == "vocabulary" {
                        "vocabulary"
                    } else {
                        "blueprint"
                    },
                    name: "version".into(),
                    reason: "`abc` is not a version such as `1.10.0`".into(),
                }]
            );
            assert!(bad(name, "1.10").is_empty(), "{name}");
        }
    }

    #[test]
    fn unevaluated_expressions_in_version_are_skipped() {
        for src in [
            "::version(${expr.ver})\n",
            "::vocabulary.deck(version: ${expr.ver})\n",
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
    fn a_block_macro_may_hold_only_places_for_arguments() {
        let doc = parse(
            "::macro(ok: [ @card(title: ${1})[ ${who} ] ], bad: [ ${a.b} and ${f(1)} ${1} ])\n",
        );
        let decls: Vec<_> = declarations(&doc)
            .into_iter()
            .map(|f| f.declaration)
            .collect();
        let problems = check_declarations(&decls, Host::Tmt);
        assert_eq!(problems.len(), 1, "{problems:?}");
        let Problem::InvalidDefinition { name, reason, .. } = &problems[0].1 else {
            panic!("expected an invalid definition, got {:?}", problems[0].1);
        };
        assert_eq!(name, "bad");
        // What follows "found" is the expressions, and `${1}` is a place, not one of them.
        assert_eq!(reason.split("found ").nth(1), Some("`${a.b}`, `${f(1)}`"));
    }

    #[test]
    fn macros_and_vars_checks() {
        let bad_macro = Declaration::new(
            "macro",
            Some(Value::Map(vec![("gh".into(), Value::Int(42))])),
            None,
        );
        let problems = check_declarations(&[bad_macro], Host::Tmt);
        assert_eq!(problems.len(), 1);
        assert_eq!(
            problems[0].1,
            Problem::InvalidDefinition {
                declaration: "macro",
                name: "gh".into(),
                reason: "a macro is a string template, such as `\"https://x/${1}\"`, or block \
                         content, such as `[ @card[ ${1} ] ]`"
                    .into(),
            }
        );

        let bad_var = Declaration::new(
            "var",
            Some(Value::Map(vec![(
                "n".into(),
                Value::Map(vec![
                    ("type".into(), Value::String("int".into())),
                    ("default".into(), Value::String("3".into())),
                ]),
            )])),
            None,
        );
        let problems = check_declarations(&[bad_var], Host::Tmt);
        assert_eq!(problems.len(), 1);
        assert_eq!(
            problems[0].1,
            Problem::InvalidDefinition {
                declaration: "var",
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

    #[test]
    fn repeated_singletons_are_reported() {
        let meta1 = Declaration::new("id", Some(Value::String("a".into())), None);
        let meta2 = Declaration::new("id", Some(Value::String("b".into())), None);
        let problems = check_declarations(&[meta1, meta2], Host::Ctmt);
        assert_eq!(problems.len(), 1);
        assert_eq!(
            problems[0],
            (
                1,
                Problem::Repeated {
                    declaration: "id",
                    first: 0,
                }
            )
        );
        assert_eq!(
            problems[0].1.to_string(),
            "`id` may appear once in a document"
        );

        // Many cardinality is allowed to repeat
        let use1 = Declaration::new(
            "use",
            Some(Value::Map(vec![(
                "target".into(),
                Value::String("a".into()),
            )])),
            None,
        );
        let use2 = Declaration::new(
            "use",
            Some(Value::Map(vec![(
                "target".into(),
                Value::String("b".into()),
            )])),
            None,
        );
        let problems = check_declarations(&[use1, use2], Host::Tmt);
        assert!(problems.is_empty());
    }

    #[test]
    fn fields_type_declaration_validation() {
        let valid = problems_of("::field(tags: { type: list(string) })\n");
        assert!(valid.is_empty(), "{valid:?}");

        let invalid = problems_of("::field(owner: { type: person })\n");
        assert_eq!(invalid.len(), 1);
        assert!(matches!(
            &invalid[0],
            Problem::InvalidDefinition {
                declaration: "field",
                name,
                reason,
            } if name == "owner" && reason.contains("`person` is not a type")
        ));
    }

    #[test]
    fn a_meta_id_is_reported_wherever_it_is_written_in_the_parentheses() {
        assert_eq!(problems_of("::meta(id: x)\n"), [Problem::MetaIdMoved]);
        assert_eq!(
            problems_of("::meta(title: T, id: x)\n"),
            [Problem::MetaIdMoved]
        );
        assert!(problems_of("::meta(title: T)\n").is_empty());
        assert!(problems_of("::meta()\n").is_empty());
    }

    #[test]
    fn a_brace_body_is_refused_on_every_declaration_that_takes_none() {
        let src = |name: &str| format!("::{name}(){{ a: 1 }}\n");
        for kind in DeclKind::ALL {
            let found = problems_of(&src(kind.name()));
            let unexpected = Problem::UnexpectedData {
                declaration: kind.name(),
            };
            assert_eq!(
                found.contains(&unexpected),
                kind.shape() != Shape::ArgsAndData,
                "{kind}: {found:?}"
            );
        }
        assert_eq!(
            Problem::UnexpectedData {
                declaration: "meta"
            }
            .to_string(),
            "`::meta` takes no `{...}`; write its entries in the parentheses, `::meta(name: value)`"
        );
    }

    #[test]
    fn the_blueprint_header_still_takes_its_body() {
        assert!(problems_of("::blueprint(note){ version: 1.0 }\n").is_empty());
    }

    #[test]
    fn a_dictionary_header_takes_no_body_and_points_at_the_document_body() {
        assert!(problems_of("::dictionary(tomet)\n").is_empty());
        assert!(problems_of("::dictionary.tomet(@file(tomet.dict.tmt))\n").is_empty());
        let found = problems_of("::dictionary(tomet){ \"a\": @term[x] }\n");
        assert_eq!(
            found,
            [Problem::UnexpectedData {
                declaration: "dictionary"
            }]
        );
        assert_eq!(
            found[0].to_string(),
            "`::dictionary` takes no `{...}`; its terms are blocks of the document body, \
             `;@term(\"word\")[meaning]#(id)`"
        );
    }

    #[test]
    fn the_parentheses_of_an_entries_declaration_hold_entries() {
        assert!(problems_of("::macro()\n").is_empty());
        assert!(problems_of("::meta(a: 1, b: 2)\n").is_empty());
        for src in ["::macro(gh)\n", "::meta(a: 1, 2)\n", "::workspace(x)\n"] {
            let found = problems_of(src);
            assert_eq!(found.len(), 1, "{src}: {found:?}");
            assert!(
                matches!(found[0], Problem::ExpectedEntries { .. }),
                "{src}: {found:?}"
            );
        }
        assert_eq!(
            problems_of("::meta(1)\n")[0].to_string(),
            "`::meta(...)` holds `name: value` entries; found a value without a name. \
             Name it in the head, `::meta.name(value)`"
        );
    }

    #[test]
    fn a_dotted_head_and_the_grouped_form_are_checked_alike() {
        for src in [
            "::macro.gh(\"https://x/${1}\")\n",
            "::macro(gh: \"https://x/${1}\")\n",
            "::macro.card[ @card[ ${1} ] ]\n",
            "::field.url.wiki(type: list(string))[ the wiki ]\n",
            "::var.n(type: int, default: 3)[ how many ]\n",
            "::workspace.ignore(\"target\", \".git\")\n",
            "::format.table(max_col_width: 50)\n",
            "::blueprint.daily(@file(daily.blueprint.tmt))\n",
            "::vocabulary.deck(@file(deck.vocabulary.tmt))\n",
            "::dictionary.tomet(@file(tomet.dict.tmt))\n",
            "::element.card(display: block)[ a card ]\n",
        ] {
            assert!(problems_of(src).is_empty(), "{src}: {:?}", problems_of(src));
        }
        // The same invalid definition is found either way.
        for src in [
            "::field.owner(type: person)\n",
            "::field(owner: { type: person })\n",
        ] {
            let found = problems_of(src);
            assert!(
                matches!(&found[..], [Problem::InvalidDefinition { name, .. }] if name == "owner"),
                "{src}: {found:?}"
            );
        }
    }

    #[test]
    fn the_registrations_are_many_and_the_header_of_a_blueprint_is_one() {
        assert!(
            problems_of("::blueprint.a(@file(a.tmt))\n::blueprint.b(@file(b.tmt))\n").is_empty()
        );
        assert!(problems_of("::format.meta(always_newline: true)\n::format.table()\n").is_empty());
        assert_eq!(
            problems_of("::blueprint(a)\n::blueprint(b)\n"),
            [Problem::Repeated {
                declaration: "blueprint",
                first: 0
            }]
        );
        // A registration beside the header is not a second header.
        assert!(problems_of("::blueprint(a)\n::blueprint.b(@file(b.tmt))\n").is_empty());
    }

    #[test]
    fn a_path_belongs_to_the_declarations_of_entries() {
        assert_eq!(
            problems_of("::kind.version(2)\n"),
            [Problem::UnexpectedPath {
                declaration: "kind"
            }]
        );
        assert_eq!(
            problems_of("::use.deck(3)\n"),
            [Problem::UnexpectedPath { declaration: "use" }]
        );
    }

    #[test]
    fn a_prose_goes_where_it_describes_something() {
        // A field, a variable, an element and a macro take one after a head with a path.
        assert!(problems_of("::element.card[ a card ]\n").is_empty());
        // Nowhere to attach it.
        for src in [
            "::field(author: { type: string })[ x ]\n",
            "::kind(note)[ x ]\n",
            "::format.table()[ x ]\n",
            "::macro.m(\"x\")[ y ]\n",
        ] {
            let found = problems_of(src);
            assert_eq!(found.len(), 1, "{src}: {found:?}");
            assert!(
                matches!(found[0], Problem::UnexpectedProse { .. }),
                "{src}: {found:?}"
            );
        }
    }

    #[test]
    fn a_plural_declaration_says_what_to_write_instead() {
        let found = problems_of("::fields(a: { type: string })\n");
        assert!(
            found.is_empty(),
            "a retired name is not a standard one: {found:?}"
        );

        let doc = parse("::fields(a: { type: string })\n::macros(m: \"x\")\n");
        let decls: Vec<_> = doc
            .header
            .iter()
            .filter_map(Declaration::from_ast)
            .collect();
        let found = check_declarations(&decls, Host::Tmt);
        assert_eq!(
            found,
            [
                (
                    0,
                    Problem::Retired {
                        written: "fields".into(),
                        instead: "field"
                    }
                ),
                (
                    1,
                    Problem::Retired {
                        written: "macros".into(),
                        instead: "macro"
                    }
                ),
            ]
        );
        assert_eq!(
            found[0].1.to_string(),
            "`::fields` is retired; write `::field.<name>(...)` for each entry, or \
             `::field(<name>: ...)` to group them"
        );
    }
}
