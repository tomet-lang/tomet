//! Markdown -> TypedMark Migration Engine.

use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use super::printer::document_to_tm;

#[derive(Debug, Clone)]
pub struct MigrationItem {
    pub source_path: PathBuf,
    pub target_path: PathBuf,
    pub markdown_src: String,
    pub typedmark_src: String,
    pub selected: bool,
    pub converted: bool,
}

pub struct MigrationEngine;

impl MigrationEngine {
    /// Scan a directory or single file path for Markdown files (`.md`).
    pub fn scan(path: &Path) -> Vec<MigrationItem> {
        let mut items = Vec::new();
        if path.is_file() {
            if is_markdown_file(path) {
                if let Some(item) = Self::load_item(path) {
                    items.push(item);
                }
            }
        } else if path.is_dir() {
            for entry in WalkDir::new(path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
            {
                let p = entry.path();
                if is_markdown_file(p) {
                    if let Some(item) = Self::load_item(p) {
                        items.push(item);
                    }
                }
            }
        }
        items.sort_by(|a, b| a.source_path.cmp(&b.source_path));
        items
    }

    fn load_item(path: &Path) -> Option<MigrationItem> {
        let markdown_src = fs::read_to_string(path).ok()?;
        let target_path = path.with_extension("tm");

        let doc = typedmark_markdown::from_markdown(&markdown_src);
        let typedmark_src = document_to_tm(&doc);

        Some(MigrationItem {
            source_path: path.to_path_buf(),
            target_path,
            markdown_src,
            typedmark_src,
            selected: true,
            converted: false,
        })
    }

    /// Convert selected items and write `.tm` files.
    pub fn execute(items: &mut [MigrationItem], remove_original: bool) -> anyhow::Result<usize> {
        let mut count = 0;
        for item in items.iter_mut() {
            if !item.selected || item.converted {
                continue;
            }
            fs::write(&item.target_path, &item.typedmark_src)?;
            item.converted = true;
            count += 1;
            if remove_original && item.source_path != item.target_path {
                let _ = fs::remove_file(&item.source_path);
            }
        }
        Ok(count)
    }
}

fn is_markdown_file(p: &Path) -> bool {
    p.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown"))
        .unwrap_or(false)
}
