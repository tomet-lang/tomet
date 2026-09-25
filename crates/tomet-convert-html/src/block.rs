//! Block-level rendering -- sections, headings, lists and inline runs. This is
//! the recursive core; the other renderers are called from it and call back in.

use crate::element::render_element;
use crate::headings::HeadingState;
use crate::util::{
    escape_attr, escape_html, inlines_to_plain, push_named_attrs, split_attrs, value_to_plain,
};
use crate::{HeadingInfo, RenderCtx};
use tomet_ast::{Block, Element, Inline, Section, Span, Value};
use tomet_semantics::{ElementKind, classify_std_lenient, heading_level, list_items, list_ordered};

/// Dispatches one top-level document block, and -- when
/// `RenderOptions::emit_source_spans` is set -- tags whatever it emits as
/// one editable "leaf" with `data-tmt-start`/`data-tmt-end`.
///
/// What counts as a leaf here: a paragraph or heading is tagged as itself;
/// a list is not tagged at all, only each of its items (`render_list`
/// tags those individually, at item granularity, including any nested
/// sub-list inside an item -- editing an item edits its whole subtree's
/// source text); everything else (table, quote, hr, raw block, generic
/// element, ...) is tagged as a single opaque leaf covering its entire
/// rendered output, since none of those currently expose finer-grained
/// spans (a table's cells, notably, do not -- see `RenderOptions::
/// emit_source_spans`'s doc comment).
pub(crate) fn render_block(
    cx: &RenderCtx,
    block: &Block,
    out: &mut String,
    state: &mut HeadingState,
) {
    match block {
        Block::Paragraph(p) => {
            // Elements with no visible output (`@meta`, ...) placed on
            // adjacent lines with no blank line between them lazily
            // continue into one paragraph together with the inter-element
            // whitespace text (see `document.rs::parse_paragraph`) -- if
            // every one of them is invisible, skip the wrapper entirely
            // rather than emitting a stray whitespace-only `<p>`.
            let mut inner = String::new();
            render_inlines(cx, &p.content, &mut inner);
            if !inner.trim().is_empty() {
                out.push_str("<p");
                push_span_attrs(cx, out, p.span);
                out.push('>');
                out.push_str(&inner);
                out.push_str("</p>\n");
            }
        }
        Block::Element(el) if list_ordered(el).is_some() => render_list(cx, el, out),
        Block::Element(el) if classify_std_lenient(el) == ElementKind::Heading => {
            render_heading_element(cx, el, out, state)
        }
        Block::Element(el) => {
            // The catch-all: dispatches into whichever of `render_element`'s
            // many branches (table/quote/hr/raw/embed/link/ruby/generic)
            // this element's kind matches. Rather than threading a "tag
            // your own opening tag" parameter through every one of those,
            // splice the attributes into the first tag the call actually
            // produced -- safe because `escape_attr` never lets a literal
            // `>` reach an attribute value, so the first `>` after this
            // point is that tag's own close.
            let start = out.len();
            render_element(cx, el, out, false);
            if cx.options.emit_source_spans && out.len() > start {
                splice_span_attrs(out, start, el.span);
            }
        }
        Block::Section(sec) => render_section(cx, sec, out, state),
    }
}

fn render_section(cx: &RenderCtx, sec: &Section, out: &mut String, state: &mut HeadingState) {
    let level = (sec.level as u8).clamp(1, 6);
    let value_data = match &sec.value {
        Some(v) => v.as_data(),
        _ => None,
    };
    let (mut id, class, data) = split_attrs(value_data.as_ref());
    let text = inlines_to_plain(&sec.title);
    if id.is_none() && cx.options.auto_slug_headings {
        let slug = state.slugs.slug_for(&text);
        if !slug.is_empty() {
            id = Some(slug);
        }
    }
    if cx.options.wrap_sections {
        out.push_str(&format!("<section class=\"tmt-section level-{level}\">\n"));
    }
    if !sec.title.is_empty() {
        out.push_str(&format!("<h{level}"));
        push_named_attrs(out, &id, &class, &data);
        push_span_attrs(cx, out, sec.span);
        out.push('>');
        let mut number = None;
        if cx.options.number_headings {
            let label = state.counters.advance(level);
            out.push_str(&format!("<span class=\"tm-heading-number\">{label}</span>"));
            number = Some(label);
        }
        render_inlines(cx, &sec.title, out);
        out.push_str(&format!("</h{level}>\n"));

        state.outline.push(HeadingInfo {
            level,
            id,
            text,
            number,
        });
    }

    for child in &sec.blocks {
        render_block(cx, child, out, state);
    }

    if cx.options.wrap_sections {
        out.push_str("</section>\n");
    }
}

/// Appends ` data-tmt-start="…" data-tmt-end="…"` (the byte offsets from
/// `span`) when `RenderOptions::emit_source_spans` is set, otherwise
/// nothing. Call this after an opening tag's other attributes and before
/// its closing `>`.
pub(crate) fn push_span_attrs(cx: &RenderCtx, out: &mut String, span: Span) {
    if cx.options.emit_source_spans {
        out.push_str(&format!(
            " data-tmt-start=\"{}\" data-tmt-end=\"{}\"",
            span.start.offset, span.end.offset
        ));
    }
}

