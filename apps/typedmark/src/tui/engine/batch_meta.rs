//! Batch Metadata (`@meta` / `@config`) Editor Engine.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use typedmark_ast::{Block, Document, Element, ElementValue, Inline, Sigil, Value};
use typedmark_parser::parse_document;
use typedmark_semantics::classify;

use super::printer::document_to_tm;

#[derive(Debug, Clone)]
pub struct MetaFileEntry {
    pub path: PathBuf,
    pub original_src: String,
    pub modified_src: String,
    pub metadata: BTreeMap<String, String>,
    pub selected: bool,
}

pub struct BatchMetaEngine;

impl BatchMetaEngine {
    /// Scan directory for `.tm` / `.tmt` files and extract `@meta` and `@config` key-value maps.
    pub fn scan(dir: &Path) -> Vec<MetaFileEntry> {
        let mut entries = Vec::new();
        let paths = collect_tm_files(dir);
        for path in paths {
            if let Ok(src) = fs::read_to_string(&path) {
                let metadata = extract_metadata(&src);
                entries.push(MetaFileEntry {
                    path,
                    original_src: src.clone(),
                    modified_src: src,
                    metadata,
                    selected: true,
                });
            }
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

pub fn collect_tm_files(path: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if path.is_file() {
        if is_tm_file(path) {
            files.push(path.to_path_buf());
        }
    } else if path.is_dir() {
        for entry in WalkDir::new(path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let p = entry.path();
            if is_tm_file(p) {
                files.push(p.to_path_buf());
            }
        }
    }
    files.sort();
    files
}

fn is_tm_file(p: &Path) -> bool {
    p.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("tm") || ext.eq_ignore_ascii_case("tmt"))
        .unwrap_or(false)
}

/// Extract all `@meta` and `@config` metadata fields from document text.
pub fn extract_metadata(src: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    if let Ok(doc) = parse_document(src) {
        walk_elements(&doc, |el| {
            let kind = classify(el);
            let kind = kind.as_str();
            if kind == "meta" || kind == "config" {
                if let Some(ElementValue::Data(Value::Map(entries))) = &el.value {
                    for (k, v) in entries {
                        map.insert(format!("{kind}.{k}"), value_to_string(v));
                    }
                }
                if let Some(Value::Map(entries)) = &el.args {
                    for (k, v) in entries {
                        map.insert(format!("{kind}.(args).{k}"), value_to_string(v));
                    }
                }
            }
        });
    }
    map
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

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        Value::Seq(items) => {
            let s: Vec<_> = items.iter().map(value_to_string).collect();
            format!("[{}]", s.join(", "))
        }
        Value::Map(entries) => {
            let s: Vec<_> = entries
                .iter()
                .map(|(k, v)| format!("{k}: {}", value_to_string(v)))
                .collect();
            format!("{{{}}}", s.join(", "))
        }
    }
}

fn walk_elements<F>(doc: &Document, mut f: F)
where
    F: FnMut(&Element),
{
    for block in &doc.blocks {
        walk_block(block, &mut f);
    }
}

pub(crate) fn walk_block<F>(block: &Block, f: &mut F)
where
    F: FnMut(&Element),
{
    match block {
        Block::Element(el) => walk_element(el, f),
        Block::Paragraph(p) => {
            for inline in &p.content {
                if let Inline::Element(el) = inline {
                    walk_element(el, f);
                }
            }
        }
        Block::Heading(h) => {
            for inline in &h.content {
                if let Inline::Element(el) = inline {
                    walk_element(el, f);
                }
            }
        }
        Block::List(list) => {
            for item in &list.items {
                for inline in &item.content {
                    if let Inline::Element(el) = inline {
                        walk_element(el, f);
                    }
                }
            }
        }
    }
}

fn walk_element<F>(el: &Element, f: &mut F)
where
    F: FnMut(&Element),
{
    f(el);
    if let Some(inlines) = &el.content {
        for inline in inlines {
            if let Inline::Element(child_el) = inline {
                walk_element(child_el, f);
            }
        }
    }
    if let Some(ElementValue::Children(children)) = &el.value {
        for child in children {
            walk_element(child, f);
        }
    }
}

fn walk_elements_mut<F>(doc: &mut Document, mut f: F)
where
    F: FnMut(&mut Element),
{
    for block in &mut doc.blocks {
        walk_block_mut(block, &mut f);
    }
}

pub(crate) fn walk_block_mut<F>(block: &mut Block, f: &mut F)
where
    F: FnMut(&mut Element),
{
    match block {
        Block::Element(el) => walk_element_mut(el, f),
        Block::Paragraph(p) => {
            for inline in &mut p.content {
                if let Inline::Element(el) = inline {
                    walk_element_mut(el, f);
                }
            }
        }
        Block::Heading(h) => {
            for inline in &mut h.content {
                if let Inline::Element(el) = inline {
                    walk_element_mut(el, f);
                }
            }
        }
        Block::List(list) => {
            for item in &mut list.items {
                for inline in &mut item.content {
                    if let Inline::Element(el) = inline {
                        walk_element_mut(el, f);
                    }
                }
            }
        }
    }
}

fn walk_element_mut<F>(el: &mut Element, f: &mut F)
where
    F: FnMut(&mut Element),
{
    f(el);
    if let Some(inlines) = &mut el.content {
        for inline in inlines {
            if let Inline::Element(child_el) = inline {
                walk_element_mut(child_el, f);
            }
        }
    }
    if let Some(ElementValue::Children(children)) = &mut el.value {
        for child in children {
            walk_element_mut(child, f);
        }
    }
}
