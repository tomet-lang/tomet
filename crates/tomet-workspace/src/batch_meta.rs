//! Batch metadata editing across workspace files.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use tomet_indexer::extract_metadata;
use tomet_parser::parse_document;
use tomet_printer::document_to_tm;
use tomet_transform::set_meta_in_doc;

/// Represents a workspace file with its metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaFile {
    pub path: PathBuf,
    pub original_src: String,
    pub modified_src: String,
    pub metadata: BTreeMap<String, String>,
}

impl MetaFile {
    pub fn from_path(path: PathBuf) -> Self {
        Self {
            path,
            original_src: String::new(),
            modified_src: String::new(),
            metadata: BTreeMap::new(),
        }
    }

    /// Loads source content and extracts metadata lazily if not already loaded.
    pub fn ensure_loaded(&mut self) {
        if self.original_src.is_empty() {
            if let Ok(src) = fs::read_to_string(&self.path) {
                self.metadata = extract_metadata(&src);
                self.modified_src = src.clone();
                self.original_src = src;
            }
        }
    }

    /// Returns `true` if modified from original source.
    pub fn is_changed(&self) -> bool {
        self.original_src != self.modified_src
    }

    /// Saves the modified content to disk.
    pub fn save(&mut self) -> anyhow::Result<bool> {
        if self.is_changed() {
            if let Some(parent) = self.path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&self.path, &self.modified_src)?;
            self.original_src = self.modified_src.clone();
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

pub struct BatchMetaEngine;

impl BatchMetaEngine {
    /// Wraps paths into unloaded `MetaFile`s.
    pub fn files_from_paths(paths: Vec<PathBuf>) -> Vec<MetaFile> {
        paths.into_iter().map(MetaFile::from_path).collect()
    }

    /// Updates or inserts a key-value pair in a metadata element across files.
    pub fn update_meta_key(
        files: &mut [MetaFile],
        target_element: &str,
        key: &str,
        new_value: &str,
    ) {
        for file in files.iter_mut() {
            file.ensure_loaded();
            if let Ok(mut doc) = parse_document(&file.original_src) {
                if set_meta_in_doc(&mut doc, target_element, key, new_value) {
                    file.modified_src = document_to_tm(&doc);
                    file.metadata = extract_metadata(&file.modified_src);
                }
            }
        }
    }

    /// Saves modified files to disk, returning the count of saved files.
    pub fn save(files: &mut [MetaFile]) -> anyhow::Result<usize> {
        let mut count = 0;
        for file in files.iter_mut() {
            if file.save()? {
                count += 1;
            }
        }
        Ok(count)
    }
}
