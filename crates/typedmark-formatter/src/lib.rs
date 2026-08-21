//! Whitespace-hygiene formatter for `.tm` source text, plus an opt-in,
//! config-gated pass for small structural additions.
//!
//! [`format_source`] is AST-aware using source [`typedmark_ast::Span`]
//! metadata. Normalizes whitespace policy (LF line endings, no trailing
//! whitespace, collapsed excess blank lines, exactly one final newline)
//! while losslessly preserving literal whitespace and blank lines inside
//! raw/verbatim content (`<codeblock>[...]` and elements opting in via
//! `content:raw`). It never changes the parsed `Document` (see the
//! `does_not_change_the_parsed_document` test).
//!
//! [`format_source_with_config`] adds a config-driven patch pass in
//! front of [`format_source`]: if the given `typedmark_config::PrinterConfig`
//! has no relevant rule for a document, it degrades to exactly
//! [`format_source`]'s behavior (no semantic change). Only when config
//! opts in does it make a deliberate, structural change, mirroring
//! `typedmark-printer`'s `ensure_document_id_with_config` -- inserting a
//! brand-new `@meta{id: ...}` block into documents that don't have one
//! yet, or patching an `id` into/onto an existing one -- but by
//! splicing plain text into the original source (via
//! `typedmark-style`'s single-element `render_meta_element`, at the
//! target element's `Span`) rather than rebuilding the whole document
//! from the AST (that full-rebuild approach is `typedmark-printer`'s
//! job, not this crate's -- see `docs/develop/architecture.md`'s
//! `typedmark-formatter` bullet for why). Note: when config *is*
//! relevant, the patched meta block is rendered exactly like printer
//! would -- e.g. it can turn a hand-written single-line `@meta{id: ...}`
//! into a multi-line `@meta(format:yaml){...}` if `config.meta_format`
//! says so, even though nothing else about that decision changed. This
//! is intentional, not a lossiness bug: the "don't change what wasn't
//! asked for" guarantee is about the *absence* of a matching config
//! rule, not about preserving a touched element's original shape.

use typedmark_ast::{Block, Document, Element, ElementValue, Inline, Sigil, Value};
use typedmark_config::{FieldConfig, PrinterConfig};
use typedmark_field_utils::{generate_id_for_field, is_valid_id_format};
use typedmark_parser::parse_document;

/// Format `src` in place (returns a new `String`). Idempotent:
/// `format_source(&format_source(src)) == format_source(src)`.
pub fn format_source(src: &str) -> String {
    let normalized = src.replace("\r\n", "\n").replace('\r', "\n");
    if normalized.trim().is_empty() {
        return String::new();
    }

    let mut raw_spans = Vec::new();
    if let Ok(doc) = parse_document(&normalized) {
        collect_raw_spans(&doc, &mut raw_spans);
    }

    let is_offset_raw = |offset: usize| -> bool {
        raw_spans
            .iter()
            .any(|&(start, end)| offset >= start && offset < end)
    };

    let mut out_lines: Vec<String> = Vec::new();
    let mut pending_blank = false;
    let mut current_offset = 0;

    for line in normalized.split('\n') {
        let line_len = line.len();
        let line_end_offset = current_offset + line_len;

        // Check if any byte in this line falls inside raw content.
        let is_raw_line = (current_offset..=line_end_offset).any(is_offset_raw);

        if is_raw_line {
            if pending_blank && !out_lines.is_empty() {
                out_lines.push(String::new());
            }
            pending_blank = false;
            out_lines.push(line.to_string());
        } else {
            let trimmed = line.trim_end_matches([' ', '\t']);
            if trimmed.is_empty() {
                pending_blank = true;
            } else {
                if pending_blank && !out_lines.is_empty() {
                    out_lines.push(String::new());
                }
                pending_blank = false;
                out_lines.push(trimmed.to_string());
            }
        }

        // +1 accounts for the newline character in the normalized string
        current_offset += line_len + 1;
    }

    if out_lines.is_empty() {
        return String::new();
    }

    let mut out = out_lines.join("\n");
    out.push('\n');
    out
}

