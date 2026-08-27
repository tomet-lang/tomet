//! Whitespace-hygiene formatter for `.tm` source text, plus an opt-in,
//! config-gated pass for small structural additions.
//!
//! [`format_source`] is AST-aware using source [`typedmark_ast::Span`]
//! metadata. Normalizes whitespace policy (LF line endings, no trailing
//! whitespace, collapsed excess blank lines, exactly one final newline)
//! while losslessly preserving literal whitespace and blank lines inside
//! raw/verbatim content (`<codeblock>[...]` and elements opting in via
//! `content:raw`). It never changes the parsed `Document` (see the
//! `does_not_change_the_parsed_document` test) -- with one deliberate
//! exception: [`quote_bare_at_yaml_values`], run first, wraps a bare
//! `@...`-led value inside any `(...format:yaml...){...}` body in `""`.
//! Unquoted, `@` is a reserved YAML indicator that can't start a plain
//! scalar (`embedded_format.rs` hands the body straight to `serde_yaml`,
//! which rejects it outright), so that shape doesn't have a successfully-
//! parsed `Document` to preserve in the first place -- this pass turns
//! an unparseable file into a parseable one with the obvious intended
//! reading, it doesn't change what an already-valid file means. Raw-text
//! based rather than AST-based for exactly that reason: there's nothing
//! to walk yet for the file this exists to fix.
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
    let normalized = quote_bare_at_yaml_values(&normalized);
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

/// Finds every `(...){...}` body whose args declare `format:yaml` and
/// wraps any bare, unquoted `@...`-led value inside it in `"..."` -- see
/// the module doc comment for why. Raw-text scanning, not a real YAML
/// parse: cheap, and this only ever needs to recognize three "a value
/// starts here" shapes (`key:`, `- `, and `[`/`,` inside a flow
/// sequence), not the whole YAML grammar.
fn quote_bare_at_yaml_values(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut pos = 0;
    while let Some(rel_open) = find_next_format_yaml_body_start(&src[pos..]) {
        let open = pos + rel_open;
        out.push_str(&src[pos..open]);
        let Some(close) = find_matching(src, open, '{', '}') else {
            // Unterminated `{` -- some other problem already makes this
            // file unparseable; not this pass's job to diagnose that, so
            // just copy the rest through untouched.
            out.push_str(&src[open..]);
            return out;
        };
        out.push('{');
        quote_bare_at_values_in(src, open + 1, close, &mut out);
        out.push('}');
        pos = close + 1;
    }
    out.push_str(&src[pos..]);
    out
}

/// Byte offset (relative to `s`) of the `{` opening the next
/// `format:yaml`-tagged element's value body, or `None` if there isn't
/// one.
fn find_next_format_yaml_body_start(s: &str) -> Option<usize> {
    let mut search_from = 0;
    loop {
        let rel_open_paren = s[search_from..].find('(')?;
        let open_paren = search_from + rel_open_paren;
        let Some(close_paren) = find_matching(s, open_paren, '(', ')') else {
            search_from = open_paren + 1;
            continue;
        };
        if args_declare_format_yaml(&s[open_paren + 1..close_paren]) {
            let bytes = s.as_bytes();
            let mut i = close_paren + 1;
            while i < s.len() && matches!(bytes[i], b' ' | b'\t' | b'\n' | b'\r') {
                i += 1;
            }
            if i < s.len() && bytes[i] == b'{' {
                return Some(i);
            }
        }
        search_from = close_paren + 1;
    }
}

/// Whether an element's `(args)` body (already stripped of the
/// enclosing parens) declares a `format` key of `yaml`, e.g.
/// `format:yaml` or `id:foo, format: yaml`. Deliberately not full
/// `value.rs`-grade args parsing -- `format:yaml` args are always this
/// simple in practice, and this is only a gate for the raw-text scan
/// above, not something anything downstream relies on for correctness.
fn args_declare_format_yaml(args: &str) -> bool {
    args.split(',').any(|entry| {
        entry
            .split_once(':')
            .is_some_and(|(key, value)| key.trim() == "format" && value.trim() == "yaml")
    })
}

/// Byte offset of the `close` matching the `open` at `open_pos` in
/// `src` (which must be `open`), tracking nested `open`/`close` depth
/// and skipping over `"..."`/`'...'` quoted runs (so a stray bracket
/// character inside a string doesn't miscount). Mirrors
/// `typedmark-syntax-parser::value::find_matching_delimiter`'s
/// algorithm (not reusable directly -- that one is `pub(crate)` and
/// works off this crate's own `Cursor` type). Byte-indexed but UTF-8
/// safe: every character this function's own logic branches on (`"`,
/// `'`, `\`, `open`, `close`) is ASCII, so walking any other byte one at
/// a time never lands on -- or returns -- a non-boundary offset.
fn find_matching(src: &str, open_pos: usize, open: char, close: char) -> Option<usize> {
    let bytes = src.as_bytes();
    let mut i = open_pos + open.len_utf8();
    let mut depth: u32 = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'"' || c == b'\'' {
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' && c == b'"' && i + 1 < bytes.len() {
                    i += 2;
                    continue;
                }
                let closed = bytes[i] == c;
                i += 1;
                if closed {
                    break;
                }
            }
        } else if c == open as u8 {
            depth += 1;
            i += 1;
        } else if c == close as u8 {
            if depth == 0 {
                return Some(i);
            }
            depth -= 1;
            i += 1;
        } else {
            i += 1;
        }
    }
    None
}

