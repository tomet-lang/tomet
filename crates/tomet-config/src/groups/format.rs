//! Formatting and printer rule configuration ([`FormatConfig`], [`ElementConfig`]).

use super::parse_flag;
use std::collections::BTreeMap;
use tomet_ast::Value;
use tomet_spec_derive::Config;
use tomet_style::{
    BracketSpacing, ColonSpacing, ContentLayout, FenceLines, GroupOrder, ParagraphSeparator,
    SectionTitleStyle,
};
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

fn parse_section_title_style(value: &Value) -> Option<SectionTitleStyle> {
    SectionTitleStyle::from_name(value.as_str()?)
}

fn parse_bracket_spacing(value: &Value) -> Option<BracketSpacing> {
    parse_flag(value).map(|padded| {
        if padded {
            BracketSpacing::Padded
        } else {
            BracketSpacing::Tight
        }
    })
}

fn parse_colon_spacing(value: &Value) -> Option<ColonSpacing> {
    parse_flag(value).map(|no_space| {
        if no_space {
            ColonSpacing::Tight
        } else {
            ColonSpacing::Spaced
        }
    })
}

fn parse_content_layout(value: &Value) -> Option<ContentLayout> {
    ContentLayout::from_name(value.as_str()?)
}

fn parse_fence_lines(value: &Value) -> Option<FenceLines> {
    FenceLines::from_name(value.as_str()?)
}

/// Configuration for a specific element type (e.g. `element.link.*`, `element.table.*`, or custom `element.<name>.*`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ElementConfig {
    /// Delimiter group order for this element (`args_first` or `content_first`).
    pub group_order: Option<GroupOrder>,
    /// Whether a lone `target` argument has no space after colon (`target:x` vs `target: x`).
    pub no_space: Option<ColonSpacing>,
    /// How multi-line content is laid out between brackets (`expanded`, `block`, `box`).
    pub content_style: Option<ContentLayout>,
    /// Table column width alignment behavior ("auto", "true", "false").
    pub table_adjust_width: Option<String>,
    /// Maximum column width threshold for table alignment.
    pub table_max_col_width: Option<usize>,
    /// Default text alignment in table cells ("left", "center", "right").
    pub table_align: Option<String>,
    /// Custom / open-world properties defined under `element.<name>.*`.
    pub custom: BTreeMap<String, Value>,
}

impl ElementConfig {
    /// Retrieves a setting value by property path (e.g. `"style.content"` or `"foo"`).
    pub fn get(&self, path: &str) -> Option<&Value> {
        let parts: Vec<&str> = path.split('.').collect();
        let first = parts[0];
        let mut curr = self.custom.get(first)?;
        for &segment in &parts[1..] {
            curr = curr.get(segment)?;
        }
        Some(curr)
    }
}

fn parse_element_configs(value: &Value) -> Option<BTreeMap<String, ElementConfig>> {
    let mut elements = BTreeMap::new();
    let Value::Tuple(tuple) = value else {
        return Some(elements);
    };

    for (name, val) in tuple.named() {
        // "group_order" at root of element is the global default scalar
        if name == "group_order" {
            continue;
        }
        if let Value::Tuple(props) = val {
            let mut elem_cfg = ElementConfig::default();
            for (prop_k, prop_v) in props.named() {
                match prop_k {
                    "group_order" => elem_cfg.group_order = parse_group_order(prop_v),
                    "no_space" => elem_cfg.no_space = parse_colon_spacing(prop_v),
                    "style" => {
                        if let Some(content_v) = prop_v.get("content") {
                            elem_cfg.content_style = parse_content_layout(content_v);
                        }
                    }
                    "adjust_width" => {
                        elem_cfg.table_adjust_width = match prop_v {
                            Value::Bool(b) => Some(b.to_string()),
                            Value::String(s) => Some(s.clone()),
                            _ => None,
                        };
                    }
                    "max_col_width" => {
                        elem_cfg.table_max_col_width = prop_v.as_i64().map(|n| n as usize);
                    }
                    "align" => {
                        elem_cfg.table_align = prop_v.as_str().map(String::from);
                    }
                    _ => {}
                }
                elem_cfg.custom.insert(prop_k.to_string(), prop_v.clone());
            }
            elements.insert(name.to_string(), elem_cfg);
        }
    }
    Some(elements)
}

