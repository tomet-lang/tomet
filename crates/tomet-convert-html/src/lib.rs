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

use std::fmt;
use std::sync::Arc;

use tomet_ast::{Block, Document, Element, ElementValue, Inline, Section, Span, Value};
use tomet_semantics::{
    Bindings, EXACT_DATA_KEY, ElementKind, TargetScheme, builtin_doc_vocabularies,
    classify_std_lenient, flatten_data, heading_level, is_directive, link_target, list_items,
    list_ordered, normalized_element_args, normalized_element_args_in, path_target, target_scheme,
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
    /// A hook for element kinds this crate assigns no built-in meaning to
    /// -- `ElementKind::Custom`, most often a namespaced embedded-vocabulary
    /// element like `doc.icon`, which `tomet_semantics` resolves but
    /// deliberately never draws (see
    /// `tomet_semantics::builtin_doc_vocabularies`). `None` keeps every
    /// existing caller's output unchanged (the generic `<span>`/`<div>`
    /// fallback); a hook that returns `None` for a given element falls
    /// back to that same rendering, so it only needs to answer for the
    /// kinds it actually knows.
    pub custom_element: Option<CustomElementRenderer>,
    /// Tag every editable leaf block -- a paragraph, a heading, a list
    /// item, or any other top-level block (table, quote, hr, ...) -- with
    /// `data-tmt-start`/`data-tmt-end` holding that node's `Span` byte
    /// offsets in the source document. `false` keeps every existing
    /// caller's output byte-for-byte unchanged; a consumer that needs to
    /// map a rendered element back to its source range (e.g. an in-page
    /// editor splicing an edit into the `.tmt` file) opts in.
    ///
    /// A list/table is not itself tagged, only its items/rows-as-a-whole
    /// -- see the `render_list`/`render_block` doc comments for what
    /// "leaf" means for each block kind, including the current gap around
    /// table cells (`tomet_semantics::parse_table_rows` does not track
    /// offsets, so `@table` is tagged as a single leaf, not per-cell).
    pub emit_source_spans: bool,
}

/// What a [`RenderOptions::custom_element`] hook sees for one element.
///
/// Every fallback element in the document reaches the hook -- most books
/// have far more `card`/`callout`/custom-vocabulary elements than `doc.*`
/// ones -- so [`CustomElementCtx::normalized_args`] is lazy and behind a
/// method rather than a plain field: a hook is expected to check `kind`
/// and return `None` immediately for anything it does not recognize,
/// and elements it declines should never pay for normalizing args they
/// were never going to use.
pub struct CustomElementCtx<'a> {
    /// The dotted or bare name [`classify_std_lenient`] fell back to
    /// (`"doc.icon"`, `"my-widget"`, ...).
    pub kind: &'a str,
    /// The element's bracketed `[content]`, already parsed as inline nodes.
    pub content: Option<&'a [Inline]>,
    /// Whether this element sits inline in running text (`span`) or as a
    /// block (`div`) -- the same distinction `render_generic_element` uses.
    pub inline: bool,
    normalize_args: Box<dyn Fn() -> Option<Value> + 'a>,
}

impl<'a> CustomElementCtx<'a> {
    pub fn new(
        kind: &'a str,
        content: Option<&'a [Inline]>,
        inline: bool,
        normalize_args: impl Fn() -> Option<Value> + 'a,
    ) -> Self {
        Self {
            kind,
            content,
            inline,
            normalize_args: Box::new(normalize_args),
        }
    }

    /// `args`, normalized against the same vocabularies this crate binds
    /// for rendering (`builtin_doc_vocabularies`), so a positional first
    /// argument -- `doc.icon`'s `name` -- is already keyed by name rather
    /// than left under the parser's positional sentinel. Clones the args
    /// map on every call; see the type-level doc for why that cost is
    /// opt-in rather than paid up front for every element.
    pub fn normalized_args(&self) -> Option<Value> {
        (self.normalize_args)()
    }
}

/// A [`RenderOptions::custom_element`] hook. A newtype (not a bare
/// `Box<dyn Fn>` field) because `RenderOptions` derives `Debug`/`Clone`,
/// and trait objects need a hand-written `Debug` and an `Arc` (not `Box`)
/// to be `Clone`.
#[derive(Clone)]
pub struct CustomElementRenderer(Arc<dyn Fn(CustomElementCtx) -> Option<String> + Send + Sync>);

impl CustomElementRenderer {
    pub fn new(f: impl Fn(CustomElementCtx) -> Option<String> + Send + Sync + 'static) -> Self {
        Self(Arc::new(f))
    }
}

impl fmt::Debug for CustomElementRenderer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CustomElementRenderer(..)")
    }
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

