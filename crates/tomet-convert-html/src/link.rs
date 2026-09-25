//! Links, embeds and paths, and the `@links{}` container.

use crate::RenderCtx;
use crate::block::render_inlines;
use crate::element::render_content_or_fallback;
use crate::util::{escape_attr, escape_html, inlines_to_plain, push_data_attrs, value_to_plain};
use tomet_ast::{Element, Value};
use tomet_semantics::{
    TargetScheme, classify_std_lenient, link_target, normalized_element_args, path_target,
    target_scheme,
};

/// Strips `target`'s scheme prefix the same way `render_link_element`
/// does -- e.g. `<embed>(ref:name)` (recovered from the parser's
/// `identifier:` key split, see `normalized_element_args`) must render
/// `src="name"`, not the literal `"ref:name"`. Under the old per-scheme-key
/// design this stripping was implicit (the scheme was a separate key,
/// never part of the value string); now that both `@link` and `<embed>`
/// share one `target` key with the scheme embedded in the string, `<embed>`
/// needs the same treatment `@link` gets, not just a raw passthrough.
pub(crate) fn render_embed_element(el: &Element, out: &mut String) {
    let raw_target = link_target(el, &classify_std_lenient(el)).unwrap_or_default();
    let (scheme, src) = target_scheme(&raw_target);
    let alt = el
        .content
        .as_ref()
        .map(|a| inlines_to_plain(a))
        .unwrap_or_default();
    if scheme == TargetScheme::Unresolved {
        out.push_str(&format!(
            "<img class=\"tm-embed tm-embed-unresolved\" src=\"{}\" alt=\"{}\" aria-disabled=\"true\">\n",
            escape_attr(src),
            escape_attr(&alt)
        ));
    } else {
        out.push_str(&format!(
            "<img src=\"{}\" alt=\"{}\">\n",
            escape_attr(src),
            escape_attr(&alt)
        ));
    }
}

/// `@file(x)`/`@dir(x)` -- a path named, not navigated to.
///
/// A `<code>`, not an `<a>`, and that is the whole difference from
/// `render_link_element`. Prose saying "read `codeblock.rs`" is not
/// offering to take the reader there, and before these elements existed
/// it was written in backticks -- which is exactly what this renders back
/// to, so moving a mention onto `@file` changes what a checker can see
/// and nothing a reader can.
pub(crate) fn render_path_element(cx: &RenderCtx, el: &Element, out: &mut String, inline: bool) {
    let kind = classify_std_lenient(el);
    let path = path_target(el, &kind).unwrap_or_default();
    let class = format!("tm-{}", kind.as_str());
    let content = el.content.as_ref().filter(|c| !c.is_empty()).map(|c| {
        let mut s = String::new();
        render_inlines(cx, c, &mut s);
        s
    });

    if inline {
        // A mention. `[content]` is a label and stands in for the path,
        // the way `@link`'s does -- the whole point of writing one is
        // that the path is long and the sentence is not about its length.
        out.push_str(&format!("<code class=\"{class}\">"));
        out.push_str(&content.unwrap_or_else(|| escape_html(&path)));
        out.push_str("</code>");
        return;
    }

    // A listing row. Here `[content]` describes the path rather than
    // replacing it, and both are wanted: a directory tree whose rows say
    // only "Crates" tells the reader nothing, which is what this rendered
    // as while the path lived in a `data-value` nobody sees.
    out.push_str(&format!(
        "<div class=\"{class}\"><code>{}</code>",
        escape_html(&path)
    ));
    if let Some(content) = content {
        out.push_str(" ");
        out.push_str(&content);
    }
    out.push_str("</div>\n");
}

/// `@link(target:..)` -- the target string's own scheme prefix (see
/// `tomet_semantics::target_scheme`) decides whether this is a
/// same-document anchor reference (`id:`) or a real `href` (everything
/// else: url/file/tm/ref). The scheme prefix itself is stripped before
/// rendering -- it's addressing metadata, not part of the visible target.
pub(crate) fn render_link_element(cx: &RenderCtx, el: &Element, out: &mut String, inline: bool) {
    let normalized_args = normalized_element_args(el);
    let raw_target = link_target(el, &classify_std_lenient(el)).unwrap_or_default();
    let (scheme, target) = target_scheme(&raw_target);
    let target = target.to_string();

    match scheme {
        TargetScheme::Unresolved | TargetScheme::Ref => {
            out.push_str(&format!(
                "<a class=\"tm-ref tm-ref-unresolved\" aria-disabled=\"true\" data-ref=\"{}\"",
                escape_attr(&target)
            ));
        }
        TargetScheme::Id => {
            let href = format!("#link-{}", escape_attr(&target));
            out.push_str(&format!("<a class=\"tm-id\" href=\"{href}\""));
        }
        other => {
            let class = format!("tm-{}", other.as_str());
            let href = escape_attr(&target);
            out.push_str(&format!("<a class=\"{class}\" href=\"{href}\""));
        }
    }
    // Bare-scalar args (`@link(readme.md)`, `@link(https://example.com)`,
    // ...) are already fully captured by `target` above --
    // `push_data_attrs`'s non-map fallback would otherwise duplicate that
    // same value as a redundant `data-value` attribute. Uses the
    // *normalized* args (not raw `el.args`) so a recovered scheme key
    // (`@link(tm:foo, predicate:x)`'s `tm` entry, folded into `target` by
    // `normalized_element_args`) doesn't also leak out as a stray
    // `data-tm` attribute alongside the real `target`-derived `href`.
    if matches!(normalized_args, Some(Value::Map(_))) {
        push_data_attrs(out, normalized_args.as_ref(), &["target"]);
    }
    out.push('>');
    render_content_or_fallback(cx, el, &target, out);
    out.push_str("</a>");
    if !inline {
        out.push('\n');
    }
}

pub(crate) fn render_links_container(cx: &RenderCtx, el: &Element, out: &mut String) {
    out.push_str("<dl class=\"tm-links\">\n");
    if let Some(children) = el.value.as_ref().map(|v| v.as_children()) {
        for child in children {
            let id = child.args.as_ref().map(value_to_plain).unwrap_or_default();
            out.push_str(&format!(
                "<dt id=\"link-{}\">{}</dt>\n",
                escape_attr(&id),
                escape_html(&id)
            ));
            out.push_str("<dd>");
            if let Some(content) = &child.content {
                render_inlines(cx, content, out);
            }
            out.push_str("</dd>\n");
        }
    }
    out.push_str("</dl>\n");
}
