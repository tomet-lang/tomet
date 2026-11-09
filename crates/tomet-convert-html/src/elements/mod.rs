//! =[ elements ]
//!
//! Element dispatch and specialized HTML renderers for concrete built-in elements.

pub(crate) mod footnote;
pub(crate) mod heading;
pub(crate) mod link;
pub(crate) mod table;
pub(crate) mod text;

pub(crate) use footnote::render_footnotes;
pub(crate) use heading::HeadingState;
pub(crate) use link::{render_embed_element, render_link_element, render_path_element};
pub(crate) use table::render_table_element;
pub(crate) use text::{
    render_hr_element, render_quote_element, render_raw_element, render_ruby_element,
    render_wrapped_inline,
};

use crate::block::push_span_attrs;
use crate::util::{
    escape_attr, escape_html, id_attr, push_data_attrs, push_value_data_attrs, value_to_plain,
};
use crate::{CustomElementCtx, RenderCtx};
use tomet_ast::{Bare, BodyView, ElementView, Entry, Group, Interpolation};
use tomet_semantics::{
    classify_std_lenient, comment_text, extract_tags, is_directive, normalized_element_args_in,
};

pub(crate) fn render_element(cx: &RenderCtx, el: ElementView<'_>, out: &mut String, inline: bool) {
    let kind = classify_std_lenient(el);
    match kind.as_str() {
        // Directives -- see `tomet_semantics::is_directive`.
        _ if is_directive(&kind) => {}
        "ftn" => {
            if (inline || el.placement == tomet_ast::Placement::Inline)
                && let Some((idx, backlink)) = cx.footnotes.get_ref(&el.span)
            {
                out.push_str(&format!(
                    "<sup><a href=\"#fn-{}\" id=\"{}\" class=\"footnote-ref\">[{}]</a></sup>",
                    idx, backlink, idx
                ));
            }
        }
        "link" => render_link_element(cx, el, out, inline),
        "file" | "dir" => render_path_element(cx, el, out, inline),
        "embed" => render_embed_element(el, out),
        "hr" => render_hr_element(cx, el, out),
        "em" | "strong" | "mark" => render_wrapped_inline(cx, el, kind.as_str(), out),
        // `<del>` rather than a `strikeout` tag: the element's name is
        // Tomet's, and `<del>` is what GFM's `~~` renders as everywhere
        // it is read.
        "strikeout" => render_wrapped_inline(cx, el, "del", out),
        "ruby" => render_ruby_element(cx, el, out),
        "raw" => render_raw_element(el, out, inline),
        "quote" => render_quote_element(cx, el, out, inline),
        "table" => render_table_element(cx, el, out),
        "tag" => render_tag_element(cx, el, out, inline),
        "task" => render_task_element(cx, el, out, inline),
        "comment" => render_comment_element(el, out, inline),
        "conflict" => render_conflict_element(cx, el, out, inline),
        _ => render_custom_or_generic_element(cx, el, kind.as_str(), out, inline),
    }
}

/// `@comment` as an HTML comment, on a line of its own when it stands as a block.
fn render_comment_element(el: ElementView<'_>, out: &mut String, inline: bool) {
    out.push_str(&crate::comment::render(&comment_text(el)));
    if !inline {
        out.push('\n');
    }
}

fn render_task_element(cx: &RenderCtx, el: ElementView<'_>, out: &mut String, inline: bool) {
    let status = normalized_element_args_in(el, cx.bindings)
        .as_ref()
        .and_then(crate::util::as_map)
        .and_then(|m| crate::util::map_get(m, "status"))
        .map(value_to_plain)
        .unwrap_or_else(|| " ".to_string());
    let checked = matches!(status.as_str(), "x" | "X" | "done");
    let tag = if inline { "span" } else { "div" };
    out.push_str(&format!(
        "<{tag} class=\"tm-element tm-task\" data-status=\"{}\"",
        escape_attr(&status)
    ));
    if let Some(id) = id_attr(el.id) {
        out.push_str(&format!(" id=\"{}\"", escape_attr(&id)));
    }
    push_span_attrs(cx, out, el.span);
    out.push('>');
    let checked_attr = if checked { " checked" } else { "" };
    out.push_str(&format!(
        "<input type=\"checkbox\" class=\"tm-task-checkbox\" disabled{checked_attr}> "
    ));
    if let Some(content) = el.content {
        crate::block::render_content_blocks(cx, content, out);
    }
    out.push_str(&format!("</{tag}>"));
    if !inline {
        out.push('\n');
    }
}

