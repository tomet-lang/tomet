//! Blueprint template extraction, transformation, and instantiation.

use std::collections::HashMap;

use tomet_ast::{Block, Document, ElementValue, Inline, Sigil, Span, Text, Value};
use tomet_compute::EvaluationContext;
use tomet_semantics::{ElementKind, classify_lenient};
use tomet_tree::{ValueExt, for_each_element_mut};

/// Metadata and variable definitions extracted from a `@blueprint` directive.
#[derive(Debug, Clone, PartialEq)]
pub struct BlueprintInfo {
    pub target_kind: String,
    pub description: Option<String>,
    pub vars_schema: HashMap<String, Value>,
}

/// Extracts [`BlueprintInfo`] from `doc` if it declares a top-level `@blueprint` directive.
pub fn extract_blueprint_info(doc: &Document) -> Option<BlueprintInfo> {
    for block in &doc.blocks {
        if let Block::Element(el) = block {
            if classify_lenient(el) == ElementKind::Blueprint {
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

                return Some(BlueprintInfo {
                    target_kind,
                    description,
                    vars_schema,
                });
            }
        }
    }
    None
}

/// Instantiates a blueprint `doc` in-place by:
/// 1. Promoting/converting `@blueprint(target)` to `@kind(target)` and stripping schema blocks.
/// 2. Evaluating all `${...}` interpolation expressions and placeholders using `ctx`.
pub fn instantiate_blueprint(doc: &mut Document, ctx: &EvaluationContext) -> bool {
    let mut changed = false;

    // Step 1: Transform @blueprint -> @kind
    for block in &mut doc.blocks {
        if let Block::Element(el) = block {
            if classify_lenient(el) == ElementKind::Blueprint {
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

                el.sigil = Sigil::named("kind");
                el.args = Some(Value::String(target_kind));
                el.value = None;
                changed = true;
            }
        }
    }

    let doc_snapshot = doc.clone();
    let config = tomet_semantics::document_config(&doc_snapshot);

    // Step 2: Evaluate element values, args, and content (e.g. in @meta { ... }, #[ Heading ], etc.)
    for_each_element_mut(doc, |el| {
        if let Some(ElementValue::Interp(expr)) = &el.value {
            if let Ok(val) = tomet_compute::evaluate_with_context(&doc_snapshot, expr, &config, ctx)
            {
                el.value = Some(ElementValue::from_map(val));
                changed = true;
            }
        } else if let Some(value) = &mut el.value {
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

        if let Some(inlines) = &mut el.content {
            if evaluate_inlines(inlines, &doc_snapshot, &config, ctx) {
                changed = true;
            }
        }
    });

    // Step 3: Evaluate inline expressions inside top-level paragraphs
    for block in &mut doc.blocks {
        if let Block::Paragraph(p) = block {
            if evaluate_inlines(&mut p.content, &doc_snapshot, &config, ctx) {
                changed = true;
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
            Inline::Element(el) if classify_lenient(el) == ElementKind::Interp => {
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

        // Verify @blueprint became @kind(daily-note)
        let first_el = match &doc.blocks[0] {
            Block::Element(el) => el,
            _ => panic!("expected element"),
        };
        assert_eq!(classify_lenient(first_el), ElementKind::Kind);
        assert_eq!(first_el.args, Some(Value::String("daily-note".into())));

        // Verify @meta fields
        let meta_el = match &doc.blocks[1] {
            Block::Element(el) => el,
            _ => panic!("expected element"),
        };
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
