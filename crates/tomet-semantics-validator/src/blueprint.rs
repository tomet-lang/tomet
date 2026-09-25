//! Blueprint structural validation against a target document.

use tomet_ast::{Block, Document, Element, Inline, Section, Span, Value};
use tomet_semantics::{ElementKind, classify_std_lenient};
use tomet_tree::{ValueExt, for_each_top_level_element};

use crate::Diagnostic;

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
    let mut found_blueprint = false;

    // Top-level per `for_each_top_level_element`, not a raw `blueprint.blocks`
    // walk: `@blueprint`/`@meta` may have joined an adjacent paragraph
    // (`docs/spec/syntax.tmt`'s `##[ 区切り ]`, as a real blueprint's own
    // preamble commonly does -- `@kind(blueprint)` right above
    // `@blueprint(...)` right above `@meta{...}`, no blank lines) and come
    // back as `Inline::Element` there instead of its own `Block::Element`
    // -- still top-level, just a different tree shape.
    for_each_top_level_element(blueprint, |el| {
        let kind = classify_std_lenient(el);
        // `@blueprint` only. This used to accept any `@kind` too,
        // which meant every ordinary document read as a blueprint of
        // itself and nothing could tell the two apart. A document is a
        // blueprint because it says `@kind(blueprint)` and carries
        // `@blueprint(target)`, not because it has a kind at all.
        if kind == ElementKind::Blueprint {
            found_blueprint = true;
            if let Some(Value::String(k)) = &el.args {
                target_kind = k.clone();
            } else if let Some(Value::Map(entries)) = &el.args
                && let Some((_, v)) = entries.iter().find(|(k, _)| k == "target" || k == "kind")
                && let Some(s) = v.as_str()
            {
                target_kind = s.to_string();
            }
        } else if kind == ElementKind::Meta {
            if let Some(entries) = el.value.as_ref().map(|v| v.pairs().collect::<Vec<_>>()) {
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
                    if let Some(entries) = el.value.as_ref().map(|v| v.pairs().collect::<Vec<_>>())
                    {
                        entries
                            .iter()
                            .find(|(k, _)| *k == "id")
                            .and_then(|(_, v)| v.as_str())
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
    });

    walk_blueprint_sections(&blueprint.blocks, &mut required_sections);

    if found_blueprint {
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
pub fn validate_against_blueprint(doc: &Document, blueprint: &Document) -> Vec<Diagnostic> {
    let Some(schema) = extract_blueprint_schema(blueprint) else {
        return Vec::new();
    };

    let mut errors = Vec::new();
    let doc_span = doc.span;

    // 1. Verify required @meta keys
    let mut doc_meta_keys = Vec::new();
    let mut doc_meta_span = doc_span;

    for_each_top_level_element(doc, |el| {
        if classify_std_lenient(el) == ElementKind::Meta {
            doc_meta_span = el.span;
            if let Some(entries) = el.value.as_ref().map(|v| v.pairs().collect::<Vec<_>>()) {
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
    });

    for (req_key, _) in &schema.required_meta_keys {
        if !doc_meta_keys.contains(req_key) {
            errors.push(Diagnostic::MissingRequiredMetaKey {
                key: req_key.clone(),
                kind: schema.target_kind.clone(),
                span: doc_meta_span,
            });
        }
    }

    // 2. Verify required sections
    let mut doc_headings: Vec<(String, Option<String>)> = Vec::new();

    for_each_top_level_element(doc, |el| {
        if classify_std_lenient(el) == ElementKind::Heading {
            let id = el
                .args
                .as_ref()
                .and_then(|a| a.get("id"))
                .and_then(|v| v.as_str())
                .or_else(|| {
                    if let Some(entries) = el.value.as_ref().map(|v| v.pairs().collect::<Vec<_>>())
                    {
                        entries
                            .iter()
                            .find(|(k, _)| *k == "id")
                            .and_then(|(_, v)| v.as_str())
                    } else {
                        None
                    }
                })
                .map(String::from);

            let title = extract_element_title(el);
            doc_headings.push((title, id));
        }
    });

    walk_doc_sections(&doc.blocks, &mut doc_headings);

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
            errors.push(Diagnostic::MissingRequiredSection {
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
            if let Inline::Text(t) = inline {
                title.push_str(&t.value)
            }
        }
    }
    title
}

fn section_id(sec: &Section) -> Option<String> {
    sec.args
        .as_ref()
        .and_then(|a| a.get("id"))
        .and_then(|v| v.as_str())
        .or_else(|| {
            if let Some(entries) = sec.value.as_ref().map(|v| v.pairs().collect::<Vec<_>>()) {
                entries
                    .iter()
                    .find(|(k, _)| *k == "id")
                    .and_then(|(_, v)| v.as_str())
            } else {
                None
            }
        })
        .map(String::from)
}

fn extract_section_title(title_inlines: &[Inline]) -> String {
    let mut title = String::new();
    for inline in title_inlines {
        if let Inline::Text(t) = inline {
            title.push_str(&t.value);
        }
    }
    title
}

fn walk_blueprint_sections(blocks: &[Block], required_sections: &mut Vec<RequiredSection>) {
    for block in blocks {
        if let Block::Section(sec) = block {
            let id = section_id(sec);
            let title = extract_section_title(&sec.title);
            if id.is_some() || !title.is_empty() {
                required_sections.push(RequiredSection {
                    title,
                    id,
                    span: sec.span,
                });
            }
            walk_blueprint_sections(&sec.blocks, required_sections);
        }
    }
}

fn walk_doc_sections(blocks: &[Block], doc_headings: &mut Vec<(String, Option<String>)>) {
    for block in blocks {
        if let Block::Section(sec) = block {
            let id = section_id(sec);
            let title = extract_section_title(&sec.title);
            doc_headings.push((title, id));
            walk_doc_sections(&sec.blocks, doc_headings);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Document {
        tomet_parser::parse_document(src).unwrap()
    }

    /// An ordinary document is not a blueprint of itself.
    ///
    /// `extract_blueprint_schema` used to accept any `@kind` as evidence of
    /// blueprint-ness, so every document in the vault answered "yes, I am a
    /// schema" and nothing could tell a blueprint from what it describes.
    /// No test exercised that branch, which is how it survived.
    #[test]
    fn a_plain_kind_document_is_not_a_blueprint() {
        let doc = tomet_parser::parse_document(
            "@kind(daily-note)\n@meta{\n  id: doc-1\n}\n\n=[ Plan ] {id: plan}\n",
        )
        .unwrap();
        assert!(
            extract_blueprint_schema(&doc).is_none(),
            "a document with a kind but no `@blueprint` must not read as a schema"
        );
    }

    #[test]
    fn a_blueprint_still_extracts_its_schema() {
        let doc = tomet_parser::parse_document(
            "@kind(blueprint)\n@blueprint(daily-note)\n\n=[ Plan ] {id: plan}\n",
        )
        .unwrap();
        assert!(
            extract_blueprint_schema(&doc).is_some(),
            "the decided shape must still be recognised"
        );
    }

    #[test]
    fn validates_matching_document_successfully() {
        let blueprint = parse(
            "@blueprint(daily-note)\n@meta{\n  id: ${uuid()}\n  date: ${date()}\n}\n\n=[ Plan ] {id: plan}\n\n=[ Review ] {id: review}\n",
        );
        let doc = parse(
            "@kind(daily-note)\n@meta{\n  id: doc-123\n  date: 2026-09-01\n}\n\n=[ Plan ] {id: plan}\n- ( ) My task\n\n=[ Review ] {id: review}\nGood day.\n",
        );

        let errors = validate_against_blueprint(&doc, &blueprint);
        assert!(errors.is_empty(), "expected no errors, got: {errors:?}");
    }

    #[test]
    fn reports_missing_meta_key_and_missing_section() {
        let blueprint = parse(
            "@blueprint(daily-note)\n@meta{\n  id: ${uuid()}\n  date: ${date()}\n}\n\n=[ Plan ] {id: plan}\n\n=[ Review ] {id: review}\n",
        );
        // doc is missing `@meta.date` and `=[ Review ] {id: review}`
        let doc = parse(
            "@kind(daily-note)\n@meta{\n  id: doc-123\n}\n\n=[ Plan ] {id: plan}\n- ( ) My task\n",
        );

        let errors = validate_against_blueprint(&doc, &blueprint);
        assert_eq!(errors.len(), 2);
        assert!(matches!(
            &errors[0],
            Diagnostic::MissingRequiredMetaKey { key, kind, .. } if key == "date" && kind == "daily-note"
        ));
        assert!(matches!(
            &errors[1],
            Diagnostic::MissingRequiredSection { title, id, kind, .. } if title == "Review" && id.as_deref() == Some("review") && kind == "daily-note"
        ));
    }
}
