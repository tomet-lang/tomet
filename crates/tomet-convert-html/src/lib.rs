//! Renders a parsed Tomet [`Document`] to HTML.
//!
//! This is a generic, data-driven mapping (not a full semantic engine):
//! most `<T>`/`@name` elements become a `<div>`/`<span>` carrying their
//! `args` map as `data-*` attributes, with `content` as inner content. A
//! handful of element kinds get special handling because the spec gives
//! them fixed meaning: `@link(target:..)` becomes a link -- an `<a href>`
//! for most target schemes, or a same-document anchor reference (`<a
//! href="#link-...">`) when the target's scheme is `id:` -- `@meta`
//! carries no visible content, and `@links{}` containers render their bare
//! children as a definition list of anchors.

use tomet_ast::{
    Block, Document, Element, ElementValue, Inline, InterpExpr, InterpExprKind, Literal, Value,
};
use tomet_semantics::{
    EXACT_DATA_KEY, ElementKind, TargetScheme, classify_std_lenient, flatten_data, heading_level,
    is_directive, link_target, list_items, list_ordered, normalized_element_args, target_scheme,
};

const DEFAULT_STYLE: &str = "\
body { font-family: sans-serif; line-height: 1.6; max-width: 48rem; margin: 2rem auto; padding: 0 1rem; }
.tmt-element { border-left: 2px solid #ccc; padding-left: 0.5rem; }
.tmt-caution { border-left-color: #d9822b; background: #fff8ee; padding: 0.5rem; }
.tmt-links dt { font-weight: bold; }
.tmt-value { color: #666; font-family: monospace; }
.tmt-hr-titled { display: flex; align-items: center; gap: 0.75rem; margin: 1.5rem 0; }
.tmt-hr-titled hr { flex: 1; margin: 0; }
.tmt-heading-number { color: #888; margin-right: 0.5em; }
";

/// Output shape choices for [`render_page_with`]/[`render_body_with`]. The
/// zero-value (`Default`) is the plain, unnumbered rendering `render_page`/
/// `render_body` already produced before this existed -- opting into
/// anything here is always an explicit choice, never a behavior change for
/// existing callers of those two functions.
#[derive(Debug, Clone, Default)]
pub struct RenderOptions {
    /// Number headings by nesting level (`1`, `1.1`, `1.2`, `2`, ...),
    /// restarting a deeper level's counter whenever a shallower one
    /// advances -- the "advanced" output pattern, as opposed to the plain
    /// default.
    pub number_headings: bool,
    /// Give every heading with no explicit `{id:...}` an `id` slugified
    /// from its text (lowercased, runs of whitespace/punctuation collapsed
    /// to `-`), the way Markdown processors generate anchors -- duplicate
    /// slugs within one document get `-2`/`-3`/... suffixes. An explicit
    /// `{id:...}` always wins and is never touched.
    pub auto_slug_headings: bool,
    /// `<html lang="...">` for [`render_page`]/[`render_page_with`]'s page
    /// shell. Ignored by [`render_body`]/[`render_body_with`], which never
    /// emit an `<html>` tag at all. `None` keeps the historical `"ja"`
    /// default so nothing changes for existing callers; `Some(lang)`
    /// overrides it (e.g. `Some("en".to_string())`).
    pub lang: Option<String>,
}

/// Render a full standalone HTML document with the plain (unnumbered)
/// heading style. See [`render_page_with`] for the numbered variant.
pub fn render_page(doc: &Document, title: &str) -> String {
    render_page_with(doc, title, &RenderOptions::default())
}

/// Render a full standalone HTML document, applying `options` (e.g.
/// `RenderOptions { number_headings: true }` for the "advanced",
/// sequentially-numbered heading style).
pub fn render_page_with(doc: &Document, title: &str, options: &RenderOptions) -> String {
    let lang = options.lang.as_deref().unwrap_or("ja");
    format!(
        "<!DOCTYPE html>\n<html lang=\"{lang}\">\n<head>\n<meta charset=\"utf-8\">\n<title>{title}</title>\n<style>{style}</style>\n</head>\n<body>\n{body}</body>\n</html>\n",
        lang = escape_attr(lang),
        title = escape_html(title),
        style = DEFAULT_STYLE,
        body = render_body_with(doc, options),
    )
}

/// Render just the body content, without the surrounding `<html>` shell,
/// with the plain (unnumbered) heading style.
pub fn render_body(doc: &Document) -> String {
    render_body_with(doc, &RenderOptions::default())
}

/// Render just the body content, applying `options`.
pub fn render_body_with(doc: &Document, options: &RenderOptions) -> String {
    let mut out = String::new();
    let mut state = HeadingState::default();
    let config = tomet_semantics::document_config(doc);
    let cx = RenderCtx {
        doc,
        config,
        options,
    };
    for block in &doc.blocks {
        render_block(&cx, block, &mut out, &mut state);
    }
    out
}

struct RenderCtx<'a> {
    doc: &'a Document,
    config: tomet_semantics::DocumentConfig,
    options: &'a RenderOptions,
}

/// Per-document state threaded through heading rendering: the nesting
/// counters for `RenderOptions::number_headings` and the slug registry for
/// `RenderOptions::auto_slug_headings`, kept together since both are
/// "remember what came before while walking the same heading sequence".
#[derive(Default)]
struct HeadingState {
    counters: HeadingCounters,
    slugs: SlugTracker,
}

/// Running per-level counters for `RenderOptions::number_headings`, e.g.
/// `[1, 2]` mid-document means "currently under section 1.2". Index `i`
/// holds the count for heading level `i + 1`.
#[derive(Default)]
struct HeadingCounters(Vec<u32>);

impl HeadingCounters {
    /// Advances to the next heading at `level`, resetting any deeper
    /// levels' counters (a new "1.2" starts a fresh "1.2.1" for whatever
    /// level-3 heading comes next), and returns the dotted label (`"1.2"`).
    fn advance(&mut self, level: u8) -> String {
        let level = level.clamp(1, 6) as usize;
        if self.0.len() < level {
            self.0.resize(level, 0);
        } else {
            self.0.truncate(level);
        }
        self.0[level - 1] += 1;
        self.0
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(".")
    }
}

/// Hands out unique slugs for `RenderOptions::auto_slug_headings`: the
/// first heading with a given base text keeps the bare slug, each further
/// one with the same base gets `-2`, `-3`, ... appended. Only tracks slugs
/// it generated itself -- an explicit `{id:...}` elsewhere in the document
/// is never consulted or reserved, matching the "explicit id always wins,
/// auto-slugging only fills gaps" rule in `RenderOptions::auto_slug_headings`.
#[derive(Default)]
struct SlugTracker(std::collections::HashMap<String, u32>);

impl SlugTracker {
    fn slug_for(&mut self, text: &str) -> String {
        let base = slugify(text);
        let count = self.0.entry(base.clone()).or_insert(0);
        *count += 1;
        if *count == 1 {
            base
        } else {
            format!("{base}-{count}")
        }
    }
}

/// Lowercases and collapses runs of whitespace/punctuation into a single
/// `-`, trimming leading/trailing `-`. Keeps non-ASCII letters (e.g.
/// Japanese kanji/kana) as-is rather than stripping them -- most of this
/// project's own docs are Japanese, so an ASCII-only slugifier would
/// produce empty or near-empty ids for them.
fn slugify(text: &str) -> String {
    let mut slug = String::with_capacity(text.len());
    let mut pending_dash = false;
    for c in text.chars() {
        if c.is_alphanumeric() {
            if pending_dash && !slug.is_empty() {
                slug.push('-');
            }
            pending_dash = false;
            slug.extend(c.to_lowercase());
        } else {
            pending_dash = true;
        }
    }
    slug
}

fn render_block(cx: &RenderCtx, block: &Block, out: &mut String, state: &mut HeadingState) {
    match block {
        Block::Paragraph(p) => {
            // Elements with no visible output (`@meta`, ...) placed on
            // adjacent lines with no blank line between them lazily
            // continue into one paragraph together with the inter-element
            // whitespace text (see `document.rs::parse_paragraph`) -- if
            // every one of them is invisible, skip the wrapper entirely
            // rather than emitting a stray whitespace-only `<p>`.
            let mut inner = String::new();
            render_inlines(cx, &p.content, &mut inner);
            if !inner.trim().is_empty() {
                out.push_str("<p>");
                out.push_str(&inner);
                out.push_str("</p>\n");
            }
        }
        Block::Element(el) if list_ordered(el).is_some() => render_list(cx, el, out),
        Block::Element(el) if classify_std_lenient(el) == ElementKind::Heading => {
            render_heading_element(cx, el, out, state)
        }
        Block::Element(el) => render_element(cx, el, out, false),
    }
}

/// Called only from `render_block`'s top-level dispatch, never from
/// `render_element`'s recursive `kind.as_str()` match -- a nested/inline
/// `@heading(...)` (e.g. inside a blockquote's content, or `${...}`-free
/// prose) falls through to `render_generic_element` instead, since
/// CommonMark/Tomet headings are both block-position-only by grammar.
fn render_heading_element(
    cx: &RenderCtx,
    el: &Element,
    out: &mut String,
    state: &mut HeadingState,
) {
    let level = heading_level(el).unwrap_or(1);
    let content = el.content.as_deref().unwrap_or(&[]);
    let value_data = match &el.value {
        Some(v) => v.as_data(),
        _ => None,
    };
    let (mut id, class, data) = split_attrs(value_data.as_ref());
    if id.is_none() && cx.options.auto_slug_headings {
        let text = inlines_to_plain(content);
        let slug = state.slugs.slug_for(&text);
        if !slug.is_empty() {
            id = Some(slug);
        }
    }
    out.push_str(&format!("<h{level}"));
    push_named_attrs(out, &id, &class, &data);
    out.push('>');
    if cx.options.number_headings {
        let label = state.counters.advance(level);
        // No literal space after `</span>` -- spacing is `.tmt-heading-number`'s
        // `margin-right` in `DEFAULT_STYLE`, not baked into the content, so
        // e.g. copy-pasting the heading text doesn't pick up a stray space.
        out.push_str(&format!("<span class=\"tm-heading-number\">{label}</span>"));
    }
    render_inlines(cx, content, out);
    out.push_str(&format!("</h{level}>\n"));
}

fn render_list(cx: &RenderCtx, el: &Element, out: &mut String) {
    let tag = if list_ordered(el) == Some(true) {
        "ol"
    } else {
        "ul"
    };
    out.push_str(&format!("<{tag}>\n"));
    for item in list_items(el) {
        let attrs = match &item.value {
            Some(v) => v.as_data(),
            _ => None,
        };
        let (id, class, data) = split_attrs(attrs.as_ref());
        out.push_str("<li");
        push_named_attrs(out, &id, &class, &data);
        out.push('>');
        if let Some(marker) = &item.args {
            out.push_str("<span class=\"tm-list-marker\"");
            match marker {
                Value::Map(map) => {
                    for (k, v) in map {
                        out.push_str(&format!(
                            " data-{}=\"{}\"",
                            escape_attr(k),
                            escape_attr(&value_to_plain(v))
                        ));
                    }
                }
                other => out.push_str(&format!(
                    " data-marker=\"{}\"",
                    escape_attr(&value_to_plain(other))
                )),
            }
            out.push('>');
            let text = value_to_plain(marker);
            if !text.is_empty() {
                out.push_str(&format!("[{}]", escape_html(&text)));
            }
            out.push_str("</span> ");
        }
        render_inlines(cx, item.content.as_deref().unwrap_or(&[]), out);
        if let Some(children) = &item.children {
            out.push('\n');
            for child in children {
                if let Block::Element(sub) = child {
                    if list_ordered(sub).is_some() {
                        render_list(cx, sub, out);
                    }
                }
            }
        }
        out.push_str("</li>\n");
    }
    out.push_str(&format!("</{tag}>\n"));
}

fn render_inlines(cx: &RenderCtx, inlines: &[Inline], out: &mut String) {
    for inline in inlines {
        match inline {
            Inline::Text(t) => out.push_str(&escape_html(&t.value)),
            Inline::Element(el) => render_element(cx, el, out, true),
        }
    }
}

fn render_element(cx: &RenderCtx, el: &Element, out: &mut String, inline: bool) {
    let kind = classify_std_lenient(el);
    match kind.as_str() {
        // Directives -- see `tomet_semantics::is_directive`.
        _ if is_directive(&kind) => {}
        "interp" => {
            if let Some(value) = &el.value {
                render_element_value(cx, value, out);
            }
        }
        "links" => render_links_container(cx, el, out),
        "link" => render_link_element(cx, el, out, inline),
        "embed" => render_embed_element(el, out),
        "icon" => render_icon_element(cx, el, out, inline),
        "hr" => render_hr_element(cx, el, out),
        "em" | "strong" | "mark" => render_wrapped_inline(cx, el, kind.as_str(), out),
        "codeblock" => render_codeblock_element(el, out),
        "blockquote" => render_blockquote_element(cx, el, out, inline),
        "table" => render_table_element(cx, el, out),
        _ => render_generic_element(cx, el, kind.as_str(), out, inline),
    }
}

fn render_table_element(cx: &RenderCtx, el: &Element, out: &mut String) {
    let inlines = match &el.content {
        Some(content) => content,
        None => {
            out.push_str("<table class=\"tm-element tm-table\"></table>\n");
            return;
        }
    };
    let rows = tomet_semantics::parse_table_rows(inlines);
    if rows.is_empty() {
        out.push_str("<table class=\"tm-element tm-table\"></table>\n");
        return;
    }

    let args = normalized_element_args(el);
    let has_header = args
        .as_ref()
        .and_then(as_map)
        .and_then(|m| map_get(m, "header"))
        .map(|v| match v {
            Value::Bool(b) => *b,
            _ => true,
        })
        .unwrap_or(true);

    let attrs = match &el.value {
        Some(v) => v.as_data(),
        _ => None,
    };
    let (id, class, data) = split_attrs(attrs.as_ref());
    let class = class.or_else(|| Some("tm-element tm-table".to_string()));

    out.push_str("<table");
    push_named_attrs(out, &id, &class, &data);
    out.push_str(">\n");

    let (header_row, body_rows) = if has_header && !rows.is_empty() {
        (Some(&rows[0]), &rows[1..])
    } else {
        (None, &rows[..])
    };

    if let Some(hrow) = header_row {
        out.push_str("<thead>\n<tr>\n");
        for cell in &hrow.cells {
            out.push_str("<th>");
            render_inlines(cx, &cell.content, out);
            out.push_str("</th>\n");
        }
        out.push_str("</tr>\n</thead>\n");
    }

    if !body_rows.is_empty() {
        out.push_str("<tbody>\n");
        for row in body_rows {
            out.push_str("<tr>\n");
            for cell in &row.cells {
                out.push_str("<td>");
                render_inlines(cx, &cell.content, out);
                out.push_str("</td>\n");
            }
            out.push_str("</tr>\n");
        }
        out.push_str("</tbody>\n");
    }

    out.push_str("</table>\n");
}

/// A bare `---` break is a plain `<hr>`; a titled one (`---[ Title ]---`,
/// `document.rs::parse_titled_thematic_break`'s `content`) wraps two `<hr>`s
/// around the title, visually reproducing the source's symmetric
/// dashes-title-dashes shape (styled via `.tmt-hr-titled` in `DEFAULT_STYLE`).
fn render_hr_element(cx: &RenderCtx, el: &Element, out: &mut String) {
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
fn render_wrapped_inline(cx: &RenderCtx, el: &Element, tag: &str, out: &mut String) {
    out.push_str(&format!("<{tag}>"));
    if let Some(content) = &el.content {
        render_inlines(cx, content, out);
    }
    out.push_str(&format!("</{tag}>"));
}

/// `@codeblock(lang:xxx)[code]` -- the Markdown importer's mapping for
/// fenced (and indented) code blocks, since `tomet_ast` has no
/// dedicated code-block variant. `lang`
/// is a display-only syntax-highlighting hint, never a parse-mode switch
/// (unrelated to the generic `format` key other elements use for their
/// `{value}`). Code lives in `[content]`, parsed as raw verbatim text (see
/// `document.rs::parse_raw_content`) rather than the usual inline grammar, so
/// real source containing `*`/`<`/`@`/backticks stays literal. `{value}`,
/// if present, is `id`/`cssclass` metadata -- same convention as a
/// heading's `{ id:x, cssclass:y }`, not code content.
fn render_codeblock_element(el: &Element, out: &mut String) {
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

/// `<blockquote>[...]` -- the Markdown importer's mapping for block
/// quotes. Only single-block quotes round-trip cleanly; multi-block
/// quotes are already flattened into one inline run on import.
fn render_blockquote_element(cx: &RenderCtx, el: &Element, out: &mut String, inline: bool) {
    out.push_str("<blockquote>");
    if let Some(content) = &el.content {
        render_inlines(cx, content, out);
    }
    out.push_str("</blockquote>");
    if !inline {
        out.push('\n');
    }
}

/// Strips `target`'s scheme prefix the same way `render_link_element`
/// does -- e.g. `<embed>(ref:name)` (recovered from the parser's
/// `identifier:` key split, see `normalized_element_args`) must render
/// `src="name"`, not the literal `"ref:name"`. Under the old per-scheme-key
/// design this stripping was implicit (the scheme was a separate key,
/// never part of the value string); now that both `@link` and `<embed>`
/// share one `target` key with the scheme embedded in the string, `<embed>`
/// needs the same treatment `@link` gets, not just a raw passthrough.
fn render_embed_element(el: &Element, out: &mut String) {
    let raw_target = link_target(el, &classify_std_lenient(el)).unwrap_or_default();
    let (_, src) = target_scheme(&raw_target);
    let alt = el
        .content
        .as_ref()
        .map(|a| inlines_to_plain(a))
        .unwrap_or_default();
    out.push_str(&format!(
        "<img src=\"{}\" alt=\"{}\">\n",
        escape_attr(src),
        escape_attr(&alt)
    ));
}

/// Flattens inline content to plain text -- used for the `alt` attribute,
/// which can't itself carry markup.
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

/// `@link(target:..)` -- the target string's own scheme prefix (see
/// `tomet_semantics::target_scheme`) decides whether this is a
/// same-document anchor reference (`id:`) or a real `href` (everything
/// else: url/file/tm/ref). The scheme prefix itself is stripped before
/// rendering -- it's addressing metadata, not part of the visible target.
fn render_link_element(cx: &RenderCtx, el: &Element, out: &mut String, inline: bool) {
    let normalized_args = normalized_element_args(el);
    let raw_target = link_target(el, &classify_std_lenient(el)).unwrap_or_default();
    let (scheme, target) = target_scheme(&raw_target);
    let target = target.to_string();

    let (class, href) = match scheme {
        TargetScheme::Id => (
            "tm-id".to_string(),
            format!("#link-{}", escape_attr(&target)),
        ),
        other => (format!("tm-{}", other.as_str()), escape_attr(&target)),
    };
    out.push_str(&format!("<a class=\"{class}\" href=\"{href}\""));
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

fn render_content_or_fallback(cx: &RenderCtx, el: &Element, fallback: &str, out: &mut String) {
    match &el.content {
        Some(content) if !content.is_empty() => render_inlines(cx, content, out),
        _ => out.push_str(&escape_html(fallback)),
    }
}

fn render_icon_element(cx: &RenderCtx, el: &Element, out: &mut String, inline: bool) {
    let tag = if inline { "span" } else { "div" };
    let name = match &el.args {
        Some(Value::String(s)) => s.as_str(),
        Some(Value::Map(entries)) => entries
            .iter()
            .find(|(k, _)| k == "name")
            .and_then(|(_, v)| match v {
                Value::String(s) => Some(s.as_str()),
                _ => None,
            })
            .unwrap_or(""),
        _ => "",
    };
    let pkg = match &el.args {
        Some(Value::Map(entries)) => entries
            .iter()
            .find(|(k, _)| k == "pkg" || k == "package")
            .and_then(|(_, v)| match v {
                Value::String(s) => Some(s.as_str()),
                _ => None,
            })
            .unwrap_or(""),
        _ => "",
    };

    let mut class_str = format!("tm-element tm-icon");
    if !name.is_empty() {
        class_str.push_str(&format!(" tm-icon-{name}"));
    }
    if !pkg.is_empty() {
        class_str.push_str(&format!(" tm-icon-{pkg}"));
        if !name.is_empty() {
            class_str.push_str(&format!(" tm-icon-{pkg}-{name}"));
        }
    }

    let value_data = match &el.value {
        Some(v) => v.as_data(),
        _ => None,
    };
    let (id, custom_class, data) = split_attrs(value_data.as_ref());
    if let Some(c) = custom_class {
        class_str.push(' ');
        class_str.push_str(&c);
    }

    out.push_str(&format!("<{tag} class=\"{class_str}\""));
    if let Some(id_str) = id {
        out.push_str(&format!(" id=\"{}\"", escape_attr(&id_str)));
    }
    if !name.is_empty() {
        out.push_str(&format!(" data-icon=\"{}\"", escape_attr(name)));
    }
    if !pkg.is_empty() {
        out.push_str(&format!(" data-pkg=\"{}\"", escape_attr(pkg)));
    }
    for (k, v) in data {
        out.push_str(&format!(
            " data-{}=\"{}\"",
            escape_attr(&k),
            escape_attr(&v)
        ));
    }
    push_data_attrs(out, el.args.as_ref(), &["name", "pkg", "package"]);
    out.push('>');
    if let Some(content) = &el.content {
        render_inlines(cx, content, out);
    }
    out.push_str(&format!("</{tag}>"));
    if !inline {
        out.push('\n');
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
        // A group can hold both halves at once, so both are rendered:
        // the pairs as a value span, the elements as children.
        ElementValue::Group(_) => {
            if let Some(data) = value.as_data() {
                let text = value_to_plain(&data);
                if !text.is_empty() {
                    out.push_str("<span class=\"tm-value\">");
                    out.push_str(&escape_html(&text));
                    out.push_str("</span>");
                }
            }
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
        ElementValue::Interp(expr) => {
            let text = match tomet_compute::evaluate_with_config(cx.doc, expr, &cx.config) {
                Ok(val) => match val {
                    Value::String(s) => s,
                    Value::Int(i) => i.to_string(),
                    Value::Float(f) => f.to_string(),
                    Value::Bool(b) => b.to_string(),
                    Value::Null => String::new(),
                    other => format!("{other:?}"),
                },
                Err(_) => render_interp_expr(expr),
            };
            out.push_str("<span class=\"tm-interp\">");
            out.push_str(&escape_html(&text));
            out.push_str("</span>");
        }
    }
}

/// Re-renders an `InterpExpr` back to `${...}`-shaped source text --
/// evaluation (resolving an `Identifier`/`Member`, calling a `Call`)
/// isn't implemented yet, so this is display-only, same treatment an
/// unrecognized element gets. Not shared via `tomet-ast`: rendering
/// back to text is each consumer's own job here, same as
/// `render_value_inner`/`value_to_plain` already are for `Value`.
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

fn render_links_container(cx: &RenderCtx, el: &Element, out: &mut String) {
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

/// Writes an element's arguments as `data-*` attributes.
///
/// Follows the shared projection rule (`tomet_semantics::flatten`): a
/// scalar or a sequence of scalars gets its own readable attribute, and
/// when that projection would lose something -- a nested map -- the whole
/// group is added as JSON under `data-tomet-data`.
///
/// Before that, a nested map rendered as `data-m=""`: the key survived
/// and its contents did not.
fn push_data_attrs(out: &mut String, args: Option<&Value>, skip: &[&str]) {
    let Some(args) = args else {
        return;
    };
    let data = flatten_data(Some(args), None);

    match args {
        Value::Map(_) => {
            for (k, v) in &data.pairs {
                if skip.contains(&k.as_str()) {
                    continue;
                }
                out.push_str(&format!(" data-{}=\"{}\"", escape_attr(k), escape_attr(v)));
            }
        }
        // A positional group (`@x(1)`) has no key to project onto, so
        // the shared rule hands it back separately.
        _ => {
            if let Some(text) = data.positional.as_ref().filter(|t| !t.is_empty()) {
                out.push_str(&format!(" data-value=\"{}\"", escape_attr(text)));
            }
        }
    }

    if let Some(exact) = data.exact {
        out.push_str(&format!(
            " data-{}=\"{}\"",
            EXACT_DATA_KEY,
            escape_attr(&exact)
        ));
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
    use tomet_parser::parse_document;

    #[test]
    fn a_nested_map_survives_as_an_exact_copy() {
        // It used to render as `data-m=""` -- key kept, contents gone.
        let doc = parse_document("@deck.x(a: 1, m: { k: v })[ 本文 ]\n").unwrap();
        let html = render_body(&doc);
        assert!(html.contains(r#"data-a="1""#), "got {html}");
        assert!(
            !html.contains(r#"data-m="""#),
            "the empty attr is gone: {html}"
        );
        assert!(html.contains("data-tomet-data="), "got {html}");
        assert!(html.contains("&quot;m&quot;"), "got {html}");
    }

    #[test]
    fn flat_args_need_no_exact_copy() {
        let doc = parse_document("@deck.x(a: 1, tags: [p, q])[ 本文 ]\n").unwrap();
        let html = render_body(&doc);
        assert!(html.contains(r#"data-tags="p, q""#), "got {html}");
        assert!(!html.contains("data-tomet-data="), "got {html}");
    }

    #[test]
    fn renders_heading_with_id_and_cssclass() {
        let doc = parse_document("#[ Hello ]{ id:header1, cssclass:card }\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(body, "<h1 id=\"header1\" class=\"card\">Hello</h1>\n");
    }

    #[test]
    fn nested_inline_heading_falls_back_to_generic_rendering() {
        // `@heading(2)[...]` outside block-top-level position (nested
        // inside a blockquote's content here) never renders as a real
        // An element nested inside another element's content falls to
        // `render_element`'s generic rendering, keyed on its kind name.
        // (This used to be demonstrated with a nested `@heading`; headings
        // are block-shaped now, so an inline one is not expressible.)
        let doc = parse_document("@blockquote[ @deck.badge(2)[Nested] ]\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(
            body,
            "<blockquote><span class=\"tm-element tm-deck.badge\" data-value=\"2\">Nested</span></blockquote>\n"
        );
    }

    #[test]
    fn default_options_leave_headings_unnumbered() {
        let doc = parse_document("#[ One ]\n").unwrap();
        assert_eq!(
            render_body_with(&doc, &RenderOptions::default()),
            render_body(&doc),
        );
        assert_eq!(render_body(&doc), "<h1>One</h1>\n");
    }

    #[test]
    fn auto_slug_headings_generates_ids_from_text() {
        let doc = parse_document("#[ Hello World ]\n").unwrap();
        let body = render_body_with(
            &doc,
            &RenderOptions {
                auto_slug_headings: true,
                ..RenderOptions::default()
            },
        );
        assert_eq!(body, "<h1 id=\"hello-world\">Hello World</h1>\n");
    }

    #[test]
    fn auto_slug_headings_keeps_non_ascii_letters() {
        let doc = parse_document("#[ 見出し テスト ]\n").unwrap();
        let body = render_body_with(
            &doc,
            &RenderOptions {
                auto_slug_headings: true,
                ..RenderOptions::default()
            },
        );
        assert_eq!(body, "<h1 id=\"見出し-テスト\">見出し テスト</h1>\n");
    }

    #[test]
    fn auto_slug_headings_disambiguates_duplicates() {
        let doc = parse_document("#[ Intro ]\n##[ Intro ]\n##[ Intro ]\n").unwrap();
        let body = render_body_with(
            &doc,
            &RenderOptions {
                auto_slug_headings: true,
                ..RenderOptions::default()
            },
        );
        assert_eq!(
            body,
            "<h1 id=\"intro\">Intro</h1>\n\
             <h2 id=\"intro-2\">Intro</h2>\n\
             <h2 id=\"intro-3\">Intro</h2>\n"
        );
    }

    #[test]
    fn auto_slug_headings_never_overrides_an_explicit_id() {
        let doc = parse_document("#[ Hello World ]{ id:custom }\n").unwrap();
        let body = render_body_with(
            &doc,
            &RenderOptions {
                auto_slug_headings: true,
                ..RenderOptions::default()
            },
        );
        assert_eq!(body, "<h1 id=\"custom\">Hello World</h1>\n");
    }

    #[test]
    fn render_page_defaults_to_japanese_lang() {
        let doc = parse_document("#[ One ]\n").unwrap();
        let page = render_page(&doc, "Title");
        assert!(
            page.contains("<html lang=\"ja\">"),
            "expected default lang=\"ja\", got: {page}"
        );
    }

    #[test]
    fn render_page_with_honors_lang_override() {
        let doc = parse_document("#[ One ]\n").unwrap();
        let page = render_page_with(
            &doc,
            "Title",
            &RenderOptions {
                lang: Some("en".to_string()),
                ..RenderOptions::default()
            },
        );
        assert!(
            page.contains("<html lang=\"en\">"),
            "expected lang=\"en\", got: {page}"
        );
    }

    #[test]
    fn numbers_headings_by_nesting_level() {
        let doc = parse_document(
            "#[ One ]\n##[ One One ]\n##[ One Two ]\n#[ Two ]\n##[ Two One ]\n###[ Two One One ]\n",
        )
        .unwrap();
        let body = render_body_with(
            &doc,
            &RenderOptions {
                number_headings: true,
                ..RenderOptions::default()
            },
        );
        assert_eq!(
            body,
            "<h1><span class=\"tm-heading-number\">1</span>One</h1>\n\
             <h2><span class=\"tm-heading-number\">1.1</span>One One</h2>\n\
             <h2><span class=\"tm-heading-number\">1.2</span>One Two</h2>\n\
             <h1><span class=\"tm-heading-number\">2</span>Two</h1>\n\
             <h2><span class=\"tm-heading-number\">2.1</span>Two One</h2>\n\
             <h3><span class=\"tm-heading-number\">2.1.1</span>Two One One</h3>\n"
        );
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
    fn renders_link_with_explicit_target_key() {
        let doc = parse_document("@link(target:https://example.com)[Wiki]\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(
            body,
            "<a class=\"tm-url\" href=\"https://example.com\">Wiki</a>\n"
        );
    }

    #[test]
    fn renders_link_from_bare_scheme_uri_positional_target() {
        // `https://...` is shape-unambiguous, so the positional shorthand
        // (no `target:` key at all) works: `builtin_positional_arg_key`
        // normalizes it under `target` before `link_target` sees it.
        let doc = parse_document("@link(https://example.com)[Wiki]\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(
            body,
            "<a class=\"tm-url\" href=\"https://example.com\">Wiki</a>\n"
        );
    }

    #[test]
    fn renders_link_from_bare_absolute_path_positional_target() {
        // Same positional mechanism, other shape: a leading `/` is also
        // shape-unambiguous (`target_scheme` defaults it to `File`).
        let doc = parse_document("@link(/readme.md)[Readme]\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(
            body,
            "<a class=\"tm-file\" href=\"/readme.md\">Readme</a>\n"
        );
    }

    #[test]
    fn renders_id_link_as_a_same_document_anchor() {
        let doc = parse_document("@link(target:id:greeting)[Hello]\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(
            body,
            "<a class=\"tm-id\" href=\"#link-greeting\">Hello</a>\n"
        );
    }

    #[test]
    fn meta_element_has_no_visible_output() {
        let doc = parse_document("@meta(format:yaml)+++\nkey: value\n+++\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(body, "");
    }

    #[test]
    fn config_element_has_no_visible_output() {
        let doc = parse_document("@config(format:json)\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(body, "");
    }

    #[test]
    fn adjacent_meta_blocks_have_no_visible_output() {
        // `@meta` is documented as `placement: head` / `singleton: true`
        // (see `docs/spec/builtin-settings.tmt`) -- three of
        // them in one file, as used below, is not actually valid Tomet
        // and would eventually be rejected by `tomet-validator`. But
        // `tomet-parser` itself no longer folds them into one
        // `Block::Paragraph` (each `@meta(...)` on its own line, with no
        // blank line before the next, now parses as its own
        // `Block::Element` -- see `document.rs`'s `Stop::Paragraph`), and
        // `tomet-html` has no validation layer of its own, so this
        // just confirms it still renders each one as empty output rather
        // than leaking a stray whitespace-only `<p>` -- garbage in,
        // harmless out.
        let doc = parse_document(
            "@meta(format:json)+++\n{\"key\":\"value\"}\n+++\n@meta(format:yaml)+++\nkey:value\n+++\n@meta(format:toml)+++\nkey = \"value\"\n+++\n\n#[ next ]\n",
        )
        .unwrap();
        let body = render_body(&doc);
        assert_eq!(body, "<h1>next</h1>\n");
    }

    #[test]
    fn renders_typed_element_generically() {
        let doc = parse_document("@caution[ be careful ]\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(
            body,
            "<div class=\"tm-element tm-caution\">be careful</div>\n"
        );
    }

    #[test]
    fn renders_ordered_list_as_ol() {
        let doc = parse_document("-. one\n-. two\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(body, "<ol>\n<li>one</li>\n<li>two</li>\n</ol>\n");
    }

    #[test]
    fn renders_thematic_break_as_hr() {
        let doc = parse_document("---\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(body, "<hr>\n");
    }

    #[test]
    fn renders_titled_thematic_break() {
        let doc = parse_document("---[ Title ]---\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(
            body,
            "<div class=\"tm-hr-titled\"><hr><span>Title</span><hr></div>\n"
        );
    }

    #[test]
    fn renders_emphasis_strong_and_mark() {
        let doc = parse_document("a *em* b **strong** c ==mark==\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(
            body,
            "<p>a <em>em</em> b <strong>strong</strong> c <mark>mark</mark></p>\n"
        );
    }

    #[test]
    fn renders_embed_as_img() {
        let doc = parse_document("@embed(target:assets/pic.png)[a cat]\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(body, "<img src=\"assets/pic.png\" alt=\"a cat\">\n");
    }

    #[test]
    fn renders_codeblock_with_lang() {
        let doc = parse_document("@codeblock(lang:rust)[fn main() {}]\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(
            body,
            "<pre><code class=\"language-rust\">fn main() {}</code></pre>\n"
        );
    }

    #[test]
    fn fenced_code_block_renders_the_same_as_bracket_codeblock() {
        let doc = parse_document("```rust\nfn main() {}\n```\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(
            body,
            "<pre><code class=\"language-rust\">fn main() {}</code></pre>\n"
        );
    }

    #[test]
    fn codeblock_content_stays_literal_not_interpreted_as_markup() {
        // A ``` fence is the raw form. `#codeblock[...]` used to be raw
        // too, via a hardcoded name check in the parser -- one of the four
        // places the parser consulted an element vocabulary, and now gone.
        // Real code containing `*`/`@`/backticks must not be reinterpreted
        // as em/strong/element triggers/code spans.
        let doc = parse_document("```rust\nlet x = *ptr; let y = @T; @deco `q`\n```\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(
            body,
            "<pre><code class=\"language-rust\">let x = *ptr; let y = @T; @deco `q`</code></pre>\n"
        );
    }

    #[test]
    fn codeblock_with_a_nested_bracket_is_not_truncated_early() {
        let doc = parse_document("@codeblock(lang:rust)[let v = [1, 2, 3];]\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(
            body,
            "<pre><code class=\"language-rust\">let v = [1, 2, 3];</code></pre>\n"
        );
    }

    #[test]
    fn renders_list_with_value_markers_and_attrs() {
        let doc = parse_document("- (T) in-progress {tag: dev}\n- (\"?\") question {id: task1}\n")
            .unwrap();
        let body = render_body(&doc);
        assert_eq!(
            body,
            "<ul>\n<li data-tag=\"dev\"><span class=\"tm-list-marker\" data-marker=\"T\">[T]</span> in-progress</li>\n<li id=\"task1\"><span class=\"tm-list-marker\" data-marker=\"?\">[?]</span> question</li>\n</ul>\n"
        );
    }

    /// `[...]` after a list marker is the item's content group, the same
    /// group `@name[...]` takes -- not a checkbox, and not literal text.
    /// Text following the group stays in the item, the way
    /// `@x[T] content` keeps both halves in one paragraph.
    #[test]
    fn bracket_after_a_list_marker_is_the_content_group() {
        let doc = parse_document("- [T] content\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(body, "<ul>\n<li>T content</li>\n</ul>\n");
    }

    #[test]
    fn renders_key_value_list_marker_as_multiple_data_attrs() {
        let doc = parse_document("- (color: red, priority: high) content\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(
            body,
            "<ul>\n<li><span class=\"tm-list-marker\" data-color=\"red\" data-priority=\"high\"></span> content</li>\n</ul>\n"
        );
    }

    #[test]
    fn codeblock_value_group_is_id_cssclass_metadata_not_code() {
        let doc =
            parse_document("@codeblock(lang:rust){id:snippet1, cssclass:card}[fn main() {}]\n")
                .unwrap();
        let body = render_body(&doc);
        assert_eq!(
            body,
            "<pre id=\"snippet1\" class=\"card\"><code class=\"language-rust\">fn main() {}</code></pre>\n"
        );
    }

    #[test]
    fn renders_codeblock_with_positional_lang_arg() {
        let doc = parse_document("@codeblock(\"rust\")[fn main() {}]\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(
            body,
            "<pre><code class=\"language-rust\">fn main() {}</code></pre>\n"
        );
    }

    #[test]
    fn renders_embed_with_positional_src_arg() {
        let doc = parse_document("@embed(\"assets/pic.png\")[a cat]\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(body, "<img src=\"assets/pic.png\" alt=\"a cat\">\n");
    }

    #[test]
    fn meta_and_config_with_positional_format_arg_have_no_visible_output() {
        let doc =
            parse_document("@meta(\"json\")+++\n{\"key\": \"value\"}\n+++\n@config(\"json\")\n")
                .unwrap();
        let body = render_body(&doc);
        assert_eq!(body, "");
    }

    #[test]
    fn renders_table_element_to_html() {
        let src = "@table()[\n[ title ][  sdfasdf   ][    fasdf    ][ sdffdsf ]\n[ title ][ sdfddfasdf ][ fasddfdfdff ][ sdffdsf ]\n[ title ][  sdfasdf   ][   fasdf     ][ sdffdsf ]\n]{}\n";
        let doc = parse_document(src).unwrap();
        let body = render_body(&doc);
        assert_eq!(
            body,
            "<table class=\"tm-element tm-table\">\n\
<thead>\n\
<tr>\n\
<th>title</th>\n\
<th>sdfasdf</th>\n\
<th>fasdf</th>\n\
<th>sdffdsf</th>\n\
</tr>\n\
</thead>\n\
<tbody>\n\
<tr>\n\
<td>title</td>\n\
<td>sdfddfasdf</td>\n\
<td>fasddfdfdff</td>\n\
<td>sdffdsf</td>\n\
</tr>\n\
<tr>\n\
<td>title</td>\n\
<td>sdfasdf</td>\n\
<td>fasdf</td>\n\
<td>sdffdsf</td>\n\
</tr>\n\
</tbody>\n\
</table>\n"
        );
    }

    #[test]
    fn renders_icon_element_to_html() {
        let doc =
            parse_document("@icon(name: \"sun\", pkg: \"lucide\"){color: \"yellow\"}\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(
            body,
            "<div class=\"tm-element tm-icon tm-icon-sun tm-icon-lucide tm-icon-lucide-sun\" data-icon=\"sun\" data-pkg=\"lucide\" data-color=\"yellow\"></div>\n"
        );

        let inline_doc = parse_document(
            "Here is @icon(name: \"sun\", pkg: \"lucide\"){color: \"yellow\"} icon.\n",
        )
        .unwrap();
        let inline_body = render_body(&inline_doc);
        assert_eq!(
            inline_body,
            "<p>Here is <span class=\"tm-element tm-icon tm-icon-sun tm-icon-lucide tm-icon-lucide-sun\" data-icon=\"sun\" data-pkg=\"lucide\" data-color=\"yellow\"></span> icon.</p>\n"
        );
    }

    #[test]
    fn renders_interp_and_macros_to_html() {
        let doc = parse_document("@config{\n  macros: {\n    gh: \"https://github.com/tomet/tomet/issues/${1}\"\n    copyright: \"(C) 2026 Tomet\"\n  }\n}\n\nIssue: $gh(42)\nFooter: ${copyright}\nMath: ${add(10, 5)}\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(
            body,
            "<p>Issue: <span class=\"tm-interp\">https://github.com/tomet/tomet/issues/42</span> Footer: <span class=\"tm-interp\">(C) 2026 Tomet</span> Math: <span class=\"tm-interp\">15</span></p>\n"
        );
    }
}
