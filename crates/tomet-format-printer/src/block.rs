//! Rendering blocks, sections, headings, and lists.

use tomet_ast::{Block, Element, Inline, List, Placement, Section};
use tomet_config::PrinterConfig;
use tomet_semantics::{ElementKind, classify_std_lenient, heading_level};
use tomet_style::{BracketSpacing, ParagraphSeparator, stands_like_a_paragraph};

use crate::element::{
    render_connects, render_declaration, render_element, render_hole, render_interp,
};
use crate::id::render_id;
use crate::inline::{render_content_blocks, render_inlines};
use crate::value::{render_args, render_value};

pub(crate) fn render_file(file: &tomet_ast::File, config: &PrinterConfig, out: &mut String) {
    for (i, decl) in file.header.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&render_declaration(decl, config));
        out.push('\n');
    }
    if !file.header.is_empty() && !file.body.is_empty() {
        out.push('\n');
    }
    render_blocks(&file.body, config, out);
}

/// Renders a run of sibling blocks -- a document's, a section's, an element's `[content]` -- with a
/// blank line between each. Without one, two paragraphs read back as one, and an element that
/// starts the next line joins the paragraph before it.
///
/// With `paragraph.separator: semicolon` a `;` opening the next block's line stands in for the
/// blank line, but only between two blocks that [`flows`]; a heading, a list or a code block
/// keeps its blank line.
pub(crate) fn render_blocks(blocks: &[Block], config: &PrinterConfig, out: &mut String) {
    let semicolon = config.format.paragraph_separator == Some(ParagraphSeparator::Semicolon);
    for (i, block) in blocks.iter().enumerate() {
        if i > 0 {
            let separator = if semicolon && flows(&blocks[i - 1], config) && flows(block, config) {
                ParagraphSeparator::Semicolon
            } else {
                ParagraphSeparator::Blank
            };
            out.push_str(&separator.after_line_end(""));
        }
        render_block(block, config, out);
    }
}

/// Is `block` one that a `;` can set off from a neighbour like a blank line does? One that
/// [`stands_like_a_paragraph`], bar the printer's own exceptions: a heading, which prints as `=[ ]`
/// sugar that `;` would not precede, and a paragraph that prints nothing.
#[deny(clippy::wildcard_enum_match_arm)]
fn flows(block: &Block, config: &PrinterConfig) -> bool {
    stands_like_a_paragraph(block)
        && match block {
            Block::Paragraph(p) => {
                !matches!(
                    p.content.first(),
                    Some(Inline::Element(el)) if classify_std_lenient(el.as_view()) == ElementKind::Heading
                ) && !render_inlines(&p.content, config).trim().is_empty()
            }
            Block::Element(el) => classify_std_lenient(el.as_view()) != ElementKind::Heading,
            // No exception of their own; a section and a list are already out.
            Block::Hole(_) | Block::Interp(_) | Block::Section(_) | Block::List(_) => true,
        }
}

