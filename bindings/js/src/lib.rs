use wasm_bindgen::prelude::*;

/// Parse `.tmt` markup source text into a JavaScript `Document` AST object.
#[wasm_bindgen(js_name = parseDocument)]
pub fn parse_document(source: &str) -> Result<JsValue, JsValue> {
    let doc =
        tomet_parser::parse_document(source).map_err(|e| JsValue::from_str(&e.to_string()))?;
    serde_wasm_bindgen::to_value(&doc).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Parse a data-only `.tmt` document into a native JavaScript object/primitive.
#[wasm_bindgen(js_name = parseValue)]
pub fn parse_value(source: &str) -> Result<JsValue, JsValue> {
    let val = tomet_parser::parse_value(source).map_err(|e| JsValue::from_str(&e.to_string()))?;
    serde_wasm_bindgen::to_value(&val).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Options controlling HTML rendering and document processing.
#[derive(serde::Serialize, serde::Deserialize, Default, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProcessOptions {
    pub advanced: Option<bool>,
    #[serde(alias = "number_headings")]
    pub number_headings: Option<bool>,
    #[serde(alias = "auto_slug_headings")]
    pub auto_slug_headings: Option<bool>,
    pub lang: Option<String>,
    #[serde(alias = "current_path")]
    pub current_path: Option<String>,
    #[serde(alias = "url_prefix")]
    pub url_prefix: Option<String>,
    #[serde(alias = "asset_prefix")]
    pub asset_prefix: Option<String>,
    #[serde(alias = "vault_files")]
    pub vault_files: Option<Vec<String>>,
    pub config: Option<String>,
}

impl ProcessOptions {
    fn to_render_options(&self) -> tomet_html::RenderOptions {
        let advanced = self.advanced.unwrap_or(false);
        tomet_html::RenderOptions {
            number_headings: self.number_headings.unwrap_or(advanced),
            auto_slug_headings: self.auto_slug_headings.unwrap_or(advanced),
            lang: self.lang.clone(),
            ..Default::default()
        }
    }
}

use std::sync::Mutex;
use tomet_links::VaultLinkIndex;

static GLOBAL_VAULT_INDEX: Mutex<Option<VaultLinkIndex>> = Mutex::new(None);

/// Pre-builds and caches the link resolution index in Wasm memory once for the entire vault.
#[wasm_bindgen(js_name = setVaultFiles)]
pub fn set_vault_files(files: Vec<String>) {
    let index = VaultLinkIndex::from_paths(&files);
    let mut lock = GLOBAL_VAULT_INDEX.lock().unwrap();
    *lock = Some(index);
}

/// Clears the cached link resolution index.
#[wasm_bindgen(js_name = clearVaultFiles)]
pub fn clear_vault_files() {
    let mut lock = GLOBAL_VAULT_INDEX.lock().unwrap();
    *lock = None;
}

fn prepare_document(mut doc: tomet_ast::Document, opts: &ProcessOptions) -> tomet_ast::Document {
    // 1. Inject external workspace config (e.g. default.config.tmt) if provided
    if let Some(cfg_src) = &opts.config {
        if let Ok(cfg_doc) = tomet_parser::parse_document(cfg_src) {
            let mut prefix_blocks = Vec::new();
            for block in cfg_doc.blocks {
                if let tomet_ast::Block::Element(el) = &block {
                    let kind = tomet_semantics::classify_std_lenient(el);
                    if kind == tomet_semantics::ElementKind::Config || kind.as_str() == "settings" {
                        prefix_blocks.push(block);
                    }
                }
            }
            if !prefix_blocks.is_empty() {
                prefix_blocks.append(&mut doc.blocks);
                doc.blocks = prefix_blocks;
            }
        }
    }

    // 2. Expand macros in element arguments and interpolation expressions
    let config = tomet_semantics::document_config(&doc);
    tomet_transform::expand_document_macros(&mut doc, &config);

    // 3. Resolve links against cached index or vault_files option
    let lock = GLOBAL_VAULT_INDEX.lock().unwrap();
    let maybe_index = lock.as_ref();

    let from_path = opts.current_path.as_deref().map(std::path::Path::new);
    let mode = tomet_transform::TargetMode::WebSlug {
        url_prefix: opts.url_prefix.clone().unwrap_or_else(|| "/docs".into()),
        asset_prefix: opts.asset_prefix.clone().unwrap_or_else(|| "/vault".into()),
    };

    if let Some(index) = maybe_index {
        tomet_transform::resolve_document_links(
            &mut doc,
            from_path,
            |target, from| index.resolve_ref(target, from).map(|p| p.to_path_buf()),
            &mode,
        );
    } else if let Some(files) = &opts.vault_files {
        let index = VaultLinkIndex::from_paths(files);
        tomet_transform::resolve_document_links(
            &mut doc,
            from_path,
            |target, from| index.resolve_ref(target, from).map(|p| p.to_path_buf()),
            &mode,
        );
    }
    doc
}

/// An entry in the document's table of contents.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct TocItem {
    pub id: String,
    pub level: u32,
    pub text: String,
}

