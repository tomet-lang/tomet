use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use tomet_crate::config::PrinterConfig;
use tomet_crate::vault::Vault;

/// Parses `source` against a vault with no vocabularies of its own.
///
/// The facade has no bare parse on purpose (see `tomet`'s crate doc), and
/// nothing here needs one: everything except `validate_with` reads a
/// document as `std` alone, which is exactly what an empty vault is.
fn parse(source: &str) -> PyResult<tomet_crate::Document> {
    Vault::from_sources(PrinterConfig::default(), ".", &[])
        .parse(source)
        .map(|(doc, _bindings)| doc)
        .map_err(|e| PyValueError::new_err(e.to_string()))
}

/// Parse a data-only `.tmt` document into native Python objects (dict, list, str, int, float, bool, None).
#[pyfunction]
fn loads<'py>(py: Python<'py>, source: &str) -> PyResult<Bound<'py, PyAny>> {
    let val = tomet_crate::parse_value(source).map_err(|e| PyValueError::new_err(e.to_string()))?;
    pythonize::pythonize(py, &val).map_err(|e| PyValueError::new_err(e.to_string()))
}

/// Parse a full `.tmt` markup source text into a Python dict representing the Document AST.
#[pyfunction]
fn parse_document<'py>(py: Python<'py>, source: &str) -> PyResult<Bound<'py, PyAny>> {
    let doc = parse(source)?;
    pythonize::pythonize(py, &doc).map_err(|e| PyValueError::new_err(e.to_string()))
}

/// Convert `.tmt` source text or a `Document` AST dict into an HTML body string.
#[pyfunction]
fn to_html(_py: Python<'_>, source_or_doc: &Bound<'_, PyAny>) -> PyResult<String> {
    if let Ok(src) = source_or_doc.extract::<String>() {
        let doc = parse(&src)?;
        Ok(tomet_crate::html::render_body(&doc))
    } else {
        let doc: tomet_crate::Document = pythonize::depythonize(source_or_doc)
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(tomet_crate::html::render_body(&doc))
    }
}

/// Convert `.tmt` source text or a `Document` AST dict into a CommonMark Markdown string.
#[pyfunction]
fn to_markdown(_py: Python<'_>, source_or_doc: &Bound<'_, PyAny>) -> PyResult<String> {
    if let Ok(src) = source_or_doc.extract::<String>() {
        let doc = parse(&src)?;
        Ok(tomet_crate::markdown::to_markdown(&doc))
    } else {
        let doc: tomet_crate::Document = pythonize::depythonize(source_or_doc)
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(tomet_crate::markdown::to_markdown(&doc))
    }
}

/// Convert `.tmt` source text or a `Document` AST dict into a Typst markup string.
#[pyfunction]
fn to_typst(_py: Python<'_>, source_or_doc: &Bound<'_, PyAny>) -> PyResult<String> {
    if let Ok(src) = source_or_doc.extract::<String>() {
        let doc = parse(&src)?;
        Ok(tomet_crate::typst::to_typst(&doc))
    } else {
        let doc: tomet_crate::Document = pythonize::depythonize(source_or_doc)
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(tomet_crate::typst::to_typst(&doc))
    }
}

/// Parse CommonMark Markdown text into a `Document` AST dict.
#[pyfunction]
fn from_markdown<'py>(py: Python<'py>, markdown: &str) -> PyResult<Bound<'py, PyAny>> {
    let doc = tomet_crate::markdown::from_markdown(markdown);
    pythonize::pythonize(py, &doc).map_err(|e| PyValueError::new_err(e.to_string()))
}

/// Serialize a `Document` AST dict back into formatted `.tmt` source code.
#[pyfunction]
fn print_document(_py: Python<'_>, doc_obj: &Bound<'_, PyAny>) -> PyResult<String> {
    let doc: tomet_crate::Document =
        pythonize::depythonize(doc_obj).map_err(|e| PyValueError::new_err(e.to_string()))?;
    Ok(tomet_crate::printer::document_to_tm(&doc))
}

/// Format `.tmt` source text with lossless whitespace hygiene and span preservation.
#[pyfunction]
fn format(source: &str) -> String {
    tomet_crate::format::format_source(source)
}

/// Validate `.tmt` source text and return a list of validation errors.
#[pyfunction]
fn validate<'py>(py: Python<'py>, source: &str) -> PyResult<Bound<'py, PyAny>> {
    let doc = parse(source)?;
    let diagnostics = tomet_crate::validator::validate_document(&doc);
    pythonize::pythonize(py, &diagnostics).map_err(|e| PyValueError::new_err(e.to_string()))
}

/// Validate `.tmt` source text against `std` plus the vocabularies given
/// as source text.
///
/// Without them, `validate` knows only `std`, so every element a
/// vocabulary declares comes back as unknown. A source that does not
/// parse, has no `@vocabulary(ns)` header, or claims a reserved or
/// already-taken namespace is skipped -- it binds no namespace. `doc.index`
/// is always known, as it is to the CLI. Check vocabularies themselves with
/// `tomet check`.
#[pyfunction]
fn validate_with<'py>(
    py: Python<'py>,
    source: &str,
    vocabularies: Vec<String>,
) -> PyResult<Bound<'py, PyAny>> {
    let labels: Vec<String> = (0..vocabularies.len())
        .map(|i| format!("vocabularies[{i}]"))
        .collect();
    let sources: Vec<(&str, &str)> = labels
        .iter()
        .map(String::as_str)
        .zip(vocabularies.iter().map(String::as_str))
        .collect();
    let vault = Vault::from_sources(PrinterConfig::default(), ".", &sources);
    let (doc, bindings) = vault
        .parse(source)
        .map_err(|e| PyValueError::new_err(e.to_string()))?;
    let diagnostics = tomet_crate::validator::validate_document_with(&doc, &bindings);
    pythonize::pythonize(py, &diagnostics).map_err(|e| PyValueError::new_err(e.to_string()))
}

/// Tomet Python bindings module.
#[pymodule]
fn tomet(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(loads, m)?)?;
    m.add_function(wrap_pyfunction!(validate_with, m)?)?;
    m.add_function(wrap_pyfunction!(parse_document, m)?)?;
    m.add_function(wrap_pyfunction!(to_html, m)?)?;
    m.add_function(wrap_pyfunction!(to_markdown, m)?)?;
    m.add_function(wrap_pyfunction!(to_typst, m)?)?;
    m.add_function(wrap_pyfunction!(from_markdown, m)?)?;
    m.add_function(wrap_pyfunction!(print_document, m)?)?;
    m.add_function(wrap_pyfunction!(format, m)?)?;
    m.add_function(wrap_pyfunction!(validate, m)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format() {
        let src = "#[  Hello  ]\n\n";
        let formatted = format(src);
        assert_eq!(formatted, "#[  Hello  ]\n");
    }

    #[test]
    fn test_pyo3_integration() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let src = "title: \"Tomet\"\nversion: 1";
            let val = loads(py, src).unwrap();
            assert_eq!(
                val.get_item("title").unwrap().extract::<String>().unwrap(),
                "Tomet"
            );
            assert_eq!(
                val.get_item("version").unwrap().extract::<i64>().unwrap(),
                1
            );

            let doc_src = "#[ Heading ]\n\n<task>(done: true)[Test task]\n";
            let doc_obj = parse_document(py, doc_src).unwrap();
            let html = to_html(py, &doc_obj).unwrap();
            assert!(html.contains("Heading"));
            assert!(html.contains("Test task"));
        });
    }
}
