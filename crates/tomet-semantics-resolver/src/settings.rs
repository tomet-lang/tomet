use std::fs;
use std::path::Path;

use tomet_ast::{Block, Document, Element, ElementValue, Inline, Sigil, Value};
use tomet_tree::ValueExt;

use crate::ResolveError;

/// Extracts the import path string from a `@config(import:...)` / `@config(file:...)`
/// or legacy `@settings(file:...)` reference element.
pub fn config_import_ref(el: &Element) -> Option<&str> {
    let Sigil::At(Some(name)) = &el.sigil else {
        return None;
    };
    if name != "config" && name != "settings" {
        return None;
    }
    let args = el.args.as_ref()?;
    if let Some(val) = args.get("import") {
        return val.as_str();
    }
    if let Some(val) = args.get("file") {
        return val.as_str();
    }
    None
}

/// Legacy alias for [`config_import_ref`].
pub fn settings_file_ref(el: &Element) -> Option<&str> {
    config_import_ref(el)
}

/// Reads `path`, parses it as Tomet, and returns the `Value` held by
/// its top-level `@config{ ... }` or `@settings{ ... }` block. `path` must already be fully
/// resolved by the caller -- this function does no path interpretation
/// (see [`resolve_settings_ref`] for that).
pub fn resolve_settings_file(path: &Path) -> Result<Value, ResolveError> {
    let src = fs::read_to_string(path).map_err(|source| ResolveError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let doc = tomet_parser::parse_document(&src).map_err(|source| ResolveError::Parse {
        path: path.to_path_buf(),
        source,
    })?;
    settings_block(&doc)
        .cloned()
        .ok_or_else(|| ResolveError::MissingSettingsBlock {
            path: path.to_path_buf(),
        })
}

/// [`settings_file_ref`] on `el`, joined against `project_root` (`file:`
/// is documented as project-root-relative, see
/// `docs/guide/builtins/args.tmt`), then resolved via
/// [`resolve_settings_file`]. `None` if `el` isn't a `@settings(file:...)`
/// reference at all; `Some(Err(_))` if it is one but resolution failed.
pub fn resolve_settings_ref(
    el: &Element,
    project_root: &Path,
) -> Option<Result<Value, ResolveError>> {
    let file = settings_file_ref(el)?;
    Some(resolve_settings_file(&project_root.join(file)))
}

/// The `Value` held by `doc`'s top-level `@config{ ... }` or `@settings{ ... }` *definition*
/// block, i.e. a `Sigil::At` element with no `args`
/// (unlike a reference element) and a data `{value}` group.
fn settings_block(doc: &Document) -> Option<&Value> {
    doc.blocks.iter().find_map(|block| match block {
        Block::Element(el) => settings_value(el),
        Block::Paragraph(p) => p.content.iter().find_map(|inline| match inline {
            Inline::Element(el) => settings_value(el),
            _ => None,
        }),
    })
}

fn settings_value(el: &Element) -> Option<&Value> {
    let Sigil::At(Some(name)) = &el.sigil else {
        return None;
    };
    if name != "settings" && name != "config" {
        return None;
    }
    match &el.value {
        Some(ElementValue::Data(v)) => Some(v),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_tree::element_new;

    fn fixture(name: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures")).join(name)
    }

    #[test]
    fn resolves_a_valid_settings_file() {
        let value = resolve_settings_file(&fixture("valid_settings.tmt")).unwrap();
        match value {
            Value::Map(entries) => {
                assert!(entries.iter().any(|(k, _)| k == "elements"));
                assert!(entries.iter().any(|(k, _)| k == "types"));
            }
            other => panic!("expected a map, got {other:?}"),
        }
    }

    #[test]
    fn missing_file_is_an_io_error() {
        let err = resolve_settings_file(&fixture("does_not_exist.tmt")).unwrap_err();
        assert!(matches!(err, ResolveError::Io { .. }), "got {err:?}");
    }

    #[test]
    fn unparseable_file_is_a_parse_error() {
        let err = resolve_settings_file(&fixture("invalid_syntax.tmt")).unwrap_err();
        assert!(matches!(err, ResolveError::Parse { .. }), "got {err:?}");
    }

    #[test]
    fn file_with_no_settings_block_is_reported() {
        let err = resolve_settings_file(&fixture("no_settings_block.tmt")).unwrap_err();
        assert!(
            matches!(err, ResolveError::MissingSettingsBlock { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn settings_file_ref_extracts_the_path() {
        let mut el = element_new(Sigil::At(Some("settings".to_string())));
        el.args = Some(Value::Map(vec![(
            "file".to_string(),
            Value::String("docs/docs.settings.tmt".to_string()),
        )]));
        assert_eq!(settings_file_ref(&el), Some("docs/docs.settings.tmt"));
    }

    #[test]
    fn settings_file_ref_is_none_for_a_definition_block() {
        // `@settings{ ... }` itself -- no `args`, so it's a definition,
        // not a reference.
        let el = element_new(Sigil::At(Some("settings".to_string())));
        assert_eq!(settings_file_ref(&el), None);
    }

    #[test]
    fn settings_file_ref_is_none_for_unrelated_elements() {
        let mut el = element_new(Sigil::At(Some("meta".to_string())));
        el.args = Some(Value::Map(vec![(
            "file".to_string(),
            Value::String("x.tmt".to_string()),
        )]));
        assert_eq!(settings_file_ref(&el), None);
    }

    #[test]
    fn resolve_settings_ref_joins_against_project_root() {
        let mut el = element_new(Sigil::At(Some("settings".to_string())));
        el.args = Some(Value::Map(vec![(
            "file".to_string(),
            Value::String("valid_settings.tmt".to_string()),
        )]));
        let project_root = fixture("");
        let value = resolve_settings_ref(&el, &project_root)
            .expect("should recognize the reference")
            .expect("should resolve successfully");
        assert!(matches!(value, Value::Map(_)));
    }

    #[test]
    fn resolve_settings_ref_is_none_for_non_settings_elements() {
        let el = element_new(Sigil::Type("caution".to_string()));
        assert!(resolve_settings_ref(&el, &fixture("")).is_none());
    }
}
