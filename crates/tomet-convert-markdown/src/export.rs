//! `tomet_ast::Document` -> CommonMark, hand-written: the output space
//! is much smaller than the input space, so there is no crate worth
//! pulling in for it.
//!
//! Constructs with no CommonMark equivalent (`mark`, and any `<T>`/`@`
//! element the importer never produces but a hand-authored `.tmt` file
//! might use, e.g. `@links{}` definition containers or arbitrary typed
//! elements) fall back to raw inline/block HTML passthrough, which is
//! valid CommonMark. Heading `id`/`cssclass` attrs have no CommonMark
//! form and are dropped.

use tomet_ast::{
    Block, Document, Element, ElementValue, Inline, Value,
};
use tomet_semantics::{
    path_target,
    TargetScheme, classify_std_lenient, heading_level, is_directive, link_target, list_items,
    list_ordered, normalized_element_args, target_scheme,
};

/// Renders `doc` as CommonMark.
///
/// Takes no evaluation context. It used to -- `to_markdown_with_context`
/// carried a `DocumentConfig` and an `EvaluationContext` for one purpose,
/// evaluating `${...}`, and no converter does that any more. What a
/// `${...}` means is settled before any of them sees the tree, by
/// `tomet-transform`'s `resolve_interpolations` (reached through
/// `tomet_load::Vault::prepare`), because four converters deciding it
/// independently is how the same document came to produce three different
/// answers.
pub fn to_markdown(doc: &Document) -> String {
    let mut out = String::new();
    for block in &doc.blocks {
        render_block(block, &mut out);
    }
    out
}

fn render_block(block: &Block, out: &mut String) {
    match block {
        Block::Paragraph(p) => {
            // Same reasoning as `tomet-html`'s `render_block`: an
            // all-invisible-element paragraph (e.g. adjacent `@meta(...)`
            // lines with no blank line between them) must not leave a
            // stray blank paragraph behind.
            let text = inline_to_md(&p.content);
            if !text.trim().is_empty() {
                out.push_str(&text);
                out.push_str("\n\n");
            }
        }
        Block::Element(el) if list_ordered(el).is_some() => render_list(el, out),
        Block::Element(el) => {
            let text = element_to_md(el, false);
            if !text.is_empty() {
                out.push_str(&text);
                out.push_str("\n\n");
            }
        }
    }
}

fn render_list(el: &Element, out: &mut String) {
    render_list_with_indent(el, 0, out);
    out.push('\n');
}