/// Formatting and printer rule configuration.
#[derive(Debug, Clone, Default, PartialEq, Config)]
#[config(group = "format")]
pub struct FormatConfig {
    /// How the body lines of a fenced code block are written: `marked` or `bare` ([`FenceLines`]).
    /// The marked form (`marked`) is the canonical spelling of the language. Unset, the formatter
    /// leaves each block as written, and the printer writes the canonical marked form.
    #[config(
        key = "code.fence.lines",
        type = "string",
        parse_with = "parse_fence_lines"
    )]
    pub code_fence_lines: Option<FenceLines>,

    /// How a paragraph is separated from the block after it (`blank` or `semicolon`). Applies
    /// between paragraphs and the elements, holes, type seats and interpolations that stand like one; a
    /// section, a list or a code block keeps the blank line it has. Unset, the formatter leaves
    /// each separator as written and the printer, which has none to keep, writes a blank line.
    #[config(
        key = "paragraph.separator",
        type = "string",
        parse_with = "parse_paragraph_separator"
    )]
    pub paragraph_separator: Option<ParagraphSeparator>,

    /// Align the values of consecutive `key: value` lines of a `(...)` group written over several
    /// lines, the colon staying on the key (`meta:    (...)` over `section: (...)`). A blank line,
    /// a comment, or an entry whose value runs over several lines ends a run. Off, each line keeps
    /// the spacing it has.
    #[config(key = "map.align_values", default = false)]
    pub map_align_values: bool,

    /// Widest key, in display columns, that `map.align_values` pads the others to. A longer key
    /// is written `key: value` and neither widens the others nor ends the run.
    #[config(key = "map.max_key_width", default = 20)]
    pub map_max_key_width: Option<usize>,

    /// How the title of a section is written: `bracketed` (`===[ title ]`) or `bare` (`=== title`) ([`SectionTitleStyle`]).
    /// Unset, the formatter leaves each section as written, and the printer writes `bracketed`.
    #[config(
        key = "section.title",
        type = "string",
        parse_with = "parse_section_title_style"
    )]
    pub section_title: Option<SectionTitleStyle>,

    /// Whether the title of a bracketed section has a space inside its brackets (`=[ title ]` vs
    /// `=[title]`). Unset, the formatter leaves each section as written and the printer writes
    /// none.
    #[config(
        key = "section.space_inside_brackets",
        type = "bool",
        parse_with = "parse_bracket_spacing"
    )]
    pub section_brackets: Option<BracketSpacing>,

    /// Whether a blank line is placed before a section heading (`=[title]`). Unset, the formatter leaves
    /// each as written.
    #[config(key = "section.blank_before", type = "bool")]
    pub section_blank_before: Option<bool>,

    /// Whether a blank line is placed after a section heading (`=[title]`). Unset, the formatter leaves
    /// each as written.
    #[config(key = "section.blank_after", type = "bool")]
    pub section_blank_after: Option<bool>,

    /// Whether a blank line is placed between consecutive preamble declarations (`::kind(...)`).
    /// Unset, the formatter leaves each as written.
    #[config(key = "declaration.blank_between", type = "bool")]
    pub declaration_blank_between: Option<bool>,

    /// Whether a blank line is placed after the preamble declarations (`::kind(...)`), before the
    /// document body. Unset, the formatter leaves each as written.
    #[config(key = "declaration.blank_after", type = "bool")]
    pub declaration_blank_after: Option<bool>,

    /// Delimiter group order for links (`()[]` vs `[]()`).
    #[config(
        key = "element.link.group_order",
        type = "string",
        default = "()[]",
        parse_with = "parse_group_order"
    )]
    pub link_group_order: Option<GroupOrder>,

    /// Default delimiter group order for all elements.
    #[config(
        key = "element.group_order",
        type = "string",
        default = "args_first",
        parse_with = "parse_group_order"
    )]
    pub group_order: Option<GroupOrder>,

    /// Whether a `target` that is the only argument of an element, as in `@link(target: x)`, has no
    /// space after its colon (`target:x` vs `target: x`). Any other argument is written with the
    /// space. Unset, the formatter leaves each as written and the printer writes the space.
    #[config(
        key = "element.link.no_space",
        type = "bool",
        parse_with = "parse_colon_spacing"
    )]
    pub link_target_colon: Option<ColonSpacing>,

    /// How the content of a `@callout` is laid out between its brackets: `expanded`, `block` or
    /// `box` ([`ContentLayout`]). Unset, or any other value, the printer writes it as it writes the
    /// content of any element.
    #[config(
        key = "element.callout.style.content",
        type = "string",
        parse_with = "parse_content_layout"
    )]
    pub callout_content_style: Option<ContentLayout>,

    /// How the content of a list item that runs over several lines is laid out between its
    /// brackets: `expanded`, `block` or `box` ([`ContentLayout`]). Unset, or any other value, the
    /// printer writes the content after the marker as it is.
    #[config(
        key = "list.multiline.style.content",
        type = "string",
        parse_with = "parse_content_layout"
    )]
    pub list_multiline_style_content: Option<ContentLayout>,

    /// Table column width alignment behavior ("auto", "true", "false").
    #[config(
        key = "element.table.adjust_width",
        type = "string | bool",
        default = "auto"
    )]
    pub table_adjust_width: Option<String>,

    /// Maximum column width threshold for table alignment.
    #[config(key = "element.table.max_col_width", default = 20)]
    pub table_max_col_width: Option<usize>,

    /// Default text alignment in table cells ("left", "center", "right").
    #[config(key = "element.table.align", type = "string")]
    pub table_align: Option<String>,

    /// Configuration for specific elements (`element.<name>.*`), including custom elements.
    #[config(
        key = "element",
        type = "map",
        hide,
        parse_with = "parse_element_configs"
    )]
    pub elements: BTreeMap<String, ElementConfig>,
}

