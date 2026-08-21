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
    Block, Document, Element, ElementValue, Heading, Inline, InterpExpr, InterpExprKind, ListItem,
    Literal, Value,
};
use typedmark_semantics::classify;

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
        Block::Paragraph(p) => {
            // Same reasoning as `typedmark-html`'s `render_block`: an
            // all-invisible-element paragraph (e.g. adjacent `@meta(...)`
            // lines with no blank line between them) must not leave a
            // stray blank paragraph behind.
            let text = inline_to_md(&p.content);
            if !text.trim().is_empty() {
                out.push_str(&text);
                out.push_str("\n\n");
            }
        }
        Block::List(list) => render_list(&list.items, list.ordered, out),
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
    render_list_with_indent(items, ordered, 0, out);
    out.push('\n');
}

fn render_list_with_indent(items: &[ListItem], ordered: bool, indent: usize, out: &mut String) {
    let indent_str = "  ".repeat(indent);
    for (i, item) in items.iter().enumerate() {
        out.push_str(&indent_str);
        let marker = if ordered {
            format!("{}. ", i + 1)
        } else {
            "- ".to_string()
        };
        out.push_str(&marker);
        if let Some(m) = &item.marker {
            out.push_str(&format!("[{m}] "));
        }
        out.push_str(&inline_to_md(&item.content));
        out.push('\n');
        for child in &item.children {
            if let Block::List(sub) = child {
                render_list_with_indent(&sub.items, sub.ordered, indent + 1, out);
            }
        }
    }
}

fn inline_to_md(inlines: &[Inline]) -> String {
    let mut out = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => out.push_str(&escape_text(&t.value)),
            Inline::Element(el) => out.push_str(&element_to_md(el, true)),
        }
    }
    out
}

fn element_to_md(el: &Element, inline: bool) -> String {
    let kind = classify(el);
    match kind.as_str() {
        "meta" => String::new(),
        "config" => String::new(),
        "hr" => render_hr(el),
        "em" => format!("*{}*", content_to_md(el)),
        "strong" => format!("**{}**", content_to_md(el)),
        "mark" => format!("<mark>{}</mark>", content_to_md(el)),
        "codeblock" => render_code_block(el),
        "blockquote" => render_blockquote(el),
        "callout" => render_callout(el),
        "table" => render_table(el),
        "url" | "file" => render_link(el, kind.as_str()),
        "ref" => render_ref(el),
        "wiki" => render_wiki(el),
        "embed" => render_embed(el),
        "links" => render_links_container(el),
        // No CommonMark equivalent for `${...}` -- round-trips as literal
        // source text, same lossy-but-faithful treatment `render_generic`'s
        // fallback gives other unrecognized constructs. Given its own
        // dedicated case (not falling to `render_generic`) because that
        // fallback only looks at `args`/`content`, never `value`, and
        // `${...}`'s entire payload lives in `value`.
        "interp" => render_interp(el),
        _ => render_generic(el, kind.as_str(), inline),
    }
}

fn render_table(el: &Element) -> String {
    let inlines = match &el.content {
        Some(content) => content,
        None => return String::new(),
    };
    let rows = typedmark_semantics::parse_table_rows(inlines);
    if rows.is_empty() {
        return String::new();
    }

    let mut col_count = 0;
    for row in &rows {
        col_count = col_count.max(row.cells.len());
    }
    if col_count == 0 {
        return String::new();
    }

    let mut lines = Vec::new();

    // Row 0 (Header)
    let header_cells = &rows[0].cells;
    let mut header_line = String::from("|");
    for i in 0..col_count {
        let cell_md = if i < header_cells.len() {
            inline_to_md(&header_cells[i].content).replace('|', "\\|")
        } else {
            String::new()
        };
        header_line.push_str(&format!(" {cell_md} |"));
    }
    lines.push(header_line);

    // Delimiter row
    let mut delim_line = String::from("|");
    for _ in 0..col_count {
        delim_line.push_str(" --- |");
    }
    lines.push(delim_line);

    // Body rows
    for row in rows.iter().skip(1) {
        let mut row_line = String::from("|");
        for i in 0..col_count {
            let cell_md = if i < row.cells.len() {
                inline_to_md(&row.cells[i].content).replace('|', "\\|")
            } else {
                String::new()
            };
            row_line.push_str(&format!(" {cell_md} |"));
        }
        lines.push(row_line);
    }

    lines.join("\n")
}

