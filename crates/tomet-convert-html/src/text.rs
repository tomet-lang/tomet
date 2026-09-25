//! Small text-level elements: thematic breaks, wrapped inline markup, ruby,
//! raw text and quotes.

use crate::RenderCtx;
use crate::block::render_inlines;
use crate::util::{
    as_map, escape_attr, escape_html, inlines_to_plain, map_get, push_named_attrs, split_attrs,
    value_to_plain,
};
use tomet_ast::{Element, Value};
use tomet_semantics::normalized_element_args;

/// A bare `---` break is a plain `<hr>`; a titled one (`---[ Title ]---`,
/// `document.rs::parse_titled_thematic_break`'s `content`) wraps two `<hr>`s
/// around the title, visually reproducing the source's symmetric
/// dashes-title-dashes shape (styled via `.tmt-hr-titled` in `DEFAULT_STYLE`).
pub(crate) fn render_hr_element(cx: &RenderCtx, el: &Element, out: &mut String) {
    match &el.content {
        Some(title) if !title.is_empty() => {
            out.push_str("<div class=\"tm-hr-titled\"><hr><span>");
            render_inlines(cx, title, out);
            out.push_str("</span><hr></div>\n");
        }
        _ => out.push_str("<hr>\n"),
    }
}

/// `em`/`strong`/`mark` all wrap their `content` in a same-named HTML tag --
/// the element's own name doubles as the HTML tag name for these three.
pub(crate) fn render_wrapped_inline(cx: &RenderCtx, el: &Element, tag: &str, out: &mut String) {
    out.push_str(&format!("<{tag}>"));
    if let Some(content) = &el.content {
        render_inlines(cx, content, out);
    }
    out.push_str(&format!("</{tag}>"));
}

/// `@ruby[漢字](rt:"かんじ")` -- real `<ruby>`/`<rt>` markup, not the
/// generic `<span data-rt=...>` `render_generic_element` would fall back to.
/// `rt` is meaningful content (the reading), not decoration, so it has to
/// reach the output as text a reader/screen-reader sees.
pub(crate) fn render_ruby_element(cx: &RenderCtx, el: &Element, out: &mut String) {
    let args = normalized_element_args(el);
    let rt = args
        .as_ref()
        .and_then(as_map)
        .and_then(|m| map_get(m, "rt"))
        .and_then(|v| match v {
            Value::String(s) => Some(s.as_str()),
            _ => None,
        })
        .unwrap_or("");

    out.push_str("<ruby>");
    if let Some(content) = &el.content {
        render_inlines(cx, content, out);
    }
    out.push_str("<rt>");
    out.push_str(&escape_html(rt));
    out.push_str("</rt></ruby>");
}

/// `@raw(lang:xxx)[code]` -- raw verbatim text. Standing alone as a block,
/// it renders `<pre><code>...</code></pre>`. Inside running text (`inline == true`),
/// it renders `<code class="...">...</code>`.
pub(crate) fn render_raw_element(el: &Element, out: &mut String, inline: bool) {
    let args = normalized_element_args(el);
    let lang = args
        .as_ref()
        .and_then(as_map)
        .and_then(|m| map_get(m, "lang"))
        .map(value_to_plain)
        .unwrap_or_default();
    let code = el
        .content
        .as_ref()
        .map(|a| inlines_to_plain(a))
        .unwrap_or_default();
    let attrs = match &el.value {
        Some(v) => v.as_data(),
        _ => None,
    };
    let (id, class, data) = split_attrs(attrs.as_ref());

    if inline {
        out.push_str("<code");
        push_named_attrs(out, &id, &class, &data);
        if !lang.is_empty() {
            out.push_str(&format!(" class=\"language-{}\"", escape_attr(&lang)));
        }
        out.push('>');
        out.push_str(&escape_html(&code));
        out.push_str("</code>");
        return;
    }

    out.push_str("<pre");
    push_named_attrs(out, &id, &class, &data);
    out.push_str("><code");
    if !lang.is_empty() {
        out.push_str(&format!(" class=\"language-{}\"", escape_attr(&lang)));
    }
    out.push('>');
    out.push_str(&escape_html(&code));
    out.push_str("</code></pre>\n");
}

/// `<blockquote>` standing alone, `<q>` inside a sentence.
///
/// HTML needs two element names for this because it has two elements.
/// Tomet has one, `@quote`, and decides which by position -- the same
/// rule everything else here is placed by. Only single-block quotes
/// round-trip cleanly; multi-block quotes are already flattened into one
/// inline run on import.
pub(crate) fn render_quote_element(cx: &RenderCtx, el: &Element, out: &mut String, inline: bool) {
    let tag = if inline { "q" } else { "blockquote" };
    out.push_str(&format!("<{tag}>"));
    if let Some(content) = &el.content {
        render_inlines(cx, content, out);
    }
    out.push_str(&format!("</{tag}>"));
    if !inline {
        out.push('\n');
    }
}
