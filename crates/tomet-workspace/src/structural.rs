//! Structural search and replace engine across workspace files.

use std::fs;
use std::path::Path;

use tomet_parser::parse_document;
use tomet_printer::document_to_tm;
use tomet_search::{StructuralQuery, count_structural_matches};
use tomet_transform::{StructuralAction, apply_structural_action};

use crate::diff::FileDiff;

pub struct StructuralEngine;

impl StructuralEngine {
    /// Perform structural query search across `.tmt` files.
    pub fn search(dir: &Path, query: &StructuralQuery) -> Vec<FileDiff> {
        let (config, _, config_root) = tomet_config::find_config_file(dir).unwrap_or_else(|| {
            (
                tomet_config::PrinterConfig::default(),
                dir.to_path_buf(),
                dir.to_path_buf(),
            )
        });
        Self::search_with_config(dir, query, &config, &config_root)
    }

    pub fn search_with_config(
        dir: &Path,
        query: &StructuralQuery,
        config: &tomet_config::PrinterConfig,
        config_root: &Path,
    ) -> Vec<FileDiff> {
        let mut matches = Vec::new();
        let paths = tomet_indexer::collect_tm_files_with_config(dir, config, config_root);

        for path in paths {
            if let Ok(src) = fs::read_to_string(&path)
                && let Ok(doc) = parse_document(&src)
            {
                let count = count_structural_matches(&doc, query);
                if count > 0 {
                    matches.push(FileDiff::new(path, src.clone(), src, count));
                }
            }
        }
        matches
    }

    /// Apply structural refactoring action to matched items.
    pub fn apply_action(matches: &mut [FileDiff], action: &StructuralAction) {
        for m in matches.iter_mut() {
            if let Ok(mut doc) = parse_document(&m.original_src) {
                let count = apply_structural_action(&mut doc, action);
                if count > 0 {
                    m.changes_count = count;
                    m.modified_src = document_to_tm(&doc);
                }
            }
        }
    }
}
