//! Rendering elements, values, args, and connects.

use tomet_ast::{Element, ElementValue, Entry, Placement, Sigil, Value};
use tomet_config::{GroupOrder, PrinterConfig};
use tomet_style::render_nested;

use crate::id::render_id;
use crate::inline::render_content_blocks;

/// Render an [`Element`] AST node into Tomet syntax:
/// `<sigil><name>(args)[content]{value}`.
pub fn render_element(el: &Element, config: &PrinterConfig) -> String {
    {
        if el.sigil.is_bare_named("hr") && el.args.is_none() && el.value.is_none() {
            if let Some(content) = &el.content {
                return format!(
                    "---[{}]---{}{}",
                    render_content_blocks(content, config),
                    render_id(el.id.as_ref()),
                    render_connects(&el.connects, config)
                );
            } else {
                return format!(
                    "---{}{}",
                    render_id(el.id.as_ref()),
                    render_connects(&el.connects, config)
                );
            }
        }
    }

    if el.sigil.is_bare_named("meta") {
        let mut out = tomet_style::render_meta_element(el, config);
        out.push_str(&render_id(el.id.as_ref()));
        out.push_str(&render_connects(&el.connects, config));
        return out;
    }

    if el.sigil.is_bare_named("raw") {
        if el.placement == Placement::Inline
            && el.args.is_none()
            && el.value.is_none()
            && el.connects.is_empty()
        {
            let from_content = el
                .content
                .as_ref()
                .map(|content| render_content_blocks(content, config));
            let body = from_content.unwrap_or_default();
            let longest = body
                .split(|c| c != '`')
                .map(|run| run.len())
                .max()
                .unwrap_or(0);
            let fence = "`".repeat(longest + 1);
            let pad = if body.starts_with('`') || body.ends_with('`') {
                " "
            } else {
                ""
            };
            return format!("{fence}{pad}{body}{pad}{fence}");
        }

        if el.placement == Placement::Block {
            let lang = el
                .args
                .as_ref()
                .and_then(|args| match args {
                    Value::String(s) => Some(s.clone()),
                    Value::Map(entries) => {
                        entries
                            .iter()
                            .find(|(k, _)| k == "lang")
                            .and_then(|(_, v)| match v {
                                Value::String(s) => Some(s.clone()),
                                _ => None,
                            })
                    }
                    _ => None,
                })
                .unwrap_or_default();
            let body = el
                .content
                .as_ref()
                .map(|content| render_content_blocks(content, config))
                .unwrap_or_default();
            let longest = body
                .lines()
                .map(|l| l.trim_end())
                .filter(|l| !l.is_empty() && l.chars().all(|c| c == '`'))
                .map(str::len)
                .max()
                .unwrap_or(0);
            let fence = "`".repeat(longest.max(2) + 1);
            let mut out = format!("{fence}{lang}\n");
            out.push_str(&body);
            if !body.is_empty() && !body.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(&fence);
            out.push_str(&render_id(el.id.as_ref()));
            out.push_str(&render_connects(&el.connects, config));
            return out;
        }
    }

    if el.sigil.is_bare_named("callout")
        && let Some(style) = config.format.callout_content_style.as_deref()
    {
        if style == "expanded" {
            let mut out = String::from("@callout");
            if let Some(args) = &el.args {
                out.push('(');
                out.push_str(&render_args(args, config));
                out.push(')');
            }
            if let Some(content) = &el.content {
                let inlines_text = render_content_blocks(content, config);
                out.push_str("[\n");
                for line in inlines_text.lines() {
                    out.push_str("  ");
                    out.push_str(line);
                    out.push('\n');
                }
                out.push(']');
            }
            if let Some(value) = &el.value {
                out.push_str(&render_element_value(value, config));
            }
            out.push_str(&render_id(el.id.as_ref()));
            out.push_str(&render_connects(&el.connects, config));
            return out;
        } else if style == "block" {
            let mut out = String::from("@callout");
            if let Some(args) = &el.args {
                out.push('(');
                out.push_str(&render_args(args, config));
                out.push(')');
            }
            if let Some(content) = &el.content {
                let inlines_text = render_content_blocks(content, config);
                let lines: Vec<&str> = inlines_text.lines().collect();
                out.push('\n');
                if lines.is_empty() {
                    out.push_str("[ ]");
                } else if lines.len() == 1 {
                    out.push_str("[ ");
                    out.push_str(lines[0]);
                    out.push_str(" ]");
                } else {
                    for (idx, line) in lines.iter().enumerate() {
                        if idx == 0 {
                            out.push_str("[ ");
                            out.push_str(line);
                            out.push('\n');
                        } else {
                            out.push_str("  ");
                            out.push_str(line);
                            out.push('\n');
                        }
                    }
                    out.push(']');
                }
            }
            if let Some(value) = &el.value {
                out.push_str(&render_element_value(value, config));
            }
            out.push_str(&render_id(el.id.as_ref()));
            out.push_str(&render_connects(&el.connects, config));
            return out;
        } else if style == "box" {
            let mut out = String::from("@callout");
            if let Some(args) = &el.args {
                out.push('(');
                out.push_str(&render_args(args, config));
                out.push(')');
            }
            if let Some(content) = &el.content {
                let inlines_text = render_content_blocks(content, config);
                let lines: Vec<&str> = inlines_text.lines().collect();
                out.push('\n');
                if lines.is_empty() {
                    out.push_str("[ ]");
                } else if lines.len() == 1 {
                    out.push_str("[ ");
                    out.push_str(lines[0]);
                    out.push_str(" ]");
                } else {
                    for (idx, line) in lines.iter().enumerate() {
                        if idx == 0 {
                            out.push_str("[ ");
                            out.push_str(line);
                            out.push('\n');
                        } else if idx == lines.len() - 1 {
                            out.push_str("  ");
                            out.push_str(line);
                            out.push_str(" ]");
                        } else {
                            out.push_str("  ");
                            out.push_str(line);
                            out.push('\n');
                        }
                    }
                }
            }
            if let Some(value) = &el.value {
                out.push_str(&render_element_value(value, config));
            }
            out.push_str(&render_id(el.id.as_ref()));
            out.push_str(&render_connects(&el.connects, config));
            return out;
        }
    }

    let mut out = String::new();

    match &el.sigil {
        // One sigil, whatever the placement. A block-placed element is
        // told apart by standing alone on its line, which is the same
        // condition the parser reads it back with.
        Sigil::Named(name) => {
            out.push('@');
            out.push_str(&name.to_string());
        }
        Sigil::Bare => {}
        Sigil::Dollar => {
            out.push('$');
        }
        Sigil::Caret(name) => {
            out.push('^');
            if let Some(name) = name {
                out.push_str(&name.to_string());
            }
        }
    }

    let render_args_cb = |out: &mut String| {
        if let Some(args) = &el.args {
            out.push('(');
            out.push_str(&render_args(args, config));
            out.push(')');
        }
    };

    let render_content_cb = |out: &mut String| {
        if let Some(content) = &el.content {
            out.push('[');
            out.push_str(&render_content_blocks(content, config));
            out.push(']');
        }
    };

    let elem_name = el.sigil.name().map(|n| n.name.as_str()).unwrap_or("");
    let order = config
        .element_group_order(elem_name)
        .unwrap_or(GroupOrder::ArgsFirst);
    match order {
        GroupOrder::ArgsFirst => {
            render_args_cb(&mut out);
            render_content_cb(&mut out);
        }
        GroupOrder::ContentFirst => {
            render_content_cb(&mut out);
            render_args_cb(&mut out);
        }
    }

    if let Some(value) = &el.value {
        out.push_str(&render_element_value(value, config));
    }

    out.push_str(&render_id(el.id.as_ref()));
    out.push_str(&render_connects(&el.connects, config));

    out
}

