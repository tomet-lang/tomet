//! What reading a document for a page takes and gives.

use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use tomet::ast::{Declaration, PlainValue};

use crate::Diagnostic;

/// What to read one document with. Every field is optional.
#[derive(Serialize, Deserialize, Default, Debug, Clone)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct ProcessOptions {
    /// Number the headings (`1`, `1.1`, ...) and give each an `id` made from its text: the default
    /// of `numberHeadings` and `autoSlugHeadings`, which switch each on its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub advanced: Option<bool>,
    /// Number the headings. Defaults to `advanced`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number_headings: Option<bool>,
    /// Give every heading an `id` made from its text. Defaults to `advanced`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_slug_headings: Option<bool>,
    /// The language of the page, for the `lang` of what is rendered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    /// Where the document is in the vault, from the root of the files the vault was given: what a
    /// `./` link, a note name and an expression such as `self::path` are read from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_path: Option<String>,
}

impl ProcessOptions {
    pub(crate) fn to_render_options(&self) -> tomet::html::RenderOptions {
        let advanced = self.advanced.unwrap_or(false);
        tomet::html::RenderOptions {
            number_headings: self.number_headings.unwrap_or(advanced),
            auto_slug_headings: self.auto_slug_headings.unwrap_or(advanced),
            lang: self.lang.clone(),
            ..Default::default()
        }
    }
}

/// An entry in the document's table of contents.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TocItem {
    /// The `id` of the heading, to link to it with `#id`.
    pub id: String,
    /// The level of the heading: 2 or 3.
    pub level: u32,
    /// The text of the heading, without its number.
    pub text: String,
}

/// What reading a document for a page gives.
///
/// The document's `::meta` is seen through two windows:
///
/// - `meta` is the data of it, as an object of plain data. An entry that is not data (an element such
///   as `icon: @doc.icon("star")`, a call, `auto`, ...) is not in it: nothing stands in its place, and it
///   is not an error. `meta` is `null` if the document writes no `::meta`, and `{}` if it writes one
///   with no entry that is data.
/// - `header` is everything the document declares, `::meta` included, as the AST: where an application
///   finds what `meta` leaves out, and interprets it. Tomet does not resolve an icon; the application does.
///
/// Both are read after the expressions of the document are resolved.
#[derive(Serialize, Debug, Clone)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ProcessedFile {
    /// The `title` of `::meta`, or else the first heading of level 1. `null` if there is neither.
    pub title: Option<String>,
    /// The body, rendered.
    pub html: String,
    /// The data of `::meta`: an object, or `null` if the document writes none.
    #[cfg_attr(
        feature = "schema",
        schemars(with = "Option<std::collections::BTreeMap<String, PlainValue>>")
    )]
    pub meta: Option<PlainValue>,
    /// Everything the document declares.
    pub header: Vec<Declaration>,
    /// The headings of level 2 and 3.
    pub toc: Vec<TocItem>,
    /// Whether the document has nothing to show: it declares and holds data, and its body is empty.
    pub is_data_only: bool,
    /// Everything found wrong with the document, or that it says about itself, in the order it is in
    /// the text: what `tomet check` reports, a link or an embed that goes nowhere, an expression that
    /// has no value. A page with a diagnostic of any severity is not as its author wrote it.
    pub diagnostics: Vec<Diagnostic>,
}

/// The headings of `html` that are of level 2 and 3, and the title the first one of level 1 is.
pub(crate) fn outline(html: &str) -> (Option<String>, Vec<TocItem>) {
    static HEADING_RE: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r#"<h([1-6])\b([^>]*)>([\s\S]*?)</h([1-6])>"#).unwrap());
    static ID_RE: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r#"id="([^"]*)""#).unwrap());
    static HEADING_NUMBER_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r#"<span class="tm-heading-number">[^<]*</span>"#).unwrap()
    });
    static TAG_RE: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r#"<[^>]+>"#).unwrap());

    let mut first_h1: Option<String> = None;
    let mut toc = Vec::new();
    for cap in HEADING_RE.captures_iter(html) {
        let open_level: u32 = cap[1].parse().unwrap_or(1);
        let close_level: u32 = cap[4].parse().unwrap_or(1);
        if open_level != close_level {
            continue;
        }
        let level = open_level;
        let id = ID_RE
            .captures(&cap[2])
            .map(|c| c[1].to_string())
            .unwrap_or_default();
        let cleaned = HEADING_NUMBER_RE.replace_all(&cap[3], "");
        let text = TAG_RE.replace_all(&cleaned, "").trim().to_string();

        if level == 1 && first_h1.is_none() && !text.is_empty() {
            first_h1 = Some(text.clone());
        }
        if (level == 2 || level == 3) && !text.is_empty() {
            toc.push(TocItem { id, level, text });
        }
    }
    (first_h1, toc)
}
