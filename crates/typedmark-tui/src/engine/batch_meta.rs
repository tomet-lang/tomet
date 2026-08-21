//! Batch Metadata (`@meta` / `@config`) Editor Engine.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use typedmark_ast::{Block, Document, Element, ElementValue, Sigil, Value};
use typedmark_indexer::{collect_tm_files_with_config, extract_metadata};
use typedmark_parser::parse_document;
use typedmark_printer::document_to_tm;
use typedmark_semantics::classify;

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
    /// Scan directory for `.tm` / `.tmt` files and extract `@meta` and `@config` key-value maps.
    pub fn scan(dir: &Path) -> Vec<MetaFileEntry> {
        let (config, _, config_root) = typedmark_printer::find_config_file(dir).unwrap_or_else(|| {
            (
                typedmark_printer::PrinterConfig::default(),
                dir.to_path_buf(),
                dir.to_path_buf(),
            )
        });
        Self::scan_with_config(dir, &config, &config_root)
    }

    pub fn scan_with_config(
        dir: &Path,
        config: &typedmark_printer::PrinterConfig,
        config_root: &Path,
    ) -> Vec<MetaFileEntry> {
        let mut entries = Vec::new();
        let paths = collect_tm_files_with_config(dir, config, config_root);
        for path in paths {
            entries.push(MetaFileEntry {
                path,
                original_src: String::new(),
                modified_src: String::new(),
                metadata: BTreeMap::new(),
                selected: true,
            });
        }
        entries
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
    let mut updated = false;
    let mut found = false;

    walk_elements_mut(doc, |el| {
        let kind = classify(el);
        if kind.as_str() == target_element {
            found = true;
            match &mut el.value {
                Some(ElementValue::Data(Value::Map(entries))) => {
                    if let Some((_, val)) = entries.iter_mut().find(|(k, _)| k == key) {
                        *val = Value::String(new_val.to_string());
                    } else {
                        entries.push((key.to_string(), Value::String(new_val.to_string())));
                    }
                    updated = true;
                }
                None => {
                    el.value = Some(ElementValue::Data(Value::Map(vec![(
                        key.to_string(),
                        Value::String(new_val.to_string()),
                    )])));
                    updated = true;
                }
                _ => {}
            }
        }
    });

    if !found {
        // Add new @meta block at the beginning of the document
        let new_el = Element {
            sigil: Sigil::At(Some(target_element.to_string())),
            args: None,
            content: None,
            value: Some(ElementValue::Data(Value::Map(vec![(
                key.to_string(),
                Value::String(new_val.to_string()),
            )]))),
            span: Default::default(),
        };
        doc.blocks.insert(0, Block::Element(new_el));
        updated = true;
    }

    updated
}

/// Visits every `Element` in `doc` with mutable access (document order,
/// including ones nested inside an element's `[content]` and
/// `ElementValue::Children`) via `typedmark-walker`'s generic mutable tree
/// walk -- see that crate's module doc for why this shape lives there
/// instead of being hand-rolled here.
pub(crate) fn walk_elements_mut<F>(doc: &mut Document, mut f: F)
where
    F: FnMut(&mut Element),
{
    struct ElementVisitorMut<F>(F);
    impl<F: FnMut(&mut Element)> typedmark_walker::VisitorMut<()> for ElementVisitorMut<F> {
        fn visit_mut(&mut self, node: typedmark_walker::NodeMut<'_>) -> std::ops::ControlFlow<()> {
            if let typedmark_walker::NodeMut::Element(el) = node {
                (self.0)(el);
            }
            std::ops::ControlFlow::Continue(())
        }
    }
    let _ = typedmark_walker::walk_document_mut(doc, &mut ElementVisitorMut(&mut f));
}