/// One heading of the rendered body, as it was actually emitted.
///
/// Reported by [`render_body_with_outline`] so a caller building a table of
/// contents reads the renderer's own decisions -- the id it settled on, the
/// number it assigned -- instead of scraping them back out of the HTML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadingInfo {
    /// Nesting level, 1-6, matching the `<h1>`..`<h6>` that was emitted.
    pub level: u8,
    /// The `id` attribute the heading carries, whether written explicitly as
    /// `{id:...}` or generated by `RenderOptions::auto_slug_headings`. `None`
    /// when the heading has no id and so cannot be linked to.
    pub id: Option<String>,
    /// Heading text with inline markup flattened away. Never includes the
    /// number label, which lives in `number`.
    pub text: String,
    /// The dotted label (`"1.2"`) given by `RenderOptions::number_headings`,
    /// or `None` when numbering is off.
    pub number: Option<String>,
}

/// Render just the body content, without the surrounding `<html>` shell,
/// with the plain (unnumbered) heading style.
pub fn render_body(doc: &Document) -> String {
    render_body_with(doc, &RenderOptions::default())
}

/// Render just the body content, applying `options`.
pub fn render_body_with(doc: &Document, options: &RenderOptions) -> String {
    render_body_with_outline(doc, options).0
}

/// Render the body and report its headings, in a single pass.
///
/// The HTML is byte-for-byte what [`render_body_with`] produces; the outline
/// lists every heading the renderer emitted, in document order.
pub fn render_body_with_outline(
    doc: &Document,
    options: &RenderOptions,
) -> (String, Vec<HeadingInfo>) {
    let mut out = String::new();
    let mut state = HeadingState::default();
    // Only `doc.*` needs binding here -- a document's own `@vocabulary`
    // declarations have no bearing on this crate's syntactic, semantics-free
    // rendering, but `doc.icon`'s positional `name` still has to resolve to
    // that key (not the parser's positional sentinel) for `custom_element`
    // hooks to read it by name.
    let bindings = Bindings::for_document(doc, builtin_doc_vocabularies());
    let cx = RenderCtx {
        options,
        bindings: &bindings,
    };
    for block in &doc.blocks {
        render_block(&cx, block, &mut out, &mut state);
    }
    (out, state.outline)
}

struct RenderCtx<'a> {
    options: &'a RenderOptions,
    bindings: &'a Bindings,
}

/// Per-document state threaded through heading rendering: the nesting
/// counters for `RenderOptions::number_headings` and the slug registry for
/// `RenderOptions::auto_slug_headings`, kept together since both are
/// "remember what came before while walking the same heading sequence".
/// The outline rides along for the same reason -- it records what those two
/// decided, as each heading is emitted.
#[derive(Default)]
struct HeadingState {
    counters: HeadingCounters,
    slugs: SlugTracker,
    outline: Vec<HeadingInfo>,
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

/// Dispatches one top-level document block, and -- when
/// `RenderOptions::emit_source_spans` is set -- tags whatever it emits as
/// one editable "leaf" with `data-tmt-start`/`data-tmt-end`.
///
/// What counts as a leaf here: a paragraph or heading is tagged as itself;
/// a list is not tagged at all, only each of its items (`render_list`
/// tags those individually, at item granularity, including any nested
/// sub-list inside an item -- editing an item edits its whole subtree's
/// source text); everything else (table, quote, hr, raw block, generic
/// element, ...) is tagged as a single opaque leaf covering its entire
/// rendered output, since none of those currently expose finer-grained
/// spans (a table's cells, notably, do not -- see `RenderOptions::
/// emit_source_spans`'s doc comment).
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
                out.push_str("<p");
                push_span_attrs(cx, out, p.span);
                out.push('>');
                out.push_str(&inner);
                out.push_str("</p>\n");
            }
        }
        Block::Element(el) if list_ordered(el).is_some() => render_list(cx, el, out),
        Block::Element(el) if classify_std_lenient(el) == ElementKind::Heading => {
            render_heading_element(cx, el, out, state)
        }
        Block::Element(el) => {
            // The catch-all: dispatches into whichever of `render_element`'s
            // many branches (table/quote/hr/raw/embed/link/ruby/generic)
            // this element's kind matches. Rather than threading a "tag
            // your own opening tag" parameter through every one of those,
            // splice the attributes into the first tag the call actually
            // produced -- safe because `escape_attr` never lets a literal
            // `>` reach an attribute value, so the first `>` after this
            // point is that tag's own close.
            let start = out.len();
            render_element(cx, el, out, false);
            if cx.options.emit_source_spans && out.len() > start {
                splice_span_attrs(out, start, el.span);
            }
        }
        Block::Section(sec) => render_section(cx, sec, out, state),
    }
}

