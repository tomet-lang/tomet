//! Blueprint schema extraction, transformation, and instantiation.

use std::collections::HashMap;

use tomet_ast::{Document, Sigil, Value};
use tomet_expr::EvalContext;
use tomet_semantics::{ElementKind, classify_std_lenient, normalized_element_args};
use tomet_tree::{
    ValueExt, for_each_top_level_element, for_each_top_level_element_mut, retain_top_level_elements,
};

use crate::expressions::{Unresolved, resolve_expressions};

/// Metadata and variable definitions extracted from a `::blueprint` directive.
#[derive(Debug, Clone, PartialEq)]
pub struct BlueprintInfo {
    pub target_kind: String,
    pub description: Option<String>,
    pub vars_schema: HashMap<String, Value>,
}

/// Extracts [`BlueprintInfo`] from `doc` if it declares a top-level `::blueprint` directive.
pub fn extract_blueprint_info(doc: &Document) -> Option<BlueprintInfo> {
    let mut result = None;
    for_each_top_level_element(doc, |el| {
        if result.is_some() || classify_std_lenient(el) != ElementKind::Blueprint {
            return;
        }
        let target_kind = match &el.args {
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
        let mut vars_schema = HashMap::new();

        if let Some(entries) = el.value.as_ref().map(|v| v.pairs().collect::<Vec<_>>()) {
            for (k, v) in entries {
                if k == "description" {
                    description = v.as_str().map(String::from);
                } else if k == "vars"
                    && let Value::Map(var_entries) = v
                {
                    for (vk, vv) in var_entries {
                        vars_schema.insert(vk.clone(), vv.clone());
                    }
                }
            }
        }

        result = Some(BlueprintInfo {
            target_kind,
            description,
            vars_schema,
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
/// 1. Promoting/converting `::blueprint(target)` to `::kind(target)` and stripping schema blocks.
/// 2. Resolving every `${...}` in it, with `cx` -- which is a creating context
///    (`Phase::Create`), the only place `generate.*` and `vars` exist.
pub fn instantiate_blueprint(doc: &mut Document, cx: &EvalContext) -> Instantiated {
    let mut changed = false;

    // Step 0: drop the blueprint's own `::kind(blueprint)`.
    //
    // It says what the *blueprint* is, and the blueprint is what stops
    // existing here. Leaving it produces a document with two kinds --
    // `::kind(blueprint)` from the source and `::kind(daily-note)` from the
    // promotion below -- which is what running `tomet new` for the first
    // time after the shape was decided actually produced.
    let mut dropped_own_kind = false;
    retain_top_level_elements(doc, |el| {
        if classify_std_lenient(el) != ElementKind::Kind {
            return true;
        }
        let is_blueprint_kind = normalized_element_args(el)
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

    // Step 1: Transform ::blueprint -> ::kind
    for_each_top_level_element_mut(doc, |el| {
        if classify_std_lenient(el) == ElementKind::Blueprint {
            // The target is `::blueprint`'s positional argument, which
            // `tomet-semantics::positional` normalizes to `target`.
            // This used to also accept a spelled-out `target:` and a
            // `kind:`, three spellings for one thing that nobody had
            // chosen between -- see the root writ's `blueprint-shape`.
            let target_kind = normalized_element_args(el)
                .as_ref()
                .and_then(|args| args.get("target"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();

            el.sigil = Sigil::document("kind");
            el.args = Some(Value::String(target_kind));
            // `{version, description}` describes the blueprint, not the
            // document it produces, so it does not survive promotion.
            el.value = None;
            changed = true;
        }
    });

    let promoted = doc.clone();
    let unresolved = resolve_expressions(doc, cx);
    changed |= *doc != promoted;

    Instantiated {
        changed,
        unresolved,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::OffsetDateTime;
    use tomet_expr::{DocRef, EvalValue, Generate, NoResolver, Phase};

    /// A clock stopped at the epoch and a UUID that is always the same, so a
    /// blueprint's output is the same every time.
    struct Fixed;

    impl Generate for Fixed {
        fn now(&self) -> OffsetDateTime {
            OffsetDateTime::UNIX_EPOCH
        }
        fn uuid(&self) -> String {
            "00000000-0000-0000-0000-000000000000".to_string()
        }
    }

    fn instantiate(src: &str, vars: EvalValue) -> (Document, Instantiated) {
        let mut doc = tomet_parser::parse_document(src).unwrap();
        let macros = HashMap::new();
        let cx = EvalContext {
            phase: Phase::Create,
            resolver: &NoResolver,
            generate: &Fixed,
            self_doc: DocRef("notes/plan.tmt".into()),
            macros: &macros,
            vars: &vars,
            includes: &[],
            heading_level: 0,
            notes: Default::default(),
        };
        let report = instantiate_blueprint(&mut doc, &cx);
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

        // `::kind(blueprint)` and `::blueprint(daily-note)` sat on adjacent
        // lines with nothing between them, so they join into one
        // paragraph -- a top-level element search has to follow, not a raw
        // `doc.blocks` walk.
        let mut kinds = Vec::new();
        tomet_tree::for_each_top_level_element(&doc, |el| {
            if classify_std_lenient(el) == ElementKind::Kind
                && let Some(s) = el.args.as_ref().and_then(|v| v.as_str())
            {
                kinds.push(s.to_string());
            }
        });
        assert_eq!(kinds, vec!["daily-note".to_string()]);
    }

    #[test]
    fn extracts_blueprint_info() {
        let src = "::blueprint(daily-note){\n  description: \"Daily note\"\n  vars: {\n    author: \"Alice\"\n  }\n}\n\n=[ Plan ]#(plan)\n";
        let doc = tomet_parser::parse_document(src).unwrap();
        let info = extract_blueprint_info(&doc).unwrap();
        assert_eq!(info.target_kind, "daily-note");
        assert_eq!(info.description.as_deref(), Some("Daily note"));
        assert_eq!(
            info.vars_schema.get("author"),
            Some(&Value::String("Alice".into()))
        );
    }

    #[test]
    fn instantiates_blueprint_to_kind_and_resolves_expressions() {
        let src = "::blueprint(daily-note)\n::meta{\n  id: ${generate.uuid()}\n  date: ${generate.now().format(\"YYYY-MM-DD\")}\n  file: ${self.filename}\n  title: ${vars.title}\n}\n\n=[ Plan for ${vars.title} ]#(plan)\n";
        let vars = EvalValue::Map(vec![(
            "title".to_string(),
            EvalValue::String("Sprint 42".into()),
        )]);
        let (doc, report) = instantiate(src, vars);
        assert!(report.changed);
        assert!(report.unresolved.is_empty(), "{:?}", report.unresolved);

        // `::blueprint(daily-note)` and `::meta{...}` sat on adjacent lines, so
        // they join into one paragraph the same way `instantiating_leaves_one_kind`
        // does -- collect top-level elements instead of indexing `doc.blocks`.
        let mut top_level = Vec::new();
        tomet_tree::for_each_top_level_element(&doc, |el| top_level.push(el.clone()));

        let kind = top_level
            .iter()
            .find(|el| classify_std_lenient(el) == ElementKind::Kind)
            .expect("expected a ::kind element");
        assert_eq!(kind.args, Some(Value::String("daily-note".into())));

        let meta = top_level
            .iter()
            .find(|el| classify_std_lenient(el) == ElementKind::Meta)
            .expect("expected a ::meta element");
        let Some(Value::Map(entries)) = meta.value.as_ref().and_then(|v| v.as_data()) else {
            panic!("expected meta map");
        };
        let get = |key: &str| entries.iter().find(|(k, _)| k == key).unwrap().1.clone();
        assert_eq!(
            get("id"),
            Value::String("00000000-0000-0000-0000-000000000000".into())
        );
        assert_eq!(get("date"), Value::String("1970-01-01".into()));
        assert_eq!(get("file"), Value::String("plan.tmt".into()));
        assert_eq!(get("title"), Value::String("Sprint 42".into()));
    }

    #[test]
    fn an_expression_with_no_value_is_left_and_reported() {
        let (doc, report) = instantiate(
            "::blueprint(note)\n::meta{ title: ${vars.title} }\n",
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