fn render_list_with_indent(el: &Element, indent: usize, out: &mut String) {
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
        out.push_str(&inline_to_md(item.content.as_deref().unwrap_or(&[])));
        out.push('\n');
        if let Some(children) = &item.children {
            for child in children {
                if let Block::Element(sub) = child {
                    if list_ordered(sub).is_some() {
                        render_list_with_indent(sub, indent + 1, out);
                    }
                }
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
    let kind = classify_std_lenient(el);
    match kind.as_str() {
        // Directives configure or annotate the document and have no
        // rendering of their own. The list lives in `tomet-semantics` so
        // the three writers cannot drift apart -- which they did, all
        // three missing `settings` and `import`.
        _ if is_directive(&kind) => String::new(),
        // Block-position only, same as CommonMark's own headings and
        // Tomet's own `#[x]` grammar -- a nested/inline `@heading(...)`
        // (`inline == true`) falls through to generic/custom rendering
        // instead, rather than emitting a bare `## text` mid-paragraph
        // (which wouldn't parse back as a heading anyway).
        "heading" if !inline => render_heading(el),
        "hr" => render_hr(el),
        "em" => format!("*{}*", content_to_md(el)),
        "strong" => format!("**{}**", content_to_md(el)),
        "mark" => format!("<mark>{}</mark>", content_to_md(el)),
        "strikeout" => format!("~~{}~~", content_to_md(el)),
        "ruby" => render_ruby(el),
        "codeblock" => render_code_block(el),
        "quote" => render_quote(el, inline),
        "callout" => render_callout(el),
        "table" => render_table(el),
        "link" => render_link(el),
        "file" | "dir" => render_path(el, inline),
        "embed" => render_embed(el),
        "links" => render_links_container(el),
        // Evaluate `${...}` interpolations and macros
        "interp" => render_interp(el),
        _ => render_generic(el, kind.as_str(), inline),
    }
}

fn render_table(el: &Element) -> String {
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

/// A `${...}` that reached here unresolved, written back as it was.
///
/// Reaching here at all means the document was not prepared -- a caller
/// that went straight to this crate rather than through
/// `tomet_load::Vault`. The honest output for that is the source
/// spelling: rendering nothing would hide it, and evaluating it here is
/// what this crate stopped doing.
fn render_interp(el: &Element) -> String {
    match &el.value {
        Some(ElementValue::Interp(expr)) => format!("${{{expr}}}"),
        _ => String::new(),
    }
}

/// `@ruby[漢字](rt:"かんじ")` -- no CommonMark ruby syntax exists, so this
/// falls back to raw inline `<ruby>`/`<rt>` HTML, same as `mark` above.
fn render_ruby(el: &Element) -> String {
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
    format!("<ruby>{}<rt>{rt}</rt></ruby>", content_to_md(el))
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
/// `id`/`cssclass` attrs (`el.value`) have no CommonMark form and are
/// dropped, same as before this was folded into the generic `Element`
/// dispatch (see the module doc).
fn render_heading(el: &Element) -> String {
    let level = heading_level(el).unwrap_or(1) as usize;
    let content = el.content.as_deref().unwrap_or(&[]);
    format!("{} {}", "#".repeat(level), inline_to_md(content))
}

fn render_hr(el: &Element) -> String {
    match &el.content {
        // The blank line is load-bearing. `Title` immediately above `---`
        // is a setext heading in CommonMark, so without it a labelled
        // divider silently exports as an `<h2>` -- the one shape this is
        // trying not to be.
        Some(title) if !title.is_empty() => {
            format!("**{}**\n\n---", inline_to_md(title))
        }
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

/// `>` for a quote standing alone; quotation marks for one inside a
/// sentence.
///
/// CommonMark has no inline quote -- `>` is the only quote construct it
/// has, and it is block-only. The marks are the whole of what an inline
/// quote is in Markdown, so that is what it becomes; a reader importing
/// the result back sees text, which is what any Markdown reader would
/// have seen anyway.
fn render_quote(el: &Element, inline: bool) -> String {
    let text = content_to_md(el);
    if inline {
        return format!("\u{201c}{text}\u{201d}");
    }
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

/// `@link(target:..)` -- the target string's own scheme prefix (see
/// `tomet_semantics::target_scheme`) decides which CommonMark shape it
/// exports as: a same-document anchor link (`id:`), a wikilink (`ref:`),
/// or a plain `[text](target)`/bare-target link (everything else). The
/// scheme prefix itself is stripped before rendering -- it's addressing
/// metadata, not part of the visible target.
/// `@file(x)`/`@dir(x)` -> a code span, which is what these mentions
/// were written as before the elements existed.
///
/// Lossless in the direction that matters here: moving a backticked path
/// onto `@file` changes what `tomet check-links` can see and leaves the
/// exported Markdown byte-identical. The import direction cannot recover
/// it -- a code span in Markdown is a code span, and nothing in it says
/// whether the author meant a path.
fn render_path(el: &Element, inline: bool) -> String {
    let path = path_target(el, &classify_std_lenient(el)).unwrap_or_default();
    let content = match &el.content {
        Some(content) if !content.is_empty() => Some(inline_to_md(content)),
        _ => None,
    };
    if inline {
        // A mention: `[content]` is a label and stands in for the path.
        return format!("`{}`", content.unwrap_or(path));
    }
    // A listing row: the description goes beside the path, not instead
    // of it.
    match content {
        Some(content) => format!("`{path}` {content}"),
        None => format!("`{path}`"),
    }
}

fn render_link(el: &Element) -> String {
    let raw_target = link_target(el, &classify_std_lenient(el)).unwrap_or_default();
    let (scheme, target) = target_scheme(&raw_target);
    let text = match &el.content {
        Some(content) if !content.is_empty() => inline_to_md(content),
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
        TargetScheme::Ref | TargetScheme::Unresolved => {
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
    let raw_target = link_target(el, &classify_std_lenient(el)).unwrap_or_default();
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

fn render_links_container(el: &Element) -> String {
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
                .map(|a| inline_to_md(a))
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

/// Escapes CommonMark's special characters in a run of plain text.
///
/// A backtick-delimited span is the exception: it is passed through
/// verbatim, backticks included. Tomet does not turn `` `x` `` into an
/// element -- the parser keeps the backticks as literal text and only
/// shields the run from further markup (`inline.rs`'s backtick probe) --
/// so by the time it reaches here it looks like ordinary text. Escaping
/// it would turn the author's inline code into a literal ``\`x\``, which
/// is what every `` `spec/` `` in the docs used to export as.
///
/// The rule matches the parser's: an opening backtick pairs with the next
/// backtick on the same line. An unpaired one is escaped as before.
fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '`' {
            if let Some(span) = take_code_span(&mut chars) {
                out.push('`');
                out.push_str(&span);
                continue;
            }
            out.push('\\');
        } else if matches!(c, '\\' | '*' | '_' | '[' | ']') {
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

/// Consumes the rest of a backtick span, closing backtick included.
///
/// `chars` must sit just past the opening backtick. Returns `None` (and
/// leaves `chars` untouched) when no closing backtick follows on the same
/// line, which is the parser's condition for the span not being one.
fn take_code_span(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<String> {
    let mut look = chars.clone();
    let mut span = String::new();
    loop {
        match look.next()? {
            '`' => {
                span.push('`');
                *chars = look;
                return Some(span);
            }
            '\n' | '\r' => return None,
            ch => span.push(ch),
        }
    }
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
            | "quote"
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
        // A call is validation-only (e.g. `:rule`'s `allow:list(...)`)
        // and has no rendered form.
        Value::Call(..) => String::new(),
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
    use tomet_ast::{Paragraph, Placement, Sigil, Span, Text};

    #[test]
    fn a_backtick_span_survives_export_unescaped() {
        // Tomet keeps `` `x` `` as literal text (the parser only shields
        // the run from further markup), so it reaches the writer looking
        // like prose. Escaping it turned every `` `spec/` `` in the docs
        // into a literal ``\`spec/\``.
        let doc =
            tomet_parser::parse_document("地の文に `code` と `spec/` があります。\n").unwrap();
        assert_eq!(
            to_markdown(&doc).trim(),
            "地の文に `code` と `spec/` があります。"
        );
    }

    #[test]
    fn ruby_exports_as_raw_html() {
        let doc = tomet_parser::parse_document("a @ruby[漢字](rt:\"かんじ\") b\n").unwrap();
        assert_eq!(
            to_markdown(&doc).trim(),
            "a <ruby>漢字<rt>かんじ</rt></ruby> b"
        );
    }

    #[test]
    fn an_unpaired_backtick_is_still_escaped() {
        // No closing backtick on the line -- not a span, so it stays an
        // escaped literal, exactly as before.
        let doc = tomet_parser::parse_document("値段は 100` です\n").unwrap();
        assert_eq!(to_markdown(&doc).trim(), "値段は 100\\` です");
    }

    #[test]
    fn a_backtick_span_is_not_escaped_inside_a_table_cell() {
        let doc = tomet_parser::parse_document(
            "@table()[\n[ Dir ][ Lang ]\n[ `spec/` ][ 日本語 ]\n]{}\n",
        )
        .unwrap();
        assert!(
            to_markdown(&doc).contains("| `spec/` |"),
            "got: {}",
            to_markdown(&doc)
        );
    }

    #[test]
    fn directives_have_no_markdown_output() {
        // `settings` and the binding element joined `BUILTIN_KINDS` after
        // this list was written, so they used to fall through to the
        // generic passthrough and emit a `<div data-tm-kind="settings">`.
        //
        // This list is also what caught `@import` splitting into `@use`
        // and `@include`: dropping `import` from `BUILTIN_KINDS` made it
        // a `Custom` kind again, and the div came straight back.
        for src in [
            "@settings(file:project.settings.tmt)\n",
            "@use(deck)\n",
            "@include(./chapter.tmt)\n",
            "@meta{type: note}\n",
        ] {
            let doc = tomet_parser::parse_document(src).unwrap();
            assert_eq!(to_markdown(&doc).trim(), "", "for {src:?}");
        }
    }

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
                sigil: Sigil::named("meta"),
                placement: Placement::Inline,
                args: Some(Value::String(tag.to_string())),
                content: None,
                children: None,
                value: Some(ElementValue::from_map(Value::Map(vec![(
                    "key".to_string(),
                    Value::String("value".to_string()),
                )]))),
                connects: Vec::new(),
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
            sigil: Sigil::named("heading"),
            placement: Placement::Block,
            args: Some(Value::Int(level)),
            content: Some(content),
            children: None,
            value: None,
            connects: Vec::new(),
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
                        sigil: Sigil::named("em"),
                        placement: Placement::Inline,
                        args: None,
                        content: Some(vec![Inline::Text(Text::new("a", Span::dummy()))]),
                        children: None,
                        value: None,
                        connects: Vec::new(),
                        span: Span::dummy(),
                    }),
                    Inline::Text(Text::new(" ", Span::dummy())),
                    Inline::Element(Element {
                        sigil: Sigil::named("strong"),
                        placement: Placement::Inline,
                        args: None,
                        content: Some(vec![Inline::Text(Text::new("b", Span::dummy()))]),
                        children: None,
                        value: None,
                        connects: Vec::new(),
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
            sigil: Sigil::named("link"),
            placement: Placement::Inline,
            args: Some(Value::Map(vec![(
                "target".to_string(),
                Value::String("https://example.com".to_string()),
            )])),
            content: Some(vec![Inline::Text(Text::new("Wiki", Span::dummy()))]),
            children: None,
            value: None,
            connects: Vec::new(),
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
            sigil: Sigil::named("embed"),
            placement: Placement::Inline,
            args: Some(Value::Map(vec![(
                "target".to_string(),
                Value::String("pic.png".to_string()),
            )])),
            content: Some(vec![Inline::Text(Text::new("a cat", Span::dummy()))]),
            children: None,
            value: None,
            connects: Vec::new(),
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
            sigil: Sigil::named("codeblock"),
            placement: Placement::Block,
            args: Some(Value::Map(vec![(
                "lang".to_string(),
                Value::String("rust".to_string()),
            )])),
            content: Some(vec![Inline::Text(Text::new("fn main() {}", Span::dummy()))]),
            children: None,
            value: None,
            connects: Vec::new(),
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
            blocks: vec![Block::Element(tomet_tree::element_new(Sigil::named("hr")))],
            span: Span::dummy(),
        };
        assert_eq!(to_markdown(&doc), "---\n\n");
    }

    #[test]
    fn config_element_exports_as_nothing() {
        let mut el = tomet_tree::element_new(Sigil::named("config"));
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

    /// The pair this asserts is the point: a bold line, a blank line, then
    /// the rule. `Title\n---` -- what this used to produce, and what this
    /// test used to lock in -- is a setext heading, so the divider came out
    /// as an `<h2>`.
    #[test]
    fn titled_thematic_break_is_not_a_setext_heading() {
        let mut el = tomet_tree::element_new(Sigil::named("hr"));
        el.content = Some(vec![Inline::Text(Text::new("Title", Span::dummy()))]);
        let doc = Document {
            blocks: vec![Block::Element(el)],
            span: Span::dummy(),
        };
        assert_eq!(to_markdown(&doc), "**Title**\n\n---\n\n");
    }

    #[test]
    fn wikilink_exports_to_markdown() {
        let mut el1 = tomet_tree::element_new(Sigil::named("link"));
        el1.args = Some(Value::Map(vec![(
            "target".to_string(),
            Value::String("ref:name".to_string()),
        )]));

        let mut el2 = tomet_tree::element_new(Sigil::named("link"));
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
        let mut el = tomet_tree::element_new(Sigil::named("table"));
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

    /// This crate does not evaluate `${...}` any more, and this test is
    /// what is left of the one that said it did.
    ///
    /// It used to assert the expanded macro. That expansion now happens in
    /// `tomet-transform`'s `resolve_interpolations`, before any converter
    /// sees the tree, and is pinned there -- because four converters each
    /// deciding it produced three different answers for the same document.
    ///
    /// What reaches here is a document nobody prepared, and the only
    /// honest rendering of that is what was written.
    #[test]
    fn an_unprepared_interpolation_renders_as_its_own_source() {
        let doc = tomet_parser::parse_document(
            "Issue: $gh(42)\nFooter: ${copyright}\nMath: ${add(10, 5)}\n",
        )
        .unwrap();
        assert_eq!(
            to_markdown(&doc),
            "Issue: ${gh(42)} Footer: ${copyright} Math: ${add(10, 5)}\n\n"
        );
    }
}