fn render_section(
    cx: &RenderCtx,
    sec: &Section,
    out: &mut String,
    state: &mut HeadingState,
) {
    let level = (sec.level as u8).clamp(1, 6);
    let value_data = match &sec.value {
        Some(v) => v.as_data(),
        _ => None,
    };
    let (mut id, class, data) = split_attrs(value_data.as_ref());
    let text = inlines_to_plain(&sec.title);
    if id.is_none() && cx.options.auto_slug_headings {
        let slug = state.slugs.slug_for(&text);
        if !slug.is_empty() {
            id = Some(slug);
        }
    }
    out.push_str(&format!("<h{level}"));
    push_named_attrs(out, &id, &class, &data);
    push_span_attrs(cx, out, sec.span);
    out.push('>');
    let mut number = None;
    if cx.options.number_headings {
        let label = state.counters.advance(level);
        out.push_str(&format!("<span class=\"tm-heading-number\">{label}</span>"));
        number = Some(label);
    }
    render_inlines(cx, &sec.title, out);
    out.push_str(&format!("</h{level}>\n"));

    state.outline.push(HeadingInfo {
        level,
        id,
        text,
        number,
    });

    for child in &sec.blocks {
        render_block(cx, child, out, state);
    }
}

/// Appends ` data-tmt-start="…" data-tmt-end="…"` (the byte offsets from
/// `span`) when `RenderOptions::emit_source_spans` is set, otherwise
/// nothing. Call this after an opening tag's other attributes and before
/// its closing `>`.
fn push_span_attrs(cx: &RenderCtx, out: &mut String, span: Span) {
    if cx.options.emit_source_spans {
        out.push_str(&format!(
            " data-tmt-start=\"{}\" data-tmt-end=\"{}\"",
            span.start.offset, span.end.offset
        ));
    }
}

/// Like `push_span_attrs`, but for output already written: inserts the
/// attributes right before the first `>` found at or after byte `start`
/// of `out`. Only sound when `out[start..]` begins with a fresh opening
/// tag with no attribute value containing a literal `>` -- see the call
/// site in `render_block` for why that holds here.
fn splice_span_attrs(out: &mut String, start: usize, span: Span) {
    if let Some(rel_gt) = out[start..].find('>') {
        let gt = start + rel_gt;
        out.insert_str(
            gt,
            &format!(
                " data-tmt-start=\"{}\" data-tmt-end=\"{}\"",
                span.start.offset, span.end.offset
            ),
        );
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
    let text = inlines_to_plain(content);
    if id.is_none() && cx.options.auto_slug_headings {
        let slug = state.slugs.slug_for(&text);
        if !slug.is_empty() {
            id = Some(slug);
        }
    }
    out.push_str(&format!("<h{level}"));
    push_named_attrs(out, &id, &class, &data);
    push_span_attrs(cx, out, el.span);
    out.push('>');
    let mut number = None;
    if cx.options.number_headings {
        let label = state.counters.advance(level);
        // No literal space after `</span>` -- spacing is `.tmt-heading-number`'s
        // `margin-right` in `DEFAULT_STYLE`, not baked into the content, so
        // e.g. copy-pasting the heading text doesn't pick up a stray space.
        out.push_str(&format!("<span class=\"tm-heading-number\">{label}</span>"));
        number = Some(label);
    }
    render_inlines(cx, content, out);
    out.push_str(&format!("</h{level}>\n"));

    state.outline.push(HeadingInfo {
        level,
        id,
        text,
        number,
    });
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
        push_span_attrs(cx, out, item.span);
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
                Value::Element(_) => {}
                other => out.push_str(&format!(
                    " data-marker=\"{}\"",
                    escape_attr(&value_to_plain(other))
                )),
            }
            out.push('>');
            match marker {
                Value::Element(el) => {
                    render_element(cx, el, out, true);
                }
                other => {
                    let text = value_to_plain(other);
                    if !text.is_empty() {
                        out.push_str(&format!("[{}]", escape_html(&text)));
                    }
                }
            }
            out.push_str("</span> ");
        }
        render_inlines(cx, item.content.as_deref().unwrap_or(&[]), out);
        if let Some(children) = &item.children {
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
    for (idx, inline) in inlines.iter().enumerate() {
        match inline {
            Inline::Text(t) => out.push_str(&escape_html(&t.value)),
            Inline::Raw(t) => out.push_str(&escape_html(&t.value)),
            Inline::SoftBreak(_) => {
                let before = out.chars().last();
                let after = inlines.get(idx + 1).and_then(Inline::first_char);
                out.push_str(tomet_ast::softbreak_join(before, after));
            }
            Inline::LineBreak(_) => out.push_str("<br>"),
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
        "file" | "dir" => render_path_element(cx, el, out, inline),
        "embed" => render_embed_element(el, out),
        "hr" => render_hr_element(cx, el, out),
        "em" | "strong" | "mark" => render_wrapped_inline(cx, el, kind.as_str(), out),
        // `<del>` rather than a `strikeout` tag: the element's name is
        // Tomet's, and `<del>` is what GFM's `~~` renders as everywhere
        // it is read.
        "strikeout" => render_wrapped_inline(cx, el, "del", out),
        "ruby" => render_ruby_element(cx, el, out),
        "raw" => render_raw_element(el, out, inline),
        "quote" => render_quote_element(cx, el, out, inline),
        "table" => render_table_element(cx, el, out),
        _ => render_custom_or_generic_element(cx, el, kind.as_str(), out, inline),
    }
}

/// The fallback for a kind [`render_element`] has no dedicated handling
/// for: try [`RenderOptions::custom_element`] first, since a namespaced
/// embedded-vocabulary element like `doc.icon` reaches here too (Tomet
/// resolves it but never draws it), then fall back to
/// [`render_generic_element`] for anything the hook doesn't claim.
fn render_custom_or_generic_element(
    cx: &RenderCtx,
    el: &Element,
    kind: &str,
    out: &mut String,
    inline: bool,
) {
    if let Some(renderer) = &cx.options.custom_element {
        let bindings = cx.bindings;
        let ctx = CustomElementCtx::new(kind, el.content.as_deref(), inline, || {
            normalized_element_args_in(el, bindings)
        });
        if let Some(html) = (renderer.0)(ctx) {
            out.push_str(&html);
            return;
        }
    }
    render_generic_element(cx, el, kind, out, inline);
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

/// `@ruby[漢字](rt:"かんじ")` -- real `<ruby>`/`<rt>` markup, not the
/// generic `<span data-rt=...>` `render_generic_element` would fall back to.
/// `rt` is meaningful content (the reading), not decoration, so it has to
/// reach the output as text a reader/screen-reader sees.
fn render_ruby_element(cx: &RenderCtx, el: &Element, out: &mut String) {
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
fn render_raw_element(el: &Element, out: &mut String, inline: bool) {
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
fn render_quote_element(cx: &RenderCtx, el: &Element, out: &mut String, inline: bool) {
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

/// Flattens inline content to plain text -- used for the `alt` attribute,
/// which can't itself carry markup.
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
                    s.push_str(&inlines_to_plain(content));
                }
            }
        }
    }
    s
}

