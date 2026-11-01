//! =[ tomet-convert-standalone ]
//!
//! Makes a document standalone: what it takes from its vault is carried into the file, or frozen
//! to the value it has there, so that it can be taken out and still mean the same.
//!
//! =[ Architecture & Responsibilities ]
//!
//! - (The worklist is the validator's)
//!   | [`make_standalone`] starts from [`tomet_validator_profile::standalone_findings`], the same list
//!   | `tomet check` reports for a file that promises `::profile(standalone)`. Each finding is
//!   | carried in, frozen, or left and reported. When it is done the same check is run on the result
//!   | and what it still finds is [`Converted::remaining`], so the report is what the file is, not
//!   | what this crate meant to do.
//! - (Carry what is a definition)
//!   | A macro or a variable the vault defines and the file uses is written into the file's
//!   | `::macros` / `::vars`. The expressions that use it stay expressions.
//! - (Freeze what is a place)
//!   | What depends on where the file sits or on other files -- `self.path`, `vault`, `include`,
//!   | an address into another document -- has no definition to carry. The `${...}` is replaced by
//!   | the value it has in the vault. That loses the expression, so the change is always reported.
//!   | An expression that is frozen as a whole is not also carried: a macro called with `self.path`
//!   | in it needs no definition once the call is a value.
//! - (The minimum, and options)
//!   | By default the file gets what it needs to work and no more. What more it could get is an
//!   | [`Options`] field the person turns on.
//! - (The promise)
//!   | The result declares `::profile(standalone)`, adding `standalone` to a `::profile` that is
//!   | already there.
//!
//! =[ Not here ]
//!
//! - Printing. The result is a [`File`]; the command prints it the way the vault prints.
//! - A vocabulary, `self.meta` typed by `::fields`, and `@term`: each is left and reported. A file
//!   cannot hold a vocabulary or its own `::fields` yet, and a term is not frozen yet.
//!   `Options::freeze_fields` is the way out for `self.meta`.

use std::path::Path;

use tomet_ast::{Data, Entry, File, Group, Name, Value};
use tomet_span::Span;
use tomet_declaration::{Profile, document_profiles};
use tomet_expr::{Reach, Resolver};
use tomet_validator_profile::{Dirt, Finding, standalone_findings};
use tomet_vault::Vault;

/// What more than the minimum a conversion may do. The default does none of it.
#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    /// Freeze `${self.meta.x}` to its value. Without it the expression is left, because the
    /// vault's `::fields` that type it cannot be carried into a file yet.
    pub freeze_fields: bool,
}

/// What was done about one thing the document took from outside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Treatment {
    /// Its definition was written into the file.
    Carried,
    /// The expression was replaced by the value it has in the vault.
    Frozen,
    /// Nothing was done, and `reason` says why.
    Left { reason: String },
}

/// One finding of the original document and what was done about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub dirt: Dirt,
    /// Where it was written in the original.
    pub span: Span,
    pub treatment: Treatment,
}

/// The result of [`make_standalone`].
#[derive(Debug, Clone)]
pub struct Converted {
    pub document: File,
    /// Every finding of the original, with what was done.
    pub changes: Vec<Change>,
    /// What the validator still finds in `document`, positioned in it. Empty when the file is
    /// standalone.
    pub remaining: Vec<Finding>,
}