/// Does the actual `@...` -> `"@..."` rewriting within one already-
/// located `format:yaml` body's span `[start, end)`, appending to `out`.
/// Skips existing `"..."`/`'...'` strings and `#...` comments verbatim
/// (so it never touches an `@` that's already quoted, or one that's
/// just commentary) and only quotes a value immediately after `key:`,
/// `- `, or `[`/`,` (a flow-sequence item) -- anywhere else, a leading
/// `@` is left alone.
fn quote_bare_at_values_in(src: &str, start: usize, end: usize, out: &mut String) {
    let bytes = src.as_bytes();
    let mut i = start;
    while i < end {
        let c = bytes[i];
        match c {
            b'#' => {
                let line_end = src[i..end].find('\n').map_or(end, |p| i + p);
                out.push_str(&src[i..line_end]);
                i = line_end;
            }
            b'"' | b'\'' => {
                let quote = c;
                let str_start = i;
                i += 1;
                while i < end {
                    if bytes[i] == b'\\' && quote == b'"' && i + 1 < end {
                        i += 2;
                        continue;
                    }
                    let closed = bytes[i] == quote;
                    i += 1;
                    if closed {
                        break;
                    }
                }
                out.push_str(&src[str_start..i]);
            }
            b':' | b'-' | b',' | b'[' => {
                out.push(c as char);
                i += 1;
                let ws_start = i;
                while i < end && matches!(bytes[i], b' ' | b'\t') {
                    i += 1;
                }
                out.push_str(&src[ws_start..i]);
                if i < end && bytes[i] == b'@' {
                    let val_start = i;
                    let mut j = i;
                    while j < end && !matches!(bytes[j], b',' | b']' | b'}' | b'\n' | b'#') {
                        j += 1;
                    }
                    let mut val_end = j;
                    while val_end > val_start && matches!(bytes[val_end - 1], b' ' | b'\t') {
                        val_end -= 1;
                    }
                    out.push('"');
                    for ch in src[val_start..val_end].chars() {
                        match ch {
                            '"' => out.push_str("\\\""),
                            '\\' => out.push_str("\\\\"),
                            _ => out.push(ch),
                        }
                    }
                    out.push('"');
                    out.push_str(&src[val_end..j]);
                    i = j;
                }
            }
            _ => {
                let ch = src[i..].chars().next().expect("i < end <= src.len()");
                out.push(ch);
                i += ch.len_utf8();
            }
        }
    }
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
    // A list item is just `Element{ sigil: Bare, .. }` nested inside its
    // list's `ElementValue::Children`, so `walk_element` already reaches
    // every item's own `content` through the `Children` branch below --
    // no separate list-shaped case is needed here anymore. `el.children`
    // (an item's own nested sub-list) is the one shape `Children`/
    // `content` don't cover, so it gets its own block-level recursion.
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
        if let Some(children) = &el.children {
            for child in children {
                walk_block(child, out);
            }
        }
        if let Some(ElementValue::Children(children)) = &el.value {
            for child in children {
                walk_element(child, out);
            }
        }
    }

    fn walk_block(block: &Block, out: &mut Vec<(usize, usize)>) {
        match block {
            Block::Paragraph(p) => {
                for inline in &p.content {
                    if let Inline::Element(el) = inline {
                        walk_element(el, out);
                    }
                }
            }
            Block::Element(el) => walk_element(el, out),
        }
    }

    for block in &doc.blocks {
        walk_block(block, out);
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
    fn quotes_bare_at_led_yaml_values_so_the_document_parses() {
        // Unquoted, `@link(...)` is invalid YAML (`@` is a reserved
        // indicator) -- `parse_document` fails outright on this input, so
        // this specifically checks the *raw string* the formatter
        // produces, not a before/after `Document` comparison the way
        // `does_not_change_the_parsed_document` does (there is no
        // "before" `Document` here to compare against).
        let src = "@meta(format:yaml){\n  previous: @link(ref:x)\n  next: @link(ref:y)\n  parent: [@link(ref:z), @link(ref:w)]\n}\n";
        let expected = "@meta(format:yaml){\n  previous: \"@link(ref:x)\"\n  next: \"@link(ref:y)\"\n  parent: [\"@link(ref:z)\", \"@link(ref:w)\"]\n}\n";
        let out = format_source(src);
        assert_eq!(out, expected);
        typedmark_parser::parse_document(&out)
            .unwrap_or_else(|e| panic!("formatted output should now parse: {e}"));
    }

    #[test]
    fn quote_bare_at_yaml_values_handles_block_list_items() {
        let src =
            "@meta(format:yaml){\n  refs:\n    - @link(ref:x)\n    - already \"@link(ref:y)\"\n}\n";
        let out = format_source(src);
        assert!(out.contains("- \"@link(ref:x)\""));
        assert!(
            out.contains("- already \"@link(ref:y)\""),
            "existing quote untouched: {out:?}"
        );
    }

    #[test]
    fn quote_bare_at_yaml_values_leaves_already_quoted_and_unrelated_content_alone() {
        // Already-quoted values, and `@` outside a `format:yaml` body
        // entirely (an ordinary `@name` element), must be left untouched.
        let src =
            "@meta(format:yaml){\n  previous: \"@link(ref:x)\"\n}\n\n@link(ref:x)[some text]\n";
        assert_eq!(format_source(src), src);
    }

    #[test]
    fn quote_bare_at_yaml_values_is_idempotent() {
        let src = "@meta(format:yaml){\n  previous: @link(ref:x)\n}\n";
        let once = format_source(src);
        let twice = format_source(&once);
        assert_eq!(once, twice);
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
