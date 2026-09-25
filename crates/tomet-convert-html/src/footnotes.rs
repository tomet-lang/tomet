//! The footnotes section written after the body.

use crate::RenderCtx;
use crate::block::{render_block, render_inlines};
use crate::headings::HeadingState;

pub(crate) fn render_footnotes(cx: &RenderCtx, out: &mut String) {
    if cx.footnotes.items.is_empty() {
        return;
    }
    out.push_str("<section role=\"doc-endnotes\" class=\"footnotes\">\n<hr>\n<ol>\n");
    for item in &cx.footnotes.items {
        out.push_str(&format!("<li id=\"fn-{}\">\n", item.index));
        let mut content_html = String::new();
        if let Some(def_el) = &item.definition {
            if let Some(content) = &def_el.content {
                render_inlines(cx, content, &mut content_html);
            }
            if let Some(children) = &def_el.children {
                for child in children {
                    let mut dummy_state = HeadingState::default();
                    render_block(cx, child, &mut content_html, &mut dummy_state);
                }
            }
        }

        let backlink_html = if item.backlinks.len() == 1 {
            format!(
                " <a href=\"#{}\" role=\"doc-backlink\" class=\"footnote-backref\">↩</a>",
                item.backlinks[0]
            )
        } else {
            let links: Vec<String> = item
                .backlinks
                .iter()
                .enumerate()
                .map(|(i, id)| {
                    format!(
                        "<a href=\"#{}\" role=\"doc-backlink\" class=\"footnote-backref\">^{}</a>",
                        id,
                        i + 1
                    )
                })
                .collect();
            format!(
                " <span class=\"footnote-backrefs\">{}</span>",
                links.join(" ")
            )
        };

        if content_html.trim().is_empty() {
            out.push_str(&format!("<p>{}</p>\n", backlink_html.trim_start()));
        } else if content_html.ends_with("</p>\n") {
            let pos = content_html.rfind("</p>\n").unwrap();
            content_html.insert_str(pos, &backlink_html);
            out.push_str(&content_html);
        } else {
            out.push_str(&format!("<p>{}{}</p>\n", content_html, backlink_html));
        }
        out.push_str("</li>\n");
    }
    out.push_str("</ol>\n</section>\n");
}
