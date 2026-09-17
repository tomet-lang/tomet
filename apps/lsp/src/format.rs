use lsp_types::{TextEdit, Uri};
use tomet_ast::Document;

use crate::position::{uri_to_file_path, whole_document_range};

/// Formats the document using `tomet-formatter` with loaded or inferred configuration.
pub fn format_edits(text: &str, uri: Option<&Uri>) -> Vec<TextEdit> {
    let path = uri.and_then(uri_to_file_path);
    let config = tomet_config::config_for(path.as_deref(), text);
    let formatted = tomet_formatter::format_source_with_config(text, &config);
    if formatted == text {
        return Vec::new();
    }
    vec![TextEdit {
        range: whole_document_range(text),
        new_text: formatted,
    }]
}

/// Resolves the merged configuration for a document by combining:
/// 1. Workspace config discovered from ancestor directories (`default.config.tmt` / `tomet.config.tmt`)
/// 2. Explicit `@config(import: ...)` / `@config(file: ...)` imported configs
/// 3. Document's local `@config` definitions (highest precedence)
pub fn resolve_effective_config(
    doc: &Document,
    uri: Option<&Uri>,
) -> tomet_semantics::DocumentConfig {
    let mut config = tomet_semantics::document_config(doc);
    let file_path_opt = uri.and_then(uri_to_file_path);

    // 1. Merge macros from workspace config file (e.g. default.config.tmt / tomet.config.tmt)
    if let Some(fp) = &file_path_opt {
        if let Some((_, cfg_path, _)) = tomet_config::find_config_file(fp) {
            if let Ok(src) = std::fs::read_to_string(&cfg_path) {
                if let Ok(cfg_doc) = tomet_parser::parse_document(&src) {
                    let ext_cfg = tomet_semantics::document_config(&cfg_doc);
                    for (k, v) in ext_cfg.macros {
                        config.macros.entry(k).or_insert(v);
                    }
                }
            }
        }
    }

    // 2. Merge macros from explicit @config(import: ...) / @settings references in the document
    let mut import_targets = config.imports.clone();
    for block in &doc.blocks {
        if let tomet_ast::Block::Element(el) = block {
            if let Some(target) = tomet_resolver::config_import_ref(el) {
                if !import_targets.iter().any(|t| t == target) {
                    import_targets.push(target.to_string());
                }
            }
        }
    }

    for target in import_targets {
        let clean = target.strip_prefix("file:").unwrap_or(&target).trim();
        let candidate_paths = if let Some(fp) = &file_path_opt {
            let start_dir = if fp.is_file() {
                fp.parent().unwrap_or(fp)
            } else {
                fp.as_path()
            };
            let mut paths = Vec::new();
            let mut cur = start_dir.to_path_buf();
            loop {
                paths.push(cur.join(clean));
                if !cur.pop() {
                    break;
                }
            }
            paths
        } else {
            vec![std::path::PathBuf::from(clean)]
        };
        for p in candidate_paths {
            if p.exists() {
                if let Ok(src) = std::fs::read_to_string(&p) {
                    if let Ok(ext_doc) = tomet_parser::parse_document(&src) {
                        let ext_cfg = tomet_semantics::document_config(&ext_doc);
                        for (k, v) in ext_cfg.macros {
                            config.macros.entry(k).or_insert(v);
                        }
                    }
                }
                break;
            }
        }
    }

    config
}
