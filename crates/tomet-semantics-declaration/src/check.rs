//! Validation of a document's declarations apart from any host syntax.
//!
//! Checks the positional argument count, version slots, declared versions,
//! macros, and variable definitions, and that a declaration is written in the [`Shape`] it has:
//! `(...)` alone, or `(...)` with a `{...}` body only for the three that read one.

use tomet_ast::Value;
use tomet_semantics_core::admits::admits;
use tomet_type::Type;

use crate::declaration::Declaration;
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
    /// the `(...)`: `::meta(a: 1)`. Only `::vocabulary`, `::blueprint` and `::dictionary` take one.
    UnexpectedData { declaration: &'static str },
    /// The `(...)` of a declaration of `name: value` entries is something else: `::macros(gh)`, or
    /// a bare value among the entries, as in `::meta(a: 1, 2)`.
    ExpectedEntries { declaration: &'static str },
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
                    "`::{declaration}(...)` holds `name: value` entries; found a value without a name"
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

        if spec.cardinality == Cardinality::Once {
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

        if spec.shape != Shape::ArgsAndData && decl.data.is_some() {
            problems.push((
                idx,
                Problem::UnexpectedData {
                    declaration: spec.name,
                },
            ));
        }
        if spec.shape == Shape::Entries && !is_entries(decl) {
            problems.push((
                idx,
                Problem::ExpectedEntries {
                    declaration: spec.name,
                },
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
            DeclKind::Macros => {
                check_macros(idx, decl, spec.name, &mut problems);
            }
            DeclKind::Vars => {
                check_vars(idx, decl, spec.name, &mut problems);
            }
            DeclKind::Fields => {
                check_fields(idx, decl, spec.name, &mut problems);
            }
            DeclKind::Meta => {
                if let Some(Value::Map(entries)) = decl.bound_args()
                    && entries.iter().any(|(key, _)| key == "id")
                {
                    problems.push((idx, Problem::MetaIdMoved));
                }
            }
            _ => {}
        }
    }

    problems
}

/// Whether the `(...)` of `decl` is `name: value` entries, none of them a bare value. `::name()` is
/// entries with none, and so is a declaration built without any `(...)`, which no parse produces.
fn is_entries(decl: &Declaration) -> bool {
    match &decl.args {
        None => true,
        Some(Value::Map(entries)) => entries
            .iter()
            .all(|(key, _)| key != tomet_semantics_core::POSITIONAL_ENTRY_KEY),
        Some(_) => false,
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
/// argument, `::vocabulary` and `::blueprint` as `version:` in their `{...}` body.
fn written_version(decl: &Declaration, kind: DeclKind) -> Option<Value> {
    match kind {
        DeclKind::Version => decl.bound_args().and_then(|args| match args {
            Value::Map(entries) => entries
                .into_iter()
                .find(|(key, _)| key == "version")
                .map(|(_, v)| v),
            other => Some(other),
        }),
        DeclKind::Vocabulary | DeclKind::Blueprint => match &decl.data {
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
    fn a_block_macro_may_hold_only_places_for_arguments() {
        let doc = parse(
            "::macros(ok: [ @card(title: ${1})[ ${who} ] ], bad: [ ${a.b} and ${f(1)} ${1} ])\n",
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
            "macros",
            Some(Value::Map(vec![("gh".into(), Value::Int(42))])),
            None,
        );
        let problems = check_declarations(&[bad_macro], Host::Tmt);
        assert_eq!(problems.len(), 1);
        assert_eq!(
            problems[0].1,
            Problem::InvalidDefinition {
                declaration: "macros",
                name: "gh".into(),
                reason: "a macro is a string template, such as `\"https://x/${1}\"`, or block \
                         content, such as `[ @card[ ${1} ] ]`"
                    .into(),
            }
        );

        let bad_var = Declaration::new(
            "vars",
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

    #[test]
    fn repeated_singletons_are_reported() {
        let meta1 = Declaration::new(
            "meta",
            Some(Value::Map(vec![("a".into(), Value::Int(1))])),
            None,
        );
        let meta2 = Declaration::new(
            "meta",
            Some(Value::Map(vec![("b".into(), Value::Int(2))])),
            None,
        );
        let problems = check_declarations(&[meta1, meta2], Host::Ctmt);
        assert_eq!(problems.len(), 1);
        assert_eq!(
            problems[0],
            (
                1,
                Problem::Repeated {
                    declaration: "meta",
                    first: 0,
                }
            )
        );
        assert_eq!(
            problems[0].1.to_string(),
            "`meta` may appear once in a document"
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
        let valid = problems_of("::fields(tags: { type: list(string) })\n");
        assert!(valid.is_empty(), "{valid:?}");

        let invalid = problems_of("::fields(owner: { type: person })\n");
        assert_eq!(invalid.len(), 1);
        assert!(matches!(
            &invalid[0],
            Problem::InvalidDefinition {
                declaration: "fields",
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
    fn the_declarations_that_take_a_body_still_take_it() {
        for src in [
            "::vocabulary(deck){ version: 1.0 }\n",
            "::blueprint(note){ version: 1.0 }\n",
            "::dictionary(tomet){ \"a\": @term[x] }\n",
        ] {
            assert!(problems_of(src).is_empty(), "{src}");
        }
    }

    #[test]
    fn the_parentheses_of_an_entries_declaration_hold_entries() {
        assert!(problems_of("::macros()\n").is_empty());
        assert!(problems_of("::meta(a: 1, b: 2)\n").is_empty());
        for src in ["::macros(gh)\n", "::meta(a: 1, 2)\n", "::workspace(x)\n"] {
            let found = problems_of(src);
            assert_eq!(found.len(), 1, "{src}: {found:?}");
            assert!(
                matches!(found[0], Problem::ExpectedEntries { .. }),
                "{src}: {found:?}"
            );
        }
        assert_eq!(
            problems_of("::meta(1)\n")[0].to_string(),
            "`::meta(...)` holds `name: value` entries; found a value without a name"
        );
    }
}
