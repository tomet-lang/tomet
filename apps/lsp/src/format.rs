use lsp_types::{TextEdit, Uri};
use tomet_ast::Document;

use crate::position::{uri_to_file_path, whole_document_range};

/// Formats the document using `tomet-formatter` with loaded or inferred configuration.
///
/// A `.ctmt` is formatted by whitespace alone, whatever the config says, as
/// `tomet format` does.
pub fn format_edits(text: &str, uri: Option<&Uri>) -> Vec<TextEdit> {
    let formatted = if uri.is_some_and(crate::data::is_data_uri) {
        tomet_formatter::format_data_source(text)
    } else {
        let path = uri.and_then(uri_to_file_path);
        let config = tomet_load_config::config_for(path.as_deref(), text);
        tomet_formatter::format_source_with_config(text, &config)
    };
    if formatted == text {
        return Vec::new();
    }
    vec![TextEdit {
        range: whole_document_range(text),
        new_text: formatted,
    }]
}

/// Resolves the merged configuration for a document by combining:
/// 1. The vault config and the folder configs down to the document, already stacked
///    (see `tomet_load_config`), for the macros and variables they declare
/// 2. The document's own `::macros` and `::vars` (highest precedence)
pub fn resolve_effective_config(
    doc: &Document,
    uri: Option<&Uri>,
) -> tomet_semantics::DocumentConfig {
    let mut config = tomet_semantics::document_config(doc);
    let file_path_opt = uri.and_then(uri_to_file_path);

    // 1. Merge macros and variables from the stacked vault and folder configs
    if let Some(fp) = &file_path_opt
        && let Some((stacked, _, _)) = tomet_load_config::find_config_file(fp)
    {
        for (k, v) in stacked.macros {
            config.macros.entry(k).or_insert(v);
        }
        for (name, decl) in stacked.vars {
            if !config.vars.iter().any(|(held, _)| *held == name) {
                config.vars.push((name, decl));
            }
        }
    }

    config
}
