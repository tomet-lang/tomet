//! Single-node renderers: a [`Value`], an element or hole standing in a value slot, and a
//! `::meta` declaration, written as `.tmt` text per a [`PrinterConfig`].
//!
//! A key and a string are spelled by `tomet-spelling`; this module decides where a value goes and
//! how a group reads, and writes the `[content]` of an embedded element through the printer's own
//! block renderer ([`render_content_blocks`]).

use crate::element::render_interp;
use crate::inline::render_content_blocks;
use tomet_ast::{Block, Declaration, Entry, Group, Value};
use tomet_config::{FieldConfig, PrinterConfig};
use tomet_field_utils::is_iso8601;
use tomet_spelling::{key_text, scalar_text};
use tomet_style::{ColonSpacing, GroupLayout, ValueAlignment};

/// What stands between the colon of a link's `target` and its value: a space unless asked not to.
fn link_colon(config: &PrinterConfig) -> &'static str {
    config
        .format
        .link_target_colon
        .unwrap_or(ColonSpacing::Spaced)
        .after_colon()
}

pub(crate) fn render_value(v: &Value) -> String {
    format!(
        "{{{}}}",
        render_value_inner_with_config(v, &PrinterConfig::default())
    )
}

/// Renders a value that sits *inside* another one.
///
/// A map needs its own braces there. [`render_value_inner_with_config`] renders a map unwrapped,
/// because at the top level the caller supplies the braces, and a map nested that way would print
/// `m: { k: v }` as `m: k: v`, which reads back as the string `"k: v"`.
pub(crate) fn render_nested(v: &Value, config: &PrinterConfig) -> String {
    match v {
        Value::Map(_) => format!("{{{}}}", render_value_inner_with_config(v, config)),
        _ => render_value_inner_with_config(v, config),
    }
}

/// Renders a value in a slot of its own. A map is written without braces, since the caller
/// supplies them at the top level; [`render_nested`] is the form for a value inside another.
#[deny(clippy::wildcard_enum_match_arm)]
pub(crate) fn render_value_inner_with_config(v: &Value, config: &PrinterConfig) -> String {
    match v {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => Value::float_text(*f),
        // As written: formatting must not turn `1.10` into `1.1`.
        Value::Number(text) => text.clone(),
        Value::String(s) => scalar_text(s),
        // `(a, b)` is the sequence spelling -- `[...]` already means
        // `[content]`, and `list(...)` is a type.
        Value::Seq(items) => {
            let rendered: Vec<_> = items
                .iter()
                .map(|item| render_nested(item, config))
                .collect();
            format!("({})", rendered.join(", "))
        }
        Value::Map(entries) => {
            if entries.len() == 1 {
                let key = &entries[0].0;
                if key == "target" {
                    let space = link_colon(config);
                    format!("@({key}:{space}{})", render_nested(&entries[0].1, config))
                } else {
                    let mut parts = Vec::new();
                    for (idx, (k, val)) in entries.iter().enumerate() {
                        let val_str = render_nested(val, config);
                        // An empty key is the parser's positional-entry
                        // sentinel (`k.is_empty()`, since a real map key
                        // can never be empty -- an empty quoted key is
                        // a parse error): `(a, key:value)` mixes a bare positional
                        // value with named ones, and the bare one prints
                        // bare too, not as `: a`.
                        if k.is_empty() || (idx == 0 && (k == "variant" || k == "lang")) {
                            parts.push(val_str);
                        } else {
                            parts.push(format!("{}: {val_str}", key_text(k)));
                        }
                    }
                    parts.join(", ")
                }
            } else {
                let mut parts = Vec::new();
                for (idx, (k, val)) in entries.iter().enumerate() {
                    let val_str = render_nested(val, config);
                    // See the matching comment in the single-entry branch
                    // above -- an empty key is a positional entry, printed
                    // bare rather than as `: value`.
                    if k.is_empty() || (idx == 0 && (k == "variant" || k == "lang")) {
                        parts.push(val_str);
                    } else {
                        parts.push(format!("{}: {val_str}", key_text(k)));
                    }
                }
                parts.join(", ")
            }
        }
        Value::Call(name, args) => {
            let rendered: Vec<_> = args.iter().map(|a| render_nested(a, config)).collect();
            format!("{name}({})", rendered.join(", "))
        }
        // `parse_entry_value` only ever produces one of these with `args`
        // set and `content`/`value`/`children` all `None` (an MVP scope
        // limit enforced at parse time, not just convention) -- so `args`
        // alone is the whole element to print back.
        Value::Interp(interp) => render_interp(interp),
        Value::Element(el) => render_embedded(
            format!("@{}", el.name),
            &el.data,
            el.content.as_deref(),
            config,
        ),
        Value::Hole(hole) => render_embedded_hole(hole, config),
        Value::Blocks(blocks) => format!("[{}]", render_content_blocks(blocks, config)),
    }
}