/// Makes `doc`, which sits at `path` in `vault`, standalone.
///
/// `resolver` answers what the document's expressions ask of other files, as it does when the
/// document is rendered.
pub fn make_standalone(
    vault: &Vault,
    doc: &File,
    path: &Path,
    resolver: &dyn Resolver,
    options: &Options,
) -> Converted {
    let findings = standalone_findings(doc);
    let groups = group_by_place(&findings);
    let mut decisions: Vec<Decision> = groups
        .iter()
        .map(|group| decide(group, vault, options))
        .collect();

    // Freezing is tried on a copy first: an expression that has no value in the vault is
    // left as it is, not turned into text.
    let freeze: Vec<Span> = places(&groups, &decisions, |d| matches!(d, Decision::Freeze));
    if !freeze.is_empty() {
        let mut probe = doc.clone();
        let prepared =
            vault.prepare_selected(&mut probe, path, resolver, &|span| is_among(&freeze, span));
        for failed in prepared.unresolved {
            for (group, decision) in groups.iter().zip(decisions.iter_mut()) {
                if matches!(decision, Decision::Freeze) && group.span.exact_eq(&failed.span) {
                    *decision = Decision::Leave(failed.reason.clone());
                }
            }
        }
    }

    let mut document = doc.clone();
    let freeze: Vec<Span> = places(&groups, &decisions, |d| matches!(d, Decision::Freeze));
    if !freeze.is_empty() {
        vault.prepare_selected(&mut document, path, resolver, &|span| {
            is_among(&freeze, span)
        });
    }

    carry(vault, &groups, &decisions, &mut document);
    declare_standalone(&mut document);

    let mut changes = Vec::new();
    for (group, decision) in groups.iter().zip(&decisions) {
        for finding in &group.findings {
            changes.push(Change {
                dirt: finding.dirt.clone(),
                span: finding.span,
                treatment: match decision {
                    Decision::Carry => Treatment::Carried,
                    Decision::Freeze => Treatment::Frozen,
                    Decision::Leave(reason) => Treatment::Left {
                        reason: reason.clone(),
                    },
                },
            });
        }
    }
    changes.sort_by_key(|change| change.span.start);

    let remaining = standalone_findings(&document);
    Converted {
        document,
        changes,
        remaining,
    }
}

/// The findings that sit in one place: every part of one `${...}` is one group, and anything else
/// is a group of its own.
struct Place<'a> {
    span: Span,
    findings: Vec<&'a Finding>,
}

fn group_by_place(findings: &[Finding]) -> Vec<Place<'_>> {
    let mut groups: Vec<Place<'_>> = Vec::new();
    for finding in findings {
        let same_expression = matches!(finding.dirt, Dirt::Expression(_));
        match groups.iter_mut().find(|group| {
            same_expression
                && group.span.exact_eq(&finding.span)
                && matches!(group.findings[0].dirt, Dirt::Expression(_))
        }) {
            Some(group) => group.findings.push(finding),
            None => groups.push(Place {
                span: finding.span,
                findings: vec![finding],
            }),
        }
    }
    groups
}

enum Decision {
    Carry,
    Freeze,
    Leave(String),
}

/// What one place needs: if any part of it is a place or another file, the whole expression is
/// frozen; if every part is a definition the vault has, they are carried; otherwise it is left.
fn decide(group: &Place<'_>, vault: &Vault, options: &Options) -> Decision {
    let mut leave = None;
    for finding in &group.findings {
        match need(&finding.dirt, vault, options) {
            Need::Freeze => return Decision::Freeze,
            Need::Carry => {}
            Need::Cannot(reason) => {
                leave.get_or_insert(reason);
            }
        }
    }
    match leave {
        Some(reason) => Decision::Leave(reason),
        None => Decision::Carry,
    }
}

enum Need {
    Carry,
    Freeze,
    Cannot(String),
}

fn need(dirt: &Dirt, vault: &Vault, options: &Options) -> Need {
    match dirt {
        Dirt::Expression(Reach::Macro(name)) => {
            if vault.config.macros.contains_key(name) {
                Need::Carry
            } else {
                Need::Cannot(format!(
                    "neither the file nor the vault defines the macro `{name}`"
                ))
            }
        }
        Dirt::Expression(Reach::Var(name)) => {
            if vault
                .config
                .vars
                .iter()
                .any(|(declared, _)| declared == name)
            {
                Need::Carry
            } else {
                Need::Cannot(format!(
                    "neither the file nor the vault declares the variable `{name}`"
                ))
            }
        }
        Dirt::Expression(Reach::FieldType) if !options.freeze_fields => Need::Cannot(
            "a file cannot hold its own `::fields` yet; `freeze_fields` writes the value instead"
                .to_string(),
        ),
        Dirt::Expression(_) => Need::Freeze,
        Dirt::Vocabulary { .. } => Need::Cannot("a file cannot hold a vocabulary yet".to_string()),
        Dirt::Term { .. } => Need::Cannot("a term is not frozen yet".to_string()),
    }
}

