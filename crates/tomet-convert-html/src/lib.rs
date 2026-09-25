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

mod block;
mod element;
mod footnotes;
mod headings;
mod link;
mod table;
mod text;
mod util;

use std::fmt;
use std::sync::Arc;

use crate::block::render_block;
use crate::footnotes::render_footnotes;
use crate::headings::HeadingState;
use crate::util::{escape_attr, escape_html};
use tomet_ast::{Document, Inline, Value};
use tomet_semantics::{Bindings, FootnoteRegistry, builtin_doc_vocabularies};

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
    /// Wrap hierarchical sections in `<section class="tmt-section level-{level}">` tags.
    /// Default is `false` (plain unnested `<hN>` headings).
    pub wrap_sections: bool,
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
    let footnotes = FootnoteRegistry::from_document(doc);
    let cx = RenderCtx {
        options,
        bindings: &bindings,
        footnotes: &footnotes,
    };
    for block in &doc.blocks {
        render_block(&cx, block, &mut out, &mut state);
    }
    render_footnotes(&cx, &mut out);
    (out, state.outline)
}

struct RenderCtx<'a> {
    options: &'a RenderOptions,
    bindings: &'a Bindings,
    footnotes: &'a FootnoteRegistry,
}

#[cfg(test)]
mod tests;
