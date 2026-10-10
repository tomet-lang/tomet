//! Formatting and printer rule configuration ([`FormatConfig`], [`GroupOrder`]).

use tomet_ast::Value;
use tomet_tree::ValueExt;

use crate::tree::get_path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GroupOrder {
    #[default]
    ArgsFirst, // @link(args)[content]{value}
    ContentFirst, // @link[content](args){value}
}

fn parse_group_order(value: &Value) -> Option<GroupOrder> {
    match value.as_str()? {
        "content_first" | "content_args" | "[]()" | "content" => Some(GroupOrder::ContentFirst),
        "args_first" | "args_content" | "()[]" | "args" => Some(GroupOrder::ArgsFirst),
        _ => None,
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FormatConfig {
    pub heading_space_inside_brackets: bool,
    pub link_no_space: bool,
    pub link_group_order: Option<GroupOrder>,
    pub group_order: Option<GroupOrder>,
    pub callout_content_style: Option<String>,
    pub list_multiline_style_content: Option<String>,
    pub table_adjust_width: Option<String>,
    pub table_max_col_width: Option<usize>,
    pub table_align: Option<String>,
}

impl FormatConfig {
    /// Extracts formatting configuration from a normalized configuration root.
    pub fn from_root(root: &Value) -> Self {
        let mut cfg = Self::default();

        // 1. Heading
        if let Some(space) =
            get_path(root, &["heading", "space_inside_brackets"]).and_then(|v| v.as_bool())
        {
            cfg.heading_space_inside_brackets = space;
        }

        // 2. Link and wikilink
        let parse_bool_or_str = |v: &Value| {
            if let Some(b) = v.as_bool() {
                Some(b)
            } else if let Some(s) = v.as_str() {
                Some(s == "true" || s == "1")
            } else {
                None
            }
        };
        if let Some(b) = get_path(root, &["link", "no_space"])
            .or_else(|| get_path(root, &["wikilink", "no_space"]))
            .and_then(parse_bool_or_str)
        {
            cfg.link_no_space = b;
        }
        if let Some(order) = get_path(root, &["link", "group_order"])
            .or_else(|| get_path(root, &["link", "order"]))
            .or_else(|| get_path(root, &["wikilink", "group_order"]))
            .or_else(|| get_path(root, &["wikilink", "order"]))
            .and_then(parse_group_order)
        {
            cfg.link_group_order = Some(order);
        }

        // 3. Element group order
        if let Some(order) = get_path(root, &["element", "group_order"])
            .or_else(|| get_path(root, &["element", "order"]))
            .or_else(|| get_path(root, &["group_order"]))
            .and_then(parse_group_order)
        {
            cfg.group_order = Some(order);
        }

        // 4. Callout
        if let Some(s) = get_path(root, &["callout", "style", "content"]).and_then(|v| v.as_str()) {
            cfg.callout_content_style = Some(s.to_string());
        }

        // 5. List
        if let Some(s) =
            get_path(root, &["list", "multiline", "style", "content"]).and_then(|v| v.as_str())
        {
            cfg.list_multiline_style_content = Some(s.to_string());
        }

        // 6. Table
        if let Some(v) = get_path(root, &["table", "adjust_width"]) {
            if let Some(b) = v.as_bool() {
                cfg.table_adjust_width = Some(b.to_string());
            } else if let Some(s) = v.as_str() {
                cfg.table_adjust_width = Some(s.to_string());
            }
        }
        if let Some(n) = get_path(root, &["table", "max_col_width"]).and_then(|v| v.as_i64()) {
            cfg.table_max_col_width = Some(n as usize);
        }
        if let Some(s) = get_path(root, &["table", "align"]).and_then(|v| v.as_str()) {
            cfg.table_align = Some(s.to_string());
        }

        cfg
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