/// A hole sitting in a value slot: `<?matcher(args){value}>[content]`. The head is
/// inside the box and the content follows it; no `id` is printed, as for an element.
fn render_embedded_hole(hole: &tomet_ast::Hole, config: &PrinterConfig) -> String {
    let mut s = String::from("<?");
    if let Some(matcher) = &hole.matcher {
        s.push_str(&matcher.to_string());
    }
    if let Some(args) = &hole.data.args {
        s.push_str(&format!("({})", render_args(args, config)));
    }
    if let Some(group) = &hole.data.value {
        s.push_str(&render_embedded_value(group, config));
    }
    s.push('>');
    if let Some(content) = hole.content.as_deref() {
        s.push('[');
        s.push_str(&render_content_blocks(content, config));
        s.push(']');
    }
    s
}

/// An element (or a bare entry) sitting in a value slot: its `head`, then
/// `(args)[content]{value}`. Neither `id` nor connects are printed; the
/// parser never puts one in a value slot.
fn render_embedded(
    head: String,
    data: &tomet_ast::Data,
    content: Option<&[Block]>,
    config: &PrinterConfig,
) -> String {
    let mut s = match &data.args {
        Some(args) => format!("{head}({})", render_args(args, config)),
        None => head,
    };
    if let Some(content) = content {
        s.push('[');
        s.push_str(&render_content_blocks(content, config));
        s.push(']');
    }
    if let Some(group) = &data.value {
        s.push_str(&render_embedded_value(group, config));
    }
    s
}

/// A `{...}` group of an embedded node, its entries in source order.
#[deny(clippy::wildcard_enum_match_arm)]
fn render_embedded_value(group: &Group, config: &PrinterConfig) -> String {
    let Group(entries) = group;
    let mut s = String::from('{');
    for (i, entry) in entries.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        match entry {
            Entry::Pair(k, v) => {
                s.push_str(&format!("{}: {}", key_text(k), render_nested(v, config)))
            }
            Entry::Element(nested) => {
                let nested = Value::Element(Box::new(nested.clone()));
                s.push_str(&render_nested(&nested, config));
            }
            Entry::Hole(nested) => {
                let nested = Value::Hole(Box::new(nested.clone()));
                s.push_str(&render_nested(&nested, config));
            }
            Entry::Bare(bare) => s.push_str(&render_embedded(
                String::new(),
                &bare.data,
                bare.content.as_deref(),
                config,
            )),
        }
    }
    s.push('}');
    s
}

/// Renders the `(args)` of an element or declaration. A lone `target` entry is written
/// `target: value`; any other value is rendered by [`render_value_inner_with_config`], with a
/// `key: [...]` value written through the printer's block renderer, so
/// `@conflict(a: [multi-paragraph content], ...)` keeps everything past the first paragraph.
pub(crate) fn render_args(v: &Value, config: &PrinterConfig) -> String {
    if let Value::Map(entries) = v
        && entries.len() == 1
    {
        let key = &entries[0].0;
        if key == "target" {
            let space = link_colon(config);
            return format!(
                "{key}:{space}{}",
                render_value_inner_with_config(&entries[0].1, config)
            );
        }
    }
    render_value_inner_with_config(v, config)
}