/// Renders an element's value: a `{...}` group.
pub(crate) fn render_element_value(value: &ElementValue, config: &PrinterConfig) -> String {
    match value {
        ElementValue::Interp(expr) => format!("{{{expr}}}"),
        ElementValue::Group(entries) => {
            format!("{{{}}}", render_group_entries(entries, config))
        }
    }
}

/// Renders a value group's entries in source order.
///
/// Order is preserved deliberately: a group may interleave `key: value`
/// pairs and nested elements, and reordering them would change the
/// document the formatter promises not to change.
pub(crate) fn render_group_entries(entries: &[Entry], config: &PrinterConfig) -> String {
    if entries.is_empty() {
        return String::new();
    }
    let has_element = entries.iter().any(|e| matches!(e, Entry::Element(_)));
    let rendered: Vec<String> = entries
        .iter()
        .map(|entry| match entry {
            // `render_nested`, not `render_value_inner_with_config`: a
            // nested map needs its own braces here or `m: { k: v }`
            // prints as `m: k: v` and reparses as a string.
            Entry::Pair(k, v) => format!("{k}: {}", render_nested(v, config)),
            Entry::Element(child) => render_element(child, config),
        })
        .collect();

    // A group of pairs stays on one line; one holding elements gets a
    // line each, which is how `@links{ (1)[..] (2)[..] }` has always been
    // written.
    if !has_element {
        format!(" {} ", rendered.join(", "))
    } else if rendered.len() == 1 {
        format!(" {} ", rendered[0])
    } else {
        let mut out = String::from("\n");
        for line in &rendered {
            out.push_str("  ");
            out.push_str(line);
            out.push('\n');
        }
        out
    }
}

