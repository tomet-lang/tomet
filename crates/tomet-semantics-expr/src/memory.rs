//! A resolver over documents held in memory.

use std::collections::BTreeMap;
use std::sync::Arc;

use tomet_ast::{Block, Document, Value};

use super::context::Resolver;
use super::value::{DirRef, DocRef, EvalValue};

/// Documents by vault-relative path.
///
/// `meta` is the document's `@meta` as written, with no settings applied: a
/// `list` field that the document lacks is simply absent here. The vault's
/// resolver, which knows the settings, is what gives the typed view.
#[derive(Default)]
pub struct MemoryResolver {
    docs: BTreeMap<String, Arc<Document>>,
}

impl MemoryResolver {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an already parsed document at `path`.
    pub fn with_document(mut self, path: &str, doc: Arc<Document>) -> Self {
        self.docs.insert(path.to_string(), doc);
        self
    }

    /// Adds the document read from `source` at `path`.
    pub fn with_source(mut self, path: &str, source: &str) -> Result<Self, tomet_parser::Error> {
        let doc = tomet_parser::parse_document(source)?;
        self.docs.insert(path.to_string(), Arc::new(doc));
        Ok(self)
    }
}

impl Resolver for MemoryResolver {
    fn document(&self, doc: &DocRef) -> Result<Arc<Document>, String> {
        self.docs
            .get(&doc.0)
            .cloned()
            .ok_or_else(|| "not found".to_string())
    }

    fn note(&self, name: &str) -> Result<DocRef, String> {
        let stem = |path: &str| {
            path.rsplit('/')
                .next()
                .unwrap_or(path)
                .trim_end_matches(".tmt")
                .to_string()
        };
        self.docs
            .keys()
            .find(|path| path.as_str() == name || stem(path) == name)
            .map(|path| DocRef(path.clone()))
            .ok_or_else(|| "no such note".to_string())
    }

    fn dir_docs(&self, dir: &DirRef) -> Result<Vec<DocRef>, String> {
        let prefix = if dir.0.is_empty() {
            String::new()
        } else {
            format!("{}/", dir.0)
        };
        Ok(self
            .docs
            .keys()
            .filter(|path| path.starts_with(&prefix))
            .map(|path| DocRef(path.clone()))
            .collect())
    }

    fn meta(&self, doc: &DocRef) -> Result<EvalValue, String> {
        let document = self.document(doc)?;
        Ok(match tomet_semantics::document_meta(&document) {
            Some(meta) => EvalValue::from_ast(&meta, doc),
            None => EvalValue::from_ast(&Value::Map(Vec::new()), doc),
        })
    }

    fn prepare_blocks(
        &self,
        _doc: &DocRef,
        blocks: Vec<Block>,
        _includes: &[DocRef],
    ) -> Result<Vec<Block>, String> {
        // Nothing here evaluates; the blocks come back as they are.
        Ok(blocks)
    }
}
