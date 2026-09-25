//! Element dispatch: which renderer an element gets, and the generic
//! `<div>`/`<span>` rendering of the ones with no fixed meaning.

use crate::block::{push_span_attrs, render_inlines};
use crate::link::{
    render_embed_element, render_link_element, render_links_container, render_path_element,
};
use crate::table::render_table_element;
use crate::text::{
    render_hr_element, render_quote_element, render_raw_element, render_ruby_element,
    render_wrapped_inline,
};
use crate::util::{escape_html, push_data_attrs, push_value_data_attrs};
use crate::{CustomElementCtx, RenderCtx};
use tomet_ast::{Element, ElementValue};
use tomet_semantics::{
    classify_std_lenient, extract_tags, is_directive, normalized_element_args_in,
};

pub(crate) fn render_element(cx: &RenderCtx, el: &Element, out: &mut String, inline: bool) {
    let kind = classify_std_lenient(el);
    match kind.as_str() {
        // Directives -- see `tomet_semantics::is_directive`.
        _ if is_directive(&kind) => {}
        "interp" => {
            if let Some(value) = &el.value {
                render_element_value(cx, value, out);
            }
        }
        "footnote" => {
            if inline || el.placement == tomet_ast::Placement::Inline {
                if let Some((idx, backlink)) = cx.footnotes.get_ref(&el.span) {
                    out.push_str(&format!(
                        "<sup><a href=\"#fn-{}\" id=\"{}\" class=\"footnote-ref\">[{}]</a></sup>",
                        idx, backlink, idx
                    ));
                }
            }
        }
        "caret" => {
            if let Some((idx, backlink)) = cx.footnotes.get_ref(&el.span) {
                out.push_str(&format!(
                    "<sup><a href=\"#fn-{}\" id=\"{}\" class=\"footnote-ref\">[{}]</a></sup>",
                    idx, backlink, idx
                ));
            }
        }
        "links" => render_links_container(cx, el, out),
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
        _ => render_custom_or_generic_element(cx, el, kind.as_str(), out, inline),
    }
}

fn render_tag_element(cx: &RenderCtx, el: &Element, out: &mut String, inline: bool) {
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
    el: &Element,
    kind: &str,
    out: &mut String,
    inline: bool,
) {
    if let Some(renderer) = &cx.options.custom_element {
        let bindings = cx.bindings;
        let ctx = CustomElementCtx::new(kind, el.content.as_deref(), inline, || {
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
    el: &Element,
    fallback: &str,
    out: &mut String,
) {
    match &el.content {
        Some(content) if !content.is_empty() => render_inlines(cx, content, out),
        _ => out.push_str(&escape_html(fallback)),
    }
}

fn render_generic_element(
    cx: &RenderCtx,
    el: &Element,
    kind: &str,
    out: &mut String,
    inline: bool,
) {
    let tag = if inline { "span" } else { "div" };
    out.push_str(&format!("<{tag} class=\"tm-element tm-{kind}\""));
    push_data_attrs(out, el.args.as_ref(), &[]);
    push_value_data_attrs(out, el);
    out.push('>');
    if let Some(content) = &el.content {
        render_inlines(cx, content, out);
    }
    if let Some(value) = &el.value {
        render_element_value(cx, value, out);
    }
    out.push_str(&format!("</{tag}>"));
    if !inline {
        out.push('\n');
    }
}

fn render_element_value(cx: &RenderCtx, value: &ElementValue, out: &mut String) {
    match value {
        // Only the elements. A group's *pairs* are data and left the body
        // for `data-*` beside `(args)`; see `push_value_data_attrs`.
        ElementValue::Group(_) => {
            let children = value.as_children();
            if !children.is_empty() {
                out.push_str("<div class=\"tm-children\">\n");
                for child in children {
                    render_element(cx, child, out, false);
                }
                out.push_str("</div>\n");
            }
        }
        // A `+++` fence body is opaque text.
        ElementValue::Raw(body) => {
            out.push_str("<pre class=\"tm-raw\">");
            out.push_str(&escape_html(body));
            out.push_str("</pre>");
        }
        // A `${...}` that reached here unresolved, written back as it was.
        //
        // Reaching here at all means the document was not prepared -- a
        // caller that went straight to this crate rather than through
        // `tomet_load::Vault`. Rendering nothing would hide it, and
        // evaluating it here is what this crate stopped doing: what a
        // `${...}` means is settled by `tomet-transform`'s
        // `resolve_interpolations` before any converter sees the tree.
        //
        // This used to evaluate with `evaluate_with_config` -- no
        // `EvaluationContext` and no vault config, so `${self.path}`
        // resolved in CommonMark and not here, and `default.config.tmt`'s
        // macros expanded in neither. No wrapper element is emitted now
        // that the text is plain: `<span class="tm-interp">` carried no
        // style and marked a distinction that no longer survives to here.
        ElementValue::Interp(expr) => {
            out.push_str(&escape_html(&format!("${{{expr}}}")));
        }
    }
}