fn render_meta_field_value(
    v: &Value,
    field_cfg: Option<&FieldConfig>,
    config: &PrinterConfig,
) -> String {
    if let Value::Null = v {
        return String::new();
    }
    if let Some(cfg) = field_cfg {
        if (cfg.format.as_deref() == Some("rfc3339")
            || cfg
                .field_type
                .as_deref()
                .and_then(tomet_type::Type::from_name)
                == Some(tomet_type::Type::Datetime))
            && let Value::String(s) = v
        {
            return tomet_field_utils::format_rfc3339(s, cfg.offset.as_deref());
        }
        if cfg.always_newline
            && let Value::Seq(items) = v
        {
            if items.is_empty() {
                return "[]".to_string();
            }
            let mut s = String::new();
            for item in items {
                s.push_str("\n    - ");
                s.push_str(&render_value_inner_with_config(item, config));
            }
            return s;
        }
    }
    if let Value::String(s) = v
        && is_iso8601(s)
    {
        return s.clone();
    }
    render_value_inner_with_config(v, config)
}

/// Renders a `::meta(...)` declaration per `config`'s meta format settings: with
/// `config.meta.field_layout` (`meta.always_newline`), one field per line with the per-field formatting rules
///
/// ```text
/// ::meta(
///   title: T
///   tags:
///     - a
/// )
/// ```
///
/// else a single `::meta(k: v, ...)` line, and `::meta()` for a document with no field. The fields
/// are the declaration's `(args)`; `{...}` is no part of `::meta`, but a declaration that has one
/// (`tomet check` reports it) is printed with it rather than losing it.
pub(crate) fn render_meta_declaration(declaration: &Declaration, config: &PrinterConfig) -> String {
    let head = "::meta";
    if config.meta.field_layout == Some(GroupLayout::OnePerLine)
        && declaration.data.value.is_none()
        && let Some(Value::Map(entries)) = tomet_semantics::data::declaration_entries(declaration)
        && !entries.is_empty()
    {
        let rows: Vec<(String, String)> = entries
            .iter()
            .map(|(k, v)| {
                let val_str = render_meta_field_value(v, config.meta.fields.get(k), config);
                (key_text(k), val_str)
            })
            .collect();
        // A value that runs over several lines, or is empty, has nothing on the line to align.
        let on_the_line = |value: &str| !(value.starts_with('\n') || value.is_empty());
        let alignment = config.format.map_align_values.then(|| ValueAlignment {
            widest_key: config.format.max_key_width(),
        });
        let entries: Vec<String> = rows
            .iter()
            .enumerate()
            .map(|(i, (key, value))| {
                let pad = match alignment {
                    // The run of rows this one is in: those around it whose value is on the line.
                    Some(alignment) if on_the_line(value) => {
                        let forward = rows[i..].iter().take_while(|(_, value)| on_the_line(value));
                        let backward = rows[..i]
                            .iter()
                            .rev()
                            .take_while(|(_, value)| on_the_line(value));
                        let column =
                            alignment.column(forward.chain(backward).map(|(key, _)| key.as_str()));
                        alignment.padding(key, column)
                    }
                    None if on_the_line(value) => 1,
                    _ => 0,
                };
                format!("{key}:{}{value}", " ".repeat(pad))
            })
            .collect();
        return format!("{head}{}", GroupLayout::OnePerLine.render(&entries, ""));
    }

    let mut out = String::from(head);
    out.push('(');
    if let Some(args) = &declaration.data.args {
        out.push_str(&render_args(args, config));
    }
    out.push(')');
    if let Some(v) = tomet_semantics::data::declaration_data(declaration) {
        out.push('{');
        out.push_str(&render_value_inner_with_config(&v, config));
        out.push('}');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_ast::Group;

    fn render_value_inner(v: &Value) -> String {
        render_value_inner_with_config(v, &PrinterConfig::default())
    }

    fn map(entries: &[(&str, Value)]) -> Value {
        Value::Map(
            entries
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        )
    }

    #[test]
    fn a_nested_map_keeps_its_braces() {
        // Without them `m: { k: v }` printed as `m: k: v`, which reparses
        // as the string "k: v" -- nesting destroyed by any
        // print-then-parse cycle.
        assert_eq!(
            render_value(&map(&[
                ("id", Value::String("c1".into())),
                ("m", map(&[("k", Value::String("v".into()))])),
            ])),
            "{id: c1, m: {k: v}}"
        );
    }

    #[test]
    fn a_map_inside_a_sequence_keeps_its_braces() {
        assert_eq!(
            render_value_inner(&Value::Seq(vec![
                map(&[("k", Value::Int(1))]),
                Value::Int(2),
            ])),
            "({k: 1}, 2)"
        );
    }

    #[test]
    fn a_top_level_map_is_still_wrapped_once() {
        assert_eq!(render_value(&map(&[("a", Value::Int(1))])), "{a: 1}");
    }

    #[test]
    fn a_call_renders_as_name_and_parenthesized_args() {
        assert_eq!(
            render_value_inner(&Value::Call(
                "enum".into(),
                vec![Value::String("card".into())]
            )),
            "enum(card)"
        );
    }

    #[test]
    fn render_value_inner_quotes_strings_needing_it() {
        assert_eq!(
            render_value_inner(&Value::String("plain".to_string())),
            "plain"
        );
        assert_eq!(
            render_value_inner(&Value::String("has space".to_string())),
            "\"has space\""
        );
        assert_eq!(render_value_inner(&Value::Null), "");
    }

    #[test]
    fn render_args_handles_target() {
        let cfg = PrinterConfig::default();
        let reference = Value::Map(vec![(
            "target".to_string(),
            Value::String("ref:name".to_string()),
        )]);
        assert_eq!(render_args(&reference, &cfg), "target: \"ref:name\"");

        let cfg_no_space = PrinterConfig {
            format: tomet_config::FormatConfig {
                link_target_colon: Some(tomet_style::ColonSpacing::Tight),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(
            render_args(&reference, &cfg_no_space),
            "target:\"ref:name\""
        );
    }

    #[test]
    fn render_meta_declaration_single_line_by_default() {
        let mut el = tomet_tree::declaration_new("meta");
        el.data.args = Some(Value::Map(vec![(
            "id".to_string(),
            Value::String("doc-12345678".to_string()),
        )]));
        let cfg = PrinterConfig::default();
        assert_eq!(
            render_meta_declaration(&el, &cfg),
            "::meta(id: doc-12345678)"
        );
    }

    #[test]
    fn render_meta_declaration_multiline_with_a_config_that_asks_for_it() {
        let mut el = tomet_tree::declaration_new("meta");
        el.data.args = Some(Value::Map(vec![(
            "id".to_string(),
            Value::String("doc-12345678".to_string()),
        )]));
        assert_eq!(
            render_meta_declaration(&el, &PrinterConfig::default()),
            "::meta(id: doc-12345678)"
        );
        let mut cfg = PrinterConfig::default();
        cfg.meta.field_layout = Some(tomet_style::GroupLayout::OnePerLine);
        assert_eq!(
            render_meta_declaration(&el, &cfg),
            "::meta(\n  id: doc-12345678\n)"
        );
    }

    #[test]
    fn render_meta_declaration_multiline_when_always_newline_configured() {
        let mut el = tomet_tree::declaration_new("meta");
        el.data.args = Some(Value::Map(vec![(
            "id".to_string(),
            Value::String("doc-12345678".to_string()),
        )]));
        let cfg = PrinterConfig {
            meta: tomet_config::MetaConfig {
                field_layout: Some(tomet_style::GroupLayout::OnePerLine),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(
            render_meta_declaration(&el, &cfg),
            "::meta(\n  id: doc-12345678\n)"
        );
    }

    #[test]
    fn the_fields_of_a_meta_are_aligned_when_values_are_to_be() {
        let text = |s: &str| Value::String(s.to_string());
        let mut el = tomet_tree::declaration_new("meta");
        el.data.args = Some(Value::Map(vec![
            ("id".to_string(), text("x")),
            ("title".to_string(), text("T")),
            ("tags".to_string(), Value::Seq(vec![text("a"), text("b")])),
            ("kind".to_string(), text("k")),
            ("created".to_string(), text("c")),
        ]));
        let mut cfg = PrinterConfig::default();
        cfg.meta.field_layout = Some(tomet_style::GroupLayout::OnePerLine);
        cfg.meta.fields.insert(
            "tags".to_string(),
            tomet_config::FieldConfig {
                always_newline: true,
                ..Default::default()
            },
        );
        assert_eq!(
            render_meta_declaration(&el, &cfg),
            "::meta(\n  id: x\n  title: T\n  tags:\n    - a\n    - b\n  kind: k\n  created: c\n)"
        );
        // A field that runs over several lines ends a run: `tags` is not aligned with its
        // neighbours, and they are aligned among themselves.
        cfg.format.map_align_values = true;
        assert_eq!(
            render_meta_declaration(&el, &cfg),
            "::meta(\n  id:    x\n  title: T\n  tags:\n    - a\n    - b\n  kind:    k\n  created: c\n)"
        );
    }

    #[test]
    fn a_meta_field_wider_than_the_limit_is_left_out_of_the_alignment() {
        let text = |s: &str| Value::String(s.to_string());
        let mut el = tomet_tree::declaration_new("meta");
        el.data.args = Some(Value::Map(vec![
            ("id".to_string(), text("x")),
            ("a_field_that_is_far_too_long".to_string(), text("y")),
            ("kind".to_string(), text("k")),
        ]));
        let mut cfg = PrinterConfig::default();
        cfg.meta.field_layout = Some(tomet_style::GroupLayout::OnePerLine);
        cfg.format.map_align_values = true;
        assert_eq!(
            render_meta_declaration(&el, &cfg),
            "::meta(\n  id:   x\n  a_field_that_is_far_too_long: y\n  kind: k\n)"
        );
    }

    #[test]
    fn a_meta_with_no_field_is_printed_empty_whatever_the_config_asks() {
        let el = tomet_tree::declaration_new("meta");
        let mut cfg = PrinterConfig::default();
        assert_eq!(render_meta_declaration(&el, &cfg), "::meta()");
        cfg.meta.field_layout = Some(tomet_style::GroupLayout::OnePerLine);
        assert_eq!(render_meta_declaration(&el, &cfg), "::meta()");

        let mut empty = tomet_tree::declaration_new("meta");
        empty.data.args = Some(Value::Map(Vec::new()));
        assert_eq!(render_meta_declaration(&empty, &cfg), "::meta()");
    }

    #[test]
    fn the_fields_of_a_meta_keep_their_per_field_rules_on_separate_lines() {
        let mut el = tomet_tree::declaration_new("meta");
        el.data.args = Some(Value::Map(vec![
            ("title".to_string(), Value::String("T".to_string())),
            (
                "tags".to_string(),
                Value::Seq(vec![
                    Value::String("a".to_string()),
                    Value::String("b".to_string()),
                ]),
            ),
        ]));
        let mut cfg = PrinterConfig::default();
        cfg.meta.field_layout = Some(tomet_style::GroupLayout::OnePerLine);
        cfg.meta.fields.insert(
            "tags".to_string(),
            tomet_config::FieldConfig {
                always_newline: true,
                ..Default::default()
            },
        );
        assert_eq!(
            render_meta_declaration(&el, &cfg),
            "::meta(\n  title: T\n  tags:\n    - a\n    - b\n)"
        );
        // Without the layout asked for, the same fields are one line.
        assert_eq!(
            render_meta_declaration(&el, &PrinterConfig::default()),
            "::meta(title: T, tags: (a, b))"
        );
    }

    #[test]
    fn a_brace_body_on_a_meta_is_printed_not_dropped() {
        let mut el = tomet_tree::declaration_new("meta");
        el.data.args = Some(Value::Map(vec![("a".to_string(), Value::Int(1))]));
        el.data.value = Some(Group::from_map(Value::Map(vec![(
            "b".to_string(),
            Value::Int(2),
        )])));
        let mut cfg = PrinterConfig::default();
        cfg.meta.field_layout = Some(tomet_style::GroupLayout::OnePerLine);
        assert_eq!(render_meta_declaration(&el, &cfg), "::meta(a: 1){b: 2}");
    }

    #[test]
    fn render_meta_declaration_quotes_keys_that_are_not_identifiers() {
        let mut el = tomet_tree::declaration_new("meta");
        el.data.args = Some(Value::Map(vec![
            ("plain.key".to_string(), Value::Int(1)),
            ("tmtroot/a b.tmt".to_string(), Value::Int(2)),
        ]));
        let cfg = PrinterConfig {
            meta: tomet_config::MetaConfig {
                field_layout: Some(tomet_style::GroupLayout::OnePerLine),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(
            render_meta_declaration(&el, &cfg),
            "::meta(\n  plain.key: 1\n  \"tmtroot/a b.tmt\": 2\n)"
        );
    }
}