/// `@conflict(a: ..., b: ...)` -- unlike the generic fallback's
/// `data-*` attributes (lossy for block content, which is exactly what
/// `a`/`b` hold), both sides are rendered as real nested content so
/// nothing is silently dropped. Marked, not resolved: picking one side
/// and discarding the other here would make `tomet check`'s own
/// `Diagnostic::Conflict` warning pointless the moment someone exports.
fn render_conflict_element(cx: &RenderCtx, el: ElementView<'_>, out: &mut String, inline: bool) {
    let (a, b) = conflict_sides(cx, el);
    let tag = if inline { "span" } else { "div" };
    out.push_str(&format!("<{tag} class=\"tm-element tm-conflict\""));
    if let Some(id) = id_attr(el.id) {
        out.push_str(&format!(" id=\"{}\"", escape_attr(&id)));
    }
    out.push('>');
    out.push_str("<div class=\"tm-conflict-a\">");
    crate::block::render_content_blocks(cx, &a, out);
    out.push_str("</div>");
    out.push_str("<div class=\"tm-conflict-b\">");
    crate::block::render_content_blocks(cx, &b, out);
    out.push_str("</div>");
    out.push_str(&format!("</{tag}>"));
    if !inline {
        out.push('\n');
    }
}

/// Reads `a`/`b` out of `el`'s `(args)` -- block content
/// (`tomet_ast::Value::Blocks`), not a scalar, so it cannot go through
/// `push_data_attrs`/the generic flatten path at all.
fn conflict_sides(
    cx: &RenderCtx,
    el: ElementView<'_>,
) -> (Vec<tomet_ast::Block>, Vec<tomet_ast::Block>) {
    let args = normalized_element_args_in(el, cx.bindings);
    let map = args.as_ref().and_then(crate::util::as_map);
    let side = |key: &str| -> Vec<tomet_ast::Block> {
        match map.and_then(|m| crate::util::map_get(m, key)) {
            Some(tomet_ast::Value::Blocks(blocks)) => blocks.clone(),
            _ => Vec::new(),
        }
    };
    (side("a"), side("b"))
}

fn render_tag_element(cx: &RenderCtx, el: ElementView<'_>, out: &mut String, inline: bool) {
    let tags = extract_tags(el);
    if tags.is_empty() {
        return;
    }

    let tag_spans: Vec<String> = tags
        .iter()
        .map(|t| {
            let label = if t.starts_with('#') {
                escape_html(t)
            } else {
                format!("#{}", escape_html(t))
            };
            format!("<span class=\"tmt-tag\">{}</span>", label)
        })
        .collect();

    let tag = if inline { "span" } else { "div" };
    out.push_str(&format!("<{} class=\"tmt-tag-list\"", tag));
    push_span_attrs(cx, out, el.span);
    out.push('>');
    out.push_str(&tag_spans.join(" "));
    out.push_str(&format!("</{}>", tag));
    if !inline {
        out.push('\n');
    }
}

/// The fallback for a kind [`render_element`] has no dedicated handling
/// for: try [`RenderOptions::custom_element`] first, since a namespaced
/// embedded-vocabulary element like `doc.icon` reaches here too (Tomet
/// resolves it but never draws it), then fall back to
/// [`render_generic_element`] for anything the hook doesn't claim.
fn render_custom_or_generic_element(
    cx: &RenderCtx,
    el: ElementView<'_>,
    kind: &str,
    out: &mut String,
    inline: bool,
) {
    if let Some(renderer) = &cx.options.custom_element {
        let bindings = cx.bindings;
        let ctx = CustomElementCtx::new(kind, el.content, inline, || {
            normalized_element_args_in(el, bindings)
        });
        if let Some(html) = (renderer.0)(ctx) {
            out.push_str(&html);
            return;
        }
    }
    render_generic_element(cx, el, kind, out, inline);
}

