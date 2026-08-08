//! Renders a parsed TypedMark [`Document`] to HTML.
//!
//! This is a generic, data-driven mapping (not a full semantic engine):
//! most `<T>`/`@name` elements become a `<div>`/`<span>` carrying their
//! `input` map as `data-*` attributes, with `area` as inner content. A
//! handful of element kinds get special handling because the spec (see
//! `docs/tmt/typedmark.tm`) gives them fixed meaning: `@(url:..)` /
//! `@(file:..)` become links, `@(ref:..)` becomes an anchor reference,
//! `@meta` carries no visible content, and `@links{}` containers render
//! their bare children as a definition list of anchors.

use typedmark_ast::{
    Block, Document, Element, ElementValue, Heading, Inline, ListItem, Sigil, Value,
};

const DEFAULT_STYLE: &str = "\
body { font-family: sans-serif; line-height: 1.6; max-width: 48rem; margin: 2rem auto; padding: 0 1rem; }
.tm-element { border-left: 2px solid #ccc; padding-left: 0.5rem; }
.tm-caution { border-left-color: #d9822b; background: #fff8ee; padding: 0.5rem; }
.tm-links dt { font-weight: bold; }
.tm-value { color: #666; font-family: monospace; }
";

/// Render a full standalone HTML document.
pub fn render_page(doc: &Document, title: &str) -> String {
    format!(
        "<!DOCTYPE html>\n<html lang=\"ja\">\n<head>\n<meta charset=\"utf-8\">\n<title>{title}</title>\n<style>{style}</style>\n</head>\n<body>\n{body}</body>\n</html>\n",
        title = escape_html(title),
        style = DEFAULT_STYLE,
        body = render_body(doc),
    )
}

/// Render just the body content, without the surrounding `<html>` shell.
pub fn render_body(doc: &Document) -> String {
    let mut out = String::new();
    for block in &doc.blocks {
        render_block(block, &mut out);
    }
    out
}

fn render_block(block: &Block, out: &mut String) {
    match block {
        Block::Heading(h) => render_heading(h, out),
        Block::Paragraph(inlines) => {
            out.push_str("<p>");
            render_inlines(inlines, out);
            out.push_str("</p>\n");
        }
        Block::List(items) => render_list(items, out),
        Block::Element(el) => render_element(el, out, false),
    }
}

fn render_heading(h: &Heading, out: &mut String) {
    let level = h.level.clamp(1, 6);
    let (id, class, data) = split_attrs(h.attrs.as_ref());
    out.push_str(&format!("<h{level}"));
    push_named_attrs(out, &id, &class, &data);
    out.push('>');
    render_inlines(&h.content, out);
    out.push_str(&format!("</h{level}>\n"));
}

fn render_list(items: &[ListItem], out: &mut String) {
    out.push_str("<ul>\n");
    for item in items {
        out.push_str("<li>");
        render_inlines(&item.content, out);
        out.push_str("</li>\n");
    }
    out.push_str("</ul>\n");
}

fn render_inlines(inlines: &[Inline], out: &mut String) {
    for inline in inlines {
        match inline {
            Inline::Text(t) => out.push_str(&escape_html(t)),
            Inline::Element(el) => render_element(el, out, true),
        }
    }
}

const INFERRED_AT_KEYS: [&str; 4] = ["url", "file", "ref", "meta"];

fn element_kind(el: &Element) -> String {
    match &el.sigil {
        Sigil::Type(name) => name.clone(),
        Sigil::At(Some(name)) => name.clone(),
        Sigil::At(None) => infer_at_kind(el.input.as_ref()).unwrap_or_else(|| "at".to_string()),
        Sigil::Bare => "bare".to_string(),
    }
}

fn infer_at_kind(input: Option<&Value>) -> Option<String> {
    let map = as_map(input?)?;
    INFERRED_AT_KEYS
        .iter()
        .find(|k| map_get(map, k).is_some())
        .map(|k| k.to_string())
}

fn render_element(el: &Element, out: &mut String, inline: bool) {
    let kind = element_kind(el);
    match kind.as_str() {
        "meta" => {}
        "links" => render_links_container(el, out),
        "url" => render_href_element(el, "url", out, inline),
        "file" => render_href_element(el, "file", out, inline),
        "ref" => render_ref_element(el, out, inline),
        _ => render_generic_element(el, &kind, out, inline),
    }
}

fn render_href_element(el: &Element, key: &str, out: &mut String, inline: bool) {
    let href = el
        .input
        .as_ref()
        .and_then(as_map)
        .and_then(|m| map_get(m, key))
        .map(value_to_plain)
        .unwrap_or_default();
    out.push_str(&format!(
        "<a class=\"tm-{key}\" href=\"{}\"",
        escape_attr(&href)
    ));
    push_data_attrs(out, el.input.as_ref(), &[key]);
    out.push('>');
    render_area_or_fallback(el, &href, out);
    out.push_str("</a>");
    if !inline {
        out.push('\n');
    }
}

fn render_ref_element(el: &Element, out: &mut String, inline: bool) {
    let target = el
        .input
        .as_ref()
        .and_then(as_map)
        .and_then(|m| map_get(m, "ref"))
        .map(value_to_plain)
        .unwrap_or_default();
    out.push_str(&format!(
        "<a class=\"tm-ref\" href=\"#link-{}\"",
        escape_attr(&target)
    ));
    push_data_attrs(out, el.input.as_ref(), &["ref"]);
    out.push('>');
    render_area_or_fallback(el, &target, out);
    out.push_str("</a>");
    if !inline {
        out.push('\n');
    }
}

fn render_area_or_fallback(el: &Element, fallback: &str, out: &mut String) {
    match &el.area {
        Some(area) if !area.is_empty() => render_inlines(area, out),
        _ => out.push_str(&escape_html(fallback)),
    }
}

fn render_generic_element(el: &Element, kind: &str, out: &mut String, inline: bool) {
    let tag = if inline { "span" } else { "div" };
    out.push_str(&format!("<{tag} class=\"tm-element tm-{kind}\""));
    push_data_attrs(out, el.input.as_ref(), &[]);
    out.push('>');
    if let Some(area) = &el.area {
        render_inlines(area, out);
    }
    if let Some(value) = &el.value {
        render_element_value(value, out);
    }
    out.push_str(&format!("</{tag}>"));
    if !inline {
        out.push('\n');
    }
}

fn render_element_value(value: &ElementValue, out: &mut String) {
    match value {
        ElementValue::Data(v) => {
            let text = value_to_plain(v);
            if !text.is_empty() {
                out.push_str("<span class=\"tm-value\">");
                out.push_str(&escape_html(&text));
                out.push_str("</span>");
            }
        }
        ElementValue::Children(children) => {
            out.push_str("<div class=\"tm-children\">\n");
            for child in children {
                render_element(child, out, false);
            }
            out.push_str("</div>\n");
        }
    }
}

fn render_links_container(el: &Element, out: &mut String) {
    out.push_str("<dl class=\"tm-links\">\n");
    if let Some(ElementValue::Children(children)) = &el.value {
        for child in children {
            let id = child.input.as_ref().map(value_to_plain).unwrap_or_default();
            out.push_str(&format!(
                "<dt id=\"link-{}\">{}</dt>\n",
                escape_attr(&id),
                escape_html(&id)
            ));
            out.push_str("<dd>");
            if let Some(area) = &child.area {
                render_inlines(area, out);
            }
            out.push_str("</dd>\n");
        }
    }
    out.push_str("</dl>\n");
}

fn split_attrs(attrs: Option<&Value>) -> (Option<String>, Option<String>, Vec<(String, String)>) {
    let mut id = None;
    let mut class = None;
    let mut data = Vec::new();
    if let Some(map) = attrs.and_then(as_map) {
        for (k, v) in map {
            match k.as_str() {
                "id" => id = Some(value_to_plain(v)),
                "cssclass" => class = Some(value_to_plain(v)),
                _ => data.push((k.clone(), value_to_plain(v))),
            }
        }
    }
    (id, class, data)
}

fn push_named_attrs(
    out: &mut String,
    id: &Option<String>,
    class: &Option<String>,
    data: &[(String, String)],
) {
    if let Some(id) = id {
        out.push_str(&format!(" id=\"{}\"", escape_attr(id)));
    }
    if let Some(class) = class {
        out.push_str(&format!(" class=\"{}\"", escape_attr(class)));
    }
    for (k, v) in data {
        out.push_str(&format!(" data-{}=\"{}\"", escape_attr(k), escape_attr(v)));
    }
}

fn push_data_attrs(out: &mut String, input: Option<&Value>, skip: &[&str]) {
    match input {
        Some(Value::Map(map)) => {
            for (k, v) in map {
                if skip.contains(&k.as_str()) {
                    continue;
                }
                out.push_str(&format!(
                    " data-{}=\"{}\"",
                    escape_attr(k),
                    escape_attr(&value_to_plain(v))
                ));
            }
        }
        Some(v) => {
            let text = value_to_plain(v);
            if !text.is_empty() {
                out.push_str(&format!(" data-value=\"{}\"", escape_attr(&text)));
            }
        }
        None => {}
    }
}

fn value_to_plain(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::String(s) => s.clone(),
        Value::Seq(items) => items
            .iter()
            .map(value_to_plain)
            .collect::<Vec<_>>()
            .join(", "),
        // A map has no single scalar representation; callers that need
        // per-key access use `as_map`/`map_get` instead.
        Value::Map(_) => String::new(),
    }
}

fn as_map(v: &Value) -> Option<&Vec<(String, Value)>> {
    match v {
        Value::Map(m) => Some(m),
        _ => None,
    }
}

fn map_get<'a>(map: &'a [(String, Value)], key: &str) -> Option<&'a Value> {
    map.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

fn escape_attr(s: &str) -> String {
    escape_html(s).replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use typedmark_parser::parse_document;

    #[test]
    fn renders_heading_with_id_and_cssclass() {
        let doc = parse_document("#[ Hello ]{ id:header1, cssclass:card }\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(body, "<h1 id=\"header1\" class=\"card\">Hello</h1>\n");
    }

    #[test]
    fn escapes_paragraph_text() {
        let doc = parse_document("a < b & c\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(body, "<p>a &lt; b &amp; c</p>\n");
    }

    #[test]
    fn renders_list() {
        let doc = parse_document("- one\n- two\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(body, "<ul>\n<li>one</li>\n<li>two</li>\n</ul>\n");
    }

    #[test]
    fn renders_links_container_as_definition_list() {
        let doc = parse_document("@links {\n  (1)[ note ]\n  (anotation1)[ note2 ]\n}\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(
            body,
            "<dl class=\"tm-links\">\n<dt id=\"link-1\">1</dt>\n<dd>note</dd>\n<dt id=\"link-anotation1\">anotation1</dt>\n<dd>note2</dd>\n</dl>\n"
        );
    }

    #[test]
    fn infers_url_link_from_at_element() {
        let doc = parse_document("@(url:https://example.com)[Wiki]\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(
            body,
            "<a class=\"tm-url\" href=\"https://example.com\">Wiki</a>\n"
        );
    }

    #[test]
    fn meta_element_has_no_visible_output() {
        let doc = parse_document("@meta(yaml){\n  key: value\n}\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(body, "");
    }

    #[test]
    fn renders_typed_element_generically() {
        let doc = parse_document("<caution>[ be careful ]\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(
            body,
            "<div class=\"tm-element tm-caution\">be careful</div>\n"
        );
    }
}
