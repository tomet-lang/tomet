//! Serializes a `tomet_ast::Document` back to `.tmt` source text
//! (no span info required, unlike `tomet-formatter`, which
//! re-formats existing `.tmt` text losslessly using spans -- this crate
//! is for documents that never had `.tmt` source to begin with, e.g.
//! ones built from Markdown or edited purely at the AST level). Uses
//! `tomet_config::PrinterConfig` (see that crate for config
//! loading/discovery) to drive formatting choices: meta format
//! (yaml/json/toml), link-key spacing, callout/list style, via
//! `tomet-style`'s single-node renderers. Also owns `@meta` id
//! auto-generation (`ensure_document_id_with_config`, which mutates the
//! AST directly), built on the pure generate/validate/convert helpers
//! in `tomet-field-utils`.

use tomet_ast::{Block, Document, Element, ElementValue, Entry, Inline, Placement, Sigil, Value};
use tomet_config::PrinterConfig;
use tomet_field_utils::{generate_id_for_field, is_valid_id_format};
use tomet_semantics::{ElementKind, classify_std_lenient, heading_level, list_items, list_ordered};
use tomet_style::{render_args_with_config, render_nested, render_value};
use tomet_tree::element_new;

pub fn ensure_document_id_with_config(doc: &mut Document, config: &PrinterConfig) {
    let Some(id_cfg) = config.meta_fields.get("id") else {
        return;
    };
    if id_cfg.field_type.is_none() {
        return;
    }

    let force = id_cfg.force.unwrap_or(true);
    let overwrite = id_cfg.overwrite.unwrap_or(false);

    let mut meta_found = false;
    for block in &mut doc.blocks {
        if let Block::Element(el) = block {
            if el.sigil.is_bare_named("meta") {
                meta_found = true;
                if let Some(ElementValue::Group(entries)) = &mut el.value {
                    let existing_idx =
                        entries
                            .iter()
                            .enumerate()
                            .find_map(|(idx, entry)| match entry {
                                Entry::Pair(k, v) if k == "id" => Some((idx, v.clone())),
                                _ => None,
                            });

                    if let Some((idx, val)) = existing_idx {
                        if overwrite {
                            let existing_str = match &val {
                                Value::String(s) => s.as_str(),
                                _ => "",
                            };
                            if !is_valid_id_format(existing_str, id_cfg) {
                                let new_id = generate_id_for_field(id_cfg);
                                entries[idx] = Entry::Pair("id".to_string(), Value::String(new_id));
                            }
                        }
                    } else if force || overwrite {
                        let new_id = generate_id_for_field(id_cfg);
                        entries.insert(0, Entry::Pair("id".to_string(), Value::String(new_id)));
                    }
                }
                break;
            }
        }
    }

    if !meta_found && (force || overwrite) {
        let new_id = generate_id_for_field(id_cfg);
        let mut meta_el = element_new(Sigil::named("meta"));
        meta_el.value = Some(ElementValue::Group(vec![Entry::Pair(
            "id".to_string(),
            Value::String(new_id),
        )]));
        doc.blocks.insert(0, Block::Element(meta_el));
    }
}

/// Serialize a [`Document`] AST into Tomet (`.tmt`) source string using document-level config or defaults.
pub fn document_to_tm(doc: &Document) -> String {
    let config = PrinterConfig::from_doc(doc);
    document_to_tm_with_config(doc, &config)
}

pub fn document_to_tm_with_config(doc: &Document, config: &PrinterConfig) -> String {
    let mut out = String::new();
    for (i, block) in doc.blocks.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        render_block(block, config, &mut out);
    }
    tomet_formatter::format_source(&out)
}

fn render_block(block: &Block, config: &PrinterConfig, out: &mut String) {
    match block {
        Block::Paragraph(p) => {
            let inlines_text = render_inlines(&p.content, config);
            if !inlines_text.trim().is_empty() {
                out.push_str(&inlines_text);
                out.push('\n');
            }
        }
        Block::Element(el) if list_ordered(el).is_some() => render_list(el, config, out),
        Block::Element(el) if classify_std_lenient(el) == ElementKind::Heading => {
            render_heading_element(el, config, out)
        }
        Block::Element(el) => {
            out.push_str(&render_element(el, config));
            out.push('\n');
        }
    }
}

/// Called only from `render_block`'s top-level dispatch, never from the
/// shared, recursively-called `render_element` (which `render_inlines`/
/// `ElementValue::Children` both call for nested/inline elements) -- a
/// nested/inline `@heading(...)` must not round-trip back to `#`-sugar,
/// consistent with `tomet-html`/`tomet-markdown`'s
/// equivalent gating for the same resolved decision.
fn render_heading_element(el: &Element, config: &PrinterConfig, out: &mut String) {
    let level = heading_level(el).unwrap_or(1) as usize;
    let content = el.content.as_deref().unwrap_or(&[]);
    out.push_str(&"#".repeat(level));
    if config.heading_space_inside_brackets {
        out.push_str("[ ");
        out.push_str(&render_inlines(content, config));
        out.push_str(" ]");
    } else {
        out.push('[');
        out.push_str(&render_inlines(content, config));
        out.push(']');
    }
    if let Some(v) = el.value.as_ref().and_then(|v| v.as_data()) {
        out.push(' ');
        out.push_str(&render_value(&v));
    }
    out.push_str(&render_connects(&el.connects, config));
    out.push('\n');
}

