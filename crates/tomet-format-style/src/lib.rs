//! Applies `tomet_config::PrinterConfig`'s style rules (meta format,
//! link-key spacing (`target`, `@link`/`<embed>`'s one shared key), per-field
//! `::meta` formatting) to a single
//! `tomet_ast::Value`, `Element` or `Declaration`, returning `.tmt`-syntax text.
//! Deliberately scoped to single nodes -- no `File`, no tree
//! position, no recursion into sibling/child elements of other kinds --
//! so both `tomet-printer` (rebuilds a whole `File`) and
//! `tomet-formatter` (patches specific spans of existing `.tmt` text)
//! can depend on this without a circular dependency (`tomet-printer`
//! already depends on `tomet-formatter`). `render_meta_declaration`
//! specifically covers only `::meta` declarations' actual shape per the
//! grammar (`args` the map of the document's fields, written `::meta(k: v)`) -- not the general
//! element-rendering logic for every other sigil/kind, which stays in
//! `tomet-printer` since it recurses into inline/child content and
//! is genuinely part of rebuilding a whole file from its AST.

use tomet_ast::{Block, Declaration, Entry, Group, Inline, Value};

/// Renders one `Block` of a value-embedded element's `[content]` (`Vec<Block>`):
/// a value-embedded element's content is inline in practice, so this only
/// descends into a `Paragraph`'s own inlines, not into nested structure.
#[deny(clippy::wildcard_enum_match_arm)]
fn render_nested_block(block: &Block, config: &PrinterConfig, out: &mut String) {
    let Block::Paragraph(p) = block else {
        return;
    };
    for inline in &p.content {
        match inline {
            Inline::Text(t) => out.push_str(&t.value),
            Inline::Raw(r) => out.push_str(&r.value),
            Inline::LineBreak(_) => out.push('\n'),
            Inline::SoftBreak(_) => out.push(' '),
            Inline::Element(child_el) => {
                let child_val = Value::Element(Box::new(child_el.clone()));
                out.push_str(&render_nested(&child_val, config));
            }
            Inline::Interp(interp) => {
                let child_val = Value::Interp(interp.clone());
                out.push_str(&render_nested(&child_val, config));
            }
            Inline::Hole(hole) => {
                let child_val = Value::Hole(Box::new(hole.clone()));
                out.push_str(&render_nested(&child_val, config));
            }
        }
    }
}
use tomet_config::{FieldConfig, PrinterConfig};
use tomet_field_utils::is_iso8601;
use unicode_width::UnicodeWidthStr;

pub fn render_value(v: &Value) -> String {
    format!("{{{}}}", render_value_inner(v))
}

pub fn render_value_inner(v: &Value) -> String {
    render_value_inner_with_config(v, &PrinterConfig::default())
}

/// Formats a string scalar for Tomet source text, wrapping in `"` and escaping
/// inner quotes and special characters only when necessary.
pub fn quote_scalar_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    write_scalar_string(s, &mut out);
    out
}

/// Characters legal inside a bare map key.
pub fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-' || c == '.'
}

/// Writes `s` as a `"..."` string, with `\"`, `\\`, `\n`, `\r`, `\t` escapes.
pub fn write_quoted(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
}

/// `s` as a `"..."` string whatever it holds.
pub fn quoted_text(s: &str) -> String {
    let mut out = String::new();
    write_quoted(s, &mut out);
    out
}

/// Writes a map key: bare when it is a non-empty run of [`is_ident_char`],
/// quoted (`"tmtroot/a/b.tmt"`) otherwise.
pub fn write_key(key: &str, out: &mut String) {
    if !key.is_empty() && key.chars().all(is_ident_char) {
        out.push_str(key);
    } else {
        write_quoted(key, out);
    }
}

/// [`write_key`] as an owned string.
pub fn key_text(key: &str) -> String {
    let mut out = String::new();
    write_key(key, &mut out);
    out
}