/// High-level output of processing a `.tmt` document.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ProcessedDoc {
    pub title: Option<String>,
    pub html: String,
    pub meta: Option<tomet_ast::Value>,
    pub toc: Vec<TocItem>,
    pub is_data_only: bool,
}

pub fn process_document_internal(
    mut doc: tomet_ast::Document,
    opts: &ProcessOptions,
) -> ProcessedDoc {
    doc = prepare_document(doc, opts);
    let render_opts = opts.to_render_options();
    let html = tomet_html::render_body_with(&doc, &render_opts);
    let is_data_only = html.trim().is_empty();

    // 1. Meta & Title from @meta{title}
    let meta = tomet_semantics::document_meta(&doc);
    let meta_title = meta.as_ref().and_then(|v| match v {
        tomet_ast::Value::Map(m) => m.iter().find_map(|(k, val)| {
            if k == "title" {
                match val {
                    tomet_ast::Value::String(s) => Some(s.clone()),
                    _ => None,
                }
            } else {
                None
            }
        }),
        _ => None,
    });

    // 2. Headings and TOC from HTML
    let mut first_h1: Option<String> = None;
    let mut toc = Vec::new();

    use std::sync::LazyLock;
    static HEADING_RE: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r#"<h([1-6])\b([^>]*)>([\s\S]*?)</h([1-6])>"#).unwrap());
    static ID_RE: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r#"id="([^"]*)""#).unwrap());
    static HEADING_NUMBER_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r#"<span class="tm-heading-number">[^<]*</span>"#).unwrap()
    });
    static TAG_RE: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r#"<[^>]+>"#).unwrap());

    for cap in HEADING_RE.captures_iter(&html) {
        let open_level: u32 = cap[1].parse().unwrap_or(1);
        let close_level: u32 = cap[4].parse().unwrap_or(1);
        if open_level != close_level {
            continue;
        }
        let level = open_level;
        let attrs = &cap[2];
        let inner = &cap[3];

        let id = ID_RE
            .captures(attrs)
            .map(|c| c[1].to_string())
            .unwrap_or_default();
        let cleaned = HEADING_NUMBER_RE.replace_all(inner, "");
        let text = TAG_RE.replace_all(&cleaned, "").trim().to_string();

        if level == 1 && first_h1.is_none() && !text.is_empty() {
            first_h1 = Some(text.clone());
        }
        if (level == 2 || level == 3) && !text.is_empty() {
            toc.push(TocItem { id, level, text });
        }
    }

    let title = meta_title.or(first_h1);

    ProcessedDoc {
        title,
        html,
        meta,
        toc,
        is_data_only,
    }
}

/// Convert `.tmt` source text or a `Document` AST object into an HTML body string.
#[wasm_bindgen(js_name = toHtml)]
pub fn to_html(source_or_doc: &JsValue, options: Option<JsValue>) -> Result<String, JsValue> {
    let opts: ProcessOptions = if let Some(opts_val) = options {
        serde_wasm_bindgen::from_value(opts_val).map_err(|e| JsValue::from_str(&e.to_string()))?
    } else {
        ProcessOptions::default()
    };
    let render_opts = opts.to_render_options();

    let mut doc = if let Some(src) = source_or_doc.as_string() {
        tomet_parser::parse_document(&src).map_err(|e| JsValue::from_str(&e.to_string()))?
    } else {
        let doc: tomet_ast::Document = serde_wasm_bindgen::from_value(source_or_doc.clone())
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        doc
    };
    doc = prepare_document(doc, &opts);
    Ok(tomet_html::render_body_with(&doc, &render_opts))
}

