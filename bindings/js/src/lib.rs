//! The WebAssembly binding: `@tomet/wasm`.
//!
//! This crate moves values across the boundary and nothing more. What a vault is, what reading a
//! document for a page gives and what the diagnostics about it are, is `tomet-binding`'s; the
//! TypeScript types of those are generated from its definitions (`bindings/js/binding.d.ts`) and of
//! the AST from the AST's (`bindings/js/ast.d.ts`), so `index.d.ts` only declares the functions.

use tomet::config::PrinterConfig;
use tomet_binding::{Input, Vault as Inner, VaultSources};
use wasm_bindgen::prelude::*;

/// A vault with no vocabularies of its own: what the functions that take no vault read every document in.
///
/// The facade has no bare parse on purpose (see `tomet`'s crate doc), and nothing here but a vault
/// needs one: reading a document as `std` alone is exactly what an empty vault does.
fn empty_vault() -> tomet::vault::Vault {
    tomet::vault::Vault::from_sources(PrinterConfig::default(), ".", &[])
}

/// The names `doc` has in scope in the [`empty_vault`], which is what a converter is handed.
fn bindings_of(doc: &tomet::File) -> tomet::semantics::Bindings {
    empty_vault().bindings(doc)
}

/// Hands `value` to JavaScript in the JSON shape its `Serialize` gives, whatever that is: the AST
/// wire for a `File` (see `tomet-ast`'s `Value`), plain data for a `PlainValue`. A map is a plain
/// object, never a `Map`, and an absent value is `null`, never `undefined`. Every Rust value leaves
/// this module through here, so JavaScript sees the shape `serde_json` gives Python and Java.
fn to_js<T: serde::Serialize + ?Sized>(value: &T) -> Result<JsValue, JsValue> {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

fn from_js<T: serde::de::DeserializeOwned + Default>(value: Option<JsValue>) -> Result<T, JsValue> {
    match value {
        Some(value) if !value.is_undefined() && !value.is_null() => {
            serde_wasm_bindgen::from_value(value).map_err(|e| JsValue::from_str(&e.to_string()))
        }
        _ => Ok(T::default()),
    }
}

fn error(e: impl ToString) -> JsValue {
    JsValue::from_str(&e.to_string())
}

/// Runs `read` with the document `source_or_file` is: text, or a `File` AST object.
fn with_input<T>(
    source_or_file: &JsValue,
    read: impl FnOnce(Input<'_>) -> Result<T, String>,
) -> Result<T, JsValue> {
    let result = match source_or_file.as_string() {
        Some(text) => read(Input::Source(&text)),
        None => {
            let doc: tomet::ast::File =
                serde_wasm_bindgen::from_value(source_or_file.clone()).map_err(error)?;
            read(Input::File(doc))
        }
    };
    result.map_err(error)
}

/// Parses `source` in the [`empty_vault`].
fn parse_plain(source: &str) -> Result<tomet::File, String> {
    empty_vault()
        .parse(source)
        .map(|(doc, _bindings)| doc)
        .map_err(|e| e.to_string())
}

/// Parse `.tmt` markup source text into a JavaScript `File` AST object.
#[wasm_bindgen(js_name = parseFile)]
pub fn parse_file(source: &str) -> Result<JsValue, JsValue> {
    let doc = parse_plain(source).map_err(error)?;
    to_js(&doc)
}

/// Parse a data-only `.tmt` document into plain JavaScript data: `null`, booleans, numbers, strings,
/// arrays and objects. A document that holds anything that is not data (a call, an interpolation, an
/// element, `auto`, ...) is an error naming it: read it with `parseFile` to see the AST.
#[wasm_bindgen(js_name = parseValue)]
pub fn parse_value(source: &str) -> Result<JsValue, JsValue> {
    let val = tomet::parse_value(source).map_err(error)?;
    let plain = val.to_plain().map_err(error)?;
    to_js(&plain)
}

/// A vault to read documents in: made once from its sources, it owns all of its state, so two
/// vaults never see each other's files.
#[wasm_bindgen]
pub struct Vault {
    inner: Inner,
}

#[wasm_bindgen]
impl Vault {
    /// Makes a vault of `sources`. Throws, naming it, if a part of them is not usable.
    #[wasm_bindgen(constructor)]
    pub fn new(sources: Option<JsValue>) -> Result<Vault, JsValue> {
        let sources: VaultSources = from_js(sources)?;
        Ok(Vault {
            inner: Inner::new(&sources).map_err(error)?,
        })
    }

    /// Reads `.tmt` source text or a `File` AST object for a page: HTML, metadata, title, table of
    /// contents, and the diagnostics about the document. Throws if the document does not parse.
    #[wasm_bindgen(js_name = processFile)]
    pub fn process_file(
        &self,
        source_or_file: &JsValue,
        options: Option<JsValue>,
    ) -> Result<JsValue, JsValue> {
        let options = from_js(options)?;
        let processed = with_input(source_or_file, |input| self.inner.process(input, &options))?;
        to_js(&processed)
    }

    /// Validates `.tmt` source text or a `File` AST object the way `tomet check` does, in this vault.
    pub fn validate(&self, source_or_file: &JsValue) -> Result<JsValue, JsValue> {
        let found = with_input(source_or_file, |input| self.inner.check(input))?;
        to_js(&found)
    }
}

/// The files `config` registers (vocabularies, dictionaries, blueprints), as paths from the config's
/// directory: what to read to make the sources of a vault.
#[wasm_bindgen(js_name = vaultRegistry)]
pub fn vault_registry(config: &str) -> Result<JsValue, JsValue> {
    to_js(&tomet_binding::registry(config).map_err(error)?)
}

/// Convert `.tmt` source text or a `File` AST object into an HTML string, in a vault of nothing.
#[wasm_bindgen(js_name = toHtml)]
pub fn to_html(source_or_file: &JsValue, options: Option<JsValue>) -> Result<String, JsValue> {
    let options = from_js(options)?;
    let processed = with_input(source_or_file, |input| {
        Inner::empty().process(input, &options)
    })?;
    Ok(processed.html)
}

/// Process `.tmt` markup source text or a `File` AST object into HTML, metadata, title, and TOC, in
/// a vault of nothing. For a document that links to others, make a `Vault`.
#[wasm_bindgen(js_name = processFile)]
pub fn process_file(
    source_or_file: &JsValue,
    options: Option<JsValue>,
) -> Result<JsValue, JsValue> {
    let options = from_js(options)?;
    let processed = with_input(source_or_file, |input| {
        Inner::empty().process(input, &options)
    })?;
    to_js(&processed)
}

/// Convert `.tmt` source text or a `File` AST object into a CommonMark Markdown string.
#[wasm_bindgen(js_name = toMarkdown)]
pub fn to_markdown(source_or_file: &JsValue) -> Result<String, JsValue> {
    if let Some(src) = source_or_file.as_string() {
        let doc = parse_plain(&src).map_err(error)?;
        Ok(tomet::markdown::to_markdown(&doc, &bindings_of(&doc)))
    } else {
        let doc: tomet::ast::File =
            serde_wasm_bindgen::from_value(source_or_file.clone()).map_err(error)?;
        Ok(tomet::markdown::to_markdown(&doc, &bindings_of(&doc)))
    }
}

/// Convert `.tmt` source text or a `File` AST object into a Typst markup string.
#[wasm_bindgen(js_name = toTypst)]
pub fn to_typst(source_or_file: &JsValue) -> Result<String, JsValue> {
    if let Some(src) = source_or_file.as_string() {
        let doc = parse_plain(&src).map_err(error)?;
        Ok(tomet::typst::to_typst(&doc, &bindings_of(&doc)))
    } else {
        let doc: tomet::ast::File =
            serde_wasm_bindgen::from_value(source_or_file.clone()).map_err(error)?;
        Ok(tomet::typst::to_typst(&doc, &bindings_of(&doc)))
    }
}

/// Parse CommonMark Markdown text into a `File` AST object.
#[wasm_bindgen(js_name = fromMarkdown)]
pub fn from_markdown(markdown: &str) -> Result<JsValue, JsValue> {
    let doc = tomet::markdown::from_markdown(markdown);
    to_js(&doc)
}

/// Serialize a `File` AST object back into formatted `.tmt` source code.
#[wasm_bindgen(js_name = printFile)]
pub fn print_file(doc_val: &JsValue) -> Result<String, JsValue> {
    let doc: tomet::ast::File = serde_wasm_bindgen::from_value(doc_val.clone()).map_err(error)?;
    Ok(tomet::printer::document_to_tm(&doc))
}

/// Format `.tmt` source text with lossless whitespace hygiene and span preservation.
#[wasm_bindgen(js_name = formatSource)]
pub fn format_source(source: &str) -> String {
    tomet::format::format_source(source)
}

/// Validate `.tmt` source text the way `tomet check` does, in a vault of nothing: `std` alone, so
/// every element a vocabulary declares comes back as unknown. For the vault of a project, make a `Vault`.
#[wasm_bindgen]
pub fn validate(source_or_file: &JsValue) -> Result<JsValue, JsValue> {
    let found = with_input(source_or_file, |input| Inner::empty().check(input))?;
    to_js(&found)
}

/// Compute syntax highlight token spans for CodeMirror and other editors.
#[wasm_bindgen(js_name = highlightSpans)]
pub fn highlight_spans(source: &str) -> Result<JsValue, JsValue> {
    let spans = tomet_binding::compute_highlight_spans(source);
    to_js(&spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_and_html_roundtrip() {
        let src = "#[ Hello World ]\n\n<task>(done: true)[Buy milk]\n";
        let doc = parse_plain(src).unwrap();
        let html = tomet::html::render_body(&doc, &bindings_of(&doc));
        assert!(html.contains("Hello World"));
        assert!(html.contains("Buy milk"));
    }

    #[test]
    fn test_format_source() {
        let src = "=[  Hello  ]\n\n";
        let formatted = format_source(src);
        assert_eq!(formatted, "=[  Hello  ]\n");
    }

    #[test]
    fn parse_value_is_plain_data_or_an_error_that_names_what_is_not() {
        let plain = tomet::parse_value("a: 1\nb: (x, y)\n")
            .unwrap()
            .to_plain()
            .unwrap();
        let tomet::ast::PlainValue::Object(entries) = plain else {
            panic!("an object")
        };
        assert_eq!(entries.len(), 2);
        let err = tomet::parse_value("a: enum(1)\n")
            .unwrap()
            .to_plain()
            .unwrap_err();
        assert_eq!(err.to_string(), "a call is not plain data (at `a`)");
    }
}