pub(crate) fn render_content_or_fallback(
    cx: &RenderCtx,
    el: ElementView<'_>,
    fallback: &str,
    out: &mut String,
) {
    match el.content {
        Some(content) if !content.is_empty() => {
            crate::block::render_content_blocks(cx, content, out)
        }
        _ => out.push_str(&escape_html(fallback)),
    }
}

fn render_generic_element(
    cx: &RenderCtx,
    el: ElementView<'_>,
    kind: &str,
    out: &mut String,
    inline: bool,
) {
    render_generic(cx, kind, el.body(), out, inline);
}

/// What a bare `(marker)[content]` entry of a `{...}` group is drawn as: the
/// generic element of the kind `bare`, since the group it sits in is what says
/// what it is.
fn render_bare(cx: &RenderCtx, bare: &Bare, out: &mut String) {
    render_generic(cx, "bare", bare.as_body_view(), out, false);
}

/// The generic `tm-element` box of an element, or of anything that takes
/// the same groups.
fn render_generic(cx: &RenderCtx, kind: &str, body: BodyView<'_>, out: &mut String, inline: bool) {
    let tag = if inline { "span" } else { "div" };
    out.push_str(&format!("<{tag} class=\"tm-element tm-{kind}\""));
    if let Some(id) = id_attr(body.id) {
        out.push_str(&format!(" id=\"{}\"", escape_attr(&id)));
    }
    push_data_attrs(out, body.data.args.as_ref(), &[]);
    push_value_data_attrs(out, body.data);
    out.push('>');
    if let Some(content) = body.content {
        crate::block::render_content_blocks(cx, content, out);
    }
    if let Some(value) = &body.data.value {
        render_element_value(cx, value, out);
    }
    out.push_str(&format!("</{tag}>"));
    if !inline {
        out.push('\n');
    }
}

#[deny(clippy::wildcard_enum_match_arm)]
fn render_element_value(cx: &RenderCtx, value: &Group, out: &mut String) {
    match value {
        // Only the elements. A group's *pairs* are data and left the body
        // for `data-*` beside `(args)`; see `push_value_data_attrs`.
        Group(entries) => {
            let mut inner = String::new();
            for entry in entries {
                match entry {
                    Entry::Element(child) => {
                        render_element(cx, child.as_view(), &mut inner, false);
                    }
                    Entry::Bare(child) => render_bare(cx, child, &mut inner),
                    Entry::Pair(..) | Entry::Hole(_) => {}
                }
            }
            if !inner.is_empty() {
                out.push_str("<div class=\"tm-children\">\n");
                out.push_str(&inner);
                out.push_str("</div>\n");
            }
        }
    }
}

/// A `${...}` that reached here unresolved, written back as it was.
///
/// Reaching here at all means the document was not prepared -- a
/// caller that went straight to this crate rather than through
/// `tomet_load::Vault`. Rendering nothing would hide it, and
/// evaluating it here is what this crate stopped doing: what a
/// `${...}` means is settled by `tomet-transform`'s
/// `resolve_expressions` before any converter sees the tree.
///
/// This used to evaluate with `evaluate_with_config` -- no
/// `EvaluationContext` and no vault config, so `${self::path}`
/// resolved in CommonMark and not here, and `default.config.tmt`'s
/// macros expanded in neither. No wrapper element is emitted now
/// that the text is plain: `<span class="tm-interp">` carried no
/// style and marked a distinction that no longer survives to here.
pub(crate) fn render_interp(interp: &Interpolation, out: &mut String) {
    out.push_str(&escape_html(&format!("${{{}}}", interp.expr)));
}