fn render_list(el: &Element, config: &PrinterConfig, out: &mut String) {
    render_list_with_indent(el, 0, config, out);
}

fn render_list_with_indent(el: &Element, indent: usize, config: &PrinterConfig, out: &mut String) {
    let ordered = list_ordered(el).unwrap_or(false);
    let indent_str = "  ".repeat(indent);
    for item in list_items(el) {
        let prefix = if ordered {
            "-. ".to_string()
        } else {
            "- ".to_string()
        };
        let mut head_prefix = String::new();
        head_prefix.push_str(&indent_str);
        head_prefix.push_str(&prefix);
        if let Some(marker) = &item.args {
            head_prefix.push('(');
            head_prefix.push_str(&render_args_with_config(marker, config));
            head_prefix.push_str(") ");
        }

        let content_str = render_inlines(item.content.as_deref().unwrap_or(&[]), config);
        let lines: Vec<&str> = content_str.lines().collect();
        let item_attrs = match &item.value {
            Some(v) => v.as_data(),
            _ => None,
        };

        if lines.len() > 1 && config.list_multiline_style_content.is_some() {
            let style = config.list_multiline_style_content.as_deref().unwrap();
            let prefix_width = head_prefix.chars().count();
            let pad = " ".repeat(prefix_width + 2);

            out.push_str(&head_prefix);
            if style == "expanded" {
                out.push_str("[\n");
                for line in &lines {
                    out.push_str(&pad);
                    out.push_str(line);
                    out.push('\n');
                }
                out.push_str(&pad);
                out.push(']');
            } else if style == "block" {
                if lines.len() == 1 {
                    out.push_str("[ ");
                    out.push_str(lines[0]);
                    out.push_str(" ]");
                } else {
                    out.push_str("[ ");
                    for (idx, line) in lines.iter().enumerate() {
                        if idx == 0 {
                            out.push_str(line);
                            out.push('\n');
                        } else {
                            out.push_str(&pad);
                            out.push_str(line);
                            out.push('\n');
                        }
                    }
                    out.push_str(&pad);
                    out.push(']');
                }
            } else if style == "box" {
                out.push_str("[ ");
                for (idx, line) in lines.iter().enumerate() {
                    if idx == 0 {
                        out.push_str(line);
                        out.push('\n');
                    } else if idx == lines.len() - 1 {
                        out.push_str(&pad);
                        out.push_str(line);
                        out.push_str(" ]");
                    } else {
                        out.push_str(&pad);
                        out.push_str(line);
                        out.push('\n');
                    }
                }
            } else {
                out.push_str(&content_str);
            }
            if let Some(attrs) = item_attrs {
                out.push(' ');
                out.push_str(&render_value(&attrs));
            }
            out.push_str(&render_connects(&item.connects, config));
            out.push('\n');
        } else {
            out.push_str(&head_prefix);
            out.push_str(&content_str);
            if let Some(attrs) = item_attrs {
                out.push(' ');
                out.push_str(&render_value(&attrs));
            }
            out.push_str(&render_connects(&item.connects, config));
            out.push('\n');
        }
        if let Some(children) = &item.children {
            for child in children {
                if let Block::Element(sub) = child {
                    if list_ordered(sub).is_some() {
                        render_list_with_indent(sub, indent + 1, config, out);
                    }
                }
            }
        }
    }
}

/// Prints each of `connects` as `:name(...)`/`:name[...]`/`:name{...}`,
/// stacked in source order right after whatever comes before it -- a
/// connect is structurally an ordinary `Element` (its own `args`/
/// `content`/`value`), just introduced by `:` instead of `@`, so this
/// reuses the same group renderers [`render_element`]'s own generic tail
/// does. No connect nests further connects (the parser never lets one),
/// so this does not recurse into `connect.connects`.
///
/// No leading space or newline: a connect attaches directly to what
/// precedes it, the same tight style `(args)[content]{value}` already
/// print in.
fn render_connects(connects: &[Element], config: &PrinterConfig) -> String {
    let mut out = String::new();
    for connect in connects {
        out.push(':');
        if let Some(name) = connect.sigil.name() {
            out.push_str(&name.to_string());
        }
        if let Some(args) = &connect.args {
            out.push('(');
            out.push_str(&render_args_with_config(args, config));
            out.push(')');
        }
        if let Some(content) = &connect.content {
            out.push('[');
            out.push_str(&render_inlines(content, config));
            out.push(']');
        }
        if let Some(value) = &connect.value {
            out.push_str(&render_element_value_with_format(
                value,
                get_format_from_args(connect.args.as_ref()),
                config,
            ));
        }
    }
    out
}