#[deny(clippy::wildcard_enum_match_arm)]
pub(crate) fn render_block(block: &Block, config: &PrinterConfig, out: &mut String) {
    match block {
        // A heading element that opens a paragraph (it shares the paragraph
        // with other text, so it is `Placement::Inline`) --
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
        // A list cannot join a paragraph this way: `List` is a
        // first-class `Block` variant, with no `Inline` form at all
        // (see `tomet_ast::List`'s own doc comment), so it can never sit
        // as `p.content.first()` in the first place -- a list is always
        // block-level, by construction, the same way a `Section` is.
        Block::Paragraph(p)
            if matches!(
                p.content.first(),
                Some(Inline::Element(el)) if el.placement == Placement::Inline
                    && classify_std_lenient(el.as_view()) == ElementKind::Heading
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
        Block::Element(el) if classify_std_lenient(el.as_view()) == ElementKind::Heading => {
            render_heading_element(el, config, out)
        }
        Block::Element(el) => {
            out.push_str(&render_element(el.as_view(), config));
            out.push('\n');
        }
        Block::Hole(hole) => {
            out.push_str(&render_hole(hole, config));
            out.push('\n');
        }
        Block::Section(sec) => render_section(sec, config, out),
        Block::List(list) => render_list(list, config, out),
        Block::Interp(interp) => {
            out.push_str(&render_interp(interp));
            out.push('\n');
        }
    }
}

/// The spaces inside a heading's brackets: none unless asked for.
fn heading_spacing(config: &PrinterConfig) -> BracketSpacing {
    config
        .format
        .heading_brackets
        .unwrap_or(BracketSpacing::Tight)
}

pub(crate) fn render_section(sec: &Section, config: &PrinterConfig, out: &mut String) {
    let level = sec.level.max(1);
    out.push_str(&"=".repeat(level));
    if !sec.title.is_empty() {
        out.push_str(&heading_spacing(config).wrap(&render_inlines(&sec.title, config)));
    }
    if let Some(args) = &sec.data.args {
        out.push('(');
        out.push_str(&render_args(args, config));
        out.push(')');
    }
    if let Some(v) = sec.data.value.as_ref().map(|v| v.as_data()) {
        out.push(' ');
        out.push_str(&render_value(&v));
    }
    if sec.id.is_some() {
        out.push(' ');
    }
    out.push_str(&render_id(sec.id.as_ref()));
    out.push_str(&render_connects(&sec.connects, config));
    out.push('\n');

    render_blocks(&sec.blocks, config, out);
}

/// Called only from `render_block`'s two top-level dispatch arms (a
/// standalone `Block::Element`, or the first item of a `Block::Paragraph`
/// it joined by the default placement rule), never from the shared,
/// recursively-called `render_element` (which `render_inlines`/
/// `Group::Children` both call for genuinely nested/inline
/// elements) -- a nested `@heading(...)` must not round-trip back to
/// `#`-sugar,
/// consistent with `tomet-html`/`tomet-markdown`'s
/// equivalent gating for the same resolved decision.
pub(crate) fn render_heading_element(el: &Element, config: &PrinterConfig, out: &mut String) {
    let level = heading_level(el.as_view()).unwrap_or(1) as usize;
    let content = el.content.as_deref().unwrap_or(&[]);
    out.push_str(&"#".repeat(level));
    out.push_str(&heading_spacing(config).wrap(&render_content_blocks(content, config)));
    if let Some(v) = el.data.value.as_ref().map(|v| v.as_data()) {
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
    for item in &list.items {
        // The combine notation (`-@name(...)`): the item is a real, named
        // `Element` identical to what a standalone `@name(...)` would
        // parse to, so it round-trips through the exact same
        // `render_element` a standalone one would use -- no marker, no
        // `( )`-wrapping, just the bare `-`/`-.` prefix directly against
        // `@name`.
        if let Some(element) = item.as_element_view() {
            out.push_str(&indent_str);
            out.push_str(if ordered { "-." } else { "-" });
            out.push_str(&render_element(element, config));
            out.push('\n');
            if let Some(sub) = &item.sublist {
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
        if let Some(marker) = &item.data.args {
            head_prefix.push('(');
            head_prefix.push_str(&render_args(marker, config));
            head_prefix.push_str(") ");
        }

        let content_str = render_content_blocks(&item.content, config);
        let lines: Vec<&str> = content_str.lines().collect();
        let item_attrs = item.data.value.as_ref().map(|v| v.as_data());

        if lines.len() > 1
            && let Some(layout) = config.format.list_multiline_style_content
        {
            // The content stands two columns under the marker, and so does the closing bracket.
            let pad = " ".repeat(head_prefix.chars().count() + 2);
            out.push_str(&head_prefix);
            out.push_str(&layout.render_item(&lines, &pad));
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
        if let Some(sub) = &item.sublist {
            render_list_with_indent(sub, indent + 1, config, out);
        }
    }
}
