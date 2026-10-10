//! Rendering blocks, sections, headings, and lists.

use tomet_ast::{Block, Element, Inline, List, Placement, Section, Sigil};
use tomet_config::PrinterConfig;
use tomet_semantics::{ElementKind, classify_std_lenient, heading_level};
use tomet_style::render_value;

use crate::element::{render_args, render_connects, render_element};
use crate::id::render_id;
use crate::inline::{render_content_blocks, render_inlines};

pub(crate) fn render_block(block: &Block, config: &PrinterConfig, out: &mut String) {
    match block {
        // A heading joined into a paragraph by an explicit `\`
        // continuation trigger (`docs/spec/syntax.tmt`'s `##[ 継続 ]`) --
        // rare, and flagged by `tomet-semantics::shape_mismatch` as a
        // kind that cannot actually be inline, but still round-trips.
        // It still sits at the document's own top level, at a genuine
        // line start, not buried inside another element's content the
        // way `nested_inline_heading_does_not_reserialize_as_hash_sugar`
        // guards against. So its sugar is still valid to print here: only
        // the placement, not the sugar, has changed. `render_inlines`
        // (shared with truly-nested content) has no heading arm and
        // would otherwise fall back to the full `@heading(...)` form for
        // exactly this case.
        //
        // A list cannot join a paragraph this way any more: `List` is a
        // first-class `Block` variant now, with no `Inline` form at all
        // (see `tomet_ast::List`'s own doc comment), so it can never sit
        // as `p.content.first()` in the first place -- a list is always
        // block-level, by construction, the same way a `Section` already
        // was.
        Block::Paragraph(p)
            if matches!(
                p.content.first(),
                Some(Inline::Element(el)) if el.placement == Placement::Inline
                    && classify_std_lenient(el) == ElementKind::Heading
            ) =>
        {
            let Some(Inline::Element(first)) = p.content.first() else {
                unreachable!()
            };
            let mut head = String::new();
            render_heading_element(first, config, &mut head);
            out.push_str(head.trim_end_matches('\n'));
            out.push_str(&render_inlines(&p.content[1..], config));
            out.push('\n');
        }
        Block::Paragraph(p) => {
            let inlines_text = render_inlines(&p.content, config);
            if !inlines_text.trim().is_empty() {
                out.push_str(&inlines_text);
                out.push('\n');
            }
        }
        Block::Element(el) if classify_std_lenient(el) == ElementKind::Heading => {
            render_heading_element(el, config, out)
        }
        Block::Element(el) => {
            out.push_str(&render_element(el, config));
            out.push('\n');
        }
        Block::Section(sec) => render_section(sec, config, out),
        Block::List(list) => render_list(list, config, out),
    }
}

pub(crate) fn render_section(sec: &Section, config: &PrinterConfig, out: &mut String) {
    let level = sec.level.max(1);
    out.push_str(&"=".repeat(level));
    if !sec.title.is_empty() {
        if config.format.heading_space_inside_brackets {
            out.push_str("[ ");
            out.push_str(&render_inlines(&sec.title, config));
            out.push_str(" ]");
        } else {
            out.push('[');
            out.push_str(&render_inlines(&sec.title, config));
            out.push(']');
        }
    }
    if let Some(args) = &sec.args {
        out.push('(');
        out.push_str(&render_args(args, config));
        out.push(')');
    }
    if let Some(v) = sec.value.as_ref().and_then(|v| v.as_data()) {
        out.push(' ');
        out.push_str(&render_value(&v));
    }
    if sec.id.is_some() {
        out.push(' ');
    }
    out.push_str(&render_id(sec.id.as_ref()));
    out.push_str(&render_connects(&sec.connects, config));
    out.push('\n');

    for child in &sec.blocks {
        render_block(child, config, out);
    }
}

/// Called only from `render_block`'s two top-level dispatch arms (a
/// standalone `Block::Element`, or the first item of a `Block::Paragraph`
/// it joined by the default placement rule), never from the shared,
/// recursively-called `render_element` (which `render_inlines`/
/// `ElementValue::Children` both call for genuinely nested/inline
/// elements) -- a nested `@heading(...)` must not round-trip back to
/// `#`-sugar,
/// consistent with `tomet-html`/`tomet-markdown`'s
/// equivalent gating for the same resolved decision.
pub(crate) fn render_heading_element(el: &Element, config: &PrinterConfig, out: &mut String) {
    let level = heading_level(el).unwrap_or(1) as usize;
    let content = el.content.as_deref().unwrap_or(&[]);
    out.push_str(&"#".repeat(level));
    if config.format.heading_space_inside_brackets {
        out.push_str("[ ");
        out.push_str(&render_content_blocks(content, config));
        out.push_str(" ]");
    } else {
        out.push('[');
        out.push_str(&render_content_blocks(content, config));
        out.push(']');
    }
    if let Some(v) = el.value.as_ref().and_then(|v| v.as_data()) {
        out.push(' ');
        out.push_str(&render_value(&v));
    }
    if el.id.is_some() {
        out.push(' ');
    }
    out.push_str(&render_id(el.id.as_ref()));
    out.push_str(&render_connects(&el.connects, config));
    out.push('\n');
}

pub(crate) fn render_list(list: &List, config: &PrinterConfig, out: &mut String) {
    render_list_with_indent(list, 0, config, out);
}

pub(crate) fn render_list_with_indent(
    list: &List,
    indent: usize,
    config: &PrinterConfig,
    out: &mut String,
) {
    let ordered = list.ordered;
    let indent_str = "  ".repeat(indent);
    for list_item in &list.items {
        let item = &list_item.element;

        // The combine notation (`-@name(...)`): `item` is a real, named
        // `Element` identical to what a standalone `@name(...)` would
        // parse to, so it round-trips through the exact same
        // `render_element` a standalone one would use -- no marker, no
        // `( )`-wrapping, just the bare `-`/`-.` prefix directly against
        // `@name`.
        if matches!(item.sigil, Sigil::Named(_)) {
            out.push_str(&indent_str);
            out.push_str(if ordered { "-." } else { "-" });
            out.push_str(&render_element(item, config));
            out.push('\n');
            if let Some(sub) = &list_item.sublist {
                render_list_with_indent(sub, indent + 1, config, out);
            }
            continue;
        }

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
            head_prefix.push_str(&render_args(marker, config));
            head_prefix.push_str(") ");
        }

        let content_str = render_content_blocks(item.content.as_deref().unwrap_or(&[]), config);
        let lines: Vec<&str> = content_str.lines().collect();
        let item_attrs = match &item.value {
            Some(v) => v.as_data(),
            _ => None,
        };

        if lines.len() > 1 && config.format.list_multiline_style_content.is_some() {
            let style = config
                .format
                .list_multiline_style_content
                .as_deref()
                .unwrap();
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
            if item.id.is_some() {
                out.push(' ');
            }
            out.push_str(&render_id(item.id.as_ref()));
            out.push_str(&render_connects(&item.connects, config));
            out.push('\n');
        } else {
            out.push_str(&head_prefix);
            out.push_str(&content_str);
            if let Some(attrs) = item_attrs {
                out.push(' ');
                out.push_str(&render_value(&attrs));
            }
            if item.id.is_some() {
                out.push(' ');
            }
            out.push_str(&render_id(item.id.as_ref()));
            out.push_str(&render_connects(&item.connects, config));
            out.push('\n');
        }
        if let Some(sub) = &list_item.sublist {
            render_list_with_indent(sub, indent + 1, config, out);
        }
    }
}
