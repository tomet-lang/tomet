//! `typedmark_ast::Document` -> CommonMark, hand-written (the output
//! space is much smaller than the input space, so no need for a crate
//! here -- see `docs/commonmark-support.md`).
//!
//! Constructs with no CommonMark equivalent (`mark`, and any `<T>`/`@`
//! element the importer never produces but a hand-authored `.tm` file
//! might use, e.g. `@links{}` definition containers or arbitrary typed
//! elements) fall back to raw inline/block HTML passthrough, which is
//! valid CommonMark. Heading `id`/`cssclass` attrs have no CommonMark
//! form and are dropped.

use typedmark_ast::{
    Block, Document, Element, ElementValue, Heading, Inline, ListItem, Sigil, Value,
};

pub fn to_markdown(doc: &Document) -> String {
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
            out.push_str(&inline_to_md(inlines));
            out.push_str("\n\n");
        }
        Block::List { ordered, items } => render_list(items, *ordered, out),
        Block::Element(el) => {
            let text = element_to_md(el, false);
            if !text.is_empty() {
                out.push_str(&text);
                out.push_str("\n\n");
            }
        }
    }
}

fn render_heading(h: &Heading, out: &mut String) {
    let level = h.level.clamp(1, 6) as usize;
    out.push_str(&"#".repeat(level));
    out.push(' ');
    out.push_str(&inline_to_md(&h.content));
    out.push_str("\n\n");
}

fn render_list(items: &[ListItem], ordered: bool, out: &mut String) {
    for (i, item) in items.iter().enumerate() {
        if ordered {
            out.push_str(&format!("{}. ", i + 1));
        } else {
            out.push_str("- ");
        }
        out.push_str(&inline_to_md(&item.content));
        out.push('\n');
    }
    out.push('\n');
}

fn inline_to_md(inlines: &[Inline]) -> String {
    let mut out = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => out.push_str(&escape_text(t)),
            Inline::Element(el) => out.push_str(&element_to_md(el, true)),
        }
    }
    out
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

fn element_to_md(el: &Element, inline: bool) -> String {
    let kind = element_kind(el);
    match kind.as_str() {
        "meta" => String::new(),
        "hr" => "---".to_string(),
        "em" => format!("*{}*", area_to_md(el)),
        "strong" => format!("**{}**", area_to_md(el)),
        "mark" => format!("<mark>{}</mark>", area_to_md(el)),
        "pre" => render_code_block(el),
        "blockquote" => render_blockquote(el),
        "url" | "file" => render_link(el, &kind),
        "ref" => render_ref(el),
        "embed" => render_embed(el),
        "links" => render_links_container(el),
        _ => render_generic(el, &kind, inline),
    }
}

fn area_to_md(el: &Element) -> String {
    el.area
        .as_ref()
        .map(|a| inline_to_md(a))
        .unwrap_or_default()
}

fn render_code_block(el: &Element) -> String {
    let lang = el
        .input
        .as_ref()
        .and_then(as_map)
        .and_then(|m| map_get(m, "lang"))
        .map(value_to_plain)
        .unwrap_or_default();
    let code = match &el.value {
        Some(ElementValue::Data(Value::String(s))) => s.clone(),
        _ => String::new(),
    };
    let fence = fence_for(&code);
    format!("{fence}{lang}\n{code}\n{fence}")
}

/// Fenced code blocks need a fence at least one backtick longer than the
/// longest run of backticks already inside the code, or the fence would
/// terminate early on re-parse.
fn fence_for(code: &str) -> String {
    let longest_run = code
        .split(|c: char| c != '`')
        .map(|run| run.len())
        .max()
        .unwrap_or(0);
    "`".repeat((longest_run + 1).max(3))
}

fn render_blockquote(el: &Element) -> String {
    let text = area_to_md(el);
    text.lines()
        .map(|line| format!("> {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_link(el: &Element, key: &str) -> String {
    let href = el
        .input
        .as_ref()
        .and_then(as_map)
        .and_then(|m| map_get(m, key))
        .map(value_to_plain)
        .unwrap_or_default();
    let text = match &el.area {
        Some(area) if !area.is_empty() => inline_to_md(area),
        _ => href.clone(),
    };
    format!("[{text}]({href})")
}

fn render_ref(el: &Element) -> String {
    let target = el
        .input
        .as_ref()
        .and_then(as_map)
        .and_then(|m| map_get(m, "ref"))
        .map(value_to_plain)
        .unwrap_or_default();
    let text = match &el.area {
        Some(area) if !area.is_empty() => inline_to_md(area),
        _ => target.clone(),
    };
    format!("[{text}](#link-{target})")
}

fn render_embed(el: &Element) -> String {
    let src = el
        .input
        .as_ref()
        .and_then(as_map)
        .and_then(|m| map_get(m, "file").or_else(|| map_get(m, "url")))
        .map(value_to_plain)
        .unwrap_or_default();
    let alt = el
        .area
        .as_ref()
        .map(|a| inlines_to_plain(a))
        .unwrap_or_default();
    format!("![{alt}]({src})")
}

fn inlines_to_plain(inlines: &[Inline]) -> String {
    let mut s = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => s.push_str(t),
            Inline::Element(el) => {
                if let Some(area) = &el.area {
                    s.push_str(&inlines_to_plain(area));
                }
            }
        }
    }
    s
}