/// Re-renders an `InterpExpr` back to `${...}`-shaped source text. Not
/// shared via `typedmark-ast`: rendering back to text is each consumer's
/// own job here, same as `render_value_inner`-equivalent helpers already
/// are for `Value` elsewhere in this file.
fn render_interp(el: &Element) -> String {
    match &el.value {
        Some(ElementValue::Interp(expr)) => format!("${{{}}}", render_interp_expr(expr)),
        _ => String::new(),
    }
}

fn render_interp_expr(expr: &InterpExpr) -> String {
    match &expr.kind {
        InterpExprKind::Identifier(name) => name.clone(),
        InterpExprKind::Literal(Literal::Int(i)) => i.to_string(),
        InterpExprKind::Literal(Literal::Float(x)) => x.to_string(),
        InterpExprKind::Literal(Literal::String(s)) => format!("{s:?}"),
        InterpExprKind::Call { callee, args } => {
            let args = args.iter().map(render_interp_expr).collect::<Vec<_>>();
            format!("{}({})", render_interp_expr(callee), args.join(", "))
        }
        InterpExprKind::Member { object, member } => {
            format!("{}.{member}", render_interp_expr(object))
        }
    }
}

fn content_to_md(el: &Element) -> String {
    el.content
        .as_ref()
        .map(|a| inline_to_md(a))
        .unwrap_or_default()
}

/// A bare `---` break exports as-is; a titled one (`---[ Title ]---`) has
/// no CommonMark equivalent, so it's lossy: a bold line followed by a
/// plain rule.
fn render_hr(el: &Element) -> String {
    match &el.content {
        Some(title) if !title.is_empty() => format!("{}\n---", inline_to_md(title)),
        _ => "---".to_string(),
    }
}