/// Process `.tmt` markup source text or a `Document` AST object into HTML, metadata, title, and TOC.
#[wasm_bindgen(js_name = processDocument)]
pub fn process_document(
    source_or_doc: &JsValue,
    options: Option<JsValue>,
) -> Result<JsValue, JsValue> {
    let opts: ProcessOptions = if let Some(opts_val) = options {
        serde_wasm_bindgen::from_value(opts_val).map_err(|e| JsValue::from_str(&e.to_string()))?
    } else {
        ProcessOptions::default()
    };

    let processed = if let Some(src) = source_or_doc.as_string() {
        let doc =
            tomet_parser::parse_document(&src).map_err(|e| JsValue::from_str(&e.to_string()))?;
        process_document_internal(doc, &opts)
    } else {
        let doc: tomet_ast::Document = serde_wasm_bindgen::from_value(source_or_doc.clone())
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        process_document_internal(doc, &opts)
    };

    serde_wasm_bindgen::to_value(&processed).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Convert `.tmt` source text or a `Document` AST object into a CommonMark Markdown string.
#[wasm_bindgen(js_name = toMarkdown)]
pub fn to_markdown(source_or_doc: &JsValue) -> Result<String, JsValue> {
    if let Some(src) = source_or_doc.as_string() {
        let doc =
            tomet_parser::parse_document(&src).map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(tomet_markdown::to_markdown(&doc))
    } else {
        let doc: tomet_ast::Document = serde_wasm_bindgen::from_value(source_or_doc.clone())
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(tomet_markdown::to_markdown(&doc))
    }
}

/// Convert `.tmt` source text or a `Document` AST object into a Typst markup string.
#[wasm_bindgen(js_name = toTypst)]
pub fn to_typst(source_or_doc: &JsValue) -> Result<String, JsValue> {
    if let Some(src) = source_or_doc.as_string() {
        let doc =
            tomet_parser::parse_document(&src).map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(tomet_typst::to_typst(&doc))
    } else {
        let doc: tomet_ast::Document = serde_wasm_bindgen::from_value(source_or_doc.clone())
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(tomet_typst::to_typst(&doc))
    }
}

/// Parse CommonMark Markdown text into a `Document` AST object.
#[wasm_bindgen(js_name = fromMarkdown)]
pub fn from_markdown(markdown: &str) -> Result<JsValue, JsValue> {
    let doc = tomet_markdown::from_markdown(markdown);
    serde_wasm_bindgen::to_value(&doc).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Serialize a `Document` AST object back into formatted `.tmt` source code.
#[wasm_bindgen(js_name = printDocument)]
pub fn print_document(doc_val: &JsValue) -> Result<String, JsValue> {
    let doc: tomet_ast::Document = serde_wasm_bindgen::from_value(doc_val.clone())
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(tomet_printer::document_to_tm(&doc))
}

/// Format `.tmt` source text with lossless whitespace hygiene and span preservation.
#[wasm_bindgen(js_name = formatSource)]
pub fn format_source(source: &str) -> String {
    tomet_formatter::format_source(source)
}

pub mod highlight;
pub use highlight::{HighlightSpan, compute_highlight_spans};

/// Validate `.tmt` source text and return an array of validation diagnostics.
#[wasm_bindgen(js_name = validate)]
pub fn validate(source: &str) -> Result<JsValue, JsValue> {
    let doc =
        tomet_parser::parse_document(source).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let diagnostics = tomet_validator::validate_document(&doc);
    serde_wasm_bindgen::to_value(&diagnostics).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Validate `.tmt` source text against `std` plus the vocabularies given
/// as source text.
///
/// A host that has the vault's `@vocabulary(...)` files can pass their
/// contents here. Without them, `validate` knows only `std`, so every
/// element a vocabulary declares comes back as unknown -- correct, since
/// nothing said it existed, but not useful in an editor that could have
/// said so.
///
/// A source that does not parse, or that carries no `@vocabulary(ns)`
/// header, is skipped: it declares no namespace, so there is nothing to
/// bind. Check vocabularies themselves with `tomet check`.
#[wasm_bindgen(js_name = validateWith)]
pub fn validate_with(source: &str, vocabularies: Vec<String>) -> Result<JsValue, JsValue> {
    let doc =
        tomet_parser::parse_document(source).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let bindings = bindings_from_sources(&doc, &vocabularies);
    let diagnostics = tomet_validator::validate_document_with(&doc, &bindings);
    serde_wasm_bindgen::to_value(&diagnostics).map_err(|e| JsValue::from_str(&e.to_string()))
}

fn bindings_from_sources(
    doc: &tomet_ast::Document,
    vocabularies: &[String],
) -> tomet_semantics::Bindings {
    let parsed = vocabularies.iter().filter_map(|src| {
        tomet_parser::parse_document(src)
            .ok()
            .and_then(|d| tomet_semantics::Vocabulary::from_document(&d))
    });
    tomet_semantics::Bindings::for_document(doc, parsed)
}

/// Compute syntax highlight token spans for CodeMirror and other editors.
#[wasm_bindgen(js_name = highlightSpans)]
pub fn highlight_spans(source: &str) -> Result<JsValue, JsValue> {
    let spans = compute_highlight_spans(source);
    serde_wasm_bindgen::to_value(&spans).map_err(|e| JsValue::from_str(&e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_and_html_roundtrip() {
        let src = "#[ Hello World ]\n\n<task>(done: true)[Buy milk]\n";
        let doc = tomet_parser::parse_document(src).unwrap();
        let html = tomet_html::render_body(&doc);
        assert!(html.contains("Hello World"));
        assert!(html.contains("Buy milk"));
    }

    #[test]
    fn test_format_source() {
        let src = "#[  Hello  ]\n\n";
        let formatted = format_source(src);
        assert_eq!(formatted, "#[  Hello  ]\n");
    }

    #[test]
    fn test_process_document_with_meta_title() {
        let src = "@meta{\n  title: \"My Meta Title\"\n}\n\n#[ Document Title ]\n\n##[ Section One ]\n\nContent.\n";
        let doc = tomet_parser::parse_document(src).unwrap();
        let opts = ProcessOptions {
            advanced: Some(true),
            ..Default::default()
        };
        let res = process_document_internal(doc, &opts);
        assert_eq!(res.title.as_deref(), Some("My Meta Title"));
        assert!(!res.is_data_only);
        assert_eq!(res.toc.len(), 1);
        assert_eq!(res.toc[0].level, 2);
        assert_eq!(res.toc[0].text, "Section One");
        assert_eq!(res.toc[0].id, "section-one");
    }

    #[test]
    fn test_process_document_with_h1_fallback() {
        let src = "#[ First Heading Title ]\n\n##[ Section A ]\n\n###[ Sub Section ]\n";
        let doc = tomet_parser::parse_document(src).unwrap();
        let opts = ProcessOptions {
            advanced: Some(true),
            ..Default::default()
        };
        let res = process_document_internal(doc, &opts);
        assert_eq!(res.title.as_deref(), Some("First Heading Title"));
        assert!(!res.is_data_only);
        assert_eq!(res.toc.len(), 2);
        assert_eq!(res.toc[0].text, "Section A");
        assert_eq!(res.toc[0].level, 2);
        assert_eq!(res.toc[1].text, "Sub Section");
        assert_eq!(res.toc[1].level, 3);
    }

    #[test]
    fn test_process_document_data_only() {
        let src = "@version(1.0)\n@meta{\n  title: \"Just Config\"\n}\n";
        let doc = tomet_parser::parse_document(src).unwrap();
        let opts = ProcessOptions::default();
        let res = process_document_internal(doc, &opts);
        assert_eq!(res.title.as_deref(), Some("Just Config"));
        assert!(res.is_data_only);
        assert!(res.toc.is_empty());
    }

    #[test]
    fn test_process_document_with_resolved_links() {
        let src =
            "- @link(\"ref:Linux\")[Go to Linux]\n- @link(\"ref:NonExistentNote\")[Missing]\n";
        let doc = tomet_parser::parse_document(src).unwrap();
        let opts = ProcessOptions {
            vault_files: Some(vec![
                "30-39 Knowledge/Linux.tmt".into(),
                "10-19 Journal/daily/memo.tmt".into(),
            ]),
            url_prefix: Some("/docs".into()),
            current_path: Some("10-19 Journal/daily/memo.tmt".into()),
            ..Default::default()
        };
        let res = process_document_internal(doc, &opts);
        assert!(
            res.html
                .contains("href=\"/docs/30-39 Knowledge/Linux\">Go to Linux</a>")
        );
        assert!(res.html.contains("class=\"tm-ref tm-ref-unresolved\" aria-disabled=\"true\" data-ref=\"NonExistentNote\">Missing</a>"));
    }
}
