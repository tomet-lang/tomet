//! Markdown -> Tomet conversion, plus the App-facing `MigrationItem`
//! (a `tomet_indexer::workspace_scan::MigrationCandidate` decorated
//! with lazily-loaded content and UI/session state: `selected`,
//! `converted`). The index only ever knows about candidates (facts); this
//! module owns the mutable, per-session view of them.

use std::fs;
use std::path::PathBuf;

use tomet_config::PrinterConfig;
use tomet_indexer::workspace_scan::MigrationCandidate;
use tomet_printer::document_to_tm_with_config;

#[derive(Debug, Clone)]
pub struct MigrationItem {
    pub source_path: PathBuf,
    pub target_path: PathBuf,
    pub markdown_src: String,
    pub tomet_src: String,
    pub selected: bool,
    pub converted: bool,
}

impl MigrationItem {
    pub fn from_candidate(candidate: MigrationCandidate) -> Self {
        Self {
            source_path: candidate.source_path,
            target_path: candidate.target_path,
            markdown_src: String::new(),
            tomet_src: String::new(),
            selected: false,
            converted: false,
        }
    }

    pub fn ensure_loaded(&mut self) {
        self.ensure_loaded_with_config(&PrinterConfig::default());
    }

    pub fn ensure_loaded_with_config(&mut self, config: &PrinterConfig) {
        if self.markdown_src.is_empty() {
            if let Ok(src) = fs::read_to_string(&self.source_path) {
                let mut doc = tomet_markdown::from_markdown(&src);
                tomet_printer::ensure_document_id_with_config(&mut doc, config);
                self.tomet_src = document_to_tm_with_config(&doc, config);
                self.markdown_src = src;
            }
        }
    }
}

pub struct MigrationEngine;

impl MigrationEngine {
    /// Convert selected items and write `.tmt` files using specified printer config.
    pub fn execute_with_config(
        items: &mut [MigrationItem],
        remove_original: bool,
        config: &PrinterConfig,
    ) -> anyhow::Result<usize> {
        let mut count = 0;
        for item in items.iter_mut() {
            if !item.selected || item.converted {
                continue;
            }
            item.ensure_loaded_with_config(config);
            fs::write(&item.target_path, &item.tomet_src)?;
            item.converted = true;
            count += 1;
            if remove_original && item.source_path != item.target_path {
                let _ = fs::remove_file(&item.source_path);
            }
        }
        Ok(count)
    }

    /// Convert selected items and write `.tmt` files with default config.
    pub fn execute(items: &mut [MigrationItem], remove_original: bool) -> anyhow::Result<usize> {
        Self::execute_with_config(items, remove_original, &PrinterConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_indexer::workspace_scan::WorkspaceIndex;

    #[test]
    fn test_markdown_migration_conversion() {
        let dir_path = std::env::temp_dir().join(format!("tm_test_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir_path).unwrap();
        let md_path = dir_path.join("doc.md");
        fs::write(&md_path, "# Migration Test\n\n- item 1\n- item 2\n").unwrap();

        let index = WorkspaceIndex::build(&dir_path, &PrinterConfig::default(), &dir_path);
        let mut items: Vec<MigrationItem> = index
            .migration_candidates()
            .into_iter()
            .map(MigrationItem::from_candidate)
            .collect();
        assert_eq!(items.len(), 1);
        items[0].ensure_loaded();
        assert!(items[0].tomet_src.contains("#[Migration Test]"));

        items[0].selected = true;
        let count = MigrationEngine::execute(&mut items, false).unwrap();
        assert_eq!(count, 1);
        assert!(dir_path.join("doc.tmt").exists());
        let _ = fs::remove_dir_all(&dir_path);
    }
}
