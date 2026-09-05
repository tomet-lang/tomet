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

/// Convert `.tmt` source text or a `Document` AST object into an HTML body string.
#[wasm_bindgen(js_name = toHtml)]
pub fn to_html(source_or_doc: &JsValue) -> Result<String, JsValue> {
    if let Some(src) = source_or_doc.as_string() {
        let doc =
            tomet_parser::parse_document(&src).map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(tomet_html::render_body(&doc))
    } else {
        let doc: tomet_ast::Document = serde_wasm_bindgen::from_value(source_or_doc.clone())
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(tomet_html::render_body(&doc))
    }
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
}
