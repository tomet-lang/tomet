//! Whitespace-hygiene and layout formatter for `.tmt` source text.
//!
//! [`format_source`] is AST-aware using source `Span`
//! metadata. Normalizes whitespace policy (LF line endings, no trailing
//! whitespace, collapsed excess blank lines, exactly one final newline)
//! while losslessly preserving literal whitespace and blank lines inside
//! raw/verbatim content (a ``` fenced code block, nested in `[content]`
//! or at the document's own top level). It never changes the parsed
//! `File` -- the `does_not_change_the_parsed_document` test in the
//! `tomet-tests` package asserts this across the whole shared corpus, and
//! its `KNOWN_FORMAT_CHANGES_DOCUMENT` exception list is empty.
//!
//! The list is empty because a raw body is a backtick fence, which is
//! delimited by a *line*: no character inside the body can end it early
//! or make the formatter disagree with the parser about where verbatim
//! content begins. Were a raw body delimited by matched brackets, a
//! source that wrote full-width `｛｝` where the grammar wants ASCII `{}`
//! would not opt into raw at all, leave no raw span to protect, and have
//! the trailing-whitespace rule run over content meant to be verbatim.
//! The bug class is excluded by construction rather than by exception.
//!
//! [`format_source_with_config`] formats tables according to `PrinterConfig`
//! (e.g. `table.adjust_width`, `table.max_col_width`, `table.align`), sets paragraphs
//! off by a blank line or a `;` (`paragraph.separator`, [`format_paragraph_separators`]),
//! aligns the values of a multi-line `(...)` group (`map.align_values`,
//! [`format_map_alignment`]), lays out the content of a callout or a list item that runs over several
//! lines (`callout.style.content`, `list.multiline.style.content`, [`format_content_layouts`]), spaces
//! the brackets of a heading and the colon of a link's `target` (`heading.space_inside_brackets`,
//! `link.no_space`, [`format_heading_brackets`], [`format_link_targets`]), lays the fields of a
//! `::meta` out (`meta.always_newline`, [`format_meta_layout`]) and applies
//! [`format_source`]. It never changes what document metadata (`::meta`) says -- a field's type, format
//! or prefix is the printer's business, not its -- and injects no structural elements.

pub mod align;
pub mod clean;
pub mod data;
mod edit;
pub mod layout;
pub mod meta;
pub mod paragraph;
pub mod reorder;
pub mod spacing;
pub mod table;

#[cfg(test)]
mod tests;

pub use align::format_map_alignment;
pub use clean::clean_whitespace;
pub use data::format_data_source;
pub use layout::format_content_layouts;
pub use meta::format_meta_layout;
pub use paragraph::format_paragraph_separators;
pub use reorder::format_element_group_order;
pub use spacing::{format_heading_brackets, format_link_targets};
pub use table::format_tables_with_config;

use tomet_config::PrinterConfig;

/// Format `src` in place (returns a new `String`). Idempotent:
/// `format_source(&format_source(src)) == format_source(src)`.
pub fn format_source(src: &str) -> String {
    let normalized = src.replace("\r\n", "\n").replace('\r', "\n");
    clean::clean_whitespace(&normalized)
}

/// Formats `src` by applying configured table layout formatting, element group order, paragraph
/// separators, value alignment, and whitespace hygiene.
/// Never changes what metadata says, only where its fields break and how they are aligned, and inserts no structural blocks.
pub fn format_source_with_config(src: &str, config: &PrinterConfig) -> String {
    let formatted_tables = table::format_tables_with_config(src, config);
    let formatted_elements =
        if config.format.group_order.is_some() || config.format.link_group_order.is_some() {
            reorder::format_element_group_order(&formatted_tables, config)
        } else {
            formatted_tables
        };
    let laid_out = layout::format_content_layouts(&formatted_elements, config);
    let metad = meta::format_meta_layout(&laid_out, config);
    let separated = paragraph::format_paragraph_separators(&metad, config);
    let aligned = align::format_map_alignment(&separated, config);
    let headed = spacing::format_heading_brackets(&aligned, config);
    let linked = spacing::format_link_targets(&headed, config);
    format_source(&linked)
}