/// Like [`format_source`], but first runs a config-gated patch pass --
/// see the module doc comment and `docs/develop/architecture.md`'s
/// `typedmark-formatter` bullet for the design.
pub fn format_source_with_config(src: &str, config: &PrinterConfig) -> String {
    let Some(id_cfg) = config.meta_fields.get("id") else {
        return format_source(src);
    };
    if id_cfg.field_type.is_none() {
        return format_source(src);
    }
    let force = id_cfg.force.unwrap_or(true);
    let overwrite = id_cfg.overwrite.unwrap_or(false);
    if !(force || overwrite) {
        return format_source(src);
    }

    let Ok(doc) = parse_document(src) else {
        return format_source(src);
    };

    let meta_el = doc.blocks.iter().find_map(|block| {
        if let Block::Element(el) = block {
            if typedmark_semantics::classify(el) == typedmark_semantics::ElementKind::Meta
                || matches!(&el.sigil, Sigil::At(Some(name)) if name == "meta")
            {
                return Some(el);
            }
        }
        None
    });

    let combined = match meta_el {
        Some(el) => match patch_existing_meta(el, id_cfg, overwrite, config) {
            Some(rendered) => {
                let start = el.span.start.offset;
                let end = el.span.end.offset;
                format!("{}{}{}", &src[..start], rendered, &src[end..])
            }
            None => return format_source(src),
        },
        None => {
            let id = generate_id_for_field(id_cfg);
            let mut new_el = Element::new(Sigil::At(Some("meta".to_string())));
            new_el.value = Some(ElementValue::Data(Value::Map(vec![(
                "id".to_string(),
                Value::String(id),
            )])));
            let rendered = typedmark_style::render_meta_element(&new_el, config);
            format!("{rendered}\n\n{src}")
        }
    };

    format_source(&combined)
}

/// Decides whether `el` (an existing `@meta` element) needs its `id`
/// field inserted or replaced, mirroring the per-case gating in
/// `typedmark-printer`'s `ensure_document_id_with_config` (existing
/// valid id -> untouched; existing invalid id -> replaced only if
/// `overwrite`; no id field -> inserted, since the caller already
/// checked `force || overwrite` before calling this). Returns the
/// freshly rendered `@meta{...}` text if a change is needed, `None` if
/// nothing needs to change (caller should leave `src` untouched).
fn patch_existing_meta(
    el: &Element,
    id_cfg: &FieldConfig,
    overwrite: bool,
    config: &PrinterConfig,
) -> Option<String> {
    let Some(ElementValue::Data(Value::Map(entries))) = &el.value else {
        return None;
    };
    let mut entries = entries.clone();
    let existing_idx = entries.iter().position(|(k, _)| k == "id");
    let changed = match existing_idx {
        Some(idx) => {
            if overwrite {
                let existing_str = match &entries[idx].1 {
                    Value::String(s) => s.as_str(),
                    _ => "",
                };
                if !is_valid_id_format(existing_str, id_cfg) {
                    entries[idx].1 = Value::String(generate_id_for_field(id_cfg));
                    true
                } else {
                    false
                }
            } else {
                false
            }
        }
        None => {
            entries.insert(
                0,
                (
                    "id".to_string(),
                    Value::String(generate_id_for_field(id_cfg)),
                ),
            );
            true
        }
    };
    if !changed {
        return None;
    }
    let mut modified = el.clone();
    modified.value = Some(ElementValue::Data(Value::Map(entries)));
    Some(typedmark_style::render_meta_element(&modified, config))
}

fn is_raw_element(el: &Element) -> bool {
    if matches!(&el.sigil, Sigil::Type(name) if name == "codeblock") {
        return true;
    }
    if let Some(Value::Map(entries)) = &el.args {
        if entries
            .iter()
            .any(|(k, v)| k == "content" && matches!(v, Value::String(s) if s == "raw"))
        {
            return true;
        }
    }
    false
}

