//! Structural search and replace engine across workspace files.

use std::fs;
use std::path::Path;

use tomet_printer::document_to_tm;
use tomet_query::{StructuralQuery, count_structural_matches};
use tomet_transform::{StructuralAction, apply_structural_action};
use tomet_vault::Vault;

use crate::diff::FileDiff;

pub struct StructuralEngine;

impl StructuralEngine {
    /// Perform structural query search across the `.tmt` files under `dir`, in `vault`.
    ///
    /// Each file is read with the names it has in scope in `vault` (its kind and what it brings
    /// in with `::use`), so a query names a vocabulary's element in that vocabulary's own
    /// namespace, whatever alias a file writes it under.
    pub fn search(vault: &Vault, dir: &Path, query: &StructuralQuery) -> Vec<FileDiff> {
        let mut matches = Vec::new();
        let paths = tomet_indexer::collect_tm_files_with_config(dir, &vault.config, &vault.root);

        for path in paths {
            if let Ok(src) = fs::read_to_string(&path)
                && let Ok((doc, bindings)) = vault.parse(&src)
            {
                let count = count_structural_matches(&doc, &bindings, query);
                if count > 0 {
                    matches.push(FileDiff::new(path, src.clone(), src, count));
                }
            }
        }
        matches
    }

    /// Apply structural refactoring action to matched items.
    /// Each file's names are read in `vault`, as in [`StructuralEngine::search`].
    pub fn apply_action(vault: &Vault, matches: &mut [FileDiff], action: &StructuralAction) {
        for m in matches.iter_mut() {
            if let Ok((mut doc, bindings)) = vault.parse(&m.original_src) {
                let count = apply_structural_action(&mut doc, &bindings, action);
                if count > 0 {
                    m.changes_count = count;
                    m.modified_src = document_to_tm(&doc);
                }
            }
        }
    }
}
