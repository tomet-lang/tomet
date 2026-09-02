//! `tomet_ast::Document` -> CommonMark, hand-written (the output
//! space is much smaller than the input space, so no need for a crate
//! here -- see `docs/design/decisions/2026-08-09-commonmark-support.md`).
//!
//! Constructs with no CommonMark equivalent (`mark`, and any `<T>`/`@`
//! element the importer never produces but a hand-authored `.tmt` file
//! might use, e.g. `@links{}` definition containers or arbitrary typed
//! elements) fall back to raw inline/block HTML passthrough, which is
//! valid CommonMark. Heading `id`/`cssclass` attrs have no CommonMark
//! form and are dropped.

use tomet_ast::{
    Block, Document, Element, ElementValue, Inline, InterpExpr, InterpExprKind, Literal, Value,
};
use tomet_semantics::{
    TargetScheme, classify_lenient, heading_level, link_target, list_items, list_ordered,
    target_scheme,
};

struct RenderCtx<'a> {
    doc: &'a Document,
    config: tomet_semantics::DocumentConfig,
}

pub fn to_markdown(doc: &Document) -> String {
    let mut out = String::new();
    let config = tomet_semantics::document_config(doc);
    let cx = RenderCtx { doc, config };
    for block in &doc.blocks {
        render_block(&cx, block, &mut out);
    }
    out
}

fn render_block(cx: &RenderCtx, block: &Block, out: &mut String) {
    match block {
        Block::Paragraph(p) => {
            // Same reasoning as `tomet-html`'s `render_block`: an
            // all-invisible-element paragraph (e.g. adjacent `@meta(...)`
            // lines with no blank line between them) must not leave a
            // stray blank paragraph behind.
            let text = inline_to_md(cx, &p.content);
            if !text.trim().is_empty() {
                out.push_str(&text);
                out.push_str("\n\n");
            }
        }
        Block::Element(el) if list_ordered(el).is_some() => render_list(cx, el, out),
        Block::Element(el) => {
            let text = element_to_md(cx, el, false);
            if !text.is_empty() {
                out.push_str(&text);
                out.push_str("\n\n");
            }
        }
    }
}

fn render_list(cx: &RenderCtx, el: &Element, out: &mut String) {
    render_list_with_indent(cx, el, 0, out);
    out.push('\n');
}

fn render_list_with_indent(cx: &RenderCtx, el: &Element, indent: usize, out: &mut String) {
    let ordered = list_ordered(el).unwrap_or(false);
    let indent_str = "  ".repeat(indent);
    for (i, item) in list_items(el).iter().enumerate() {
        out.push_str(&indent_str);
        let marker = if ordered {
            format!("{}. ", i + 1)
        } else {
            "- ".to_string()
        };
        out.push_str(&marker);
        // `args` (the `(...)` marker `Value`) has no CommonMark equivalent
        // -- dropped on export, same as this crate's other documented
        // lossy cases (see the module doc).
        out.push_str(&inline_to_md(cx, item.content.as_deref().unwrap_or(&[])));
        out.push('\n');
        if let Some(children) = &item.children {
            for child in children {
                if let Block::Element(sub) = child {
                    if list_ordered(sub).is_some() {
                        render_list_with_indent(cx, sub, indent + 1, out);
                    }
                }
            }
        }
    }
}

fn inline_to_md(cx: &RenderCtx, inlines: &[Inline]) -> String {
    let mut out = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => out.push_str(&escape_text(&t.value)),
            Inline::Element(el) => out.push_str(&element_to_md(cx, el, true)),
        }
    }
    out
}

fn element_to_md(cx: &RenderCtx, el: &Element, inline: bool) -> String {
    let kind = classify_lenient(el);
    match kind.as_str() {
        "version" | "kind" | "meta" | "config" | "blueprint" => String::new(),
        // Block-position only, same as CommonMark's own headings and
        // Tomet's own `#[x]` grammar -- a nested/inline `@heading(...)`
        // (`inline == true`) falls through to generic/custom rendering
        // instead, rather than emitting a bare `## text` mid-paragraph
        // (which wouldn't parse back as a heading anyway).
        "heading" if !inline => render_heading(cx, el),
        "hr" => render_hr(cx, el),
        "em" => format!("*{}*", content_to_md(cx, el)),
        "strong" => format!("**{}**", content_to_md(cx, el)),
        "mark" => format!("<mark>{}</mark>", content_to_md(cx, el)),
        "codeblock" => render_code_block(el),
        "blockquote" => render_blockquote(cx, el),
        "callout" => render_callout(cx, el),
        "table" => render_table(cx, el),
        "link" => render_link(cx, el),
        "embed" => render_embed(el),
        "links" => render_links_container(cx, el),
        // Evaluate `${...}` interpolations and macros
        "interp" => render_interp(cx, el),
        _ => render_generic(cx, el, kind.as_str(), inline),
    }
}