fn places(
    groups: &[Place<'_>],
    decisions: &[Decision],
    wanted: impl Fn(&Decision) -> bool,
) -> Vec<Span> {
    groups
        .iter()
        .zip(decisions)
        .filter(|(_, decision)| wanted(decision))
        .map(|(group, _)| group.span)
        .collect()
}

/// `Span` is `==` to every other span, by design; a position is compared with `exact_eq`.
fn is_among(spans: &[Span], span: Span) -> bool {
    spans.iter().any(|candidate| candidate.exact_eq(&span))
}

/// Writes the macros and variables the carried expressions use into the file.
fn carry(vault: &Vault, groups: &[Place<'_>], decisions: &[Decision], document: &mut File) {
    let mut macros: Vec<(String, Value)> = Vec::new();
    let mut vars: Vec<(String, Value)> = Vec::new();
    for (group, decision) in groups.iter().zip(decisions) {
        if !matches!(decision, Decision::Carry) {
            continue;
        }
        for finding in &group.findings {
            match &finding.dirt {
                Dirt::Expression(Reach::Macro(name))
                    if !macros.iter().any(|(held, _)| held == name) =>
                {
                    if let Some(template) = vault.config.macros.get(name) {
                        macros.push((name.clone(), Value::String(template.clone())));
                    }
                }
                Dirt::Expression(Reach::Var(name))
                    if !vars.iter().any(|(held, _)| held == name) =>
                {
                    if let Some((_, decl)) = vault.config.vars.iter().find(|(n, _)| n == name) {
                        vars.push((name.clone(), var_declaration(decl)));
                    }
                }
                _ => {}
            }
        }
    }
    macros.sort_by(|a, b| a.0.cmp(&b.0));
    vars.sort_by(|a, b| a.0.cmp(&b.0));
    add_entries(document, "macros", macros);
    add_entries(document, "vars", vars);
}

/// A variable as `::vars` writes it: `{ type, default, about }`.
fn var_declaration(decl: &tomet_declaration::VarDecl) -> Value {
    let mut fields = vec![(
        "type".to_string(),
        Value::String(decl.ty.name().to_string()),
    )];
    if let Some(default) = &decl.default {
        fields.push(("default".to_string(), default.clone()));
    }
    if let Some(about) = &decl.about {
        fields.push(("about".to_string(), Value::String(about.clone())));
    }
    Value::Map(fields)
}

/// Adds `entries` to the document's `::name{ ... }`, writing one if it has none.
fn add_entries(document: &mut File, name: &str, entries: Vec<(String, Value)>) {
    if entries.is_empty() {
        return;
    }
    let entries = entries
        .into_iter()
        .map(|(key, value)| Entry::Pair(key, value));
    let existing = document.header
        .iter_mut()
        .find(|declaration| declaration.name == Name::bare(name));
    match existing {
        Some(declaration) => declaration
            .data
            .value
            .get_or_insert_with(Group::default)
            .0
            .extend(entries),
        None => document.header.push(tomet_ast::Declaration {
            name: Name::bare(name),
            data: Data {
                args: None,
                value: Some(Group(entries.collect())),
            },
            content: None,
            id: None,
            span: Span::dummy(),
        }),
    }
}

/// Makes the document declare `::profile(standalone)`, keeping the profiles it already declares.
fn declare_standalone(document: &mut File) {
    let mut names: Vec<&str> = document_profiles(document)
        .into_iter()
        .map(Profile::name)
        .collect();
    if names.contains(&Profile::Standalone.name()) {
        return;
    }
    names.push(Profile::Standalone.name());
    // Read from text so the arguments have the shape the parser gives them, whatever it is.
    let mut written = tomet_ast_parser::parse_file(&format!("::profile({})\n", names.join(", ")))
        .expect("`::profile(...)` of known names parses");
    let declaration = written.header.pop().expect("header has declaration");
    let existing = document.header.iter().position(
        |held| held.name == Name::bare("profile"),
    );
    match existing {
        Some(index) => document.header[index] = declaration,
        None => document.header.push(declaration),
    }
}

#[cfg(test)]
mod tests;