/// Like `push_span_attrs`, but for output already written: inserts the
/// attributes right before the first `>` found at or after byte `start`
/// of `out`. Only sound when `out[start..]` begins with a fresh opening
/// tag with no attribute value containing a literal `>` -- see the call
/// site in `render_block` for why that holds here.
fn splice_span_attrs(out: &mut String, start: usize, span: Span) {
    if let Some(rel_gt) = out[start..].find('>') {
        let gt = start + rel_gt;
        out.insert_str(
            gt,
            &format!(
                " data-tmt-start=\"{}\" data-tmt-end=\"{}\"",
                span.start.offset, span.end.offset
            ),
        );
    }
}

/// Called only from `render_block`'s top-level dispatch, never from
/// `render_element`'s recursive `kind.as_str()` match -- a nested/inline
/// `@heading(...)` (e.g. inside a blockquote's content, or `${...}`-free
/// prose) falls through to `render_generic_element` instead, since
/// CommonMark/Tomet headings are both block-position-only by grammar.
fn render_heading_element(
    cx: &RenderCtx,
    el: &Element,
    out: &mut String,
    state: &mut HeadingState,
) {
    let level = heading_level(el).unwrap_or(1);
    let content = el.content.as_deref().unwrap_or(&[]);
    let value_data = match &el.value {
        Some(v) => v.as_data(),
        _ => None,
    };
    let (mut id, class, data) = split_attrs(value_data.as_ref());
    let text = inlines_to_plain(content);
    if id.is_none() && cx.options.auto_slug_headings {
        let slug = state.slugs.slug_for(&text);
        if !slug.is_empty() {
            id = Some(slug);
        }
    }
    out.push_str(&format!("<h{level}"));
    push_named_attrs(out, &id, &class, &data);
    push_span_attrs(cx, out, el.span);
    out.push('>');
    let mut number = None;
    if cx.options.number_headings {
        let label = state.counters.advance(level);
        // No literal space after `</span>` -- spacing is `.tmt-heading-number`'s
        // `margin-right` in `DEFAULT_STYLE`, not baked into the content, so
        // e.g. copy-pasting the heading text doesn't pick up a stray space.
        out.push_str(&format!("<span class=\"tm-heading-number\">{label}</span>"));
        number = Some(label);
    }
    render_inlines(cx, content, out);
    out.push_str(&format!("</h{level}>\n"));

    state.outline.push(HeadingInfo {
        level,
        id,
        text,
        number,
    });
}

fn render_list(cx: &RenderCtx, el: &Element, out: &mut String) {
    let tag = if list_ordered(el) == Some(true) {
        "ol"
    } else {
        "ul"
    };
    out.push_str(&format!("<{tag}>\n"));
    for item in list_items(el) {
        let attrs = match &item.value {
            Some(v) => v.as_data(),
            _ => None,
        };
        let (id, class, data) = split_attrs(attrs.as_ref());
        out.push_str("<li");
        push_named_attrs(out, &id, &class, &data);
        push_span_attrs(cx, out, item.span);
        out.push('>');
        if let Some(marker) = &item.args {
            out.push_str("<span class=\"tm-list-marker\"");
            match marker {
                Value::Map(map) => {
                    for (k, v) in map {
                        out.push_str(&format!(
                            " data-{}=\"{}\"",
                            escape_attr(k),
                            escape_attr(&value_to_plain(v))
                        ));
                    }
                }
                Value::Element(_) => {}
                other => out.push_str(&format!(
                    " data-marker=\"{}\"",
                    escape_attr(&value_to_plain(other))
                )),
            }
            out.push('>');
            match marker {
                Value::Element(el) => {
                    render_element(cx, el, out, true);
                }
                other => {
                    let text = value_to_plain(other);
                    if !text.is_empty() {
                        out.push_str(&format!("[{}]", escape_html(&text)));
                    }
                }
            }
            out.push_str("</span> ");
        }
        render_inlines(cx, item.content.as_deref().unwrap_or(&[]), out);
        if let Some(children) = &item.children {
            for child in children {
                if let Block::Element(sub) = child
                    && list_ordered(sub).is_some()
                {
                    render_list(cx, sub, out);
                }
            }
        }
        out.push_str("</li>\n");
    }
    out.push_str(&format!("</{tag}>\n"));
}

pub(crate) fn render_inlines(cx: &RenderCtx, inlines: &[Inline], out: &mut String) {
    for (idx, inline) in inlines.iter().enumerate() {
        match inline {
            Inline::Text(t) => out.push_str(&escape_html(&t.value)),
            Inline::Raw(t) => out.push_str(&escape_html(&t.value)),
            Inline::SoftBreak(_) => {
                let before = out.chars().last();
                let after = inlines.get(idx + 1).and_then(Inline::first_char);
                out.push_str(tomet_ast::softbreak_join(before, after));
            }
            Inline::LineBreak(_) => out.push_str("<br>"),
            Inline::Element(el) => render_element(cx, el, out, true),
        }
    }
}