fn render_table(cx: &RenderCtx, el: &Element) -> String {
    let inlines = match &el.content {
        Some(content) => content,
        None => return String::new(),
    };
    let rows = tomet_semantics::parse_table_rows(inlines);
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
            inline_to_md(cx, &header_cells[i].content).replace('|', "\\|")
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
                inline_to_md(cx, &row.cells[i].content).replace('|', "\\|")
            } else {
                String::new()
            };
            row_line.push_str(&format!(" {cell_md} |"));
        }
        lines.push(row_line);
    }

    lines.join("\n")
}

/// Evaluates `${...}` interpolation expressions and macro templates against the document.
fn render_interp(cx: &RenderCtx, el: &Element) -> String {
    match &el.value {
        Some(ElementValue::Interp(expr)) => {
            match tomet_compute::evaluate_with_config(cx.doc, expr, &cx.config) {
                Ok(val) => match val {
                    Value::String(s) => s,
                    Value::Int(i) => i.to_string(),
                    Value::Float(f) => f.to_string(),
                    Value::Bool(b) => b.to_string(),
                    Value::Null => String::new(),
                    other => format!("{other:?}"),
                },
                Err(_) => format!("${{{}}}", render_interp_expr(expr)),
            }
        }
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
        InterpExprKind::NamedArg { name, value } => {
            format!("{name}: {}", render_interp_expr(value))
        }
    }
}

fn content_to_md(cx: &RenderCtx, el: &Element) -> String {
    el.content
        .as_ref()
        .map(|a| inline_to_md(cx, a))
        .unwrap_or_default()
}

/// A bare `---` break exports as-is; a titled one (`---[ Title ]---`) has
/// no CommonMark equivalent, so it's lossy: a bold line followed by a
/// plain rule.
/// `id`/`cssclass` attrs (`el.value`) have no CommonMark form and are
/// dropped, same as before this was folded into the generic `Element`
/// dispatch (see the module doc).
fn render_heading(cx: &RenderCtx, el: &Element) -> String {
    let level = heading_level(el).unwrap_or(1) as usize;
    let content = el.content.as_deref().unwrap_or(&[]);
    format!("{} {}", "#".repeat(level), inline_to_md(cx, content))
}

fn render_hr(cx: &RenderCtx, el: &Element) -> String {
    match &el.content {
        Some(title) if !title.is_empty() => format!("{}\n---", inline_to_md(cx, title)),
        _ => "---".to_string(),
    }
}

/// `codeblock`'s `{value}` (`id`/`cssclass` metadata, if present -- see
/// `tomet-html`'s `render_codeblock_element`) has no CommonMark
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

