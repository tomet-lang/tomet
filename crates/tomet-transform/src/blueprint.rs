//! Blueprint schema extraction, transformation, and instantiation.

use tomet_ast::{File, Name, Value};
use tomet_expr::EvalContext;
use tomet_semantics::DeclKind;
use tomet_semantics::{ElementKind, classify_declaration_lenient, normalized_declaration_args};
use tomet_tree::{ValueExt, for_each_declaration, for_each_declaration_mut};

use crate::expressions::{Unresolved, resolve_expressions};
use crate::seats::{SeatProblem, fill_seats, seats_left};
use crate::source::Source;

/// What a `::blueprint` directive says about the blueprint. Its variables are
/// not here: the blueprint declares them with `::var`.
#[derive(Debug, Clone, PartialEq)]
pub struct BlueprintInfo {
    pub target_kind: String,
    pub description: Option<String>,
}

/// Extracts [`BlueprintInfo`] from `doc` if it declares a top-level `::blueprint` directive.
pub fn extract_blueprint_info(doc: &File) -> Option<BlueprintInfo> {
    let mut result = None;
    for_each_declaration(doc, |declaration| {
        if result.is_some()
            || classify_declaration_lenient(declaration)
                != ElementKind::Declaration(DeclKind::Blueprint)
        {
            return;
        }
        let target_kind = match &declaration.data.args {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Map(entries)) => entries
                .iter()
                .find(|(k, _)| k == "target" || k == "kind")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            _ => "unknown".to_string(),
        };

        let mut description = None;

        if let Some(entries) = declaration
            .data
            .value
            .as_ref()
            .map(|v| v.pairs().collect::<Vec<_>>())
        {
            for (k, v) in entries {
                if k == "description" {
                    description = v.as_str().map(String::from);
                }
            }
        }

        result = Some(BlueprintInfo {
            target_kind,
            description,
        });
    });
    result
}

/// What [`instantiate_blueprint`] did.
#[derive(Debug, Default)]
pub struct Instantiated {
    /// Whether the document differs from the blueprint it started as.
    pub changed: bool,
    /// Expressions that could not be resolved. They are left in the
    /// document as written, so the person who runs `tomet new` can see them.
    pub unresolved: Vec<Unresolved>,
}

