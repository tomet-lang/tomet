//! Formatting and printer rule configuration ([`FormatConfig`]).

use tomet_ast::Value;
use tomet_spec_derive::Config;
use tomet_style::{GroupOrder, ParagraphSeparator};
use tomet_tree::ValueExt;

fn parse_group_order(value: &Value) -> Option<GroupOrder> {
    match value.as_str()? {
        "content_first" | "content_args" | "[]()" | "content" => Some(GroupOrder::ContentFirst),
        "args_first" | "args_content" | "()[]" | "args" => Some(GroupOrder::ArgsFirst),
        _ => None,
    }
}

fn parse_paragraph_separator(value: &Value) -> Option<ParagraphSeparator> {
    match value.as_str()? {
        "blank" => Some(ParagraphSeparator::Blank),
        "semicolon" => Some(ParagraphSeparator::Semicolon),
        _ => None,
    }
}

/// Formatting and printer rule configuration.
#[derive(Debug, Clone, Default, PartialEq, Config)]
#[config(group = "format")]
pub struct FormatConfig {
    /// How a paragraph is separated from the block after it (`blank` or `semicolon`). Applies
    /// between paragraphs and the elements, holes and interpolations that stand like one; a
    /// heading, a list or a code block keeps the blank line it has. Unset, the formatter leaves
    /// each separator as written and the printer, which has none to keep, writes a blank line.
    #[config(
        key = "paragraph.separator",
        type = "string",
        parse_with = "parse_paragraph_separator"
    )]
    pub paragraph_separator: Option<ParagraphSeparator>,

    /// Align the values of consecutive `key: value` lines of a `(...)` group written over several
    /// lines, the colon staying on the key (`meta:    {...}` over `heading: {...}`). A blank line,
    /// a comment, or an entry whose value runs over several lines ends a run. Off, each line keeps
    /// the spacing it has.
    #[config(key = "map.align_values", default = false)]
    pub map_align_values: bool,

    /// Widest key, in display columns, that `map.align_values` pads the others to. A longer key
    /// is written `key: value` and neither widens the others nor ends the run.
    #[config(key = "map.max_key_width", default = 20)]
    pub map_max_key_width: Option<usize>,

    /// Space inside brackets for headings (`=[ title ]` vs `=[title]`).
    #[config(key = "heading.space_inside_brackets", default = false)]
    pub heading_space_inside_brackets: bool,

    /// Omit spaces around link content (`@[content]` vs `@ [content]`).
    #[config(key = "link.no_space", alias = "wikilink.no_space", default = false)]
    pub link_no_space: bool,

    /// Delimiter group order for links (`()[]` vs `[]()`).
    #[config(
        key = "link.group_order",
        alias = "link.order",
        alias = "wikilink.group_order",
        alias = "wikilink.order",
        type = "string",
        default = "()[]",
        parse_with = "parse_group_order"
    )]
    pub link_group_order: Option<GroupOrder>,

    /// Default delimiter group order for all elements.
    #[config(
        key = "element.group_order",
        alias = "element.order",
        alias = "group_order",
        type = "string",
        default = "args_first",
        parse_with = "parse_group_order"
    )]
    pub group_order: Option<GroupOrder>,

    /// Callout box content style (e.g. "block", "quote").
    #[config(key = "callout.style.content", type = "string")]
    pub callout_content_style: Option<String>,

    /// Multiline list item content style (e.g. "box", "bullet").
    #[config(key = "list.multiline.style.content", type = "string")]
    pub list_multiline_style_content: Option<String>,

    /// Table column width alignment behavior ("auto", "true", "false").
    #[config(key = "table.adjust_width", type = "string | bool", default = "auto")]
    pub table_adjust_width: Option<String>,

    /// Maximum column width threshold for table alignment.
    #[config(key = "table.max_col_width", default = 20)]
    pub table_max_col_width: Option<usize>,

    /// Default text alignment in table cells ("left", "center", "right").
    #[config(key = "table.align", type = "string")]
    pub table_align: Option<String>,
}

impl FormatConfig {
    /// [`FormatConfig::map_max_key_width`], or its default when unset.
    pub fn max_key_width(&self) -> usize {
        self.map_max_key_width.unwrap_or(20)
    }

    /// Returns the configured group order for a specific element name (e.g. "link"),
    /// falling back to the global `group_order`.
    pub fn element_group_order(&self, element_name: &str) -> Option<GroupOrder> {
        if (element_name == "link" || element_name == "wikilink")
            && let Some(order) = self.link_group_order
        {
            return Some(order);
        }
        self.group_order
    }
}
