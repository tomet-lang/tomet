//! The vault-wide metadata table an index document's `${filter(...)}` is
//! answered from.
//!
//! `tomet-indexer` does the I/O and `tomet-transform` does the rewrite,
//! and neither may know about the other -- the transform sits a layer
//! below (`crate-layering` in the root `.writ.tmt`: transform is layer 4,
//! indexer is layer 5). This crate is already the place where a vault's
//! pieces are assembled, so the join happens here and neither of them
//! grows a dependency for it.
//!
//! Building the table parses every `.tmt` in the vault, which is why
//! [`Vault`](crate::Vault) holds it behind a `OnceLock` and fills it on
//! the first document that actually carries a query.

use std::path::Path;

use tomet_ast::{Document, Value};
use tomet_transform::{IndexRow, expand_index_queries};

pub use tomet_transform::{IndexQueryError, has_index_query};

/// Every document in one vault, as `${filter(...)}` can query them.
pub struct VaultIndex {
    rows: Vec<IndexRow>,
}

impl VaultIndex {
    /// Reads the vault governing `root`.
    ///
    /// The config found from `root` decides what is in the vault, so
    /// `ignore`/`unswept` mean here what they mean to every other sweep.
    pub fn build(root: &Path) -> Self {
        let (config, config_root) = tomet_config::find_config_file(root)
            .map(|(config, _, config_root)| (config, config_root))
            .unwrap_or_else(|| (tomet_config::PrinterConfig::default(), root.to_path_buf()));

        let rows = tomet_indexer::collect_metadata_table(root, &config, &config_root)
            .into_iter()
            .map(|file| IndexRow {
                // `document_fields` already measured this from the project
                // root and wrote it with forward slashes, which is the
                // spelling a generated `@file(...)` has to carry. Falling
                // back to the absolute path would produce a link that
                // resolves only on this machine, so it is worth taking the
                // one the table computed.
                path: match file.fields.get("path") {
                    Some(Value::String(path)) => path.clone(),
                    _ => file.path.to_string_lossy().replace('\\', "/"),
                },
                fields: file.fields,
            })
            .collect();

        Self { rows }
    }

    /// Expands every `${filter(...)}` in `doc`, returning how many.
    pub fn expand(&self, doc: &mut Document) -> Result<usize, IndexQueryError> {
        expand_index_queries(doc, &self.rows)
    }

    pub fn rows(&self) -> &[IndexRow] {
        &self.rows
    }
}
