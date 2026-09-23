//! Blueprint schema extraction, transformation, and instantiation.

use std::collections::HashMap;

use tomet_ast::{Block, Document, ElementValue, Inline, Sigil, Span, Text, Value};
use tomet_compute::EvaluationContext;
use tomet_semantics::{ElementKind, classify_std_lenient, normalized_element_args};
use tomet_tree::{
    ValueExt, for_each_element_mut, for_each_top_level_element, for_each_top_level_element_mut,
    retain_top_level_elements,
};

/// Metadata and variable definitions extracted from a `@blueprint` directive.
#[derive(Debug, Clone, PartialEq)]
pub struct BlueprintInfo {
    pub target_kind: String,
    pub description: Option<String>,
    pub vars_schema: HashMap<String, Value>,
}

/// Extracts [`BlueprintInfo`] from `doc` if it declares a top-level `@blueprint` directive.
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
                } else if k == "vars" {
                    if let Value::Map(var_entries) = v {
                        for (vk, vv) in var_entries {
                            vars_schema.insert(vk.clone(), vv.clone());
                        }
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

/// Instantiates a blueprint `doc` in-place by:
/// 1. Promoting/converting `@blueprint(target)` to `@kind(target)` and stripping schema blocks.
/// 2. Evaluating all `${...}` interpolation expressions and placeholders using `ctx`.
pub fn instantiate_blueprint(doc: &mut Document, ctx: &EvaluationContext) -> bool {
    let mut changed = false;

    // Step 0: drop the blueprint's own `@kind(blueprint)`.
    //
    // It says what the *blueprint* is, and the blueprint is what stops
    // existing here. Leaving it produces a document with two kinds --
    // `@kind(blueprint)` from the source and `@kind(daily-note)` from the
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

    // Step 1: Transform @blueprint -> @kind
    for_each_top_level_element_mut(doc, |el| {
        if classify_std_lenient(el) == ElementKind::Blueprint {
            // The target is `@blueprint`'s positional argument, which
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

            el.sigil = Sigil::named("kind");
            el.args = Some(Value::String(target_kind));
            // `{version, description}` describes the blueprint, not the
            // document it produces, so it does not survive promotion.
            el.value = None;
            changed = true;
        }
    });

    let doc_snapshot = doc.clone();
    let config = tomet_semantics::document_config(&doc_snapshot);

    // Step 2: Evaluate element values, args, and block-level Interp (e.g. in @meta { ... })
    for_each_element_mut(doc, |el| {
        if el.placement == tomet_ast::Placement::Block {
            if let Some(ElementValue::Interp(expr)) = &el.value {
                if let Ok(val) =
                    tomet_compute::evaluate_with_context(&doc_snapshot, expr, &config, ctx)
                {
                    el.value = Some(ElementValue::from_map(val));
                    changed = true;
                }
            }
        }
        if let Some(value) = &mut el.value {
            // A group holds its data as separate entries, so each pair's
            // value is walked on its own rather than one `Value` tree.
            for (_, val) in value.pairs_mut() {
                if evaluate_value_recursively(val, &doc_snapshot, &config, ctx) {
                    changed = true;
                }
            }
        }

        if let Some(args_val) = &mut el.args {
            if evaluate_value_recursively(args_val, &doc_snapshot, &config, ctx) {
                changed = true;
            }
        }
    });

    // Step 3: Evaluate inlines inside blocks recursively
    if evaluate_blocks(&mut doc.blocks, &doc_snapshot, &config, ctx) {
        changed = true;
    }

    changed
}

fn evaluate_blocks(
    blocks: &mut [Block],
    doc: &Document,
    config: &tomet_semantics::DocumentConfig,
    ctx: &EvaluationContext,
) -> bool {
    let mut changed = false;
    for block in blocks {
        match block {
            Block::Paragraph(p) => {
                if evaluate_inlines(&mut p.content, doc, config, ctx) {
                    changed = true;
                }
            }
            Block::Section(sec) => {
                if evaluate_inlines(&mut sec.title, doc, config, ctx) {
                    changed = true;
                }
                if let Some(args_val) = &mut sec.args {
                    if evaluate_value_recursively(args_val, doc, config, ctx) {
                        changed = true;
                    }
                }
                if let Some(value) = &mut sec.value {
                    for (_, val) in value.pairs_mut() {
                        if evaluate_value_recursively(val, doc, config, ctx) {
                            changed = true;
                        }
                    }
                }
                if evaluate_blocks(&mut sec.blocks, doc, config, ctx) {
                    changed = true;
                }
            }
            Block::Element(el) => {
                if let Some(inlines) = &mut el.content {
                    if evaluate_inlines(inlines, doc, config, ctx) {
                        changed = true;
                    }
                }
                if let Some(children) = &mut el.children {
                    if evaluate_blocks(children, doc, config, ctx) {
                        changed = true;
                    }
                }
            }
        }
    }
    changed
}

fn evaluate_inlines(
    inlines: &mut Vec<Inline>,
    doc: &Document,
    config: &tomet_semantics::DocumentConfig,
    ctx: &EvaluationContext,
) -> bool {
    let mut changed = false;
    let mut new_content = Vec::new();
    for inline in inlines.iter() {
        match inline {
            Inline::Element(el) if classify_std_lenient(el) == ElementKind::Interp => {
                if let Some(ElementValue::Interp(expr)) = &el.value {
                    if let Ok(val) = tomet_compute::evaluate_with_context(doc, expr, config, ctx) {
                        new_content.push(Inline::Text(Text {
                            value: value_to_display_string(&val),
                            span: Span::default(),
                        }));
                        changed = true;
                        continue;
                    }
                }
                new_content.push(inline.clone());
            }
            Inline::Text(t) => {
                let expanded = expand_interpolations_in_string(&t.value, doc, config, ctx);
                if expanded != t.value {
                    new_content.push(Inline::Text(Text {
                        value: expanded,
                        span: t.span.clone(),
                    }));
                    changed = true;
                } else {
                    new_content.push(inline.clone());
                }
            }
            other => new_content.push(other.clone()),
        }
    }
    *inlines = new_content;
    changed
}

fn evaluate_value_recursively(
    val: &mut Value,
    doc: &Document,
    config: &tomet_semantics::DocumentConfig,
    ctx: &EvaluationContext,
) -> bool {
    let mut changed = false;
    match val {
        Value::String(s) => {
            let expanded = expand_interpolations_in_string(s, doc, config, ctx);
            if &expanded != s {
                *s = expanded;
                changed = true;
            }
        }
        // `${uuid}` in a value position (`@meta{uuid: ${uuid}}`) now
        // parses directly to this, rather than to a `Value::String`
        // `expand_interpolations_in_string` has to re-parse -- evaluate
        // it in place and splice in the resolved value, same as that
        // string path does, rather than leave the `$` wrapper for
        // `for_each_element_mut`'s own top-level-interp branch below to
        // mutate a second, incompatible way. Replacing `*val` here runs
        // before `walk_element_mut` descends into it (`visit_mut` on the
        // parent element runs to completion first), so that branch never
        // gets a turn on an already-resolved value.
        Value::Element(el) if matches!(el.sigil, Sigil::Dollar) => {
            if let Some(ElementValue::Interp(expr)) = &el.value {
                if let Ok(evaluated) = tomet_compute::evaluate_with_context(doc, expr, config, ctx)
                {
                    *val = evaluated;
                    changed = true;
                }
            }
        }
        Value::Seq(items) => {
            for item in items {
                if evaluate_value_recursively(item, doc, config, ctx) {
                    changed = true;
                }
            }
        }
        Value::Map(entries) => {
            for (_, v) in entries {
                if evaluate_value_recursively(v, doc, config, ctx) {
                    changed = true;
                }
            }
        }
        _ => {}
    }
    changed
}

fn expand_interpolations_in_string(
    s: &str,
    doc: &Document,
    config: &tomet_semantics::DocumentConfig,
    ctx: &EvaluationContext,
) -> String {
    let mut result = String::new();
    let mut rest = s;

    while let Some(start_idx) = rest.find("${") {
        result.push_str(&rest[..start_idx]);
        let after_start = &rest[start_idx + 2..];
        if let Some(end_idx) = after_start.find('}') {
            let expr_str = &after_start[..end_idx];
            let parsed_interp = format!("${{{expr_str}}}");
            if let Ok(parsed_doc) = tomet_parser::parse_document(&parsed_interp) {
                if let Some(Block::Element(el)) = parsed_doc.blocks.first() {
                    if let Some(ElementValue::Interp(expr)) = &el.value {
                        if let Ok(evaluated_val) =
                            tomet_compute::evaluate_with_context(doc, expr, config, ctx)
                        {
                            result.push_str(&value_to_display_string(&evaluated_val));
                            rest = &after_start[end_idx + 1..];
                            continue;
                        }
                    }
                }
            }
            // If evaluation failed or couldn't parse, fall back to literal
            result.push_str("${");
            result.push_str(expr_str);
            result.push('}');
            rest = &after_start[end_idx + 1..];
        } else {
            result.push_str("${");
            rest = after_start;
        }
    }
    result.push_str(rest);
    result
}

fn value_to_display_string(val: &Value) -> String {
    match val {
        Value::String(s) => s.clone(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        _ => format!("{val:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Instantiating leaves exactly one kind behind.
    ///
    /// A blueprint carries two: `@kind(blueprint)`, saying what the file
    /// is, and `@blueprint(target)`, saying what it produces. Only the
    /// second survives -- the first describes a thing that stops existing.
    /// The first run of `tomet new` after the shape was decided emitted
    /// both, which is what this pins.
    #[test]
    fn instantiating_leaves_one_kind() {
        let mut doc = tomet_parser::parse_document(
            "@kind(blueprint)\n@blueprint(daily-note){\n  version: \"1.0\"\n}\n\n#[ Plan ]\n",
        )
        .unwrap();
        instantiate_blueprint(&mut doc, &EvaluationContext::default());

        // `@kind(blueprint)` and `@blueprint(daily-note)` sat on adjacent
        // lines with nothing between them, so they join into one
        // paragraph (`docs/spec/syntax.tmt`'s `##[ 区切り ]`) -- a
        // top-level element search has to follow, not a raw `doc.blocks`
        // walk.
        let mut kinds = Vec::new();
        tomet_tree::for_each_top_level_element(&doc, |el| {
            if classify_std_lenient(el) == ElementKind::Kind {
                if let Some(s) = el.args.as_ref().and_then(|v| v.as_str()) {
                    kinds.push(s.to_string());
                }
            }
        });
        assert_eq!(kinds, vec!["daily-note".to_string()]);
    }

    #[test]
    fn extracts_blueprint_info() {
        let src = "@blueprint(daily-note){\n  description: \"Daily note\"\n  vars: {\n    author: \"Alice\"\n  }\n}\n\n#[ Plan ] {id: plan}\n";
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
    fn instantiates_blueprint_to_kind_and_evaluates_interp() {
        let src = "@blueprint(daily-note)\n@meta{\n  id: ${uuid(\"nil\")}\n  date: ${date(\"YYYY-MM-DD\")}\n  title: ${vars.title}\n}\n\n#[ Plan for ${vars.title} ] {id: plan}\n";
        let mut doc = tomet_parser::parse_document(src).unwrap();

        let mut vars = HashMap::new();
        vars.insert(
            "vars".to_string(),
            Value::Map(vec![(
                "title".to_string(),
                Value::String("Sprint 42".into()),
            )]),
        );
        let ctx = EvaluationContext { vars };

        let changed = instantiate_blueprint(&mut doc, &ctx);
        assert!(changed);

        // `@blueprint(daily-note)` and `@meta{...}` sat on adjacent
        // lines, so they join into one paragraph the same way
        // `instantiating_leaves_one_kind` does -- collect top-level
        // elements instead of indexing `doc.blocks` directly.
        let mut top_level = Vec::new();
        tomet_tree::for_each_top_level_element(&doc, |el| top_level.push(el.clone()));

        // Verify @blueprint became @kind(daily-note)
        let first_el = top_level
            .iter()
            .find(|el| classify_std_lenient(el) == ElementKind::Kind)
            .expect("expected a @kind element");
        assert_eq!(first_el.args, Some(Value::String("daily-note".into())));

        // Verify @meta fields
        let meta_el = top_level
            .iter()
            .find(|el| classify_std_lenient(el) == ElementKind::Meta)
            .expect("expected a @meta element");
        if let Some(Value::Map(entries)) = meta_el.value.as_ref().and_then(|v| v.as_data()) {
            assert_eq!(
                entries.iter().find(|(k, _)| k == "id").unwrap().1.clone(),
                Value::String("00000000-0000-0000-0000-000000000000".into())
            );
            assert_eq!(
                entries
                    .iter()
                    .find(|(k, _)| k == "title")
                    .unwrap()
                    .1
                    .clone(),
                Value::String("Sprint 42".into())
            );
        } else {
            panic!("expected meta map");
        }
    }
}
