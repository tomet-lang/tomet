//! Blueprint structural validation against a target document.

use tomet_ast::{Block, Document, Element, ElementValue, Inline, Span, Value};
use tomet_semantics::{ElementKind, classify};
use tomet_tree::ValueExt;

use crate::ValidationError;

/// Structural requirement extracted from a blueprint.
#[derive(Debug, Clone, PartialEq)]
pub struct BlueprintSchema {
    pub target_kind: String,
    pub required_meta_keys: Vec<(String, Span)>,
    pub required_sections: Vec<RequiredSection>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RequiredSection {
    pub title: String,
    pub id: Option<String>,
    pub span: Span,
}

/// Extracts the [`BlueprintSchema`] requirements from a blueprint document.
pub fn extract_blueprint_schema(blueprint: &Document) -> Option<BlueprintSchema> {
    let mut target_kind = "unknown".to_string();
    let mut required_meta_keys = Vec::new();
    let mut required_sections = Vec::new();
    let mut found_blueprint_or_kind = false;

    for block in &blueprint.blocks {
        if let Block::Element(el) = block {
            let kind = classify(el);
            if kind == ElementKind::Blueprint || kind == ElementKind::Kind {
                found_blueprint_or_kind = true;
                if let Some(Value::String(k)) = &el.args {
                    target_kind = k.clone();
                } else if let Some(Value::Map(entries)) = &el.args {
                    if let Some((_, v)) = entries.iter().find(|(k, _)| k == "target" || k == "kind") {
                        if let Some(s) = v.as_str() {
                            target_kind = s.to_string();
                        }
                    }
                }
            } else if kind == ElementKind::Meta {
                if let Some(ElementValue::Data(Value::Map(entries))) = &el.value {
                    for (k, _) in entries {
                        required_meta_keys.push((k.clone(), el.span));
                    }
                }
                if let Some(Value::Map(entries)) = &el.args {
                    for (k, _) in entries {
                        if k != "format" {
                            required_meta_keys.push((k.clone(), el.span));
                        }
                    }
                }
            } else if kind == ElementKind::Heading {
                let id = el
                    .args
                    .as_ref()
                    .and_then(|a| a.get("id"))
                    .and_then(|v| v.as_str())
                    .or_else(|| {
                        if let Some(ElementValue::Data(Value::Map(entries))) = &el.value {
                            entries.iter().find(|(k, _)| k == "id").and_then(|(_, v)| v.as_str())
                        } else {
                            None
                        }
                    })
                    .map(String::from);

                // If the heading has an explicit ID or is defined in blueprint, treat as structural section
                let title = extract_element_title(el);
                if id.is_some() || !title.is_empty() {
                    required_sections.push(RequiredSection {
                        title,
                        id,
                        span: el.span,
                    });
                }
            }
        }
    }

    if found_blueprint_or_kind {
        Some(BlueprintSchema {
            target_kind,
            required_meta_keys,
            required_sections,
        })
    } else {
        None
    }
}

/// Validates `doc` against `blueprint` and returns any missing structural elements or fields.
pub fn validate_against_blueprint(doc: &Document, blueprint: &Document) -> Vec<ValidationError> {
    let Some(schema) = extract_blueprint_schema(blueprint) else {
        return Vec::new();
    };

    let mut errors = Vec::new();
    let doc_span = doc.span;

    // 1. Verify required @meta keys
    let mut doc_meta_keys = Vec::new();
    let mut doc_meta_span = doc_span;

    for block in &doc.blocks {
        if let Block::Element(el) = block {
            if classify(el) == ElementKind::Meta {
                doc_meta_span = el.span;
                if let Some(ElementValue::Data(Value::Map(entries))) = &el.value {
                    for (k, _) in entries {
                        doc_meta_keys.push(k.clone());
                    }
                }
                if let Some(Value::Map(entries)) = &el.args {
                    for (k, _) in entries {
                        if k != "format" {
                            doc_meta_keys.push(k.clone());
                        }
                    }
                }
            }
        }
    }

    for (req_key, _) in &schema.required_meta_keys {
        if !doc_meta_keys.contains(req_key) {
            errors.push(ValidationError::MissingRequiredMetaKey {
                key: req_key.clone(),
                kind: schema.target_kind.clone(),
                span: doc_meta_span,
            });
        }
    }

    // 2. Verify required sections
    let mut doc_headings: Vec<(String, Option<String>)> = Vec::new();

    for block in &doc.blocks {
        if let Block::Element(el) = block {
            if classify(el) == ElementKind::Heading {
                let id = el
                    .args
                    .as_ref()
                    .and_then(|a| a.get("id"))
                    .and_then(|v| v.as_str())
                    .or_else(|| {
                        if let Some(ElementValue::Data(Value::Map(entries))) = &el.value {
                            entries.iter().find(|(k, _)| k == "id").and_then(|(_, v)| v.as_str())
                        } else {
                            None
                        }
                    })
                    .map(String::from);

                let title = extract_element_title(el);
                doc_headings.push((title, id));
            }
        }
    }

    for req_sec in &schema.required_sections {
        let is_present = doc_headings.iter().any(|(doc_title, doc_id)| {
            if let (Some(req_id), Some(d_id)) = (&req_sec.id, doc_id) {
                req_id == d_id
            } else if req_sec.id.is_none() {
                doc_title.trim() == req_sec.title.trim()
            } else {
                false
            }
        });

        if !is_present {
            errors.push(ValidationError::MissingRequiredSection {
                title: req_sec.title.clone(),
                id: req_sec.id.clone(),
                kind: schema.target_kind.clone(),
                span: doc_span,
            });
        }
    }

    errors
}

fn extract_element_title(el: &Element) -> String {
    let mut title = String::new();
    if let Some(inlines) = &el.content {
        for inline in inlines {
            match inline {
                Inline::Text(t) => title.push_str(&t.value),
                _ => {}
            }
        }
    }
    title
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Document {
        tomet_parser::parse_document(src).unwrap()
    }

    #[test]
    fn validates_matching_document_successfully() {
        let blueprint = parse(
            "@blueprint(daily-note)\n@meta{\n  id: ${uuid()}\n  date: ${date()}\n}\n\n#[ Plan ] {id: plan}\n\n#[ Review ] {id: review}\n",
        );
        let doc = parse(
            "@kind(daily-note)\n@meta{\n  id: doc-123\n  date: 2026-09-01\n}\n\n#[ Plan ] {id: plan}\n- ( ) My task\n\n#[ Review ] {id: review}\nGood day.\n",
        );

        let errors = validate_against_blueprint(&doc, &blueprint);
        assert!(errors.is_empty(), "expected no errors, got: {errors:?}");
    }

    #[test]
    fn reports_missing_meta_key_and_missing_section() {
        let blueprint = parse(
            "@blueprint(daily-note)\n@meta{\n  id: ${uuid()}\n  date: ${date()}\n}\n\n#[ Plan ] {id: plan}\n\n#[ Review ] {id: review}\n",
        );
        // doc is missing `@meta.date` and `#[ Review ] {id: review}`
        let doc = parse(
            "@kind(daily-note)\n@meta{\n  id: doc-123\n}\n\n#[ Plan ] {id: plan}\n- ( ) My task\n",
        );

        let errors = validate_against_blueprint(&doc, &blueprint);
        assert_eq!(errors.len(), 2);
        assert!(matches!(
            &errors[0],
            ValidationError::MissingRequiredMetaKey { key, kind, .. } if key == "date" && kind == "daily-note"
        ));
        assert!(matches!(
            &errors[1],
            ValidationError::MissingRequiredSection { title, id, kind, .. } if title == "Review" && id.as_deref() == Some("review") && kind == "daily-note"
        ));
    }
}