/// Renders a run of inlines.
///
/// A `[content]` group holds `Inline`s, but one of them may be
/// block-placed -- an element written at a line start inside the group,
/// the way `@references[` holds its entries. Placement is spelled with
/// line breaks rather than with a sigil, so such an element has to start
/// and end its own line here or it would come back from the parser as
/// part of the running text.
fn render_inlines(inlines: &[Inline], config: &PrinterConfig) -> String {
    let mut s = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => s.push_str(&t.value),
            Inline::Raw(t) => s.push_str(&t.value),
            // A literal newline, not `softbreak_join`'s space/nothing.
            // Several callers (`render_list_with_indent`'s
            // `list_multiline_style_content`, `render_callout`'s
            // `callout_content_style`) split this function's output on
            // `'\n'` to lay wrapped content out across multiple indented
            // lines -- a feature that used to ride on markdown import
            // embedding a literal `'\n'` straight into `Text.value` for a
            // softbreak. Folding to a space here would silently turn that
            // into always-one-line output.
            Inline::SoftBreak(_) | Inline::LineBreak(_) => s.push('\n'),
            Inline::Element(el) => {
                let block = el.placement == Placement::Block;
                if block && !s.is_empty() && !s.ends_with('\n') {
                    s.push('\n');
                }
                s.push_str(&render_element(el, config));
                if block {
                    s.push('\n');
                }
            }
        }
    }
    s
}

