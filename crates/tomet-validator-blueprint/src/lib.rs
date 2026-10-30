//! Blueprint structural validation against a target document.
//!
//! Extracts structural schemas (required meta keys, required sections, and section-level
//! `:rule` constraints) from blueprint documents (`::kind(blueprint)`), and validates
//! target documents for conformance.

use tomet_ast::{Block, Document, Element, Inline, Section, Span, Value};
use tomet_semantics::DeclKind;
use tomet_semantics::{Bindings, ElementKind, classify_std_lenient};
use tomet_tree::{ValueExt, for_each_top_level_element};
use tomet_validator_core::Diagnostic;
use tomet_validator_rule::{RuleArgs, check_descendants_against_rule, decode_rule_args};

/// Structural requirement extracted from a blueprint.
#[derive(Debug, Clone, PartialEq)]
pub struct BlueprintSchema {
    pub target_kind: String,
    /// The `::meta` keys the blueprint declares, and `id` when it declares `::id(...)`: the
    /// document's identity is required, though it is not written in `::meta`.
    pub required_meta_keys: Vec<(String, Span)>,
    pub required_sections: Vec<RequiredSection>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RequiredSection {
    pub title: String,
    pub id: Option<String>,
    pub span: Span,
    /// `:rule(...)` constraints attached to this section in the blueprint.
    pub rules: Vec<RuleArgs>,
}

/// Extracts the [`BlueprintSchema`] requirements from a blueprint document.
pub fn extract_blueprint_schema(blueprint: &Document) -> Option<BlueprintSchema> {
    let mut target_kind = "unknown".to_string();
    let mut required_meta_keys = Vec::new();
    let mut required_sections = Vec::new();
    let mut found_blueprint = false;

    for_each_top_level_element(blueprint, |el| {
        let kind = classify_std_lenient(el);
        if kind == ElementKind::Declaration(DeclKind::Blueprint) {
            found_blueprint = true;
            if let Some(Value::String(k)) = &el.args {
                target_kind = k.clone();
            } else if let Some(Value::Map(entries)) = &el.args
                && let Some((_, v)) = entries.iter().find(|(k, _)| k == "target" || k == "kind")
                && let Some(s) = v.as_str()
            {
                target_kind = s.to_string();
            }
        } else if kind == ElementKind::Declaration(DeclKind::Id) {
            required_meta_keys.push(("id".to_string(), el.span));
        } else if kind == ElementKind::Declaration(DeclKind::Meta) {
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
            let id = el.id.as_ref().map(|id| id.0.clone());
            let title = extract_element_title(el);
            let rules = extract_connect_rules(&el.connects);
            if id.is_some() || !title.is_empty() {
                required_sections.push(RequiredSection {
                    title,
                    id,
                    span: el.span,
                    rules,
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

/// Validates `doc` against `blueprint` using standard bindings.
pub fn validate_against_blueprint(doc: &Document, blueprint: &Document) -> Vec<Diagnostic> {
    validate_against_blueprint_with(doc, blueprint, &Bindings::default())
}

/// Validates `doc` against `blueprint` with explicit vocabulary `bindings`.
pub fn validate_against_blueprint_with(
    doc: &Document,
    blueprint: &Document,
    bindings: &Bindings,
) -> Vec<Diagnostic> {
    let Some(schema) = extract_blueprint_schema(blueprint) else {
        return Vec::new();
    };
    validate_against_schema(doc, &schema, bindings)
}

/// Validates `doc` against an extracted [`BlueprintSchema`].
pub fn validate_against_schema(
    doc: &Document,
    schema: &BlueprintSchema,
    bindings: &Bindings,
) -> Vec<Diagnostic> {
    let mut errors = Vec::new();
    let doc_span = doc.span;

    // 1. Verify required ::meta keys
    let mut doc_meta_keys = Vec::new();
    let mut doc_meta_span = doc_span;

    for_each_top_level_element(doc, |el| {
        if classify_std_lenient(el) == ElementKind::Declaration(DeclKind::Id) {
            doc_meta_keys.push("id".to_string());
        } else if classify_std_lenient(el) == ElementKind::Declaration(DeclKind::Meta) {
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

    // 2. Verify required sections and apply section-level rules
    let mut doc_sections: Vec<&Section> = Vec::new();
    collect_doc_sections(&doc.blocks, &mut doc_sections);

    for req_sec in &schema.required_sections {
        let matching_sec = doc_sections.iter().find(|sec| {
            let d_id = section_id(sec);
            let d_title = extract_section_title(&sec.title);
            if let (Some(req_id), Some(d_id)) = (&req_sec.id, d_id) {
                req_id == &d_id
            } else if req_sec.id.is_none() {
                d_title.trim() == req_sec.title.trim()
            } else {
                false
            }
        });

        match matching_sec {
            Some(sec) => {
                // Apply rules declared on the blueprint section to the matched section
                for rule_args in &req_sec.rules {
                    check_descendants_against_rule(
                        rule_args,
                        req_sec.span,
                        bindings,
                        &mut errors,
                        |direct, f| {
                            tomet_tree::for_each_descendant_in_blocks(&sec.blocks, direct, f);
                        },
                    );
                }
            }
            None => {
                errors.push(Diagnostic::MissingRequiredSection {
                    title: req_sec.title.clone(),
                    id: req_sec.id.clone(),
                    kind: schema.target_kind.clone(),
                    span: doc_span,
                });
            }
        }
    }

    errors
}

fn extract_connect_rules(connects: &[Element]) -> Vec<RuleArgs> {
    let mut rules = Vec::new();
    for conn in connects {
        if conn.sigil.is_bare_named("rule")
            && let Some(args) = decode_rule_args(conn)
        {
            rules.push(args);
        }
    }
    rules
}

fn extract_element_title(el: &Element) -> String {
    let mut title = String::new();
    if let Some(blocks) = &el.content {
        for block in blocks {
            if let Block::Paragraph(p) = block {
                for inline in &p.content {
                    if let Inline::Text(t) = inline {
                        title.push_str(&t.value)
                    }
                }
            }
        }
    }
    title
}

fn section_id(sec: &Section) -> Option<String> {
    sec.id.as_ref().map(|id| id.0.clone())
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
            let rules = extract_connect_rules(&sec.connects);
            if id.is_some() || !title.is_empty() {
                required_sections.push(RequiredSection {
                    title,
                    id,
                    span: sec.span,
                    rules,
                });
            }
            walk_blueprint_sections(&sec.blocks, required_sections);
        }
    }
}

fn collect_doc_sections<'a>(blocks: &'a [Block], doc_sections: &mut Vec<&'a Section>) {
    for block in blocks {
        if let Block::Section(sec) = block {
            doc_sections.push(sec);
            collect_doc_sections(&sec.blocks, doc_sections);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Document {
        tomet_parser::parse_document(src).unwrap()
    }

    #[test]
    fn a_plain_kind_document_is_not_a_blueprint() {
        let doc = tomet_parser::parse_document(
            "::kind(daily-note)\n::id(doc-1)\n\n=[ Plan ] {id: plan}\n",
        )
        .unwrap();
        assert!(
            extract_blueprint_schema(&doc).is_none(),
            "a document with a kind but no `::blueprint` must not read as a schema"
        );
    }

    #[test]
    fn a_blueprint_still_extracts_its_schema() {
        let doc = tomet_parser::parse_document(
            "::kind(blueprint)\n::blueprint(daily-note)\n\n=[ Plan ] {id: plan}\n",
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
            "::blueprint(daily-note)\n::id(${uuid()})\n::meta{\n  date: ${date()}\n}\n\n=[ Plan ] {id: plan}\n\n=[ Review ] {id: review}\n",
        );
        let doc = parse(
            "::kind(daily-note)\n::id(doc-123)\n::meta{\n  date: 2026-09-01\n}\n\n=[ Plan ] {id: plan}\n- ( ) My task\n\n=[ Review ] {id: review}\nGood day.\n",
        );

        let errors = validate_against_blueprint(&doc, &blueprint);
        assert!(errors.is_empty(), "expected no errors, got: {errors:?}");
    }

    #[test]
    fn a_blueprint_with_an_id_requires_the_document_to_declare_one() {
        let blueprint = parse("::blueprint(daily-note)\n::id(${uuid()})\n\n=[ Plan ]#(plan)\n");
        let without = parse("::kind(daily-note)\n\n=[ Plan ]#(plan)\n");
        let errors = validate_against_blueprint(&without, &blueprint);
        assert!(
            matches!(
                errors.as_slice(),
                [Diagnostic::MissingRequiredMetaKey { key, .. }] if key == "id"
            ),
            "{errors:?}"
        );

        let with = parse("::kind(daily-note)\n::id(doc-1)\n\n=[ Plan ]#(plan)\n");
        assert!(validate_against_blueprint(&with, &blueprint).is_empty());
    }

    #[test]
    fn reports_missing_meta_key_and_missing_section() {
        let blueprint = parse(
            "::blueprint(daily-note)\n::id(${uuid()})\n::meta{\n  date: ${date()}\n}\n\n=[ Plan ]#(plan)\n\n=[ Review ]#(review)\n",
        );
        let doc = parse("::kind(daily-note)\n::id(doc-123)\n\n=[ Plan ]#(plan)\n- ( ) My task\n");

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

    #[test]
    fn enforces_blueprint_section_rule_constraint() {
        let blueprint =
            parse("::blueprint(task-list)\n\n=[ Tasks ]#(tasks):rule(allow: list(task))\n");
        // doc has a section with a heading that is not allowed
        let doc_bad =
            parse("::kind(task-list)\n\n=[ Tasks ]#(tasks)\n@heading[ Not allowed here ]\n");
        let errors = validate_against_blueprint(&doc_bad, &blueprint);
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            &errors[0],
            Diagnostic::DisallowedByRule { name, .. } if name == "heading"
        ));

        // doc with only allowed elements
        let doc_good = parse("::kind(task-list)\n\n=[ Tasks ]#(tasks)\n@task[ Do something ]\n");
        let errors_good = validate_against_blueprint(&doc_good, &blueprint);
        assert!(
            errors_good.is_empty(),
            "expected no errors, got: {errors_good:?}"
        );
    }
}
