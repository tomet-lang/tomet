//! Rendering elements, values, args, and connects.

use tomet_ast::{
    Bare, Block, Connect, Data, Declaration, ElementView, Entry, Group, Hole, Interpolation,
    Placement, Value,
};
use tomet_config::{GroupOrder, PrinterConfig};
use tomet_style::render_nested;

use crate::id::render_id;
use crate::inline::{raw_content_text, render_content_blocks};

/// Render an [`Element`] AST node into Tomet syntax:
/// `<sigil><name>(args)[content]{value}`.
pub fn render_element(el: ElementView<'_>, config: &PrinterConfig) -> String {
    {
        if el.name.is_bare_named("hr") && el.data.args.is_none() && el.data.value.is_none() {
            if let Some(content) = el.content {
                return format!(
                    "---[{}]---{}{}",
                    render_content_blocks(content, config),
                    render_id(el.id),
                    render_connects(el.connects, config)
                );
            } else {
                return format!(
                    "---{}{}",
                    render_id(el.id),
                    render_connects(el.connects, config)
                );
            }
        }
    }

    if el.name.is_bare_named("raw") {
        if el.placement == Placement::Inline
            && el.data.args.is_none()
            && el.data.value.is_none()
            && el.id.is_none()
            && el.connects.is_empty()
        {
            let body = el
                .content
                .and_then(raw_content_text)
                .unwrap_or_default();
            if !body.is_empty() && !body.contains('`') && !body.contains('\n') {
                return format!("`{body}`");
            }
        }

        if el.placement == Placement::Block {
            let lang = el
                .data
                .args
                .as_ref()
                .and_then(|args| {
                    if let Value::String(s) = args {
                        Some(s.clone())
                    } else if let Value::Map(entries) = args {
                        entries
                            .iter()
                            .find(|(k, _)| k == "lang")
                            .and_then(|(_, v)| {
                                if let Value::String(s) = v {
                                    Some(s.clone())
                                } else {
                                    None
                                }
                            })
                    } else {
                        None
                    }
                })
                .unwrap_or_default();
            let body = el
                .content
                .and_then(raw_content_text)
                .unwrap_or_else(|| {
                    el.content
                        .map(|content| render_content_blocks(content, config))
                        .unwrap_or_default()
                });
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
            out.push_str(&render_id(el.id));
            out.push_str(&render_connects(el.connects, config));
            return out;
        }
    }

    if el.name.is_bare_named("callout")
        && let Some(style) = config.format.callout_content_style.as_deref()
    {
        if style == "expanded" {
            let mut out = String::from("@callout");
            if let Some(args) = &el.data.args {
                out.push('(');
                out.push_str(&render_args(args, config));
                out.push(')');
            }
            if let Some(content) = el.content {
                let inlines_text = render_content_blocks(content, config);
                out.push_str("[\n");
                for line in inlines_text.lines() {
                    out.push_str("  ");
                    out.push_str(line);
                    out.push('\n');
                }
                out.push(']');
            }
            if let Some(value) = &el.data.value {
                out.push_str(&render_element_value(value, config));
            }
            out.push_str(&render_id(el.id));
            out.push_str(&render_connects(el.connects, config));
            return out;
        } else if style == "block" {
            let mut out = String::from("@callout");
            if let Some(args) = &el.data.args {
                out.push('(');
                out.push_str(&render_args(args, config));
                out.push(')');
            }
            if let Some(content) = el.content {
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
            if let Some(value) = &el.data.value {
                out.push_str(&render_element_value(value, config));
            }
            out.push_str(&render_id(el.id));
            out.push_str(&render_connects(el.connects, config));
            return out;
        } else if style == "box" {
            let mut out = String::from("@callout");
            if let Some(args) = &el.data.args {
                out.push('(');
                out.push_str(&render_args(args, config));
                out.push(')');
            }
            if let Some(content) = el.content {
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
            if let Some(value) = &el.data.value {
                out.push_str(&render_element_value(value, config));
            }
            out.push_str(&render_id(el.id));
            out.push_str(&render_connects(el.connects, config));
            return out;
        }
    }

    // One sigil, whatever the placement. A block-placed element is told apart
    // by standing alone on its line, which is the same condition the parser
    // reads it back with.
    let mut out = format!("@{}", el.name);
    let name = el.name.name.as_str();
    out.push_str(&render_groups(name, el.data, el.content, config));
    out.push_str(&render_id(el.id));
    out.push_str(&render_connects(el.connects, config));
    out
}

/// Render a [`Bare`] entry into Tomet syntax: `(args)[content]{value}#(id)`,
/// with no head.
pub fn render_bare(bare: &Bare, config: &PrinterConfig) -> String {
    let mut out = render_groups("", &bare.data, bare.content.as_deref(), config);
    out.push_str(&render_id(bare.id.as_ref()));
    out
}

/// Render a [`Hole`] AST node into Tomet syntax:
/// `<?matcher(args){value}>[content]`.
///
/// The head, the matcher and the `(args)` and `{value}` groups, is inside the box and
/// the `[content]` follows its `>`, however `args` and `content` are ordered for an element.
pub fn render_hole(hole: &Hole, config: &PrinterConfig) -> String {
    let mut out = String::from("<?");
    if let Some(matcher) = &hole.matcher {
        out.push_str(&matcher.to_string());
    }
    if let Some(args) = &hole.data.args {
        out.push('(');
        out.push_str(&render_args(args, config));
        out.push(')');
    }
    if let Some(value) = &hole.data.value {
        out.push_str(&render_element_value(value, config));
    }
    out.push('>');
    if let Some(content) = hole.content.as_deref() {
        out.push('[');
        out.push_str(&render_content_blocks(content, config));
        out.push(']');
    }
    out
}

/// Render a [`Declaration`] AST node into Tomet syntax:
/// `::name(args)[content]{value}#(id)`.
pub fn render_declaration(declaration: &Declaration, config: &PrinterConfig) -> String {
    if declaration.name.is_bare_named("meta") {
        let mut out = tomet_style::render_meta_declaration(declaration, config);
        out.push_str(&render_id(declaration.id.as_ref()));
        return out;
    }
    let mut out = format!("::{}", declaration.name);
    out.push_str(&render_groups(
        &declaration.name.name,
        &declaration.data,
        declaration.content.as_deref(),
        config,
    ));
    out.push_str(&render_id(declaration.id.as_ref()));
    out
}

/// `(args)`, `[content]` and `{value}` as one node of that bare `name` has
/// them: `args` and `content` in the order the configuration asks for.
fn render_groups(
    name: &str,
    data: &Data,
    content: Option<&[Block]>,
    config: &PrinterConfig,
) -> String {
    let mut out = String::new();

    let render_args_cb = |out: &mut String| {
        if let Some(args) = &data.args {
            out.push('(');
            out.push_str(&render_args(args, config));
            out.push(')');
        }
    };

    let render_content_cb = |out: &mut String| {
        if let Some(content) = content {
            out.push('[');
            out.push_str(&render_content_blocks(content, config));
            out.push(']');
        }
    };

    let order = config
        .element_group_order(name)
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

    if let Some(value) = &data.value {
        out.push_str(&render_element_value(value, config));
    }
    out
}

/// Renders an element's value: a `{...}` group.
pub(crate) fn render_element_value(value: &Group, config: &PrinterConfig) -> String {
    let Group(entries) = value;
    format!("{{{}}}", render_group_entries(entries, config))
}

/// Renders an interpolation: `${expr}`.
pub(crate) fn render_interp(interp: &Interpolation) -> String {
    format!("${{{}}}", interp.expr)
}

/// Renders a value group's entries in source order.
///
/// Order is preserved deliberately: a group may interleave `key: value`
/// pairs and nested elements, and reordering them would change the
/// document the formatter promises not to change.
#[deny(clippy::wildcard_enum_match_arm)]
pub(crate) fn render_group_entries(entries: &[Entry], config: &PrinterConfig) -> String {
    if entries.is_empty() {
        return String::new();
    }
    let has_element = entries
        .iter()
        .any(|e| matches!(e, Entry::Element(_) | Entry::Bare(_) | Entry::Hole(_)));
    let rendered: Vec<String> = entries
        .iter()
        .map(|entry| match entry {
            // `render_nested`, not `render_value_inner_with_config`: a
            // nested map needs its own braces here or `m: { k: v }`
            // prints as `m: k: v` and reparses as a string.
            Entry::Pair(k, v) => {
                format!("{}: {}", tomet_style::key_text(k), render_nested(v, config))
            }
            Entry::Element(child) => render_element(child.as_view(), config),
            Entry::Hole(hole) => render_hole(hole, config),
            Entry::Bare(bare) => render_bare(bare, config),
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
/// connect takes the same `(args)`/`[content]`/`{value}` groups an
/// element does, introduced by `:` instead of `@`, so this reuses the
/// same group renderers [`render_element`]'s own generic tail does. A
/// connect has no `#(id)` and no connects of its own.
///
/// No leading space or newline: a connect attaches directly to what
/// precedes it, the same tight style `(args)[content]{value}` already
/// print in.
pub(crate) fn render_connects(connects: &[Connect], config: &PrinterConfig) -> String {
    let mut out = String::new();
    for connect in connects {
        out.push(':');
        out.push_str(&connect.name.to_string());
        if let Some(args) = &connect.data.args {
            out.push('(');
            out.push_str(&render_args(args, config));
            out.push(')');
        }
        if let Some(content) = &connect.content {
            out.push('[');
            out.push_str(&render_content_blocks(content, config));
            out.push(']');
        }
        if let Some(value) = &connect.data.value {
            out.push_str(&render_element_value(value, config));
        }
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