impl FormatConfig {
    /// [`FormatConfig::map_max_key_width`], or its default when unset.
    pub fn max_key_width(&self) -> usize {
        self.map_max_key_width.unwrap_or(20)
    }

    /// Configuration for a specific element name (e.g. "link", "table", "callout", or custom).
    pub fn element(&self, name: &str) -> Option<&ElementConfig> {
        self.elements.get(name)
    }

    /// Returns the configured group order for a specific element name (e.g. "link"),
    /// falling back to the element-specific override in `elements`, then the specific field,
    /// then the global `group_order`.
    pub fn element_group_order(&self, element_name: &str) -> Option<GroupOrder> {
        if let Some(el) = self.elements.get(element_name)
            && let Some(order) = el.group_order
        {
            return Some(order);
        }
        if element_name == "link"
            && let Some(order) = self.link_group_order
        {
            return Some(order);
        }
        self.group_order
    }

    /// Returns the colon spacing for a specific element's `target` argument (e.g. "link"),
    /// falling back to specific `link_target_colon`.
    pub fn element_target_colon(&self, element_name: &str) -> Option<ColonSpacing> {
        if let Some(el) = self.elements.get(element_name)
            && let Some(spacing) = el.no_space
        {
            return Some(spacing);
        }
        if element_name == "link" {
            return self.link_target_colon;
        }
        None
    }

    /// Returns the content layout for an element (e.g. "callout"),
    /// falling back to specific `callout_content_style`.
    pub fn element_content_style(&self, element_name: &str) -> Option<ContentLayout> {
        if let Some(el) = self.elements.get(element_name)
            && let Some(style) = el.content_style
        {
            return Some(style);
        }
        if element_name == "callout" {
            return self.callout_content_style;
        }
        None
    }

    /// Table adjust width setting, checking `element.table` override first.
    pub fn table_adjust_width(&self) -> Option<&str> {
        if let Some(el) = self.elements.get("table")
            && let Some(w) = el.table_adjust_width.as_deref()
        {
            return Some(w);
        }
        self.table_adjust_width.as_deref()
    }

    /// Table max column width setting, checking `element.table` override first.
    pub fn table_max_col_width(&self) -> Option<usize> {
        if let Some(el) = self.elements.get("table")
            && let Some(w) = el.table_max_col_width
        {
            return Some(w);
        }
        self.table_max_col_width
    }

    /// Table alignment setting, checking `element.table` override first.
    pub fn table_align(&self) -> Option<&str> {
        if let Some(el) = self.elements.get("table")
            && let Some(a) = el.table_align.as_deref()
        {
            return Some(a);
        }
        self.table_align.as_deref()
    }
}
