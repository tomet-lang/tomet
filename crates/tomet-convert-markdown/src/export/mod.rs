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

mod escape;
#[cfg(test)]
mod tests;

use escape::{escape_attr, escape_text, fence_for};

use tomet_ast::{
    Block, Document, Element, ElementValue, Inline, List, Placement, Section, Sigil, Value,
};
use tomet_semantics::{
    FootnoteRegistry, TargetScheme, classify_std_lenient, extract_tags, heading_level,
    is_directive, link_target, normalized_element_args, path_target, target_scheme,
};

struct MarkdownCtx<'a> {
    footnotes: &'a FootnoteRegistry,
}

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
    let footnotes = FootnoteRegistry::from_document(doc);
    let cx = MarkdownCtx {
        footnotes: &footnotes,
    };
    let mut out = String::new();
    for block in &doc.blocks {
        render_block(&cx, block, &mut out);
    }
    render_footnotes(&cx, &mut out);
    out
}

fn render_block(cx: &MarkdownCtx, block: &Block, out: &mut String) {
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
        Block::Element(el) => {
            let text = element_to_md(cx, el, false);
            if !text.is_empty() {
                out.push_str(&text);
                out.push_str("\n\n");
            }
        }
        Block::Section(sec) => render_section(cx, sec, out),
        Block::List(list) => render_list(cx, list, out),
    }
}

fn render_section(cx: &MarkdownCtx, sec: &Section, out: &mut String) {
    if !sec.title.is_empty() {
        let marker = "#".repeat(sec.level.clamp(1, 6));
        let text = inline_to_md(cx, &sec.title);
        out.push_str(&marker);
        out.push(' ');
        out.push_str(&text);
        out.push_str("\n\n");
    }
    for child in &sec.blocks {
        render_block(cx, child, out);
    }
}

fn render_list(cx: &MarkdownCtx, list: &List, out: &mut String) {
    render_list_with_indent(cx, list, 0, out);
    out.push('\n');
}

fn render_list_with_indent(cx: &MarkdownCtx, list: &List, indent: usize, out: &mut String) {
    let ordered = list.ordered;
    let indent_str = "  ".repeat(indent);
    for (i, list_item) in list.items.iter().enumerate() {
        let item = &list_item.element;
        out.push_str(&indent_str);
        let marker = if ordered {
            format!("{}. ", i + 1)
        } else {
            "- ".to_string()
        };
        out.push_str(&marker);
        if matches!(item.sigil, Sigil::Named(_)) {
            // The combine notation (`-@name(...)`): no CommonMark list
            // item has an equivalent, so this falls back the same way
            // `element_to_md` already does for any other construct with
            // no CommonMark form (see the module doc).
            out.push_str(&element_to_md(cx, item, false));
        } else {
            // `args` (the `(...)` marker `Value`) has no CommonMark
            // equivalent -- dropped on export, same as this crate's
            // other documented lossy cases (see the module doc).
            out.push_str(&blocks_to_md(cx, item.content.as_deref().unwrap_or(&[])));
        }
        out.push('\n');
        if let Some(sub) = &list_item.sublist {
            render_list_with_indent(cx, sub, indent + 1, out);
        }
    }
}

/// `Element.content` (`Vec<Block>`) to CommonMark. The common case --
/// exactly one plain paragraph, what ordinary inline usage always parses
/// to -- delegates straight to `inline_to_md` on that paragraph's own
/// content; anything else falls back to `render_block` per block.
fn blocks_to_md(cx: &MarkdownCtx, blocks: &[Block]) -> String {
    if let [Block::Paragraph(p)] = blocks {
        return inline_to_md(cx, &p.content);
    }
    let mut out = String::new();
    for block in blocks {
        render_block(cx, block, &mut out);
    }
    out
}

