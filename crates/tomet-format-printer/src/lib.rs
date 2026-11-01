//! Serializes a `tomet_ast::File` back to `.tmt` source text
//! (no span info required, unlike `tomet-formatter`, which
//! re-formats existing `.tmt` text losslessly using spans -- this crate
//! is for documents that never had `.tmt` source to begin with, e.g.
//! ones built from Markdown or edited purely at the AST level). Uses
//! `tomet_config::PrinterConfig` (see that crate for config
//! loading/discovery) to drive formatting choices: meta format
//! (yaml/json/toml), link-key spacing, callout/list style, via
//! `tomet-style`'s single-node renderers. Also owns `::meta` id
//! auto-generation (`ensure_document_id_with_config`, which mutates the
//! AST directly), built on the pure generate/validate/convert helpers
//! in `tomet-field-utils`.

pub mod block;
pub mod element;
pub mod id;
pub mod inline;

#[cfg(test)]
mod tests;

pub use element::render_element;
pub use id::ensure_document_id_with_config;

use tomet_ast::File;
use tomet_config::PrinterConfig;

/// Serialize a [`File`] AST into Tomet (`.tmt`) source string using document-level config or defaults.
pub fn document_to_tm(doc: &File) -> String {
    let config = PrinterConfig::from_doc(doc);
    document_to_tm_with_config(doc, &config)
}

/// A [`tomet_ast::Hole`] as the box it was written as, `<?matcher(args){value}>[content]`,
/// with the document-independent default configuration.
///
/// For a reader that has no rendering of a hole and shows it as it was written, the way an
/// interpolation nobody evaluated is shown (`tomet-convert-*`).
pub fn hole_to_tm(hole: &tomet_ast::Hole) -> String {
    element::render_hole(hole, &PrinterConfig::default())
}

pub fn document_to_tm_with_config(doc: &File, config: &PrinterConfig) -> String {
    let mut out = String::new();
    block::render_file(doc, config, &mut out);
    tomet_formatter::format_source(&out)
}
