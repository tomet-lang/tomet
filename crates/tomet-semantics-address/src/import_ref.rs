use tomet_ast::Element;
use tomet_tree::ValueExt;

/// Extracts the import path string from a `@config(import:...)` / `@config(file:...)`
/// or legacy `@settings(file:...)` reference element.
pub fn config_import_ref(el: &Element) -> Option<&str> {
    if !el.sigil.is_bare_named("config") && !el.sigil.is_bare_named("settings") {
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

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_ast::{Sigil, Value};
    use tomet_tree::element_new;

    #[test]
    fn settings_file_ref_extracts_the_path() {
        let mut el = element_new(Sigil::named("settings"));
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
        let el = element_new(Sigil::named("settings"));
        assert_eq!(settings_file_ref(&el), None);
    }

    #[test]
    fn settings_file_ref_is_none_for_unrelated_elements() {
        let mut el = element_new(Sigil::named("meta"));
        el.args = Some(Value::Map(vec![(
            "file".to_string(),
            Value::String("x.tmt".to_string()),
        )]));
        assert_eq!(settings_file_ref(&el), None);
    }
}
