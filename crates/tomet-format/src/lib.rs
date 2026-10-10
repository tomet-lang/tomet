//! Whitespace-hygiene and layout formatter for `.tmt` source text.
//!
//! [`format_source`] is AST-aware using source [`tomet_ast::Span`]
//! metadata. Normalizes whitespace policy (LF line endings, no trailing
//! whitespace, collapsed excess blank lines, exactly one final newline)
//! while losslessly preserving literal whitespace and blank lines inside
//! raw/verbatim content (a ``` fenced code block, nested in `[content]`
//! or at the document's own top level). It never changes the parsed
//! `Document` -- the `does_not_change_the_parsed_document` test in the
//! `tomet-tests` package asserts this across the whole shared corpus, and
//! its `KNOWN_FORMAT_CHANGES_DOCUMENT` exception list is empty.
//!
//! The list held one entry until `(content:raw)[...]` was retired. A raw
//! body used to be delimited by matched brackets, so a source that wrote
//! full-width `｛｝` where the grammar wants ASCII `{}` did not opt into
//! raw at all, left no raw span to protect, and had the trailing-
//! whitespace rule run over content meant to be verbatim. A backtick
//! fence is delimited by a *line*, so no character inside the body can
//! end it early or make the formatter disagree with the parser about
//! where verbatim content begins. The bug class is gone by construction
//! rather than by exception.
//!
//! [`format_source_with_config`] formats tables according to `PrinterConfig`
//! (e.g. `table.adjust_width`, `table.max_col_width`, `table.align`) and
//! applies [`format_source`]. It never alters document metadata (`@meta`)
//! or injects structural elements.

pub mod clean;
pub mod reorder;
pub mod table;

#[cfg(test)]
mod tests;

pub use clean::clean_whitespace;
pub use reorder::format_element_group_order;
pub use table::format_tables_with_config;

use tomet_config::PrinterConfig;

/// Format `src` in place (returns a new `String`). Idempotent:
/// `format_source(&format_source(src)) == format_source(src)`.
pub fn format_source(src: &str) -> String {
    let normalized = src.replace("\r\n", "\n").replace('\r', "\n");
    clean::clean_whitespace(&normalized)
}

/// Formats `src` by applying configured table layout formatting, element group order, and whitespace hygiene.
/// Never mutates metadata or inserts structural blocks.
pub fn format_source_with_config(src: &str, config: &PrinterConfig) -> String {
    let formatted_tables = table::format_tables_with_config(src, config);
    let formatted_elements =
        if config.format.group_order.is_some() || config.format.link_group_order.is_some() {
            reorder::format_element_group_order(&formatted_tables, config)
        } else {
            formatted_tables
        };
    format_source(&formatted_elements)
}