/// Render an [`Element`] AST node into Tomet syntax:
/// `<sigil><name>(args)[content]{value}`, or `<sigil><name>+++body+++`.
pub fn render_element(el: &Element, config: &PrinterConfig) -> String {
    {
        if el.sigil.is_bare_named("hr") && el.args.is_none() && el.value.is_none() {
            if let Some(content) = &el.content {
                return format!(
                    "---[{}]---{}",
                    render_inlines(content, config),
                    render_connects(&el.connects, config)
                );
            } else {
                return format!("---{}", render_connects(&el.connects, config));
            }
        }
    }

    if el.sigil.is_bare_named("meta") {
        let mut out = tomet_style::render_meta_element(el, config);
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
                .map(|content| render_inlines(content, config));
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
            let from_content = el
                .content
                .as_ref()
                .map(|content| render_inlines(content, config))
                .filter(|body| !body.is_empty());
            let body = from_content
                .or_else(|| match el.value.as_ref() {
                    Some(ElementValue::Raw(raw)) => Some(raw.clone()),
                    _ => None,
                })
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
            out.push_str(&render_connects(&el.connects, config));
            return out;
        }
    }

    if el.sigil.is_bare_named("callout") {
        if let Some(style) = config.callout_content_style.as_deref() {
            if style == "expanded" {
                let mut out = String::from("@callout");
                if let Some(args) = &el.args {
                    out.push('(');
                    out.push_str(&render_args_with_config(args, config));
                    out.push(')');
                }
                if let Some(content) = &el.content {
                    let inlines_text = render_inlines(content, config);
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
                out.push_str(&render_connects(&el.connects, config));
                return out;
            } else if style == "block" {
                let mut out = String::from("@callout");
                if let Some(args) = &el.args {
                    out.push('(');
                    out.push_str(&render_args_with_config(args, config));
                    out.push(')');
                }
                if let Some(content) = &el.content {
                    let inlines_text = render_inlines(content, config);
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
                out.push_str(&render_connects(&el.connects, config));
                return out;
            } else if style == "box" {
                let mut out = String::from("@callout");
                if let Some(args) = &el.args {
                    out.push('(');
                    out.push_str(&render_args_with_config(args, config));
                    out.push(')');
                }
                if let Some(content) = &el.content {
                    let inlines_text = render_inlines(content, config);
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
                out.push_str(&render_connects(&el.connects, config));
                return out;
            }
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
    }

    if let Some(args) = &el.args {
        out.push('(');
        out.push_str(&render_args_with_config(args, config));
        out.push(')');
    }

    if let Some(content) = &el.content {
        out.push('[');
        out.push_str(&render_inlines(content, config));
        out.push(']');
    }

    if let Some(value) = &el.value {
        out.push_str(&render_element_value_with_format(
            value,
            get_format_from_args(el.args.as_ref()),
            config,
        ));
    }

    out.push_str(&render_connects(&el.connects, config));

    out
}

/// Renders an element's value: a `{...}` group, or a `+++` fence for a
/// raw body.
fn render_element_value(value: &ElementValue, config: &PrinterConfig) -> String {
    render_element_value_with_format(value, None, config)
}

/// Renders an element's value, honouring a declared `format:`.
///
/// When an element says `(format:json)` and holds data, that data is
/// written back as JSON inside a `+++` fence -- the fence being the only
/// place another language's source can live now. Reading it back is
/// `tomet-semantics`' `embedded::element_data`, so the round trip is
/// data-in, data-out regardless of which side wrote it.
///
/// This is rendering, not parsing: the printer is free to consult an
/// element's arguments. The invariant the rework protects is that the
/// *parser* does not.
fn render_element_value_with_format(
    value: &ElementValue,
    format: Option<&str>,
    config: &PrinterConfig,
) -> String {
    if let (ElementValue::Group(_), Some(fmt)) = (value, format) {
        if let Some(data) = value.as_data() {
            let json = value_to_json(&data);
            let serialized = match fmt {
                "json" => serde_json::to_string_pretty(&json).ok(),
                "yaml" => serde_yaml::to_string(&json).ok(),
                "toml" => toml::to_string_pretty(&json).ok(),
                _ => None,
            };
            if let Some(body) = serialized {
                let body = body.trim_end();
                return format!(
                    "{}\n{body}\n{}",
                    "+".repeat(fence_len_for(body)),
                    "+".repeat(fence_len_for(body))
                );
            }
        }
    }
    match value {
        // A raw body is written back as the fence it came from. The run is
        // grown past any `+++` line inside the body, matching the rule the
        // parser reads it with.
        ElementValue::Raw(body) => {
            let fence = "+".repeat(fence_len_for(body));
            let mut out = String::new();
            out.push_str(&fence);
            out.push('\n');
            out.push_str(body);
            if !body.is_empty() && !body.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(&fence);
            out
        }
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
fn render_group_entries(entries: &[Entry], config: &PrinterConfig) -> String {
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

/// The `+` run length needed to fence `body`: three, unless the body
/// itself contains a line that would close the fence early.
fn fence_len_for(body: &str) -> usize {
    let longest = body
        .lines()
        .map(|l| l.trim_end())
        .filter(|l| !l.is_empty() && l.chars().all(|c| c == '+'))
        .map(|l| l.len())
        .max()
        .unwrap_or(0);
    longest.max(2) + 1
}

fn get_format_from_args(args: Option<&Value>) -> Option<&str> {
    if let Some(Value::Map(entries)) = args {
        for (k, v) in entries {
            if k == "format" {
                if let Value::String(fmt) = v {
                    return Some(fmt.as_str());
                }
            }
        }
    }
    None
}

fn value_to_json(val: &Value) -> serde_json::Value {
    match val {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Int(i) => serde_json::Value::Number((*i).into()),
        Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Seq(items) => serde_json::Value::Array(items.iter().map(value_to_json).collect()),
        Value::Map(entries) => {
            let mut map = serde_json::Map::new();
            for (k, v) in entries {
                map.insert(k.clone(), value_to_json(v));
            }
            serde_json::Value::Object(map)
        }
        Value::Call(name, args) => {
            let mut map = serde_json::Map::new();
            map.insert("call".to_string(), serde_json::Value::String(name.clone()));
            map.insert(
                "args".to_string(),
                serde_json::Value::Array(args.iter().map(value_to_json).collect()),
            );
            serde_json::Value::Object(map)
        }
        // Same tagged-object shape as `Call` above -- JSON has no element
        // concept, and this is only reached via `+++`-fenced `format:json`
        // export, not the normal `.tmt` printer path (see
        // `tomet_style::render_value_inner_with_config`'s `Value::Element`
        // arm for that one).
        Value::Element(el) => {
            let mut map = serde_json::Map::new();
            let name = el.sigil.name().map(|n| n.to_string()).unwrap_or_default();
            map.insert("element".to_string(), serde_json::Value::String(name));
            if let Some(args) = &el.args {
                map.insert("args".to_string(), value_to_json(args));
            }
            if let Some(content) = &el.content {
                let text: String = content
                    .iter()
                    .filter_map(|i| match i {
                        Inline::Text(t) => Some(t.value.as_str()),
                        Inline::Raw(r) => Some(r.value.as_str()),
                        _ => None,
                    })
                    .collect();
                map.insert("content".to_string(), serde_json::Value::String(text));
            }
            serde_json::Value::Object(map)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_config::{FieldConfig, load_config_from_str};
    use tomet_style::render_value_inner;

    #[test]
    fn test_printer_config_space_inside_brackets() {
        let doc = tomet_parser::parse_document("#[Title]\n").unwrap();
        let cfg = PrinterConfig {
            heading_space_inside_brackets: true,
            ..Default::default()
        };
        let printed = document_to_tm_with_config(&doc, &cfg);
        assert!(printed.contains("#[ Title ]"));
    }

    #[test]
    fn nested_inline_heading_does_not_reserialize_as_hash_sugar() {
        // `@heading(2)[...]` nested inside another element's content must
        // round-trip as a plain `@heading(...)` element, never as `##[...]`
        // -- only `render_block`'s top-level dispatch special-cases
        // headings; the shared, recursively-called `render_element` has no
        // heading arm at all (mirrors `tomet-html`/
        // `tomet-markdown`'s equivalent nested-heading tests).
        let doc = tomet_parser::parse_document("@memo[@heading(2)[Nested]]\n").unwrap();
        let printed = document_to_tm(&doc);
        assert!(!printed.contains("##["), "got: {printed:?}");
        assert!(printed.contains("@heading(2)[Nested]"), "got: {printed:?}");
    }

    #[test]
    fn a_named_connect_round_trips_on_a_plain_element() {
        let doc = tomet_parser::parse_document("@section[ x ]:rule(allow: list(card))\n").unwrap();
        let printed = document_to_tm(&doc);
        assert!(
            printed.contains(":rule(allow: list(card))"),
            "got: {printed:?}"
        );
    }

    #[test]
    fn stacked_connects_round_trip_in_order() {
        let doc = tomet_parser::parse_document("@x(a: 1):as(y):rule(allow: list(card))\n").unwrap();
        let printed = document_to_tm(&doc);
        assert!(
            printed.contains(":as(y):rule(allow: list(card))"),
            "got: {printed:?}"
        );
    }

    #[test]
    fn a_named_connect_round_trips_on_a_heading() {
        let doc = tomet_parser::parse_document("#[ h ]:rule(allow: list(card))\n").unwrap();
        let printed = document_to_tm(&doc);
        assert!(
            printed.contains(":rule(allow: list(card))"),
            "got: {printed:?}"
        );
    }

    /// Defensive coverage for the four early-returning special cases in
    /// `render_element` (`hr`/`meta`/`codeblock`/`callout`), which would
    /// otherwise silently drop a `connects` field the parser never
    /// actually attaches to them in practice today -- a `:rule(...)`
    /// after e.g. a codeblock is syntactically legal even if unlikely.
    #[test]
    fn connects_survive_the_four_specially_printed_kinds() {
        for src in [
            "@hr:rule(allow: list(card))\n",
            "@meta{a: 1}:rule(allow: list(card))\n",
            "@raw[x]:rule(allow: list(card))\n",
        ] {
            let doc = tomet_parser::parse_document(src).unwrap();
            let el = match &doc.blocks[0] {
                Block::Element(el) => el,
                other => panic!("expected element, got {other:?}"),
            };
            assert_eq!(el.connects.len(), 1, "source: {src:?}, doc: {doc:?}");
            let printed = document_to_tm(&doc);
            assert!(
                printed.contains(":rule(allow: list(card))"),
                "source: {src:?}, got: {printed:?}"
            );
        }
    }

    #[test]
    fn connects_survive_a_specially_styled_callout() {
        let doc =
            tomet_parser::parse_document("@callout(info)[x]:rule(allow: list(card))\n").unwrap();
        let cfg = PrinterConfig {
            callout_content_style: Some("block".to_string()),
            ..Default::default()
        };
        let printed = document_to_tm_with_config(&doc, &cfg);
        assert!(
            printed.contains(":rule(allow: list(card))"),
            "got: {printed:?}"
        );
    }

    #[test]
    fn test_markdown_meta_export_with_meta_format() {
        let md = "---\ntitle: Hello\n---\n\n# World\n";
        let doc = tomet_markdown::from_markdown(md);
        let cfg = PrinterConfig {
            meta_format: Some("yaml".to_string()),
            ..Default::default()
        };
        let printed = document_to_tm_with_config(&doc, &cfg);
        assert!(printed.contains("@meta(format:yaml)+++"));
    }

    #[test]
    fn test_iso8601_timestamp_rendering_in_meta() {
        let md = "---\nmodified: 2026-06-17T05:52:44\n---\n\n# Document\n";
        let doc = tomet_markdown::from_markdown(md);
        let cfg = PrinterConfig {
            meta_format: Some("yaml".to_string()),
            ..Default::default()
        };
        let printed = document_to_tm_with_config(&doc, &cfg);
        assert!(printed.contains("modified: 2026-06-17T05:52:44"));
        assert!(!printed.contains("modified: \"2026-06-17T05:52:44\""));
    }

    #[test]
    fn test_wikilink_rendering() {
        let md = "Check [[name]] and [[name|display]] here.\n";
        let doc = tomet_markdown::from_markdown(md);
        let printed = document_to_tm(&doc);
        assert!(printed.contains("@link(target: \"ref:name\")"));
        assert!(printed.contains("@link(target: \"ref:name\")[display]"));
    }

    #[test]
    fn test_rfc3339_and_aliases_always_newline_meta_field_formatting() {
        let md = "---\naliases:\n  - rust\n  - tomet\ncreated: 2026-06-17T05:52:44\nmodified: 2026-06-17T05:52:44\n---\n\n# Document\n";
        let doc = tomet_markdown::from_markdown(md);

        let mut meta_fields = std::collections::BTreeMap::new();
        meta_fields.insert(
            "aliases".to_string(),
            FieldConfig {
                field_type: Some("list".to_string()),
                always_newline: true,
                ..Default::default()
            },
        );
        meta_fields.insert(
            "created".to_string(),
            FieldConfig {
                field_type: Some("datetime".to_string()),
                format: Some("rfc3339".to_string()),
                ..Default::default()
            },
        );
        meta_fields.insert(
            "modified".to_string(),
            FieldConfig {
                field_type: Some("datetime".to_string()),
                format: Some("rfc3339".to_string()),
                ..Default::default()
            },
        );

        let cfg = PrinterConfig {
            meta_format: Some("yaml".to_string()),
            meta_fields,
            ..Default::default()
        };

        let printed = document_to_tm_with_config(&doc, &cfg);
        assert!(printed.contains("created: 2026-06-17T05:52:44Z"));
        assert!(printed.contains("modified: 2026-06-17T05:52:44Z"));
        assert!(printed.contains("aliases:\n    - rust\n    - tomet"));

        let md_empty = "---\naliases: []\nflags: []\n---\n\n# Document\n";
        let doc_empty = tomet_markdown::from_markdown(md_empty);
        let printed_empty = document_to_tm_with_config(&doc_empty, &cfg);
        assert!(printed_empty.contains("aliases: []"));
        assert!(printed_empty.contains("flags: []"));
    }

    #[test]
    fn test_rfc3339_with_offset_formatting() {
        let md = "---\ncreated: 2026-06-17T05:52:44\n---\n\n# Document\n";
        let doc = tomet_markdown::from_markdown(md);

        let mut meta_fields = std::collections::BTreeMap::new();
        meta_fields.insert(
            "created".to_string(),
            FieldConfig {
                field_type: Some("datetime".to_string()),
                format: Some("rfc3339".to_string()),
                offset: Some("+09:00".to_string()),
                ..Default::default()
            },
        );

        let cfg = PrinterConfig {
            meta_format: Some("yaml".to_string()),
            meta_fields,
            ..Default::default()
        };

        let printed = document_to_tm_with_config(&doc, &cfg);
        assert!(printed.contains("created: 2026-06-17T05:52:44+09:00"));
    }

    #[test]
    fn test_null_value_rendering() {
        let rendered = render_value_inner(&Value::Null);
        assert_eq!(rendered, "");
        let md = "---\ntitle:\n---\n\n# Document\n";
        let doc = tomet_markdown::from_markdown(md);
        let cfg = PrinterConfig {
            meta_format: Some("yaml".to_string()),
            ..Default::default()
        };
        let printed = document_to_tm_with_config(&doc, &cfg);
        assert!(printed.contains("title:\n"));
        assert!(!printed.contains("title: null"));
        assert!(!printed.contains("title: \"\""));
    }

    #[test]
    fn test_wikilink_no_space_setting() {
        let md = "Check [[target]] and [[target|display]].\n";
        let doc = tomet_markdown::from_markdown(md);

        let cfg_default = PrinterConfig::default();
        let printed_default = document_to_tm_with_config(&doc, &cfg_default);
        assert!(printed_default.contains("@link(target: \"ref:target\")"));
        assert!(printed_default.contains("@link(target: \"ref:target\")[display]"));

        let cfg_no_space = PrinterConfig {
            link_no_space: true,
            ..Default::default()
        };
        let printed_no_space = document_to_tm_with_config(&doc, &cfg_no_space);
        assert!(printed_no_space.contains("@link(target:\"ref:target\")"));
        assert!(printed_no_space.contains("@link(target:\"ref:target\")[display]"));
    }

    #[test]
    fn test_nanoid_generation_and_ensure_document_id() {
        let settings_src = r#"@settings(format:json)+++
{
  "meta": {
    "id": {
      "type": "nanoid",
      "length": 8,
      "prefix": "doc-"
    }
  }
}
+++
"#;
        let cfg = load_config_from_str(settings_src).expect("failed to parse settings");
        let id_cfg = cfg.meta_fields.get("id").expect("id config present");
        assert_eq!(id_cfg.field_type.as_deref(), Some("nanoid"));
        assert_eq!(id_cfg.length, Some(8));
        assert_eq!(id_cfg.prefix.as_deref(), Some("doc-"));

        let gen_id = generate_id_for_field(id_cfg);
        assert!(gen_id.starts_with("doc-"));
        assert_eq!(gen_id.len(), 4 + 8); // "doc-" + 8 chars

        let mut doc = tomet_markdown::from_markdown("# Test Note\nHello world\n");
        ensure_document_id_with_config(&mut doc, &cfg);

        let printed = document_to_tm_with_config(&doc, &cfg);
        assert!(printed.contains("id: doc-"));
    }

    #[test]
    fn test_nanoid_force_and_overwrite_behavior() {
        let settings_src = r#"@settings(format:json)+++
{
  "meta": {
    "id": {
      "type": "nanoid",
      "length": 8,
      "prefix": "doc-",
      "force": true,
      "overwrite": true
    }
  }
}
+++
"#;
        let cfg = load_config_from_str(settings_src).expect("failed to parse settings");
        let id_cfg = cfg.meta_fields.get("id").unwrap();
        assert_eq!(id_cfg.force, Some(true));
        assert_eq!(id_cfg.overwrite, Some(true));

        // Case 1: Valid existing ID is kept
        let md1 = "---\nid: doc-12345678\n---\n# Note 1\n";
        let mut doc1 = tomet_markdown::from_markdown(md1);
        ensure_document_id_with_config(&mut doc1, &cfg);
        let printed1 = document_to_tm_with_config(&doc1, &cfg);
        assert!(printed1.contains("id: doc-12345678"));

        // Case 2: Invalid format existing ID is overwritten when overwrite: true
        let md2 = "---\nid: invalid-slug\n---\n# Note 2\n";
        let mut doc2 = tomet_markdown::from_markdown(md2);
        ensure_document_id_with_config(&mut doc2, &cfg);
        let printed2 = document_to_tm_with_config(&doc2, &cfg);
        assert!(!printed2.contains("invalid-slug"));
        assert!(printed2.contains("id: doc-"));

        // Case 3: When force: true and overwrite: false, invalid existing ID is preserved
        let cfg_no_overwrite = PrinterConfig {
            meta_fields: std::collections::BTreeMap::from([(
                "id".to_string(),
                FieldConfig {
                    field_type: Some("nanoid".to_string()),
                    length: Some(8),
                    prefix: Some("doc-".to_string()),
                    force: Some(true),
                    overwrite: Some(false),
                    ..Default::default()
                },
            )]),
            ..Default::default()
        };
        let mut doc3 = tomet_markdown::from_markdown(md2);
        ensure_document_id_with_config(&mut doc3, &cfg_no_overwrite);
        let printed3 = document_to_tm_with_config(&doc3, &cfg_no_overwrite);
        assert!(printed3.contains("id: invalid-slug"));
    }

    #[test]
    fn test_obsidian_callout_blockquote_printing() {
        let md = "> [!info] 2025/04/29 11:09\n> コレさすがに草www\n";
        let doc = tomet_markdown::from_markdown(md);
        let printed = document_to_tm_with_config(&doc, &PrinterConfig::default());
        assert!(printed.contains("@callout(info, title: \"2025/04/29 11:09\")["));
        assert!(printed.contains("コレさすがに草www"));

        let md_plain = "> Plain quote text\n";
        let doc_plain = tomet_markdown::from_markdown(md_plain);
        let printed_plain = document_to_tm_with_config(&doc_plain, &PrinterConfig::default());
        assert!(printed_plain.contains("@quote["));
        assert!(printed_plain.contains("Plain quote text"));
        assert!(!printed_plain.contains("@quote("));
    }

    #[test]
    fn test_callout_content_style_formatting() {
        let md = "> [!info] 2025/04/29 11:09\n> コレさすがに草www\n> お前なら[[2025-04-26|どうするんだ]]？\n";
        let doc = tomet_markdown::from_markdown(md);

        // Test "block" style
        let cfg_block = PrinterConfig {
            callout_content_style: Some("block".to_string()),
            ..Default::default()
        };
        let printed_block = document_to_tm_with_config(&doc, &cfg_block);
        assert!(printed_block.contains("@callout(info, title: \"2025/04/29 11:09\")\n[ コレさすがに草www\n  お前なら@link(target: \"ref:2025-04-26\")[どうするんだ]？\n]"));

        // Test "box" style
        let cfg_box = PrinterConfig {
            callout_content_style: Some("box".to_string()),
            ..Default::default()
        };
        let printed_box = document_to_tm_with_config(&doc, &cfg_box);
        assert!(printed_box.contains("@callout(info, title: \"2025/04/29 11:09\")\n[ コレさすがに草www\n  お前なら@link(target: \"ref:2025-04-26\")[どうするんだ]？ ]"));

        // Test "expanded" style
        let cfg_expanded = PrinterConfig {
            callout_content_style: Some("expanded".to_string()),
            ..Default::default()
        };
        let printed_expanded = document_to_tm_with_config(&doc, &cfg_expanded);
        assert!(printed_expanded.contains("@callout(info, title: \"2025/04/29 11:09\")[\n  コレさすがに草www\n  お前なら@link(target: \"ref:2025-04-26\")[どうするんだ]？\n]"));
    }

    #[test]
    fn test_list_multiline_style_formatting() {
        let md = "1. いや、まずこういう話をするときの前提として、\n   Vtuberでくくってるやつがまず、ゴミだ。\n   確実に脳が言っている割合が高い。イメージだけで物事を語る。それってあなたの間奏ですよね。\n";
        let doc = tomet_markdown::from_markdown(md);

        let cfg_box = PrinterConfig {
            list_multiline_style_content: Some("box".to_string()),
            ..Default::default()
        };
        let printed_box = document_to_tm_with_config(&doc, &cfg_box);
        assert!(printed_box.contains("-. [ いや、まずこういう話をするときの前提として、\n     Vtuberでくくってるやつがまず、ゴミだ。\n     確実に脳が言っている割合が高い。イメージだけで物事を語る。それってあなたの間奏ですよね。 ]"));

        let cfg_block = PrinterConfig {
            list_multiline_style_content: Some("block".to_string()),
            ..Default::default()
        };
        let printed_block = document_to_tm_with_config(&doc, &cfg_block);
        assert!(printed_block.contains("-. [ いや、まずこういう話をするときの前提として、\n     Vtuberでくくってるやつがまず、ゴミだ。\n     確実に脳が言っている割合が高い。イメージだけで物事を語る。それってあなたの間奏ですよね。\n     ]"));
    }

    #[test]
    fn test_single_line_block_style_formatting() {
        let md_callout = "> [!info] Single Line\n> 一行テキスト\n";
        let doc_callout = tomet_markdown::from_markdown(md_callout);
        let cfg = PrinterConfig {
            callout_content_style: Some("block".to_string()),
            ..Default::default()
        };
        let printed = document_to_tm_with_config(&doc_callout, &cfg);
        assert!(printed.contains("@callout(info, title: \"Single Line\")\n[ 一行テキスト ]"));
    }

    #[test]
    fn test_link_no_space_setting() {
        let md = "[Google](https://google.com)\n";
        let doc = tomet_markdown::from_markdown(md);

        let cfg_default = PrinterConfig::default();
        let printed_default = document_to_tm_with_config(&doc, &cfg_default);
        assert!(printed_default.contains("@link(target: \"https://google.com\")[Google]"));

        let cfg_nospace = PrinterConfig {
            link_no_space: true,
            ..Default::default()
        };
        let printed_nospace = document_to_tm_with_config(&doc, &cfg_nospace);
        assert!(printed_nospace.contains("@link(target:\"https://google.com\")[Google]"));
    }

    /// A raw block's body reaches the printer in either slot, and both
    /// have to come back out.
    ///
    /// ``` parses into `[content]`; `@raw(yaml)+++...+++` parses
    /// into a `Raw` value.
    #[test]
    fn a_fenced_raw_body_survives_printing() {
        let doc = tomet_parser::parse_document("@raw(yaml)+++\na: 1\n+++\n").unwrap();
        let printed = document_to_tm(&doc);
        assert!(printed.contains("a: 1"), "the body was dropped:\n{printed}");
    }

    #[test]
    fn inline_raw_prints_as_backticks() {
        let doc = tomet_parser::parse_document("call `foo()` now\n").unwrap();
        let printed = document_to_tm(&doc);
        assert_eq!(printed.trim(), "call `foo()` now");
    }

    #[test]
    fn test_codeblock_formatting() {
        let md = "```shell\nirm \"https://christitus.com/win\" | iex\n```\n";
        let doc = tomet_markdown::from_markdown(md);
        let printed = document_to_tm(&doc);
        assert_eq!(
            printed.trim(),
            "```shell\nirm \"https://christitus.com/win\" | iex\n```"
        );
    }

    #[test]
    fn test_embedded_format_serialization_and_reparse() {
        let mut el = element_new(Sigil::named("config"));
        el.args = Some(Value::Map(vec![(
            "format".to_string(),
            Value::String("json".to_string()),
        )]));
        el.value = Some(ElementValue::from_map(Value::Map(vec![(
            "meta".to_string(),
            Value::String("yaml".to_string()),
        )])));
        let doc = Document::new(vec![Block::Element(el)], tomet_ast::Span::dummy());
        let printed = document_to_tm(&doc);
        assert!(printed.contains("\"meta\": \"yaml\""));

        // Verify re-parsing
        let re_parsed = tomet_parser::parse_document(&printed).expect("valid doc");
        assert_eq!(re_parsed.blocks.len(), 1);
    }

    #[test]
    fn test_container_children_indentation() {
        let mut child1 = element_new(Sigil::Bare);
        child1.args = Some(Value::Int(1));
        child1.content = Some(vec![Inline::Text("note 1".into())]);

        let mut child2 = element_new(Sigil::Bare);
        child2.args = Some(Value::Int(2));
        child2.content = Some(vec![Inline::Text("note 2".into())]);

        let mut links = element_new(Sigil::named("links"));
        links.value = Some(ElementValue::from_children(vec![child1, child2]));

        let doc = Document::new(vec![Block::Element(links)], tomet_ast::Span::dummy());
        let printed = document_to_tm(&doc);
        assert!(printed.contains("@links{\n  (1)[note 1]\n  (2)[note 2]\n}"));

        // Verify re-parsing
        let re_parsed = tomet_parser::parse_document(&printed).expect("valid doc");
        assert_eq!(re_parsed.blocks.len(), 1);
    }
}