fn classify_number(s: &str) -> bool {
    let b = s.as_bytes();
    let digits = |mut i: usize| {
        let start = i;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        (i > start).then_some(i)
    };
    let mut i = usize::from(b.first() == Some(&b'-'));
    match b.get(i) {
        Some(b'0') => i += 1,
        Some(b'1'..=b'9') => {
            let Some(next_i) = digits(i) else {
                return false;
            };
            i = next_i;
        }
        _ => return false,
    }
    if b.get(i) == Some(&b'.') {
        let Some(next_i) = digits(i + 1) else {
            return false;
        };
        i = next_i;
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(b.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        let Some(next_i) = digits(i) else {
            return false;
        };
        i = next_i;
    }
    i == b.len()
}

/// Appends a safely quoted/escaped string scalar to `out`.
pub fn write_scalar_string(s: &str, out: &mut String) {
    if is_iso8601(s) {
        out.push_str(s);
        return;
    }

    if (s.starts_with("${") && s.ends_with('}'))
        || (s.starts_with('$') && s.contains('(') && s.ends_with(')'))
    {
        out.push_str(s);
        return;
    }

    let first_char = s.chars().next();
    let starts_with_special = matches!(
        first_char,
        Some('&' | '*' | '!' | '%' | '@' | '`' | '|' | '>' | '?' | '-' | '~')
    );

    let is_bare_keyword_or_number = matches!(s, "null" | "true" | "false") || classify_number(s);

    let needs_quotes = s.is_empty()
        || is_bare_keyword_or_number
        || s.trim() != s
        || starts_with_special
        || s.contains([
            '"', '\'', ':', ',', '(', ')', '[', ']', '{', '}', '\n', '\r', '\t', ' ',
        ]);

    if needs_quotes {
        write_quoted(s, out);
    } else {
        out.push_str(s);
    }
}

/// Renders a `Vec<Block>` slot (an element's `[content]`, or a `key:
/// [...]` value) back into `.tmt` source text.
///
/// This crate deliberately stays scoped to single nodes (see the module
/// doc) and has no real block renderer of its own -- recursing through
/// sections/lists/nested elements is `tomet-printer`'s
/// `render_content_blocks`, one layer up, which this crate cannot depend
/// on without an upward (and circular) dependency. Threading the renderer
/// in as a callback instead lets `tomet-printer` plug its real one in
/// (full fidelity, needed for e.g. `@conflict(a: [multi-paragraph content],
/// ...)`), while every other caller (`tomet-formatter`, which patches
/// spans incrementally and has no block renderer either) keeps the
/// restricted default (`default_block_renderer`: paragraph-inlines only,
/// same limit `Value::Element`'s own `content` already lived with before
/// this existed).
pub type BlockRenderer<'a> = dyn FnMut(&[Block], &PrinterConfig) -> String + 'a;

fn default_block_renderer(blocks: &[Block], config: &PrinterConfig) -> String {
    let mut out = String::new();
    for block in blocks {
        render_nested_block(block, config, &mut out);
    }
    out
}

/// Renders a value that sits *inside* another one.
///
/// A map needs its own braces there. `render_value_inner_with_config`
/// deliberately renders a map unwrapped, because at the top level the
/// caller supplies the braces -- but recursing into it that way printed
/// `m: { k: v }` as `m: k: v`, which reparses as the string `"k: v"`.
/// Nesting was destroyed by any print-then-parse cycle.
///
/// Public because the printer renders a value group's pairs itself and
/// needs the same rule.
pub fn render_nested(v: &Value, config: &PrinterConfig) -> String {
    render_nested_with_blocks(v, config, &mut default_block_renderer)
}

/// [`render_nested`] with the block renderer supplied explicitly, the way
/// [`render_args_with_blocks`] has it for `(args)`: a printer that can render blocks in full
/// passes its own, so a `key: [...]` value of a `{...}` group is not cut down to
/// paragraph-inlines only.
pub fn render_nested_with_blocks(
    v: &Value,
    config: &PrinterConfig,
    render_blocks: &mut BlockRenderer,
) -> String {
    match v {
        Value::Map(_) => format!(
            "{{{}}}",
            render_value_inner_with_blocks(v, config, render_blocks)
        ),
        _ => render_value_inner_with_blocks(v, config, render_blocks),
    }
}

pub fn render_value_inner_with_config(v: &Value, config: &PrinterConfig) -> String {
    render_value_inner_with_blocks(v, config, &mut default_block_renderer)
}

/// Same as [`render_value_inner_with_config`], but with the block
/// renderer supplied explicitly instead of defaulting to the restricted
/// one. See [`BlockRenderer`].
#[deny(clippy::wildcard_enum_match_arm)]
pub fn render_value_inner_with_blocks(
    v: &Value,
    config: &PrinterConfig,
    render_blocks: &mut BlockRenderer,
) -> String {
    match v {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => Value::float_text(*f),
        // As written: formatting must not turn `1.10` into `1.1`.
        Value::Number(text) => text.clone(),
        Value::String(s) => quote_scalar_string(s),
        // `(a, b)` is the sequence spelling -- `[...]` already means
        // `[content]`, and `list(...)` is a type.
        Value::Seq(items) => {
            let rendered: Vec<_> = items
                .iter()
                .map(|item| render_nested_with_blocks(item, config, render_blocks))
                .collect();
            format!("({})", rendered.join(", "))
        }
        Value::Map(entries) => {
            if entries.len() == 1 {
                let key = &entries[0].0;
                if key == "target" {
                    let space = if config.format.link_no_space { "" } else { " " };
                    format!(
                        "@({key}:{space}{})",
                        render_nested_with_blocks(&entries[0].1, config, render_blocks)
                    )
                } else {
                    let mut parts = Vec::new();
                    for (idx, (k, val)) in entries.iter().enumerate() {
                        let val_str = render_nested_with_blocks(val, config, render_blocks);
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
                    let val_str = render_nested_with_blocks(val, config, render_blocks);
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
            let rendered: Vec<_> = args
                .iter()
                .map(|a| render_nested_with_blocks(a, config, render_blocks))
                .collect();
            format!("{name}({})", rendered.join(", "))
        }
        // `parse_entry_value` only ever produces one of these with `args`
        // set and `content`/`value`/`children` all `None` (an MVP scope
        // limit enforced at parse time, not just convention) -- so `args`
        // alone is the whole element to print back.
        // A `$name(args)`/`${...}` sitting in a value slot is written back in
        // the `${...}` form (`InterpExpr`'s own `Display`), the same
        // normalization `tomet-transform::interp` uses for one left
        // unresolved in prose.
        Value::Interp(interp) => format!("${{{}}}", interp.expr),
        Value::Element(el) => render_embedded(
            format!("@{}", el.name),
            &el.data,
            el.content.as_deref(),
            config,
            render_blocks,
        ),
        Value::Hole(hole) => render_embedded_hole(hole, config, render_blocks),
        Value::Blocks(blocks) => format!("[{}]", render_blocks(blocks, config)),
    }
}

/// A hole sitting in a value slot: `<?matcher(args){value}>[content]`. The head is
/// inside the box and the content follows it; no `id` is printed, as for an element.
fn render_embedded_hole(
    hole: &tomet_ast::Hole,
    config: &PrinterConfig,
    render_blocks: &mut BlockRenderer,
) -> String {
    let mut s = String::from("<?");
    if let Some(matcher) = &hole.matcher {
        s.push_str(&matcher.to_string());
    }
    if let Some(args) = &hole.data.args {
        s.push_str(&format!(
            "({})",
            render_args_with_blocks(args, config, render_blocks)
        ));
    }
    if let Some(group) = &hole.data.value {
        s.push_str(&render_embedded_value(group, config, render_blocks));
    }
    s.push('>');
    if let Some(content) = hole.content.as_deref() {
        s.push('[');
        s.push_str(&render_blocks(content, config));
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
    render_blocks: &mut BlockRenderer,
) -> String {
    let mut s = match &data.args {
        Some(args) => format!(
            "{head}({})",
            render_args_with_blocks(args, config, render_blocks)
        ),
        None => head,
    };
    if let Some(content) = content {
        s.push('[');
        s.push_str(&render_blocks(content, config));
        s.push(']');
    }
    if let Some(group) = &data.value {
        s.push_str(&render_embedded_value(group, config, render_blocks));
    }
    s
}

/// A `{...}` group of an embedded node, its entries in source order.
#[deny(clippy::wildcard_enum_match_arm)]
fn render_embedded_value(
    group: &Group,
    config: &PrinterConfig,
    render_blocks: &mut BlockRenderer,
) -> String {
    let Group(entries) = group;
    let mut s = String::from('{');
    for (i, entry) in entries.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        match entry {
            Entry::Pair(k, v) => s.push_str(&format!(
                "{}: {}",
                key_text(k),
                render_nested_with_blocks(v, config, render_blocks)
            )),
            Entry::Element(nested) => {
                let nested = Value::Element(Box::new(nested.clone()));
                s.push_str(&render_nested_with_blocks(&nested, config, render_blocks));
            }
            Entry::Hole(nested) => {
                let nested = Value::Hole(Box::new(nested.clone()));
                s.push_str(&render_nested_with_blocks(&nested, config, render_blocks));
            }
            Entry::Bare(bare) => s.push_str(&render_embedded(
                String::new(),
                &bare.data,
                bare.content.as_deref(),
                config,
                render_blocks,
            )),
        }
    }
    s.push('}');
    s
}

pub fn render_args_with_config(v: &Value, config: &PrinterConfig) -> String {
    render_args_with_blocks(v, config, &mut default_block_renderer)
}

/// Same as [`render_args_with_config`], but with the block renderer
/// supplied explicitly. See [`BlockRenderer`].
pub fn render_args_with_blocks(
    v: &Value,
    config: &PrinterConfig,
    render_blocks: &mut BlockRenderer,
) -> String {
    if let Value::Map(entries) = v
        && entries.len() == 1
    {
        let key = &entries[0].0;
        if key == "target" {
            let space = if config.format.link_no_space { "" } else { " " };
            return format!(
                "{key}:{space}{}",
                render_value_inner_with_blocks(&entries[0].1, config, render_blocks)
            );
        }
    }
    render_value_inner_with_blocks(v, config, render_blocks)
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
/// `config.meta.always_newline`, one field per line with the per-field formatting rules
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
pub fn render_meta_declaration(declaration: &Declaration, config: &PrinterConfig) -> String {
    let head = "::meta";
    if config.meta.always_newline
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
        let mut out = format!("{head}(\n");
        for (i, (key, value)) in rows.iter().enumerate() {
            out.push_str("  ");
            out.push_str(key);
            out.push(':');
            if on_the_line(value) {
                // A key wider than `map.max_key_width` is not padded, and the others do not pad
                // to it.
                let widest = config.format.max_key_width();
                let width = if config.format.map_align_values && key.width() <= widest {
                    // The widest key of the run of rows this one is in.
                    let run = |range: &mut dyn Iterator<Item = &(String, String)>| {
                        range
                            .take_while(|(_, value)| on_the_line(value))
                            .map(|(key, _)| key.width())
                            .filter(|&width| width <= widest)
                            .max()
                            .unwrap_or(0)
                    };
                    run(&mut rows[i..].iter()).max(run(&mut rows[..i].iter().rev()))
                } else {
                    key.width()
                };
                out.push_str(&" ".repeat(width - key.width() + 1));
            }
            out.push_str(value);
            out.push('\n');
        }
        out.push(')');
        return out;
    }

    let mut out = String::from(head);
    out.push('(');
    if let Some(args) = &declaration.data.args {
        out.push_str(&render_args_with_config(args, config));
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
    fn render_args_with_config_handles_target() {
        let cfg = PrinterConfig::default();
        let reference = Value::Map(vec![(
            "target".to_string(),
            Value::String("ref:name".to_string()),
        )]);
        assert_eq!(
            render_args_with_config(&reference, &cfg),
            "target: \"ref:name\""
        );

        let cfg_no_space = PrinterConfig {
            format: tomet_config::FormatConfig {
                link_no_space: true,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(
            render_args_with_config(&reference, &cfg_no_space),
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
        cfg.meta.always_newline = true;
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
                always_newline: true,
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
        cfg.meta.always_newline = true;
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
        cfg.meta.always_newline = true;
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
        cfg.meta.always_newline = true;
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
        cfg.meta.always_newline = true;
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
        cfg.meta.always_newline = true;
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
                always_newline: true,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(
            render_meta_declaration(&el, &cfg),
            "::meta(\n  plain.key: 1\n  \"tmtroot/a b.tmt\": 2\n)"
        );
    }

    #[test]
    fn test_quote_scalar_string_escaping_and_keywords() {
        assert_eq!(quote_scalar_string("true"), "\"true\"");
        assert_eq!(quote_scalar_string("false"), "\"false\"");
        assert_eq!(quote_scalar_string("null"), "\"null\"");
        assert_eq!(quote_scalar_string("123"), "\"123\"");
        assert_eq!(quote_scalar_string("45.67"), "\"45.67\"");
        assert_eq!(
            quote_scalar_string("hello \"world\""),
            "\"hello \\\"world\\\"\""
        );
        assert_eq!(quote_scalar_string("line1\nline2"), "\"line1\\nline2\"");
        assert_eq!(
            quote_scalar_string("2026-06-17T05:52:44Z"),
            "2026-06-17T05:52:44Z"
        );
    }
}