/// Instantiates a blueprint `doc` in-place by:
/// 1. Filling every type seat (`<$type(...)>`) from its source: `default: auto` from `source`, a
///    `default` written out as itself. A seat with no source stops the instantiation.
/// 2. Promoting/converting `::blueprint(target)` to `::kind(target)` and stripping schema blocks.
/// 3. Resolving every `{$ ... }` in it, with `cx`.
///
/// A document made from a blueprint holds no type seat. Every seat that cannot be filled -- it is
/// not a type, its `default` is not of its type, it has no `default`, it stands in running text and
/// its value has none -- comes back as a [`SeatProblem`], all of them and not the first, and then `doc`
/// is left as it was: nothing is made. An expression with no value is no such stop; it stays in the
/// document as written and is reported in [`Instantiated::unresolved`].
///
/// `source` is the clock and the random source `default: auto` is made from; nothing else here reads
/// either.
pub fn instantiate_blueprint(
    doc: &mut File,
    cx: &EvalContext,
    source: &dyn Source,
) -> Result<Instantiated, Vec<SeatProblem>> {
    let filled = fill_seats(doc, cx, source)?;
    let mut changed = filled > 0;

    // Step 0: drop the blueprint's own `::kind(blueprint)`.
    //
    // It says what the *blueprint* is, and the blueprint is what stops
    // existing here. Leaving it produces a document with two kinds --
    // `::kind(blueprint)` from the source and `::kind(daily-note)` from the
    // promotion below -- which is what running `tomet new` for the first
    // time after the shape was decided actually produced.
    let mut dropped_own_kind = false;
    doc.header.retain(|declaration| {
        if classify_declaration_lenient(declaration) != ElementKind::Declaration(DeclKind::Kind) {
            return true;
        }
        let is_blueprint_kind = normalized_declaration_args(declaration)
            .as_ref()
            .and_then(|args| args.get("kind"))
            .and_then(|v| v.as_str())
            == Some("blueprint");
        if is_blueprint_kind {
            dropped_own_kind = true;
        }
        !is_blueprint_kind
    });
    changed |= dropped_own_kind;

    // The blueprint's `::var` are its inputs. They have been given their values
    // (by `tomet-workspace`), and a document made from it has none to ask for.
    let mut dropped_vars = false;
    doc.header.retain(|declaration| {
        let is_vars =
            classify_declaration_lenient(declaration) == ElementKind::Declaration(DeclKind::Var);
        dropped_vars |= is_vars;
        !is_vars
    });
    changed |= dropped_vars;

    // Step 1: Transform ::blueprint -> ::kind
    for_each_declaration_mut(doc, |declaration| {
        if classify_declaration_lenient(declaration)
            == ElementKind::Declaration(DeclKind::Blueprint)
        {
            // The target is `::blueprint`'s positional argument, which
            // `tomet-semantics::positional` normalizes to `target`.
            // That is the one spelling: a spelled-out `target:` and a
            // `kind:` would be three spellings for one thing.
            let target_kind = normalized_declaration_args(declaration)
                .as_ref()
                .and_then(|args| args.get("target"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();

            declaration.name = Name::bare("kind");
            declaration.data.args = Some(Value::String(target_kind));
            // `{version, description}` describes the blueprint, not the
            // document it produces, so it does not survive promotion.
            declaration.data.value = None;
            changed = true;
        }
    });

    let promoted = doc.clone();
    let unresolved = resolve_expressions(doc, cx);
    changed |= *doc != promoted;

    // The seats are all filled by now. One that is still here -- brought in by an expression, say
    // -- would be a seat in a document, which is no document of a blueprint's making.
    let left = seats_left(doc);
    if !left.is_empty() {
        return Err(left);
    }

    Ok(Instantiated {
        changed,
        unresolved,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FixedSource;
    use std::collections::HashMap;
    use tomet_expr::{DocRef, EvalValue, NoResolver};

    fn instantiate(src: &str, vars: EvalValue) -> (File, Instantiated) {
        let mut doc = tomet_ast_parser::parse_file(src).unwrap();
        let macros = HashMap::new();
        let cx = EvalContext {
            resolver: &NoResolver,
            self_doc: DocRef("notes/plan.tmt".into()),
            macros: &macros,
            var: &vars,
            var_unset: &[],
            includes: &[],
            heading_level: 0,
            notes: Default::default(),
        };
        let source = FixedSource::at("2026-11-11T09:30:00+09:00").expect("a datetime");
        let report =
            instantiate_blueprint(&mut doc, &cx, &source).expect("no type seat to stop it");
        (doc, report)
    }

    /// Instantiating leaves exactly one kind behind.
    ///
    /// A blueprint carries two: `::kind(blueprint)`, saying what the file
    /// is, and `::blueprint(target)`, saying what it produces. Only the
    /// second survives -- the first describes a thing that stops existing.
    /// The first run of `tomet new` after the shape was decided emitted
    /// both, which is what this pins.
    #[test]
    fn instantiating_leaves_one_kind() {
        let (doc, _) = instantiate(
            "::kind(blueprint)\n::blueprint(daily-note){\n  version: \"1.0\"\n}\n\n=[ Plan ]\n",
            EvalValue::Null,
        );

        let mut kinds = Vec::new();
        for_each_declaration(&doc, |declaration| {
            if classify_declaration_lenient(declaration) == ElementKind::Declaration(DeclKind::Kind)
                && let Some(s) = declaration.data.args.as_ref().and_then(|v| v.as_str())
            {
                kinds.push(s.to_string());
            }
        });
        assert_eq!(kinds, vec!["daily-note".to_string()]);
    }

    #[test]
    fn extracts_blueprint_info() {
        let src =
            "::blueprint(daily-note){\n  description: \"Daily note\"\n}\n\n=[ Plan ]#(plan)\n";
        let doc = tomet_ast_parser::parse_file(src).unwrap();
        let info = extract_blueprint_info(&doc).unwrap();
        assert_eq!(info.target_kind, "daily-note");
        assert_eq!(info.description.as_deref(), Some("Daily note"));
    }

    #[test]
    fn instantiates_blueprint_to_kind_and_resolves_expressions() {
        let src = "::blueprint(daily-note)\n::meta(\n  file: $(self::filename)\n  title: $(var.title)\n)\n\n=[ Plan for $(var.title) ]#(plan)\n";
        let vars = EvalValue::Map(vec![(
            "title".to_string(),
            EvalValue::String("Sprint 42".into()),
        )]);
        let (doc, report) = instantiate(src, vars);
        assert!(report.changed);
        assert!(report.unresolved.is_empty(), "{:?}", report.unresolved);

        let mut top_level = Vec::new();
        for_each_declaration(&doc, |declaration| top_level.push(declaration.clone()));

        let kind = top_level
            .iter()
            .find(|d| classify_declaration_lenient(d) == ElementKind::Declaration(DeclKind::Kind))
            .expect("expected a ::kind declaration");
        assert_eq!(kind.data.args, Some(Value::String("daily-note".into())));

        let meta = top_level
            .iter()
            .find(|d| classify_declaration_lenient(d) == ElementKind::Declaration(DeclKind::Meta))
            .expect("expected a ::meta declaration");
        let Some(Value::Map(entries)) = &meta.data.args else {
            panic!("expected meta map");
        };
        let get = |key: &str| entries.iter().find(|(k, _)| k == key).unwrap().1.clone();
        assert_eq!(get("file"), Value::String("plan.tmt".into()));
        assert_eq!(get("title"), Value::String("Sprint 42".into()));
    }

    #[test]
    fn an_expression_with_no_value_is_left_and_reported() {
        let (doc, report) = instantiate(
            "::blueprint(note)\n::meta(title: $(var.title))\n",
            EvalValue::Map(vec![]),
        );
        assert_eq!(report.unresolved.len(), 1);
        assert!(
            report.unresolved[0].reason.contains("title"),
            "{:?}",
            report.unresolved
        );
        assert!(format!("{doc:?}").contains("Interp"));
    }
}
