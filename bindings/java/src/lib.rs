use jni::JNIEnv;
use jni::objects::{JClass, JString};
use jni::sys::jstring;
use tomet::config::PrinterConfig;
use tomet::vault::Vault;

const EXCEPTION_CLASS: &str = "org/tomet/tomet/TometException";

fn throw_err(env: &mut JNIEnv, msg: impl std::fmt::Display) -> jstring {
    let _ = env.throw_new(EXCEPTION_CLASS, msg.to_string());
    std::ptr::null_mut()
}

/// Parses `source` against a vault with no vocabularies of its own.
///
/// The facade has no bare parse on purpose (see `tomet`'s crate doc), and
/// nothing here but `validateJsonWith` needs one: reading a document as
/// `std` alone is exactly what an empty vault does.
fn parse(source: &str) -> Result<tomet::Document, String> {
    Vault::from_sources(PrinterConfig::default(), ".", &[])
        .parse(source)
        .map(|(doc, _bindings)| doc)
        .map_err(|e| e.to_string())
}

fn get_string(env: &mut JNIEnv, j_str: &JString) -> Result<String, String> {
    env.get_string(j_str)
        .map(|s| s.into())
        .map_err(|e| e.to_string())
}

/// Parse full `.tmt` markup source text into a JSON string representing the Document AST.
#[unsafe(no_mangle)]
pub extern "system" fn Java_org_tomet_tomet_Tomet_parseDocumentJson(
    mut env: JNIEnv,
    _class: JClass,
    source: JString,
) -> jstring {
    let src = match get_string(&mut env, &source) {
        Ok(s) => s,
        Err(e) => return throw_err(&mut env, e),
    };
    let doc = match parse(&src) {
        Ok(d) => d,
        Err(e) => return throw_err(&mut env, e),
    };
    let json = match serde_json::to_string(&doc) {
        Ok(j) => j,
        Err(e) => return throw_err(&mut env, e),
    };
    env.new_string(json)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

/// Parse a data-only `.tmt` document into a JSON string.
#[unsafe(no_mangle)]
pub extern "system" fn Java_org_tomet_tomet_Tomet_parseValueJson(
    mut env: JNIEnv,
    _class: JClass,
    source: JString,
) -> jstring {
    let src = match get_string(&mut env, &source) {
        Ok(s) => s,
        Err(e) => return throw_err(&mut env, e),
    };
    let val = match tomet::parse_value(&src) {
        Ok(v) => v,
        Err(e) => return throw_err(&mut env, e),
    };
    let json = match serde_json::to_string(&val) {
        Ok(j) => j,
        Err(e) => return throw_err(&mut env, e),
    };
    env.new_string(json)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

/// Convert `.tmt` source text into an HTML body string.
#[unsafe(no_mangle)]
pub extern "system" fn Java_org_tomet_tomet_Tomet_toHtml(
    mut env: JNIEnv,
    _class: JClass,
    source: JString,
) -> jstring {
    let src = match get_string(&mut env, &source) {
        Ok(s) => s,
        Err(e) => return throw_err(&mut env, e),
    };
    let doc = match parse(&src) {
        Ok(d) => d,
        Err(e) => return throw_err(&mut env, e),
    };
    let html = tomet::html::render_body(&doc);
    env.new_string(html)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

/// Convert `.tmt` source text into a CommonMark Markdown string.
#[unsafe(no_mangle)]
pub extern "system" fn Java_org_tomet_tomet_Tomet_toMarkdown(
    mut env: JNIEnv,
    _class: JClass,
    source: JString,
) -> jstring {
    let src = match get_string(&mut env, &source) {
        Ok(s) => s,
        Err(e) => return throw_err(&mut env, e),
    };
    let doc = match parse(&src) {
        Ok(d) => d,
        Err(e) => return throw_err(&mut env, e),
    };
    let md = tomet::markdown::to_markdown(&doc);
    env.new_string(md)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

/// Convert `.tmt` source text into a Typst markup string.
#[unsafe(no_mangle)]
pub extern "system" fn Java_org_tomet_tomet_Tomet_toTypst(
    mut env: JNIEnv,
    _class: JClass,
    source: JString,
) -> jstring {
    let src = match get_string(&mut env, &source) {
        Ok(s) => s,
        Err(e) => return throw_err(&mut env, e),
    };
    let doc = match parse(&src) {
        Ok(d) => d,
        Err(e) => return throw_err(&mut env, e),
    };
    let typ = tomet::typst::to_typst(&doc);
    env.new_string(typ)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

/// Parse CommonMark Markdown text into a `Document` AST JSON string.
#[unsafe(no_mangle)]
pub extern "system" fn Java_org_tomet_tomet_Tomet_fromMarkdownJson(
    mut env: JNIEnv,
    _class: JClass,
    markdown: JString,
) -> jstring {
    let md = match get_string(&mut env, &markdown) {
        Ok(s) => s,
        Err(e) => return throw_err(&mut env, e),
    };
    let doc = tomet::markdown::from_markdown(&md);
    let json = match serde_json::to_string(&doc) {
        Ok(j) => j,
        Err(e) => return throw_err(&mut env, e),
    };
    env.new_string(json)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

/// Serialize a `Document` AST JSON string back into formatted `.tmt` source code.
#[unsafe(no_mangle)]
pub extern "system" fn Java_org_tomet_tomet_Tomet_printDocumentJson(
    mut env: JNIEnv,
    _class: JClass,
    doc_json: JString,
) -> jstring {
    let json_str = match get_string(&mut env, &doc_json) {
        Ok(s) => s,
        Err(e) => return throw_err(&mut env, e),
    };
    let doc: tomet::Document = match serde_json::from_str(&json_str) {
        Ok(d) => d,
        Err(e) => return throw_err(&mut env, e),
    };
    let source = tomet::printer::document_to_tm(&doc);
    env.new_string(source)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

/// Format `.tmt` source text with lossless whitespace hygiene and span preservation.
#[unsafe(no_mangle)]
pub extern "system" fn Java_org_tomet_tomet_Tomet_format(
    mut env: JNIEnv,
    _class: JClass,
    source: JString,
) -> jstring {
    let src = match get_string(&mut env, &source) {
        Ok(s) => s,
        Err(e) => return throw_err(&mut env, e),
    };
    let formatted = tomet::format::format_source(&src);
    env.new_string(formatted)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

/// Validate `.tmt` source text and return a JSON list of validation errors.
#[unsafe(no_mangle)]
pub extern "system" fn Java_org_tomet_tomet_Tomet_validateJson(
    mut env: JNIEnv,
    _class: JClass,
    source: JString,
) -> jstring {
    let src = match get_string(&mut env, &source) {
        Ok(s) => s,
        Err(e) => return throw_err(&mut env, e),
    };
    let doc = match parse(&src) {
        Ok(d) => d,
        Err(e) => return throw_err(&mut env, e),
    };
    let diagnostics = tomet::validator::validate_document(&doc);
    let json = match serde_json::to_string(&diagnostics) {
        Ok(j) => j,
        Err(e) => return throw_err(&mut env, e),
    };
    env.new_string(json)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

/// Validate `.tmt` source text against `std` plus vocabularies passed as
/// a JSON array of source strings.
///
/// Without them, `validateJson` knows only `std`, so every element a
/// vocabulary declares comes back as unknown. A source that does not
/// parse, has no `@vocabulary(ns)` header, or claims a reserved or
/// already-taken namespace is skipped -- it binds no namespace. `doc.index`
/// is always known, as it is to the CLI. Check vocabularies themselves with
/// `tomet check`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_org_tomet_tomet_Tomet_validateJsonWith(
    mut env: JNIEnv,
    _class: JClass,
    source: JString,
    vocabularies_json: JString,
) -> jstring {
    let src = match get_string(&mut env, &source) {
        Ok(s) => s,
        Err(e) => return throw_err(&mut env, e),
    };
    let vocab_json = match get_string(&mut env, &vocabularies_json) {
        Ok(s) => s,
        Err(e) => return throw_err(&mut env, e),
    };
    let vocabularies: Vec<String> = match serde_json::from_str(&vocab_json) {
        Ok(v) => v,
        Err(e) => return throw_err(&mut env, e),
    };
    let labels: Vec<String> = (0..vocabularies.len())
        .map(|i| format!("vocabularies[{i}]"))
        .collect();
    let sources: Vec<(&str, &str)> = labels
        .iter()
        .map(String::as_str)
        .zip(vocabularies.iter().map(String::as_str))
        .collect();
    let vault = Vault::from_sources(PrinterConfig::default(), ".", &sources);
    let (doc, bindings) = match vault.parse(&src) {
        Ok(parsed) => parsed,
        Err(e) => return throw_err(&mut env, e),
    };
    let diagnostics = tomet::validator::validate_document_with(&doc, &bindings);
    let json = match serde_json::to_string(&diagnostics) {
        Ok(j) => j,
        Err(e) => return throw_err(&mut env, e),
    };
    env.new_string(json)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn test_core_functionality() {
        let src = "#[ Hello Java ]\n\n<task>(done: true)[Test task]\n";
        let doc = parse(src).unwrap();
        let json = serde_json::to_string(&doc).unwrap();
        assert!(json.contains("Hello Java"));
        assert!(json.contains("Test task"));

        let val_src = "name: \"Tomet\"\ncount: 42";
        let val = tomet::parse_value(val_src).unwrap();
        let val_json = serde_json::to_string(&val).unwrap();
        assert_eq!(val_json, r#"{"name":"Tomet","count":42}"#);

        let html = tomet::html::render_body(&doc);
        assert!(html.contains("Hello Java"));

        let md = tomet::markdown::to_markdown(&doc);
        assert!(md.contains("Hello Java"));

        let typ = tomet::typst::to_typst(&doc);
        assert!(typ.contains("Hello Java"));

        let formatted = tomet::format::format_source("#[  Hello  ]\n");
        assert_eq!(formatted, "#[  Hello  ]\n");
    }
}