fn inline_to_md(cx: &MarkdownCtx, inlines: &[Inline]) -> String {
    let mut out = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => out.push_str(&escape_text(&t.value)),
            Inline::Raw(t) => out.push_str(&escape_text(&t.value)),
            // A literal newline, not `softbreak_join`'s space/nothing: a
            // softbreak is CommonMark-legal either way (both parse back to
            // the same `Event::SoftBreak`), and a real line break is the
            // one that keeps the output readable rather than merging a
            // whole wrapped paragraph onto one line. It is also what
            // markdown *import* already produces for a source softbreak
            // (`Event::SoftBreak` -> `Inline::SoftBreak` below), so a
            // document round-tripped through this crate keeps its line
            // breaks rather than gaining reflowed ones only on export.
            Inline::SoftBreak(_) => out.push('\n'),
            // CommonMark's hardbreak: a backslash before the newline.
            // Backslash is chosen over the "two trailing spaces" spelling
            // because trailing whitespace is invisible and routinely
            // stripped by editors/tools -- the same reason that spelling
            // was rejected for tomet's own source syntax.
            Inline::LineBreak(_) => out.push_str("\\\n"),
            Inline::Element(el) => out.push_str(&element_to_md(cx, el, true)),
        }
    }
    out
}

fn element_to_md(cx: &MarkdownCtx, el: &Element, inline: bool) -> String {
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
        "heading" if !inline => render_heading(cx, el),
        "hr" => render_hr(cx, el),
        "em" => format!("*{}*", content_to_md(cx, el)),
        "strong" => format!("**{}**", content_to_md(cx, el)),
        "mark" => format!("<mark>{}</mark>", content_to_md(cx, el)),
        "strikeout" => format!("~~{}~~", content_to_md(cx, el)),
        "ruby" => render_ruby(cx, el),
        "raw" => render_raw(el, inline),
        "quote" => render_quote(cx, el, inline),
        "callout" => render_callout(cx, el),
        "conflict" => render_conflict(cx, el, inline),
        "table" => render_table(cx, el),
        "link" => render_link(cx, el),
        "file" | "dir" => render_path(cx, el, inline),
        "embed" => render_embed(el),
        "footnote" => {
            if inline || el.placement == Placement::Inline {
                if let Some((idx, _)) = cx.footnotes.get_ref(&el.span) {
                    format!("[^{idx}]")
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        }
        "caret" => {
            if let Some((idx, _)) = cx.footnotes.get_ref(&el.span) {
                format!("[^{idx}]")
            } else {
                String::new()
            }
        }
        "tag" => render_tag(el),
        // Evaluate `${...}` interpolations and macros
        "interp" => render_interp(el),
        _ => render_generic(cx, el, kind.as_str(), inline),
    }
}

fn render_table(cx: &MarkdownCtx, el: &Element) -> String {
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
fn render_ruby(cx: &MarkdownCtx, el: &Element) -> String {
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
    format!("<ruby>{}<rt>{rt}</rt></ruby>", content_to_md(cx, el))
}

fn content_to_md(cx: &MarkdownCtx, el: &Element) -> String {
    el.content
        .as_ref()
        .map(|a| blocks_to_md(cx, a))
        .unwrap_or_default()
}

/// A bare `---` break exports as-is; a titled one (`---[ Title ]---`) has
/// no CommonMark equivalent, so it's lossy: a bold line followed by a
/// plain rule.
/// `id`/`cssclass` attrs (`el.value`) have no CommonMark form and are
/// dropped, same as before this was folded into the generic `Element`
/// dispatch (see the module doc).
fn render_heading(cx: &MarkdownCtx, el: &Element) -> String {
    let level = heading_level(el).unwrap_or(1) as usize;
    let content = el.content.as_deref().unwrap_or(&[]);
    format!("{} {}", "#".repeat(level), blocks_to_md(cx, content))
}

fn render_hr(cx: &MarkdownCtx, el: &Element) -> String {
    match &el.content {
        // The blank line is load-bearing. `Title` immediately above `---`
        // is a setext heading in CommonMark, so without it a labelled
        // divider silently exports as an `<h2>` -- the one shape this is
        // trying not to be.
        Some(title) if !title.is_empty() => {
            format!("**{}**\n\n---", blocks_to_md(cx, title))
        }
        _ => "---".to_string(),
    }
}

/// `raw`'s `{value}` (`id`/`cssclass` metadata, if present -- see
/// `tomet-html`'s `render_raw_element`) has no CommonMark
/// form, same as a heading's attrs, so it's dropped on export.
fn render_raw(el: &Element, inline: bool) -> String {
    let code = el
        .content
        .as_ref()
        .map(|a| blocks_to_plain(a))
        .unwrap_or_default();
    if inline {
        render_inline_raw(&code)
    } else {
        let lang = el
            .args
            .as_ref()
            .and_then(as_map)
            .and_then(|m| map_get(m, "lang"))
            .map(value_to_plain)
            .unwrap_or_default();
        let fence = fence_for(&code);
        format!("{fence}{lang}\n{code}\n{fence}")
    }
}

fn render_inline_raw(code: &str) -> String {
    if !code.contains('`') {
        return format!("`{code}`");
    }
    let mut run_len = 2;
    while code.contains(&"`".repeat(run_len)) {
        run_len += 1;
    }
    let delim = "`".repeat(run_len);
    let pad = if code.starts_with('`') || code.ends_with('`') {
        " "
    } else {
        ""
    };
    format!("{delim}{pad}{code}{pad}{delim}")
}

/// `>` for a quote standing alone; quotation marks for one inside a
/// sentence.
///
/// CommonMark has no inline quote -- `>` is the only quote construct it
/// has, and it is block-only. The marks are the whole of what an inline
/// quote is in Markdown, so that is what it becomes; a reader importing
/// the result back sees text, which is what any Markdown reader would
/// have seen anyway.
fn render_quote(cx: &MarkdownCtx, el: &Element, inline: bool) -> String {
    let text = content_to_md(cx, el);
    if inline {
        return format!("\u{201c}{text}\u{201d}");
    }
    text.lines()
        .map(|line| format!("> {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_callout(cx: &MarkdownCtx, el: &Element) -> String {
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
                    } else if k == "title"
                        && let Value::String(s) = v
                    {
                        title = Some(s.clone());
                    }
                }
                if variant.is_none()
                    && !entries.is_empty()
                    && let Value::String(s) = &entries[0].1
                {
                    variant = Some(s.clone());
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
/// `@file(x)`/`@dir(x)` -> a code span, which is what these mentions
/// were written as before the elements existed.
///
/// Lossless in the direction that matters here: moving a backticked path
/// onto `@file` changes what `tomet check-links` can see and leaves the
/// exported Markdown byte-identical. The import direction cannot recover
/// it -- a code span in Markdown is a code span, and nothing in it says
/// whether the author meant a path.
fn render_path(cx: &MarkdownCtx, el: &Element, inline: bool) -> String {
    let path = path_target(el, &classify_std_lenient(el)).unwrap_or_default();
    let content = match &el.content {
        Some(content) if !content.is_empty() => Some(blocks_to_md(cx, content)),
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

fn render_link(cx: &MarkdownCtx, el: &Element) -> String {
    let raw_target = link_target(el, &classify_std_lenient(el)).unwrap_or_default();
    let (scheme, target) = target_scheme(&raw_target);
    let text = match &el.content {
        Some(content) if !content.is_empty() => blocks_to_md(cx, content),
        _ => String::new(),
    };
    match scheme {
        TargetScheme::Id => {
            let text = if text.is_empty() {
                target.to_string()
            } else {
                text
            };
            format!("[{text}](#{target})")
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
        .map(|a| blocks_to_plain(a))
        .unwrap_or_default();
    format!("![{alt}]({src})")
}

/// [`inlines_to_plain`] over `Element.content`'s `Vec<Block>` shape.
fn blocks_to_plain(blocks: &[Block]) -> String {
    let mut s = String::new();
    for block in blocks {
        match block {
            Block::Paragraph(p) => s.push_str(&inlines_to_plain(&p.content)),
            Block::Element(el) => {
                if let Some(content) = &el.content {
                    s.push_str(&blocks_to_plain(content));
                }
            }
            Block::Section(sec) => {
                s.push_str(&inlines_to_plain(&sec.title));
                s.push_str(&blocks_to_plain(&sec.blocks));
            }
            Block::List(list) => {
                for item in &list.items {
                    if let Some(content) = &item.element.content {
                        s.push_str(&blocks_to_plain(content));
                    }
                }
            }
        }
    }
    s
}

fn inlines_to_plain(inlines: &[Inline]) -> String {
    let mut s = String::new();
    for (idx, inline) in inlines.iter().enumerate() {
        match inline {
            Inline::Text(t) => s.push_str(&t.value),
            Inline::Raw(t) => s.push_str(&t.value),
            Inline::SoftBreak(_) => {
                let before = s.chars().last();
                let after = inlines.get(idx + 1).and_then(Inline::first_char);
                s.push_str(tomet_ast::softbreak_join(before, after));
            }
            Inline::LineBreak(_) => s.push(' '),
            Inline::Element(el) => {
                if let Some(content) = &el.content {
                    s.push_str(&blocks_to_plain(content));
                }
            }
        }
    }
    s
}

fn render_tag(el: &Element) -> String {
    let tags = extract_tags(el);
    if tags.is_empty() {
        return String::new();
    }
    let formatted: Vec<String> = tags
        .into_iter()
        .map(|t| {
            if t.starts_with('#') {
                t
            } else {
                format!("#{t}")
            }
        })
        .collect();
    formatted.join(" ")
}

fn render_footnotes(cx: &MarkdownCtx, out: &mut String) {
    if cx.footnotes.items.is_empty() {
        return;
    }
    for item in &cx.footnotes.items {
        out.push_str(&format!("[^{}]: ", item.index));
        let mut def_text = String::new();
        if let Some(def_el) = &item.definition
            && let Some(content) = &def_el.content
        {
            def_text.push_str(&blocks_to_md(cx, content));
        }
        out.push_str(&def_text);
        out.push_str("\n\n");
    }
}

/// Anything with no dedicated CommonMark mapping (a hand-authored `<T>`
/// or `@name` element the importer never produces) passes through as raw
/// HTML -- valid CommonMark, and matches `tomet-html`'s own
/// generic div/span fallback in spirit.
fn render_generic(cx: &MarkdownCtx, el: &Element, kind: &str, inline: bool) -> String {
    let tag = if inline { "span" } else { "div" };
    let mut out = format!("<{tag} data-tm-kind=\"{}\"", escape_attr(kind));
    if let Some(args) = &el.args {
        push_data_attrs(&mut out, args);
    }
    out.push('>');
    if let Some(content) = &el.content {
        out.push_str(&blocks_to_md(cx, content));
    }
    out.push_str(&format!("</{tag}>"));
    out
}

/// `@conflict(a: ..., b: ...)` -- raw HTML passthrough, same spirit as
/// `render_generic`, but unlike that fallback both sides are rendered as
/// real nested content rather than flattened into a lossy `data-*`
/// attribute (which is what `a`/`b`'s block content would otherwise
/// collapse to). Showing both, marked, rather than silently picking one:
/// discarding a side here would make `tomet check`'s own
/// `Diagnostic::Conflict` warning pointless the moment someone exports.
fn render_conflict(cx: &MarkdownCtx, el: &Element, inline: bool) -> String {
    let (a, b) = conflict_sides(el);
    let tag = if inline { "span" } else { "div" };
    format!(
        "<{tag} class=\"tm-element tm-conflict\"><div class=\"tm-conflict-a\">{}</div>\
         <div class=\"tm-conflict-b\">{}</div></{tag}>",
        blocks_to_md(cx, &a),
        blocks_to_md(cx, &b),
    )
}

/// Reads `a`/`b` out of `el`'s `(args)` -- block content (`Value::Blocks`),
/// not a scalar, so it cannot go through `push_data_attrs`/the generic
/// flatten path at all.
fn conflict_sides(el: &Element) -> (Vec<Block>, Vec<Block>) {
    let args = normalized_element_args(el);
    let map = args.as_ref().and_then(as_map);
    let side = |key: &str| -> Vec<Block> {
        match map.and_then(|m| map_get(m, key)) {
            Some(Value::Blocks(blocks)) => blocks.clone(),
            _ => Vec::new(),
        }
    };
    (side("a"), side("b"))
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
        // An embedded element has no scalar form either -- same as `Map`.
        Value::Element(_) => String::new(),
        // Block content has no scalar form either -- same as `Map`.
        Value::Blocks(_) => String::new(),
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