/// Prints each of `connects` as `:name(...)`/`:name[...]`/`:name{...}`,
/// stacked in source order right after whatever comes before it -- a
/// connect is structurally an ordinary `Element` (its own `args`/
/// `content`/`value`/`id`), just introduced by `:` instead of `@`, so
/// this reuses the same group renderers [`render_element`]'s own
/// generic tail does. No connect nests further connects (the parser
/// never lets one), so this does not recurse into `connect.connects`.
///
/// No leading space or newline: a connect attaches directly to what
/// precedes it, the same tight style `(args)[content]{value}` already
/// print in.
pub(crate) fn render_connects(connects: &[Element], config: &PrinterConfig) -> String {
    let mut out = String::new();
    for connect in connects {
        out.push(':');
        if let Some(name) = connect.sigil.name() {
            out.push_str(&name.to_string());
        }
        if let Some(args) = &connect.args {
            out.push('(');
            out.push_str(&render_args(args, config));
            out.push(')');
        }
        if let Some(content) = &connect.content {
            out.push('[');
            out.push_str(&render_content_blocks(content, config));
            out.push(']');
        }
        if let Some(value) = &connect.value {
            out.push_str(&render_element_value(value, config));
        }
        out.push_str(&render_id(connect.id.as_ref()));
    }
    out
}

/// Renders `(args)` with full block-content fidelity: any `key: [...]`
/// value (`Value::Blocks`, see `tomet-syntax-ast`) is rendered through
/// this crate's own `render_content_blocks` -- the real recursive block
/// renderer, which `tomet-format-style` (a lower layer) cannot call
/// itself -- rather than the restricted, paragraph-inlines-only fallback
/// `tomet_style::render_args_with_config` would otherwise use. Needed so
/// e.g. `@conflict(a: [multi-paragraph content], ...)` round-trips
/// without losing anything beyond the first paragraph.
pub(crate) fn render_args(args: &Value, config: &PrinterConfig) -> String {
    tomet_style::render_args_with_blocks(args, config, &mut |blocks, cfg| {
        render_content_blocks(blocks, cfg)
    })
}
