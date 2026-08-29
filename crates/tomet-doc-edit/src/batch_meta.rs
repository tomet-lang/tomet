//! Batch `@meta`/`@config` key-value editing across a directory.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use tomet_ast::{Block, Document, Element, ElementValue, Sigil, Value};
use tomet_indexer::extract_metadata;
use tomet_parser::parse_document;
use tomet_printer::document_to_tm;
use tomet_semantics::classify;

#[derive(Debug, Clone)]
pub struct MetaFileEntry {
    pub path: PathBuf,
    pub original_src: String,
    pub modified_src: String,
    pub metadata: BTreeMap<String, String>,
    pub selected: bool,
}

impl MetaFileEntry {
    pub fn ensure_loaded(&mut self) {
        if self.original_src.is_empty() {
            if let Ok(src) = fs::read_to_string(&self.path) {
                self.metadata = extract_metadata(&src);
                self.modified_src = src.clone();
                self.original_src = src;
            }
        }
    }
}

pub struct BatchMetaEngine;

impl BatchMetaEngine {
    /// Wrap each path into an unloaded `MetaFileEntry` (content is read
    /// lazily later via `ensure_loaded`).
    pub fn entries_from_paths(paths: Vec<PathBuf>) -> Vec<MetaFileEntry> {
        paths
            .into_iter()
            .map(|path| MetaFileEntry {
                path,
                original_src: String::new(),
                modified_src: String::new(),
                metadata: BTreeMap::new(),
                selected: true,
            })
            .collect()
    }

    /// Set or update a key-value pair in `@meta` across all selected files.
    pub fn update_meta_key(
        entries: &mut [MetaFileEntry],
        target_element: &str,
        key: &str,
        new_value: &str,
    ) {
        for entry in entries.iter_mut() {
            if !entry.selected {
                continue;
            }
            entry.ensure_loaded();
            if let Ok(mut doc) = parse_document(&entry.original_src) {
                if set_meta_in_doc(&mut doc, target_element, key, new_value) {
                    entry.modified_src = document_to_tm(&doc);
                    entry.metadata = extract_metadata(&entry.modified_src);
                }
            }
        }
    }

    /// Save modified contents to disk.
    pub fn save(entries: &mut [MetaFileEntry]) -> anyhow::Result<usize> {
        let mut count = 0;
        for entry in entries.iter_mut() {
            if entry.selected && entry.original_src != entry.modified_src {
                fs::write(&entry.path, &entry.modified_src)?;
                entry.original_src = entry.modified_src.clone();
                count += 1;
            }
        }
        Ok(count)
    }
}

fn set_meta_in_doc(doc: &mut Document, target_element: &str, key: &str, new_val: &str) -> bool {
    let mut found = false;

    tomet_walker::for_each_element_mut(doc, |el| {
        let kind = classify(el);
        if kind.as_str() == target_element {
            found = true;
            tomet_walker::element_set_prop(el, key, Value::String(new_val.to_string()));
        }
    });

    if !found {
        // Add new @meta block at the beginning of the document
        let new_el = Element {
            sigil: Sigil::At(Some(target_element.to_string())),
            args: None,
            content: None,
            children: None,
            value: Some(ElementValue::Data(Value::Map(vec![(
                key.to_string(),
                Value::String(new_val.to_string()),
            )]))),
            span: Default::default(),
        };
        doc.blocks.insert(0, Block::Element(new_el));
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_meta_update() {
        let src = "@meta{author: Bob}\n\n#[Document]\n";
        let entry = MetaFileEntry {
            path: "test.tmt".into(),
            original_src: src.to_string(),
            modified_src: src.to_string(),
            metadata: extract_metadata(src),
            selected: true,
        };

        let mut entries = vec![entry];
        BatchMetaEngine::update_meta_key(&mut entries, "meta", "author", "Alice");
        assert!(entries[0].modified_src.contains("author: Alice"));
    }
}