fn collect_raw_spans(doc: &Document, out: &mut Vec<(usize, usize)>) {
    fn walk_element(el: &Element, out: &mut Vec<(usize, usize)>) {
        if is_raw_element(el) {
            if let Some(inlines) = &el.content {
                for inline in inlines {
                    let span = inline.span();
                    if span.start.offset < span.end.offset {
                        out.push((span.start.offset, span.end.offset));
                    }
                }
            }
        }
        if let Some(inlines) = &el.content {
            for inline in inlines {
                if let Inline::Element(child_el) = inline {
                    walk_element(child_el, out);
                }
            }
        }
        if let Some(ElementValue::Children(children)) = &el.value {
            for child in children {
                walk_element(child, out);
            }
        }
    }

    for block in &doc.blocks {
        match block {
            Block::Paragraph(p) => {
                for inline in &p.content {
                    if let Inline::Element(el) = inline {
                        walk_element(el, out);
                    }
                }
            }
            Block::Heading(h) => {
                for inline in &h.content {
                    if let Inline::Element(el) = inline {
                        walk_element(el, out);
                    }
                }
            }
            Block::List(list) => {
                for item in &list.items {
                    for inline in &item.content {
                        if let Inline::Element(el) = inline {
                            walk_element(el, out);
                        }
                    }
                }
            }
            Block::Element(el) => {
                walk_element(el, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use typedmark_config::FieldConfig;

    #[test]
    fn format_source_with_config_is_noop_without_id_rule() {
        let src = "#[ Hello ]\n\n- one\n- two\n";
        let config = PrinterConfig::default();
        assert_eq!(format_source_with_config(src, &config), format_source(src));
    }

    #[test]
    fn format_source_with_config_inserts_new_meta_block_with_id() {
        let src = "#[ Hello ]\n\nSome body text.\n";
        let mut config = PrinterConfig::default();
        config.meta_fields.insert(
            "id".to_string(),
            FieldConfig {
                field_type: Some("nanoid".to_string()),
                length: Some(8),
                prefix: Some("doc-".to_string()),
                ..Default::default()
            },
        );

        let out = format_source_with_config(src, &config);
        assert!(out.contains("@meta{id: doc-"));
        assert!(out.contains("#[ Hello ]"));
        assert!(out.contains("Some body text."));
    }

    #[test]
    fn format_source_with_config_skips_insertion_without_force_or_overwrite() {
        let src = "#[ Hello ]\n\nSome body text.\n";
        let mut config = PrinterConfig::default();
        config.meta_fields.insert(
            "id".to_string(),
            FieldConfig {
                field_type: Some("nanoid".to_string()),
                length: Some(8),
                force: Some(false),
                overwrite: Some(false),
                ..Default::default()
            },
        );

        assert_eq!(format_source_with_config(src, &config), format_source(src));
    }

    #[test]
    fn format_source_with_config_respects_meta_format() {
        let src = "#[ Hello ]\n\nSome body text.\n";
        let mut config = PrinterConfig::default();
        config.meta_format = Some("yaml".to_string());
        config.meta_fields.insert(
            "id".to_string(),
            FieldConfig {
                field_type: Some("nanoid".to_string()),
                length: Some(8),
                ..Default::default()
            },
        );

        let out = format_source_with_config(src, &config);
        assert!(out.contains("@meta(format:yaml){\n  id: "));
    }

    #[test]
    fn format_source_with_config_is_idempotent() {
        let src = "#[ Hello ]\n\nSome body text.\n";
        let mut config = PrinterConfig::default();
        config.meta_fields.insert(
            "id".to_string(),
            FieldConfig {
                field_type: Some("nanoid".to_string()),
                length: Some(8),
                ..Default::default()
            },
        );

        let once = format_source_with_config(src, &config);
        let twice = format_source_with_config(&once, &config);
        assert_eq!(once, twice);
    }

    fn id_config() -> PrinterConfig {
        let mut config = PrinterConfig::default();
        config.meta_fields.insert(
            "id".to_string(),
            FieldConfig {
                field_type: Some("nanoid".to_string()),
                length: Some(8),
                prefix: Some("doc-".to_string()),
                ..Default::default()
            },
        );
        config
    }

    #[test]
    fn format_source_with_config_inserts_id_into_existing_meta_block() {
        let src = "@meta{title: Hello}\n\n#[ Hello ]\n\nSome body text.\n";
        let config = id_config();

        let out = format_source_with_config(src, &config);
        assert!(out.contains("title: Hello"));
        assert!(out.contains("id: doc-"));
        assert!(out.contains("#[ Hello ]"));
        assert!(out.contains("Some body text."));
    }

    #[test]
    fn format_source_with_config_replaces_invalid_id_when_overwrite() {
        let src = "@meta{id: not-valid, title: Hello}\n\n#[ Hello ]\n";
        let mut config = id_config();
        if let Some(id_cfg) = config.meta_fields.get_mut("id") {
            id_cfg.overwrite = Some(true);
        }

        let out = format_source_with_config(src, &config);
        assert!(!out.contains("id: not-valid"));
        assert!(out.contains("id: doc-"));
        assert!(out.contains("title: Hello"));
    }

    #[test]
    fn format_source_with_config_leaves_invalid_id_without_overwrite() {
        let src = "@meta{id: not-valid, title: Hello}\n\n#[ Hello ]\n";
        let config = id_config(); // overwrite defaults to false

        assert_eq!(format_source_with_config(src, &config), format_source(src));
    }

    #[test]
    fn format_source_with_config_reformats_existing_block_to_multiline_per_config() {
        let src = "@meta{title: Hello}\n\n#[ Hello ]\n";
        let mut config = id_config();
        config.meta_format = Some("yaml".to_string());

        let out = format_source_with_config(src, &config);
        assert!(out.contains("@meta(format:yaml){\n"));
        assert!(out.contains("  title: Hello\n"));
        assert!(out.contains("  id: doc-"));
    }

    #[test]
    fn format_source_with_config_existing_block_patch_is_idempotent() {
        let src = "@meta{title: Hello}\n\n#[ Hello ]\n";
        let config = id_config();

        let once = format_source_with_config(src, &config);
        let twice = format_source_with_config(&once, &config);
        assert_eq!(once, twice);
    }

    #[test]
    fn strips_trailing_whitespace() {
        assert_eq!(format_source("a  \nb\t\n"), "a\nb\n");
    }

    #[test]
    fn normalizes_line_endings() {
        assert_eq!(format_source("a\r\nb\r\n"), "a\nb\n");
        assert_eq!(format_source("a\rb\r"), "a\nb\n");
    }

    #[test]
    fn drops_leading_blank_lines() {
        assert_eq!(format_source("\n\n\na\n"), "a\n");
    }

    #[test]
    fn collapses_multiple_blank_lines_to_one() {
        assert_eq!(format_source("a\n\n\n\nb\n"), "a\n\nb\n");
    }

    #[test]
    fn drops_trailing_blank_lines_and_ensures_one_final_newline() {
        assert_eq!(format_source("a\n\n\n"), "a\n");
        assert_eq!(format_source("a"), "a\n");
    }

    #[test]
    fn empty_input_stays_empty() {
        assert_eq!(format_source(""), "");
        assert_eq!(format_source("\n\n  \n"), "");
    }

    #[test]
    fn already_formatted_input_is_unchanged() {
        let src = "#[ Hello ]\n\n- one\n- two\n";
        assert_eq!(format_source(src), src);
    }

    #[test]
    fn raw_content_is_preserved_losslessly() {
        let src = "<memo>(content:raw)[\nline one  \n\nline two\n]\n";
        assert_eq!(format_source(src), src);
    }

    #[test]
    fn codeblock_content_is_preserved_losslessly() {
        let src = "<codeblock>(lang:rust)[\nfn foo() {\n    let a = 1;  \n\n    let b = 2;\n}\n]\n";
        assert_eq!(format_source(src), src);
    }

    #[test]
    fn fenced_code_block_content_is_preserved_losslessly() {
        let src = "```rust\nfn foo() {\n    let a = 1;  \n\n    let b = 2;\n}\n```\n";
        assert_eq!(format_source(src), src);
    }

    #[test]
    fn is_idempotent_on_repo_spec_examples() {
        for src in repo_examples() {
            let once = format_source(src);
            let twice = format_source(&once);
            assert_eq!(once, twice, "formatting is not idempotent for: {src:?}");
        }
    }

    #[test]
    fn does_not_change_the_parsed_document() {
        for src in [
            include_str!("../../../docs/tests/readme.ja.tm"),
            include_str!("../../../docs/tests/tmt/examples/image.meta.tm"),
            include_str!("../../../docs/tests/roadmap.ja.tm"),
        ] {
            let before = typedmark_parser::parse_document(src)
                .unwrap_or_else(|e| panic!("fixture failed to parse: {e}"));
            let formatted = format_source(src);
            let after = typedmark_parser::parse_document(&formatted)
                .unwrap_or_else(|e| panic!("formatted output failed to parse: {e}"));
            assert_eq!(before, after, "formatting changed the parsed document");
        }
    }

    fn repo_examples() -> Vec<&'static str> {
        vec![
            include_str!("../../../docs/tests/readme.ja.tm"),
            include_str!("../../../docs/tests/tmt/examples/image.meta.tm"),
            include_str!("../../../docs/tests/ja/cheatsheet.tm"),
            include_str!("../../../docs/tests/roadmap.ja.tm"),
        ]
    }
}