/// `@file(x)`/`@dir(x)` -- a path named, not navigated to.
///
/// A `<code>`, not an `<a>`, and that is the whole difference from
/// `render_link_element`. Prose saying "read `codeblock.rs`" is not
/// offering to take the reader there, and before these elements existed
/// it was written in backticks -- which is exactly what this renders back
/// to, so moving a mention onto `@file` changes what a checker can see
/// and nothing a reader can.
fn render_path_element(cx: &RenderCtx, el: &Element, out: &mut String, inline: bool) {
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
fn render_link_element(cx: &RenderCtx, el: &Element, out: &mut String, inline: bool) {
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

fn render_content_or_fallback(cx: &RenderCtx, el: &Element, fallback: &str, out: &mut String) {
    match &el.content {
        Some(content) if !content.is_empty() => render_inlines(cx, content, out),
        _ => out.push_str(&escape_html(fallback)),
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
    push_value_data_attrs(out, el);
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
        // Only the elements. A group's *pairs* are data and left the body
        // for `data-*` beside `(args)`; see `push_value_data_attrs`.
        ElementValue::Group(_) => {
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
        // A `${...}` that reached here unresolved, written back as it was.
        //
        // Reaching here at all means the document was not prepared -- a
        // caller that went straight to this crate rather than through
        // `tomet_load::Vault`. Rendering nothing would hide it, and
        // evaluating it here is what this crate stopped doing: what a
        // `${...}` means is settled by `tomet-transform`'s
        // `resolve_interpolations` before any converter sees the tree.
        //
        // This used to evaluate with `evaluate_with_config` -- no
        // `EvaluationContext` and no vault config, so `${self.path}`
        // resolved in CommonMark and not here, and `default.config.tmt`'s
        // macros expanded in neither. No wrapper element is emitted now
        // that the text is plain: `<span class="tm-interp">` carried no
        // style and marked a distinction that no longer survives to here.
        ElementValue::Interp(expr) => {
            out.push_str(&escape_html(&format!("${{{expr}}}")));
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
/// A `{...}` group's pairs, as `data-*` beside the ones `(args)` gives.
///
/// `{}` has been "always data" since the uniform-group change, and this
/// is where data goes in HTML -- the same shelf `(args)` already uses,
/// invisible to the reader.
///
/// They used to be dropped in silence. `render_element_value` asked
/// `value_to_plain` for a string to show in a `tm-value` span, and
/// `as_data` returns a `Value::Map` for every group while `value_to_plain`
/// answers `""` for every map, so the span's guard was never true. It
/// appears nowhere in `tests/ref/`.
///
/// A key `(args)` already spent is skipped rather than written twice,
/// which would be invalid HTML. `(args)` wins because it is the group the
/// reader wrote closer to the name.
fn push_value_data_attrs(out: &mut String, el: &Element) {
    let Some(data) = el.value.as_ref().and_then(|v| v.as_data()) else {
        return;
    };
    let taken: Vec<String> = el
        .args
        .as_ref()
        .map(|a| {
            flatten_data(Some(a), None)
                .pairs
                .into_iter()
                .map(|(k, _)| k)
                .collect()
        })
        .unwrap_or_default();
    let skip: Vec<&str> = taken.iter().map(String::as_str).collect();
    push_data_attrs(out, Some(&data), &skip);
}

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
        // A call is validation-only (e.g. `:rule`'s `allow:list(...)`)
        // and has no rendered form.
        Value::Call(..) => String::new(),
        // An embedded element has no scalar form either -- same as `Map`.
        Value::Element(_) => String::new(),
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
        let doc = parse_document("@deck.x(a: 1, tags: list(p, q))[ 本文 ]\n").unwrap();
        let html = render_body(&doc);
        assert!(html.contains(r#"data-tags="p, q""#), "got {html}");
        assert!(!html.contains("data-tomet-data="), "got {html}");
    }

    #[test]
    fn renders_heading_with_id_and_cssclass() {
        let doc = parse_document("=[ Hello ]{ id:header1, cssclass:card }\n").unwrap();
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
        let doc = parse_document("@quote[ @deck.badge(2)[Nested] ]\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(
            body,
            "<blockquote><span class=\"tm-element tm-deck.badge\" data-value=\"2\">Nested</span></blockquote>\n"
        );
    }

    #[test]
    fn default_options_leave_headings_unnumbered() {
        let doc = parse_document("=[ One ]\n").unwrap();
        assert_eq!(
            render_body_with(&doc, &RenderOptions::default()),
            render_body(&doc),
        );
        assert_eq!(render_body(&doc), "<h1>One</h1>\n");
    }

    #[test]
    fn auto_slug_headings_generates_ids_from_text() {
        let doc = parse_document("=[ Hello World ]\n").unwrap();
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
        let doc = parse_document("=[ 見出し テスト ]\n").unwrap();
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
        let doc = parse_document("=[ Intro ]\n==[ Intro ]\n==[ Intro ]\n").unwrap();
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
        let doc = parse_document("=[ Hello World ]{ id:custom }\n").unwrap();
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
        let doc = parse_document("=[ One ]\n").unwrap();
        let page = render_page(&doc, "Title");
        assert!(
            page.contains("<html lang=\"ja\">"),
            "expected default lang=\"ja\", got: {page}"
        );
    }

    #[test]
    fn render_page_with_honors_lang_override() {
        let doc = parse_document("=[ One ]\n").unwrap();
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
            "=[ One ]\n==[ One One ]\n==[ One Two ]\n=[ Two ]\n==[ Two One ]\n===[ Two One One ]\n",
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
            "@meta(format:json)+++\n{\"key\":\"value\"}\n+++\n@meta(format:yaml)+++\nkey:value\n+++\n@meta(format:toml)+++\nkey = \"value\"\n+++\n\n=[ next ]\n",
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
        let doc = parse_document("a *em* b **strong** c @mark[mark]\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(
            body,
            "<p>a <em>em</em> b <strong>strong</strong> c <mark>mark</mark></p>\n"
        );
    }

    #[test]
    fn renders_ruby() {
        let doc = parse_document("a @ruby[漢字](rt:\"かんじ\") b\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(body, "<p>a <ruby>漢字<rt>かんじ</rt></ruby> b</p>\n");
    }

    /// `doc.icon` is `ElementKind::Custom("doc.icon")` -- this crate has no
    /// dedicated handling for it, so it only draws anything when a caller
    /// installs `RenderOptions::custom_element`. Also proves the hook sees
    /// `name`/`pkg` already resolved by key, not the parser's positional
    /// sentinel -- `doc.icon`'s `name` is a bare positional argument, and
    /// nothing in this document declares `doc`'s vocabulary (it can't:
    /// `doc` is a `RESERVED_NAMESPACES` entry).
    #[test]
    fn custom_element_hook_renders_doc_icon() {
        let doc = parse_document("a @doc.icon(\"star\", pkg:\"lucide\") b\n").unwrap();
        let options = RenderOptions {
            custom_element: Some(CustomElementRenderer::new(|ctx| {
                if ctx.kind != "doc.icon" {
                    return None;
                }
                let args = ctx.normalized_args();
                let get = |key: &str| {
                    let Some(Value::Map(entries)) = &args else {
                        return None;
                    };
                    entries
                        .iter()
                        .find(|(k, _)| k == key)
                        .and_then(|(_, v)| match v {
                            Value::String(s) => Some(s.clone()),
                            _ => None,
                        })
                };
                let name = get("name")?;
                let pkg = get("pkg").unwrap_or_default();
                Some(format!("<i data-icon=\"{name}\" data-pkg=\"{pkg}\"></i>"))
            })),
            ..RenderOptions::default()
        };
        let body = render_body_with(&doc, &options);
        assert_eq!(
            body,
            "<p>a <i data-icon=\"star\" data-pkg=\"lucide\"></i> b</p>\n"
        );
    }

    /// A hook that declines an element (returns `None`) falls back to the
    /// existing generic rendering, same as having no hook at all.
    #[test]
    fn custom_element_hook_falling_through_matches_generic_rendering() {
        let doc = parse_document("@doc.icon(\"star\", pkg:\"lucide\")\n").unwrap();
        let without_hook = render_body(&doc);
        let options = RenderOptions {
            custom_element: Some(CustomElementRenderer::new(|_ctx| None)),
            ..RenderOptions::default()
        };
        let with_declining_hook = render_body_with(&doc, &options);
        assert_eq!(with_declining_hook, without_hook);
    }

    #[test]
    fn renders_embed_as_img() {
        let doc = parse_document("@embed(target:assets/pic.png)[a cat]\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(body, "<img src=\"assets/pic.png\" alt=\"a cat\">\n");
    }

    #[test]
    fn renders_embed_unresolved() {
        let doc =
            parse_document("@embed(target:\"unresolved:missing.png\")[missing cat]\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(
            body,
            "<img class=\"tm-embed tm-embed-unresolved\" src=\"missing.png\" alt=\"missing cat\" aria-disabled=\"true\">\n"
        );
    }

    #[test]
    fn renders_raw_with_lang() {
        let doc = parse_document("@raw(lang:rust)[fn main() {}]\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(
            body,
            "<pre><code class=\"language-rust\">fn main() {}</code></pre>\n"
        );
    }

    #[test]
    fn fenced_raw_block_renders_the_same_as_bracket_raw() {
        let doc = parse_document("```rust\nfn main() {}\n```\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(
            body,
            "<pre><code class=\"language-rust\">fn main() {}</code></pre>\n"
        );
    }

    #[test]
    fn raw_content_stays_literal_not_interpreted_as_markup() {
        let doc = parse_document("```rust\nlet x = *ptr; let y = @T; @deco `q`\n```\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(
            body,
            "<pre><code class=\"language-rust\">let x = *ptr; let y = @T; @deco `q`</code></pre>\n"
        );
    }

    #[test]
    fn raw_with_a_nested_bracket_is_not_truncated_early() {
        let doc = parse_document("@raw(lang:rust)[let v = [1, 2, 3];]\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(
            body,
            "<pre><code class=\"language-rust\">let v = [1, 2, 3];</code></pre>\n"
        );
    }

    #[test]
    fn renders_inline_backtick_as_code() {
        let doc = parse_document("call `foo()` and `@kind(doc.index)` now\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(
            body,
            "<p>call <code>foo()</code> and <code>@kind(doc.index)</code> now</p>\n"
        );
    }

    #[test]
    fn renders_explicit_inline_raw() {
        let doc = parse_document("use @raw[Ctrl+C] or @raw(rust)[x = 1] here\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(
            body,
            "<p>use <code>Ctrl+C</code> or <code class=\"language-rust\">x = 1</code> here</p>\n"
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
    fn renders_list_marker_with_embedded_element() {
        let doc = parse_document("- (@em[Important]) content\n- (@link(https://example.com)[Wiki]) docs\n")
            .unwrap();
        let body = render_body(&doc);
        assert_eq!(
            body,
            "<ul>\n<li><span class=\"tm-list-marker\"><em>Important</em></span> content</li>\n<li><span class=\"tm-list-marker\"><a class=\"tm-url\" href=\"https://example.com\">Wiki</a></span> docs</li>\n</ul>\n"
        );
    }

    #[test]
    fn raw_value_group_is_id_cssclass_metadata_not_code() {
        let doc =
            parse_document("@raw(lang:rust){id:snippet1, cssclass:card}[fn main() {}]\n")
                .unwrap();
        let body = render_body(&doc);
        assert_eq!(
            body,
            "<pre id=\"snippet1\" class=\"card\"><code class=\"language-rust\">fn main() {}</code></pre>\n"
        );
    }

    #[test]
    fn renders_raw_with_positional_lang_arg() {
        let doc = parse_document("@raw(\"rust\")[fn main() {}]\n").unwrap();
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
    fn test_renders_unresolved_link_placeholder() {
        let doc = parse_document("- @link(\"unresolved:Future Note\")[Future Note]\n").unwrap();
        let body = render_body(&doc);
        assert_eq!(
            body,
            "<ul>\n<li><a class=\"tm-ref tm-ref-unresolved\" aria-disabled=\"true\" data-ref=\"Future Note\">Future Note</a></li>\n</ul>\n"
        );
    }

    /// This crate does not evaluate `${...}` any more, and this test is
    /// what is left of the one that said it did.
    ///
    /// It used to assert that `$gh(42)` came out as the expanded URL. Four
    /// converters each decided that for themselves and three of them
    /// decided differently -- this one evaluated without an
    /// `EvaluationContext` or the vault's config, so `${self.path}`
    /// resolved in CommonMark and not here. Evaluation moved to
    /// `tomet-transform`'s `resolve_interpolations`, which runs before any
    /// converter sees the tree; the expansion those assertions were about
    /// is pinned there now.
    ///
    /// What reaches here is a document nobody prepared, and the only
    /// honest rendering of that is what was written.
    #[test]
    fn an_unprepared_interpolation_renders_as_its_own_source() {
        let doc =
            parse_document("Issue: $gh(42)\nFooter: ${copyright}\nMath: ${add(10, 5)}\n").unwrap();
        assert_eq!(
            render_body(&doc),
            "<p>Issue: ${gh(42)} Footer: ${copyright} Math: ${add(10, 5)}</p>\n"
        );
    }

    fn outline_of(src: &str, options: &RenderOptions) -> Vec<HeadingInfo> {
        let doc = parse_document(src).unwrap();
        render_body_with_outline(&doc, options).1
    }

    #[test]
    fn the_outline_reports_level_text_and_order() {
        let outline = outline_of(
            "=[ First ]\n\n==[ Nested ]\n\n=[ Second ]\n",
            &RenderOptions::default(),
        );

        let seen: Vec<(u8, &str)> = outline.iter().map(|h| (h.level, h.text.as_str())).collect();
        assert_eq!(seen, vec![(1, "First"), (2, "Nested"), (1, "Second")]);
        assert!(outline.iter().all(|h| h.number.is_none()));
        assert!(outline.iter().all(|h| h.id.is_none()));
    }

    #[test]
    fn the_outline_carries_the_id_the_heading_was_rendered_with() {
        let explicit = outline_of("=[ Hello ]{ id:header1 }\n", &RenderOptions::default());
        assert_eq!(explicit[0].id.as_deref(), Some("header1"));

        let generated = outline_of(
            "=[ Hello World ]\n",
            &RenderOptions {
                auto_slug_headings: true,
                ..RenderOptions::default()
            },
        );
        assert_eq!(generated[0].id.as_deref(), Some("hello-world"));
    }

    #[test]
    fn duplicate_slugs_are_reported_as_rendered() {
        let outline = outline_of(
            "=[ Same ]\n\n=[ Same ]\n",
            &RenderOptions {
                auto_slug_headings: true,
                ..RenderOptions::default()
            },
        );
        let ids: Vec<&str> = outline.iter().filter_map(|h| h.id.as_deref()).collect();
        assert_eq!(ids, vec!["same", "same-2"]);
    }

    #[test]
    fn numbering_is_reported_separately_from_the_text() {
        let outline = outline_of(
            "=[ One ]\n\n==[ One A ]\n\n==[ One B ]\n\n=[ Two ]\n",
            &RenderOptions {
                number_headings: true,
                ..RenderOptions::default()
            },
        );

        let numbers: Vec<&str> = outline.iter().filter_map(|h| h.number.as_deref()).collect();
        assert_eq!(numbers, vec!["1", "1.1", "1.2", "2"]);
        // The label lives in `number`; `text` stays the author's words.
        assert_eq!(outline[1].text, "One A");
    }

    #[test]
    fn inline_markup_is_flattened_in_the_outline_text() {
        let outline = outline_of("=[ **bold** and plain ]\n", &RenderOptions::default());
        assert_eq!(outline[0].text, "bold and plain");
    }

    #[test]
    fn a_document_without_headings_has_an_empty_outline() {
        assert!(outline_of("just a paragraph\n", &RenderOptions::default()).is_empty());
    }

    #[test]
    fn the_outline_does_not_change_the_html() {
        // render_body_with is now a thin wrapper; guard against it drifting.
        let doc = parse_document("=[ One ]\n\n==[ Two ]\n\ntext\n").unwrap();
        let options = RenderOptions {
            number_headings: true,
            auto_slug_headings: true,
            ..RenderOptions::default()
        };
        let (html, _) = render_body_with_outline(&doc, &options);
        assert_eq!(html, render_body_with(&doc, &options));
    }

    #[test]
    fn only_block_level_headings_reach_the_outline() {
        // A heading nested in another element renders generically, so it is
        // not a heading in the output and must not appear in the outline.
        let outline = outline_of(
            "@quote[ @deck.badge(2)[Nested] ]\n",
            &RenderOptions::default(),
        );
        assert!(outline.is_empty());
    }

    /// Pulls every `data-tmt-start`/`data-tmt-end` pair out of rendered HTML,
    /// in document order, as `(start, end)` byte offsets.
    fn extract_spans(html: &str) -> Vec<(usize, usize)> {
        let mut spans = Vec::new();
        let mut rest = html;
        while let Some(idx) = rest.find("data-tmt-start=\"") {
            rest = &rest["data-tmt-start=\"".len() + idx..];
            let (start_str, after) = rest.split_once('"').unwrap();
            let start: usize = start_str.parse().unwrap();
            let after = after.strip_prefix(" data-tmt-end=\"").unwrap();
            let (end_str, after) = after.split_once('"').unwrap();
            let end: usize = end_str.parse().unwrap();
            spans.push((start, end));
            rest = after;
        }
        spans
    }

    #[test]
    fn spans_are_absent_unless_opted_in() {
        let doc = parse_document("hello world\n").unwrap();
        let html = render_body(&doc);
        assert!(!html.contains("data-tmt-start"));
    }

    #[test]
    fn a_paragraph_is_tagged_with_its_own_span() {
        let src = "hello world\n";
        let doc = parse_document(src).unwrap();
        let options = RenderOptions {
            emit_source_spans: true,
            ..RenderOptions::default()
        };
        let html = render_body_with(&doc, &options);
        let spans = extract_spans(&html);
        assert_eq!(spans.len(), 1);
        assert_eq!(&src[spans[0].0..spans[0].1], "hello world");
    }

    #[test]
    fn a_heading_and_the_paragraph_after_it_each_get_their_own_span() {
        let src = "=[ Title ]\n\nbody text\n";
        let doc = parse_document(src).unwrap();
        let options = RenderOptions {
            emit_source_spans: true,
            ..RenderOptions::default()
        };
        let html = render_body_with(&doc, &options);
        let spans = extract_spans(&html);
        assert_eq!(spans.len(), 2, "expected one span for the heading and one for the paragraph, got {html:?}");
        // Each span reaches through its own trailing newline, up to (not
        // including) the blank line that separates it from the next block.
        assert_eq!(&src[spans[0].0..spans[0].1], "=[ Title ]\n");
        // The final block's span stops at its own content -- there's no
        // following block for it to reach a separating blank line toward.
        assert_eq!(&src[spans[1].0..spans[1].1], "body text");
    }

    #[test]
    fn list_items_are_tagged_individually_and_the_list_itself_is_not() {
        let src = "- one\n- two\n";
        let doc = parse_document(src).unwrap();
        let options = RenderOptions {
            emit_source_spans: true,
            ..RenderOptions::default()
        };
        let html = render_body_with(&doc, &options);
        assert!(
            !html.starts_with("<ul data-tmt-start"),
            "the list wrapper itself must not carry a span: {html:?}"
        );
        let spans = extract_spans(&html);
        assert_eq!(spans.len(), 2);
        // Each item's span covers its whole source line, marker included --
        // exactly the raw text an editor would want to seed a textarea with.
        assert_eq!(&src[spans[0].0..spans[0].1], "- one\n");
        assert_eq!(&src[spans[1].0..spans[1].1], "- two\n");
    }

    #[test]
    fn a_non_list_non_heading_block_is_tagged_as_one_opaque_leaf() {
        let src = "@quote[ said something ]\n";
        let doc = parse_document(src).unwrap();
        let options = RenderOptions {
            emit_source_spans: true,
            ..RenderOptions::default()
        };
        let html = render_body_with(&doc, &options);
        let spans = extract_spans(&html);
        assert_eq!(spans.len(), 1);
        assert_eq!(&src[spans[0].0..spans[0].1], src.trim_end());
    }
}