fn render_links_container(el: &Element) -> String {
    let mut out = String::new();
    if let Some(ElementValue::Children(children)) = &el.value {
        for (i, child) in children.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            let id = child.input.as_ref().map(value_to_plain).unwrap_or_default();
            let content = child
                .area
                .as_ref()
                .map(|a| inline_to_md(a))
                .unwrap_or_default();
            out.push_str(&format!("**{id}**: {content}"));
        }
    }
    out
}

/// Anything with no dedicated CommonMark mapping (a hand-authored `<T>`
/// or `@name` element the importer never produces) passes through as raw
/// HTML -- valid CommonMark, and matches `typedmark-renderer`'s own
/// generic div/span fallback in spirit.
fn render_generic(el: &Element, kind: &str, inline: bool) -> String {
    let tag = if inline { "span" } else { "div" };
    let mut out = format!("<{tag} data-tm-kind=\"{}\"", escape_attr(kind));
    if let Some(input) = &el.input {
        push_data_attrs(&mut out, input);
    }
    out.push('>');
    if let Some(area) = &el.area {
        out.push_str(&inline_to_md(area));
    }
    out.push_str(&format!("</{tag}>"));
    out
}

fn push_data_attrs(out: &mut String, input: &Value) {
    if let Some(map) = as_map(input) {
        for (k, v) in map {
            out.push_str(&format!(
                " data-{}=\"{}\"",
                escape_attr(k),
                escape_attr(&value_to_plain(v))
            ));
        }
    }
}

/// Escape characters that would otherwise be read back as CommonMark
/// syntax (our own emitted delimiters chief among them).
fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '\\' | '`' | '*' | '_' | '[' | ']' | '<') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_and_paragraph() {
        let doc = Document {
            blocks: vec![
                Block::Heading(Heading {
                    level: 2,
                    content: vec![Inline::Text("Title".to_string())],
                    attrs: None,
                }),
                Block::Paragraph(vec![Inline::Text("Hello.".to_string())]),
            ],
        };
        assert_eq!(to_markdown(&doc), "## Title\n\nHello.\n\n");
    }

    #[test]
    fn bullet_and_ordered_list() {
        let doc = Document {
            blocks: vec![Block::List {
                ordered: true,
                items: vec![
                    ListItem {
                        content: vec![Inline::Text("one".to_string())],
                    },
                    ListItem {
                        content: vec![Inline::Text("two".to_string())],
                    },
                ],
            }],
        };
        assert_eq!(to_markdown(&doc), "1. one\n2. two\n\n");
    }

    #[test]
    fn emphasis_and_strong() {
        let doc = Document {
            blocks: vec![Block::Paragraph(vec![
                Inline::Element(Element {
                    sigil: Sigil::Type("em".to_string()),
                    input: None,
                    area: Some(vec![Inline::Text("a".to_string())]),
                    value: None,
                }),
                Inline::Text(" ".to_string()),
                Inline::Element(Element {
                    sigil: Sigil::Type("strong".to_string()),
                    input: None,
                    area: Some(vec![Inline::Text("b".to_string())]),
                    value: None,
                }),
            ])],
        };
        assert_eq!(to_markdown(&doc), "*a* **b**\n\n");
    }

    #[test]
    fn link_round_trips() {
        let el = Element {
            sigil: Sigil::At(None),
            input: Some(Value::Map(vec![(
                "url".to_string(),
                Value::String("https://example.com".to_string()),
            )])),
            area: Some(vec![Inline::Text("Wiki".to_string())]),
            value: None,
        };
        let doc = Document {
            blocks: vec![Block::Paragraph(vec![Inline::Element(el)])],
        };
        assert_eq!(to_markdown(&doc), "[Wiki](https://example.com)\n\n");
    }

    #[test]
    fn embed_becomes_image() {
        let el = Element {
            sigil: Sigil::Type("embed".to_string()),
            input: Some(Value::Map(vec![(
                "file".to_string(),
                Value::String("pic.png".to_string()),
            )])),
            area: Some(vec![Inline::Text("a cat".to_string())]),
            value: None,
        };
        let doc = Document {
            blocks: vec![Block::Paragraph(vec![Inline::Element(el)])],
        };
        assert_eq!(to_markdown(&doc), "![a cat](pic.png)\n\n");
    }

    #[test]
    fn code_block_uses_fence_and_lang() {
        let el = Element {
            sigil: Sigil::Type("pre".to_string()),
            input: Some(Value::Map(vec![(
                "lang".to_string(),
                Value::String("rust".to_string()),
            )])),
            area: None,
            value: Some(ElementValue::Data(Value::String(
                "fn main() {}".to_string(),
            ))),
        };
        let doc = Document {
            blocks: vec![Block::Element(el)],
        };
        assert_eq!(to_markdown(&doc), "```rust\nfn main() {}\n```\n\n");
    }

    #[test]
    fn thematic_break() {
        let doc = Document {
            blocks: vec![Block::Element(Element::new(Sigil::Type("hr".to_string())))],
        };
        assert_eq!(to_markdown(&doc), "---\n\n");
    }
}