/// `codeblock`'s `{value}` (`id`/`cssclass` metadata, if present -- see
/// `typedmark-html`'s `render_codeblock_element`) has no CommonMark
/// form, same as a heading's attrs, so it's dropped on export.
fn render_code_block(el: &Element) -> String {
    let lang = el
        .args
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
    let text = content_to_md(el);
    text.lines()
        .map(|line| format!("> {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_callout(el: &Element) -> String {
    let mut variant = None;
    let mut title = None;

    if let Some(args) = &el.args {
        match args {
            Value::String(s) => variant = Some(s.clone()),
            Value::Map(entries) => {
                for (k, v) in entries {
                    if k == "variant" {
                        if let Value::String(s) = v {
                            variant = Some(s.clone());
                        }
                    } else if k == "title" {
                        if let Value::String(s) = v {
                            title = Some(s.clone());
                        }
                    }
                }
                if variant.is_none() && !entries.is_empty() {
                    if let Value::String(s) = &entries[0].1 {
                        variant = Some(s.clone());
                    }
                }
            }
            Value::Seq(items) => {
                if let Some(Value::String(s)) = items.first() {
                    variant = Some(s.clone());
                }
            }
            _ => {}
        }
    }

    let v = variant.unwrap_or_else(|| "note".to_string());
    let body = content_to_md(el);
    let mut lines = Vec::new();

    if let Some(t) = title {
        lines.push(format!("> [!{v}] {t}"));
    } else {
        lines.push(format!("> [!{v}]"));
    }

    for line in body.lines() {
        lines.push(format!("> {line}"));
    }

    lines.join("\n")
}

fn render_link(el: &Element, key: &str) -> String {
    let href = el
        .args
        .as_ref()
        .and_then(as_map)
        .and_then(|m| map_get(m, key))
        .map(value_to_plain)
        .unwrap_or_default();
    let text = match &el.content {
        Some(content) if !content.is_empty() => inline_to_md(content),
        _ => String::new(),
    };
    if text.is_empty() || text == href {
        href
    } else {
        format!("[{text}]({href})")
    }
}

fn render_ref(el: &Element) -> String {
    let target = el
        .args
        .as_ref()
        .and_then(as_map)
        .and_then(|m| map_get(m, "ref"))
        .map(value_to_plain)
        .unwrap_or_default();
    let text = match &el.content {
        Some(content) if !content.is_empty() => inline_to_md(content),
        _ => target.clone(),
    };
    format!("[{text}](#link-{target})")
}

fn render_wiki(el: &Element) -> String {
    let target = el
        .args
        .as_ref()
        .and_then(as_map)
        .and_then(|m| map_get(m, "wiki"))
        .map(value_to_plain)
        .unwrap_or_default();
    let display = match &el.content {
        Some(content) if !content.is_empty() => inline_to_md(content),
        _ => String::new(),
    };

    if display.is_empty() || display == target {
        format!("[[{target}]]")
    } else {
        format!("[[{target}|{display}]]")
    }
}

fn render_embed(el: &Element) -> String {
    let src = el
        .args
        .as_ref()
        .and_then(as_map)
        .and_then(|m| {
            map_get(m, "path")
                .or_else(|| map_get(m, "file"))
                .or_else(|| map_get(m, "url"))
                .or_else(|| map_get(m, "wiki"))
        })
        .map(value_to_plain)
        .unwrap_or_default();
    let alt = el
        .content
        .as_ref()
        .map(|a| inlines_to_plain(a))
        .unwrap_or_default();
    format!("![{alt}]({src})")
}

fn inlines_to_plain(inlines: &[Inline]) -> String {
    let mut s = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => s.push_str(&t.value),
            Inline::Element(el) => {
                if let Some(content) = &el.content {
                    s.push_str(&inlines_to_plain(content));
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
            let id = child.args.as_ref().map(value_to_plain).unwrap_or_default();
            let content = child
                .content
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
/// HTML -- valid CommonMark, and matches `typedmark-html`'s own
/// generic div/span fallback in spirit.
fn render_generic(el: &Element, kind: &str, inline: bool) -> String {
    let tag = if inline { "span" } else { "div" };
    let mut out = format!("<{tag} data-tm-kind=\"{}\"", escape_attr(kind));
    if let Some(args) = &el.args {
        push_data_attrs(&mut out, args);
    }
    out.push('>');
    if let Some(content) = &el.content {
        out.push_str(&inline_to_md(content));
    }
    out.push_str(&format!("</{tag}>"));
    out
}

fn push_data_attrs(out: &mut String, args: &Value) {
    if let Some(map) = as_map(args) {
        for (k, v) in map {
            out.push_str(&format!(
                " data-{}=\"{}\"",
                escape_attr(k),
                escape_attr(&value_to_plain(v))
            ));
        }
    }
}

fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if matches!(c, '\\' | '`' | '*' | '_' | '[' | ']') {
            out.push('\\');
        } else if c == '<' {
            let mut look = chars.clone();
            let mut tag_name = String::new();
            while let Some(&ch) = look.peek() {
                if ch.is_alphanumeric() || ch == '_' || ch == '-' {
                    tag_name.push(ch);
                    look.next();
                } else {
                    break;
                }
            }
            if is_common_html_tag(&tag_name) {
                out.push('\\');
            }
        }
        out.push(c);
    }
    out
}

fn is_common_html_tag(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "a" | "abbr" | "address" | "article" | "aside" | "audio" | "b" | "base" | "bdi"
            | "bdo" | "blockquote" | "body" | "br" | "button" | "canvas" | "caption" | "cite"
            | "code" | "col" | "colgroup" | "data" | "datalist" | "dd" | "del" | "details"
            | "dfn" | "dialog" | "div" | "dl" | "dt" | "em" | "embed" | "fieldset"
            | "figcaption" | "figure" | "footer" | "form" | "h1" | "h2" | "h3" | "h4"
            | "h5" | "h6" | "head" | "header" | "hgroup" | "hr" | "html" | "i" | "iframe"
            | "img" | "input" | "ins" | "kbd" | "label" | "legend" | "li" | "link" | "main"
            | "map" | "mark" | "menu" | "meta" | "meter" | "nav" | "noscript" | "object"
            | "ol" | "optgroup" | "option" | "output" | "p" | "param" | "picture" | "pre"
            | "progress" | "q" | "rp" | "rt" | "ruby" | "s" | "samp" | "script" | "section"
            | "select" | "small" | "source" | "span" | "strong" | "style" | "sub" | "summary"
            | "sup" | "svg" | "table" | "tbody" | "td" | "template" | "textarea" | "tfoot"
            | "th" | "thead" | "time" | "title" | "tr" | "track" | "u" | "ul" | "var"
            | "video" | "wbr"
    )
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
    use typedmark_ast::{List, Paragraph, Sigil, Span, Text};

    #[test]
    fn adjacent_meta_blocks_have_no_visible_output() {
        // Hand-built `Document` (doesn't go through `typedmark_parser::
        // parse_document`, so it's independent of the parser's own
        // paragraph-continuation rules -- `typedmark-parser` no longer
        // folds adjacent `@meta(...)` elements with no blank line between
        // them into one `Block::Paragraph`; each becomes its own
        // `Block::Element` now, see `document.rs`'s `Stop::Paragraph`).
        // Kept as a regression test for the shape this crate must still
        // handle correctly if it ever *does* show up (e.g. a
        // hand-authored `Document`, or a future grammar change): a
        // `Block::Paragraph` containing only no-output elements plus
        // inter-element whitespace text must not leave a stray blank line
        // behind in the exported Markdown.
        fn meta_element(tag: &str) -> Element {
            Element {
                sigil: Sigil::At(Some("meta".to_string())),
                args: Some(Value::String(tag.to_string())),
                content: None,
                value: Some(ElementValue::Data(Value::Map(vec![(
                    "key".to_string(),
                    Value::String("value".to_string()),
                )]))),
                span: Span::dummy(),
            }
        }
        let doc = Document {
            blocks: vec![
                Block::Paragraph(Paragraph::new(
                    vec![
                        Inline::Element(meta_element("json")),
                        Inline::Text(Text::new(" ", Span::dummy())),
                        Inline::Element(meta_element("yaml")),
                        Inline::Text(Text::new(" ", Span::dummy())),
                        Inline::Element(meta_element("toml")),
                    ],
                    Span::dummy(),
                )),
                Block::Heading(Heading {
                    level: 1,
                    content: vec![Inline::Text(Text::new("next", Span::dummy()))],
                    attrs: None,
                    span: Span::dummy(),
                }),
            ],
            span: Span::dummy(),
        };
        assert_eq!(to_markdown(&doc), "# next\n\n");
    }

    #[test]
    fn heading_and_paragraph() {
        let doc = Document {
            blocks: vec![
                Block::Heading(Heading {
                    level: 2,
                    content: vec![Inline::Text(Text::new("Title", Span::dummy()))],
                    attrs: None,
                    span: Span::dummy(),
                }),
                Block::Paragraph(Paragraph::new(
                    vec![Inline::Text(Text::new("Hello.", Span::dummy()))],
                    Span::dummy(),
                )),
            ],
            span: Span::dummy(),
        };
        assert_eq!(to_markdown(&doc), "## Title\n\nHello.\n\n");
    }

    #[test]
    fn bullet_and_ordered_list() {
        let doc = Document {
            blocks: vec![Block::List(List::new(
                true,
                vec![
                    ListItem::new(
                        vec![Inline::Text(Text::new("one", Span::dummy()))],
                        None,
                        None,
                        Span::dummy(),
                    ),
                    ListItem::new(
                        vec![Inline::Text(Text::new("two", Span::dummy()))],
                        None,
                        None,
                        Span::dummy(),
                    ),
                ],
                Span::dummy(),
            ))],
            span: Span::dummy(),
        };
        assert_eq!(to_markdown(&doc), "1. one\n2. two\n\n");
    }

    #[test]
    fn emphasis_and_strong() {
        let doc = Document {
            blocks: vec![Block::Paragraph(Paragraph::new(
                vec![
                    Inline::Element(Element {
                        sigil: Sigil::Type("em".to_string()),
                        args: None,
                        content: Some(vec![Inline::Text(Text::new("a", Span::dummy()))]),
                        value: None,
                        span: Span::dummy(),
                    }),
                    Inline::Text(Text::new(" ", Span::dummy())),
                    Inline::Element(Element {
                        sigil: Sigil::Type("strong".to_string()),
                        args: None,
                        content: Some(vec![Inline::Text(Text::new("b", Span::dummy()))]),
                        value: None,
                        span: Span::dummy(),
                    }),
                ],
                Span::dummy(),
            ))],
            span: Span::dummy(),
        };
        assert_eq!(to_markdown(&doc), "*a* **b**\n\n");
    }

    #[test]
    fn link_round_trips() {
        let el = Element {
            sigil: Sigil::At(None),
            args: Some(Value::Map(vec![(
                "url".to_string(),
                Value::String("https://example.com".to_string()),
            )])),
            content: Some(vec![Inline::Text(Text::new("Wiki", Span::dummy()))]),
            value: None,
            span: Span::dummy(),
        };
        let doc = Document {
            blocks: vec![Block::Paragraph(Paragraph::new(
                vec![Inline::Element(el)],
                Span::dummy(),
            ))],
            span: Span::dummy(),
        };
        assert_eq!(to_markdown(&doc), "[Wiki](https://example.com)\n\n");
    }

    #[test]
    fn embed_becomes_image() {
        let el = Element {
            sigil: Sigil::Type("embed".to_string()),
            args: Some(Value::Map(vec![(
                "file".to_string(),
                Value::String("pic.png".to_string()),
            )])),
            content: Some(vec![Inline::Text(Text::new("a cat", Span::dummy()))]),
            value: None,
            span: Span::dummy(),
        };
        let doc = Document {
            blocks: vec![Block::Paragraph(Paragraph::new(
                vec![Inline::Element(el)],
                Span::dummy(),
            ))],
            span: Span::dummy(),
        };
        assert_eq!(to_markdown(&doc), "![a cat](pic.png)\n\n");
    }

    #[test]
    fn code_block_uses_fence_and_lang() {
        let el = Element {
            sigil: Sigil::Type("codeblock".to_string()),
            args: Some(Value::Map(vec![(
                "lang".to_string(),
                Value::String("rust".to_string()),
            )])),
            content: Some(vec![Inline::Text(Text::new("fn main() {}", Span::dummy()))]),
            value: None,
            span: Span::dummy(),
        };
        let doc = Document {
            blocks: vec![Block::Element(el)],
            span: Span::dummy(),
        };
        assert_eq!(to_markdown(&doc), "```rust\nfn main() {}\n```\n\n");
    }

    #[test]
    fn thematic_break() {
        let doc = Document {
            blocks: vec![Block::Element(Element::new(Sigil::Type("hr".to_string())))],
            span: Span::dummy(),
        };
        assert_eq!(to_markdown(&doc), "---\n\n");
    }

    #[test]
    fn bare_at_meta_is_not_inferred_only_the_explicit_name_is() {
        // `meta` is deliberately not in `typedmark_semantics::INFERRED_AT_KEYS`
        // (see its doc comment) -- a bare `@` with a `meta` key falls back
        // to the generic "at" element export, unlike `@meta(...)`
        // (`Sigil::At(Some("meta".into()))`), which exports as nothing.
        let mut el = Element::new(Sigil::At(None));
        el.args = Some(Value::Map(vec![(
            "meta".to_string(),
            Value::String("yaml".to_string()),
        )]));
        let doc = Document {
            blocks: vec![Block::Element(el)],
            span: Span::dummy(),
        };
        assert!(
            to_markdown(&doc).contains("data-tm-kind=\"at\""),
            "expected generic 'at' kind, got: {}",
            to_markdown(&doc)
        );
    }

    #[test]
    fn config_element_exports_as_nothing() {
        let mut el = Element::new(Sigil::At(Some("config".to_string())));
        el.args = Some(Value::Map(vec![(
            "format".to_string(),
            Value::String("json".to_string()),
        )]));
        let doc = Document {
            blocks: vec![Block::Element(el)],
            span: Span::dummy(),
        };
        assert_eq!(to_markdown(&doc), "");
    }

    #[test]
    fn titled_thematic_break() {
        let mut el = Element::new(Sigil::Type("hr".to_string()));
        el.content = Some(vec![Inline::Text(Text::new("Title", Span::dummy()))]);
        let doc = Document {
            blocks: vec![Block::Element(el)],
            span: Span::dummy(),
        };
        assert_eq!(to_markdown(&doc), "Title\n---\n\n");
    }

    #[test]
    fn wikilink_exports_to_markdown() {
        let mut el1 = Element::new(Sigil::At(None));
        el1.args = Some(Value::Map(vec![(
            "wiki".to_string(),
            Value::String("name".to_string()),
        )]));

        let mut el2 = Element::new(Sigil::At(None));
        el2.args = Some(Value::Map(vec![(
            "wiki".to_string(),
            Value::String("name".to_string()),
        )]));
        el2.content = Some(vec![Inline::Text(Text::new("display", Span::dummy()))]);

        let doc = Document {
            blocks: vec![Block::Paragraph(Paragraph::new(
                vec![
                    Inline::Element(el1),
                    Inline::Text(Text::new(" and ", Span::dummy())),
                    Inline::Element(el2),
                ],
                Span::dummy(),
            ))],
            span: Span::dummy(),
        };
        assert_eq!(to_markdown(&doc), "[[name]] and [[name|display]]\n\n");
    }

    #[test]
    fn table_exports_to_markdown() {
        let mut el = Element::new(Sigil::At(Some("table".to_string())));
        el.content = Some(vec![Inline::Text(Text::new(
            "[ col1 ][ col2 ]\n[ val1 ][ val2 ]",
            Span::dummy(),
        ))]);
        let doc = Document {
            blocks: vec![Block::Element(el)],
            span: Span::dummy(),
        };
        assert_eq!(
            to_markdown(&doc),
            "| col1 | col2 |\n| --- | --- |\n| val1 | val2 |\n\n"
        );
    }
}