fn render_blockquote(cx: &RenderCtx, el: &Element) -> String {
    let text = content_to_md(cx, el);
    text.lines()
        .map(|line| format!("> {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_callout(cx: &RenderCtx, el: &Element) -> String {
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
    let body = content_to_md(cx, el);
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

/// `@link(target:..)` -- the target string's own scheme prefix (see
/// `tomet_semantics::target_scheme`) decides which CommonMark shape it
/// exports as: a same-document anchor link (`id:`), a wikilink (`ref:`),
/// or a plain `[text](target)`/bare-target link (everything else). The
/// scheme prefix itself is stripped before rendering -- it's addressing
/// metadata, not part of the visible target.
fn render_link(cx: &RenderCtx, el: &Element) -> String {
    let raw_target = link_target(el, &classify_lenient(el)).unwrap_or_default();
    let (scheme, target) = target_scheme(&raw_target);
    let text = match &el.content {
        Some(content) if !content.is_empty() => inline_to_md(cx, content),
        _ => String::new(),
    };
    match scheme {
        TargetScheme::Id => {
            let text = if text.is_empty() {
                target.to_string()
            } else {
                text
            };
            format!("[{text}](#link-{target})")
        }
        TargetScheme::Ref => {
            if text.is_empty() || text == target {
                format!("[[{target}]]")
            } else {
                format!("[[{target}|{text}]]")
            }
        }
        _ => {
            if text.is_empty() || text == target {
                target.to_string()
            } else {
                format!("[{text}]({target})")
            }
        }
    }
}

/// Strips `target`'s scheme prefix the same way `render_link` does -- see
/// `tomet-html`'s `render_embed_element` for why `<embed>`
/// needs this too, not just a raw passthrough.
fn render_embed(el: &Element) -> String {
    let raw_target = link_target(el, &classify_lenient(el)).unwrap_or_default();
    let (_, src) = target_scheme(&raw_target);
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

fn render_links_container(cx: &RenderCtx, el: &Element) -> String {
    let mut out = String::new();
    if let Some(children) = el.value.as_ref().map(|v| v.as_children()) {
        for (i, child) in children.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            let id = child.args.as_ref().map(value_to_plain).unwrap_or_default();
            let content = child
                .content
                .as_ref()
                .map(|a| inline_to_md(cx, a))
                .unwrap_or_default();
            out.push_str(&format!("**{id}**: {content}"));
        }
    }
    out
}

/// Anything with no dedicated CommonMark mapping (a hand-authored `<T>`
/// or `@name` element the importer never produces) passes through as raw
/// HTML -- valid CommonMark, and matches `tomet-html`'s own
/// generic div/span fallback in spirit.
fn render_generic(cx: &RenderCtx, el: &Element, kind: &str, inline: bool) -> String {
    let tag = if inline { "span" } else { "div" };
    let mut out = format!("<{tag} data-tm-kind=\"{}\"", escape_attr(kind));
    if let Some(args) = &el.args {
        push_data_attrs(&mut out, args);
    }
    out.push('>');
    if let Some(content) = &el.content {
        out.push_str(&inline_to_md(cx, content));
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
        "a" | "abbr"
            | "address"
            | "article"
            | "aside"
            | "audio"
            | "b"
            | "base"
            | "bdi"
            | "bdo"
            | "blockquote"
            | "body"
            | "br"
            | "button"
            | "canvas"
            | "caption"
            | "cite"
            | "code"
            | "col"
            | "colgroup"
            | "data"
            | "datalist"
            | "dd"
            | "del"
            | "details"
            | "dfn"
            | "dialog"
            | "div"
            | "dl"
            | "dt"
            | "em"
            | "embed"
            | "fieldset"
            | "figcaption"
            | "figure"
            | "footer"
            | "form"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "head"
            | "header"
            | "hgroup"
            | "hr"
            | "html"
            | "i"
            | "iframe"
            | "img"
            | "input"
            | "ins"
            | "kbd"
            | "label"
            | "legend"
            | "li"
            | "link"
            | "main"
            | "map"
            | "mark"
            | "menu"
            | "meta"
            | "meter"
            | "nav"
            | "noscript"
            | "object"
            | "ol"
            | "optgroup"
            | "option"
            | "output"
            | "p"
            | "param"
            | "picture"
            | "pre"
            | "progress"
            | "q"
            | "rp"
            | "rt"
            | "ruby"
            | "s"
            | "samp"
            | "script"
            | "section"
            | "select"
            | "small"
            | "source"
            | "span"
            | "strong"
            | "style"
            | "sub"
            | "summary"
            | "sup"
            | "svg"
            | "table"
            | "tbody"
            | "td"
            | "template"
            | "textarea"
            | "tfoot"
            | "th"
            | "thead"
            | "time"
            | "title"
            | "tr"
            | "track"
            | "u"
            | "ul"
            | "var"
            | "video"
            | "wbr"
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
    use tomet_ast::{Paragraph, Sigil, Span, Text};

    #[test]
    fn adjacent_meta_blocks_have_no_visible_output() {
        // Hand-built `Document` (doesn't go through `tomet_parser::
        // parse_document`, so it's independent of the parser's own
        // paragraph-continuation rules -- `tomet-parser` no longer
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
                sigil: Sigil::block("meta"),
                args: Some(Value::String(tag.to_string())),
                content: None,
                children: None,
                value: Some(ElementValue::from_map(Value::Map(vec![(
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
                Block::Element(heading_element(
                    1,
                    vec![Inline::Text(Text::new("next", Span::dummy()))],
                )),
            ],
            span: Span::dummy(),
        };
        assert_eq!(to_markdown(&doc), "# next\n\n");
    }

    fn heading_element(level: i64, content: Vec<Inline>) -> Element {
        Element {
            sigil: Sigil::block("heading"),
            args: Some(Value::Int(level)),
            content: Some(content),
            children: None,
            value: None,
            span: Span::dummy(),
        }
    }

    #[test]
    fn heading_and_paragraph() {
        let doc = Document {
            blocks: vec![
                Block::Element(heading_element(
                    2,
                    vec![Inline::Text(Text::new("Title", Span::dummy()))],
                )),
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
    fn nested_inline_heading_does_not_export_as_bare_hash_line() {
        // `@heading(2)[...]` nested inside a paragraph (inline position)
        // must not emit a bare `## text` mid-paragraph -- that wouldn't
        // parse back as a heading on re-import anyway. It falls through to
        // generic/custom rendering instead (raw HTML passthrough, per this
        // module's documented fallback for constructs with no CommonMark
        // form).
        let doc = Document {
            blocks: vec![Block::Paragraph(Paragraph::new(
                vec![
                    Inline::Text(Text::new("before ", Span::dummy())),
                    Inline::Element(heading_element(
                        2,
                        vec![Inline::Text(Text::new("Nested", Span::dummy()))],
                    )),
                    Inline::Text(Text::new(" after", Span::dummy())),
                ],
                Span::dummy(),
            ))],
            span: Span::dummy(),
        };
        let md = to_markdown(&doc);
        assert!(!md.contains("## Nested"), "got: {md:?}");
    }

    #[test]
    fn bullet_and_ordered_list() {
        let doc = Document {
            blocks: vec![Block::Element(tomet_tree::element_list(
                true,
                vec![
                    tomet_tree::element_list_item(
                        vec![Inline::Text(Text::new("one", Span::dummy()))],
                        None,
                        None,
                        Vec::new(),
                        Span::dummy(),
                    ),
                    tomet_tree::element_list_item(
                        vec![Inline::Text(Text::new("two", Span::dummy()))],
                        None,
                        None,
                        Vec::new(),
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
                        sigil: Sigil::inline("em"),
                        args: None,
                        content: Some(vec![Inline::Text(Text::new("a", Span::dummy()))]),
                        children: None,
                        value: None,
                        span: Span::dummy(),
                    }),
                    Inline::Text(Text::new(" ", Span::dummy())),
                    Inline::Element(Element {
                        sigil: Sigil::inline("strong"),
                        args: None,
                        content: Some(vec![Inline::Text(Text::new("b", Span::dummy()))]),
                        children: None,
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
            sigil: Sigil::inline("link"),
            args: Some(Value::Map(vec![(
                "target".to_string(),
                Value::String("https://example.com".to_string()),
            )])),
            content: Some(vec![Inline::Text(Text::new("Wiki", Span::dummy()))]),
            children: None,
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
            sigil: Sigil::block("embed"),
            args: Some(Value::Map(vec![(
                "target".to_string(),
                Value::String("pic.png".to_string()),
            )])),
            content: Some(vec![Inline::Text(Text::new("a cat", Span::dummy()))]),
            children: None,
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
            sigil: Sigil::block("codeblock"),
            args: Some(Value::Map(vec![(
                "lang".to_string(),
                Value::String("rust".to_string()),
            )])),
            content: Some(vec![Inline::Text(Text::new("fn main() {}", Span::dummy()))]),
            children: None,
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
            blocks: vec![Block::Element(tomet_tree::element_new(Sigil::block("hr")))],
            span: Span::dummy(),
        };
        assert_eq!(to_markdown(&doc), "---\n\n");
    }

    #[test]
    fn config_element_exports_as_nothing() {
        let mut el = tomet_tree::element_new(Sigil::block("config"));
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
        let mut el = tomet_tree::element_new(Sigil::block("hr"));
        el.content = Some(vec![Inline::Text(Text::new("Title", Span::dummy()))]);
        let doc = Document {
            blocks: vec![Block::Element(el)],
            span: Span::dummy(),
        };
        assert_eq!(to_markdown(&doc), "Title\n---\n\n");
    }

    #[test]
    fn wikilink_exports_to_markdown() {
        let mut el1 = tomet_tree::element_new(Sigil::inline("link"));
        el1.args = Some(Value::Map(vec![(
            "target".to_string(),
            Value::String("ref:name".to_string()),
        )]));

        let mut el2 = tomet_tree::element_new(Sigil::inline("link"));
        el2.args = Some(Value::Map(vec![(
            "target".to_string(),
            Value::String("ref:name".to_string()),
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
        let mut el = tomet_tree::element_new(Sigil::block("table"));
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

    #[test]
    fn macro_and_interp_exports_to_markdown() {
        let doc = tomet_parser::parse_document("#config{\n  macros: {\n    gh: \"https://github.com/tomet/tomet/issues/${1}\"\n    copyright: \"(C) 2026 Tomet\"\n  }\n}\n\nIssue: $gh(42)\nFooter: ${copyright}\nMath: ${add(10, 5)}\n").unwrap();
        assert_eq!(
            to_markdown(&doc),
            "Issue: https://github.com/tomet/tomet/issues/42 Footer: (C) 2026 Tomet Math: 15\n\n"
        );
    }
}
