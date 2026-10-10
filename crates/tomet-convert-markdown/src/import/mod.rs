//! CommonMark -> `tomet_ast::Document`, folding `pulldown-cmark`'s
//! event stream directly (no intermediate tree) using an explicit frame
//! stack, one frame per currently-open tag.
//!
//! Constructs that map cleanly reuse `tomet_ast` shapes that already
//! exist for Tomet's own native shorthand (`em`/`strong`/`hr`, list
//! elements -- see `Element::list`/`Element::list_item`). Constructs with
//! no native equivalent (fenced/indented code, block quotes) go through
//! the generic named-element escape hatch -- `@raw` and
//! `@blockquote`, which are ordinary elements rather than AST variants.
//!
//! One thing is structurally lossy on import: a block
//! quote containing more than one block gets its content joined into a
//! single inline run (`Element::content` is `Vec<Inline>`, not
//! `Vec<Block>`), so a list inside a quote has its items' content
//! flattened into that run too. A nested list under a plain (non-quote)
//! list item is not lossy -- it's kept as that item's own
//! `ListItem::sublist`. HTML
//! blocks are dropped; inline HTML round-trips as literal text. Soft and
//! hard breaks map to `Inline::SoftBreak`/`Inline::LineBreak` respectively,
//! not collapsed into a space -- see those types' doc comments.
//!
//! YAML frontmatter extraction lives in `frontmatter`; `[[wiki]]`/bare-URL
//! detection and sigil-escaping (both post-processing passes over
//! already-imported inline text) live in `wikilink`. Table parsing and
//! column width calculation live in `table`. This file keeps the core
//! pulldown-cmark event-stream state machine (the part that's genuinely
//! one cohesive piece -- frame push/pop per event).

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use tomet_ast::{
    Block, Document, Element, Inline, LineBreak, List, ListItem, Paragraph, Placement, RawText,
    Section, Sigil, SoftBreak, Span, Text, Value,
};
use tomet_semantics::{ElementKind, classify_std_lenient};
use tomet_tree::{element_new, list, list_item};

mod frontmatter;
mod table;
#[cfg(test)]
mod tests;
mod wikilink;

use table::{build_table_element, preprocess_markdown_tables};

enum Frame {
    /// Top-level document, and the fallback container for anything that
    /// doesn't need special shape-adaptation (only ever the root here).
    Blocks(Vec<Block>),
    Paragraph(Vec<Inline>),
    Heading(u8, Vec<Inline>),
    /// A block quote's content, already flattened to inlines as blocks
    /// close inside it (see `merge_block_into`).
    BlockQuote(Vec<Inline>),
    /// A list item's content. `sublist` is the one nested list directly
    /// under this item, if any -- matches `ListItem.sublist`'s own
    /// single slot (see `tomet_ast::ListItem`).
    Item {
        content: Vec<Inline>,
        sublist: Option<List>,
        marker: Option<Value>,
    },
    List {
        ordered: bool,
        items: Vec<ListItem>,
    },
    Emphasis(Vec<Inline>),
    /// GFM's `~~x~~`. Tomet spells it the same way, so the tildes used to
    /// reach the output verbatim and be re-read by Tomet's own parser --
    /// which produced the right file and the wrong `Document`, since
    /// `from_markdown`'s own result held text rather than an element.
    Strikeout(Vec<Inline>),
    Strong(Vec<Inline>),
    Link {
        dest: String,
        inlines: Vec<Inline>,
    },
    Image {
        dest: String,
        alt: Vec<Inline>,
    },
    CodeBlock {
        lang: String,
        text: String,
    },
    Table {
        rows: Vec<Vec<Vec<Inline>>>,
        alignments: Vec<pulldown_cmark::Alignment>,
    },
    TableHead {
        cells: Vec<Vec<Inline>>,
    },
    TableRow {
        cells: Vec<Vec<Inline>>,
    },
    TableCell {
        content: Vec<Inline>,
    },
    /// HTML blocks/inline HTML -- swallow everything until the matching
    /// End, out of scope for v1 (see module docs).
    Discard,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportOptions {
    /// Align column widths by adding spaces inside table cells (`table.adjust_width`).
    pub adjust_table_width: bool,
    /// Mode for table width adjustment ("true", "false", "auto").
    pub table_adjust_width_mode: Option<String>,
    /// Max column width threshold for alignment when auto mode is enabled.
    pub table_max_col_width: Option<usize>,
    /// Default table column alignment ("left", "center", "right").
    pub table_align: Option<String>,
}

/// Wraps a flat inline sequence as `Element.content` (`Vec<Block>` now):
/// one `Paragraph`, or nothing for an empty sequence. Every `Frame`
/// accumulates `Vec<Inline>` internally (unchanged -- see the module doc's
/// note on block quotes/list items flattening to one inline run on
/// import), so this is the single place that adapts all of them to the
/// new field type without changing what any of them actually produce.
pub(super) fn wrap_inline_content(inlines: Vec<Inline>) -> Vec<Block> {
    if inlines.is_empty() {
        Vec::new()
    } else {
        vec![Block::Paragraph(Paragraph::new(inlines, Span::dummy()))]
    }
}

pub fn from_markdown(src: &str) -> Document {
    from_markdown_with_options(src, &ImportOptions::default())
}

pub fn from_markdown_with_options(src: &str, options: &ImportOptions) -> Document {
    let (frontmatter, markdown_body) = frontmatter::extract_yaml_frontmatter(src);
    let preprocessed_body = preprocess_markdown_tables(markdown_body);

    let mut options_flags = Options::empty();
    options_flags.insert(Options::ENABLE_TABLES);
    options_flags.insert(Options::ENABLE_TASKLISTS);
    options_flags.insert(Options::ENABLE_GFM);
    // Separate from `ENABLE_GFM`, which turns on GFM's block extensions
    // and not this. Without it `~~x~~` reaches the output as tildes, and
    // Tomet's parser reads them back as a `strikeout` -- the right file
    // from the wrong `Document`, which anyone calling `from_markdown` as
    // a library got the wrong half of.
    options_flags.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(&preprocessed_body, options_flags);
    let mut stack: Vec<Frame> = vec![Frame::Blocks(Vec::new())];

    if let Some(entries) = frontmatter {
        let mut meta_el = element_new(Sigil::named("meta"));
        meta_el.value = Some(tomet_ast::ElementValue::from_map(Value::Map(entries)));
        push_block(&mut stack, Block::Element(meta_el));
    }

    for event in parser {
        match event {
            Event::Start(tag) => stack.push(start_frame(tag)),
            Event::End(tag_end) => end_frame(&mut stack, tag_end, options),
            Event::TaskListMarker(is_checked) => {
                // Tomet has no checkbox construct anymore -- fall back
                // to the same "not a recognized construct, keep it as
                // literal text" treatment `[...]`/unrecognized brackets get
                // everywhere else in this crate and in `tomet-parser`,
                // rather than silently dropping the marker (this importer
                // is also what drives real-world Markdown migration).
                let text = if is_checked { "[x] " } else { "[ ] " };
                push_inline(&mut stack, Inline::Text(Text::new(text, Span::dummy())));
            }
            Event::Text(text) => match stack.last_mut() {
                Some(Frame::CodeBlock { text: buf, .. }) => buf.push_str(&text),
                Some(Frame::Discard) => {}
                _ => push_inline(
                    &mut stack,
                    Inline::Text(Text::new(text.to_string(), Span::dummy())),
                ),
            },
            Event::Code(code) => {
                if !matches!(stack.last(), Some(Frame::Discard)) {
                    push_inline(
                        &mut stack,
                        wrap_inline(
                            "raw",
                            vec![Inline::Raw(RawText::new(code.to_string(), Span::dummy()))],
                        ),
                    );
                }
            }
            Event::InlineHtml(html) => {
                if !matches!(stack.last(), Some(Frame::Discard)) {
                    push_inline(
                        &mut stack,
                        Inline::Text(Text::new(html.to_string(), Span::dummy())),
                    );
                }
            }
            Event::Html(_) => {}
            Event::SoftBreak => push_inline(
                &mut stack,
                Inline::SoftBreak(SoftBreak {
                    span: Span::dummy(),
                }),
            ),
            Event::HardBreak => push_inline(
                &mut stack,
                Inline::LineBreak(LineBreak {
                    span: Span::dummy(),
                }),
            ),
            Event::Rule => push_block(&mut stack, Block::Element(element_new(Sigil::named("hr")))),
            _ => {}
        }
    }

    let root = stack.pop().expect("root frame always present");
    let mut doc = match root {
        Frame::Blocks(blocks) => Document::new(structure_sections(blocks), Span::dummy()),
        _ => Document::default(),
    };
    wikilink::post_process_document_wikilinks(&mut doc);
    doc
}

fn structure_sections(blocks: Vec<Block>) -> Vec<Block> {
    let mut doc_blocks = Vec::new();
    let mut stack: Vec<Section> = Vec::new();

    for block in blocks {
        match block {
            Block::Section(sec) => {
                while let Some(top) = stack.last() {
                    if top.level >= sec.level {
                        let finished = stack.pop().unwrap();
                        if let Some(parent) = stack.last_mut() {
                            parent.blocks.push(Block::Section(finished));
                        } else {
                            doc_blocks.push(Block::Section(finished));
                        }
                    } else {
                        break;
                    }
                }
                stack.push(sec);
            }
            other => {
                if let Some(top) = stack.last_mut() {
                    top.blocks.push(other);
                } else {
                    doc_blocks.push(other);
                }
            }
        }
    }

    while let Some(finished) = stack.pop() {
        if let Some(parent) = stack.last_mut() {
            parent.blocks.push(Block::Section(finished));
        } else {
            doc_blocks.push(Block::Section(finished));
        }
    }

    doc_blocks
}

fn start_frame(tag: Tag) -> Frame {
    match tag {
        Tag::Paragraph => Frame::Paragraph(Vec::new()),
        Tag::Heading { level, .. } => Frame::Heading(level as u8, Vec::new()),
        Tag::BlockQuote(_) => Frame::BlockQuote(Vec::new()),
        Tag::CodeBlock(kind) => {
            let lang = match kind {
                CodeBlockKind::Fenced(lang) => lang.into_string(),
                CodeBlockKind::Indented => String::new(),
            };
            Frame::CodeBlock {
                lang,
                text: String::new(),
            }
        }
        Tag::List(start) => Frame::List {
            ordered: start.is_some(),
            items: Vec::new(),
        },
        Tag::Item => Frame::Item {
            content: Vec::new(),
            sublist: None,
            marker: None,
        },
        Tag::Emphasis => Frame::Emphasis(Vec::new()),
        Tag::Strong => Frame::Strong(Vec::new()),
        Tag::Strikethrough => Frame::Strikeout(Vec::new()),
        Tag::Link { dest_url, .. } => Frame::Link {
            dest: dest_url.into_string(),
            inlines: Vec::new(),
        },
        Tag::Image { dest_url, .. } => Frame::Image {
            dest: dest_url.into_string(),
            alt: Vec::new(),
        },
        Tag::Table(alignments) => Frame::Table {
            rows: Vec::new(),
            alignments,
        },
        Tag::TableHead => Frame::TableHead { cells: Vec::new() },
        Tag::TableRow => Frame::TableRow { cells: Vec::new() },
        Tag::TableCell => Frame::TableCell {
            content: Vec::new(),
        },
        // HtmlBlock and anything gated behind unset Options (footnotes, strikethrough, etc.).
        _ => Frame::Discard,
    }
}

fn end_frame(stack: &mut Vec<Frame>, tag_end: TagEnd, options: &ImportOptions) {
    let frame = stack.pop().expect("End without matching Start");
    match (frame, tag_end) {
        (Frame::Paragraph(inlines), TagEnd::Paragraph) => push_block(
            stack,
            Block::Paragraph(Paragraph::new(inlines, Span::dummy())),
        ),
        (Frame::Heading(level, inlines), TagEnd::Heading(_)) => push_block(
            stack,
            Block::Section(Section::new(level as usize, inlines, Span::dummy())),
        ),
        (Frame::BlockQuote(mut content), TagEnd::BlockQuote(_)) => {
            let mut is_callout = false;
            let mut variant = String::new();
            let mut title = None;

            // `[!type] title` is a whole line: with a source line break now
            // its own `SoftBreak` node rather than a `'\n'` embedded in
            // this `Text` (see `Inline::SoftBreak`), the entire title line
            // lives in this one first `Text` -- nothing to split on `\n`
            // for any more. The title line itself, and the `SoftBreak`
            // right after it if there is one, get dropped below rather
            // than truncated in place.
            if let Some(Inline::Text(t)) = content.first() {
                let val_trimmed = t.value.trim_start();
                if val_trimmed.starts_with("[!")
                    && let Some(end_bracket) = val_trimmed.find(']')
                {
                    let kind_str = val_trimmed[2..end_bracket].trim().to_lowercase();
                    if !kind_str.is_empty() {
                        is_callout = true;
                        variant = kind_str;
                        let clean_title = val_trimmed[end_bracket + 1..]
                            .trim_start_matches([' ', '-', '+', '|'])
                            .trim();
                        if !clean_title.is_empty() {
                            title = Some(clean_title.to_string());
                        }
                    }
                }
            }

            if is_callout {
                content.remove(0);
                // Everything between the title line and the real content --
                // the `SoftBreak` right after the title, a blank
                // blockquote line's own empty `Text`/`SoftBreak` pair, any
                // number of them -- is whitespace-equivalent and gets
                // dropped, the same as leading whitespace anywhere else.
                while let Some(first_inline) = content.first() {
                    match first_inline {
                        Inline::SoftBreak(_) => {
                            content.remove(0);
                        }
                        Inline::Text(t) if t.value.trim().is_empty() => {
                            content.remove(0);
                        }
                        _ => break,
                    }
                }
                if let Some(Inline::Text(t)) = content.first_mut() {
                    t.value = t.value.trim_start().to_string();
                }

                let args = if let Some(t_str) = title {
                    Value::Map(vec![
                        ("variant".to_string(), Value::String(variant)),
                        ("title".to_string(), Value::String(t_str)),
                    ])
                } else {
                    Value::String(variant)
                };

                let el = Element {
                    sigil: Sigil::named("callout"),
                    placement: Placement::Block,
                    args: Some(args),
                    content: Some(wrap_inline_content(content)),
                    value: None,
                    id: None,
                    connects: Vec::new(),
                    span: Span::dummy(),
                };
                push_block(stack, Block::Element(el));
            } else {
                let el = Element {
                    sigil: Sigil::named("quote"),
                    placement: Placement::Block,
                    args: None,
                    content: Some(wrap_inline_content(content)),
                    value: None,
                    id: None,
                    connects: Vec::new(),
                    span: Span::dummy(),
                };
                push_block(stack, Block::Element(el));
            }
        }
        (Frame::CodeBlock { lang, mut text }, TagEnd::CodeBlock) => {
            if text.ends_with('\n') {
                text.pop();
            }
            let args = if lang.is_empty() {
                None
            } else {
                Some(Value::Map(vec![("lang".to_string(), Value::String(lang))]))
            };
            let el = Element {
                sigil: Sigil::named("raw"),
                placement: Placement::Block,
                args,
                content: Some(vec![Block::Paragraph(Paragraph::new(
                    vec![Inline::Raw(RawText::new(text, Span::dummy()))],
                    Span::dummy(),
                ))]),
                value: None,
                id: None,
                connects: Vec::new(),
                span: Span::dummy(),
            };
            push_block(stack, Block::Element(el));
        }
        (Frame::List { ordered, items }, TagEnd::List(_)) => {
            push_block(stack, Block::List(list(ordered, items, Span::dummy())))
        }
        (
            Frame::Item {
                mut content,
                sublist,
                mut marker,
            },
            TagEnd::Item,
        ) => match stack.last_mut() {
            Some(Frame::List { items, .. }) => {
                // Not a real CommonMark construct -- sniff leading `(...)`
                // text for Tomet's own marker shorthand. No grammar
                // available here (this crate deliberately depends on
                // `tomet-ast` only, not `tomet-parser`), so its
                // inner text becomes a bare string marker rather than a
                // fully parsed `Value`.
                if marker.is_none()
                    && let Some(Inline::Text(t)) = content.first_mut()
                {
                    let s = t.value.trim_start();
                    if s.starts_with('(') {
                        if let Some(close_idx) = s.find(')')
                            && close_idx >= 1
                            && s[close_idx..].starts_with(") ")
                        {
                            let inner = &s[1..close_idx];
                            marker = Some(Value::String(inner.to_string()));
                            let remainder = s[close_idx + 2..].to_string();
                            t.value = remainder;
                        }
                    } else if s.starts_with('[')
                        && let Some(close_idx) = s.find(']')
                    {
                        let is_marker = if close_idx >= 1 {
                            if s[close_idx..].starts_with("] ") {
                                Some(close_idx + 2)
                            } else if close_idx == s.len() - 1 {
                                Some(close_idx + 1)
                            } else {
                                None
                            }
                        } else {
                            None
                        };
                        if let Some(after_idx) = is_marker {
                            let inner = &s[1..close_idx];
                            if !inner.starts_with('[') {
                                let marker_val = if inner.is_empty() || inner == " " {
                                    " "
                                } else {
                                    inner
                                };
                                marker = Some(Value::String(marker_val.to_string()));
                                let remainder = s[after_idx..].to_string();
                                t.value = remainder;
                            }
                        }
                    }
                }
                if let Some(Inline::Text(t)) = content.first()
                    && t.value.is_empty()
                    && content.len() > 1
                {
                    content.remove(0);
                }
                items.push(list_item(
                    Sigil::Bare,
                    wrap_inline_content(content),
                    marker,
                    None,
                    None,
                    Vec::new(),
                    sublist,
                    Span::dummy(),
                ));
            }
            _ => unreachable!("Item is always nested directly inside List"),
        },
        (Frame::Emphasis(inlines), TagEnd::Emphasis) => {
            push_inline(stack, wrap_inline("em", inlines));
        }
        (Frame::Strong(inlines), TagEnd::Strong) => {
            push_inline(stack, wrap_inline("strong", inlines));
        }
        (Frame::Strikeout(inlines), TagEnd::Strikethrough) => {
            push_inline(stack, wrap_inline("strikeout", inlines));
        }
        (Frame::Link { dest, inlines }, TagEnd::Link) => {
            // `target_scheme` (downstream, in `tomet-semantics`)
            // classifies `dest` by its own shape (`scheme://...` -> url,
            // otherwise -> file) once this is rendered/queried -- no key
            // choice needed here, unlike the old per-kind key scheme.
            let el = Element {
                sigil: Sigil::named("link"),
                placement: Placement::Inline,
                args: Some(Value::Map(vec![(
                    "target".to_string(),
                    Value::String(dest),
                )])),
                content: Some(wrap_inline_content(inlines)),
                value: None,
                id: None,
                connects: Vec::new(),
                span: Span::dummy(),
            };
            push_inline(stack, Inline::Element(el));
        }
        (Frame::Image { dest, alt }, TagEnd::Image) => {
            let content = if alt.is_empty() {
                None
            } else {
                Some(wrap_inline_content(alt))
            };
            let el = Element {
                sigil: Sigil::named("embed"),
                placement: Placement::Inline,
                args: Some(Value::Map(vec![(
                    "target".to_string(),
                    Value::String(dest),
                )])),
                content,
                value: None,
                id: None,
                connects: Vec::new(),
                span: Span::dummy(),
            };
            push_inline(stack, Inline::Element(el));
        }
        (Frame::TableCell { content }, TagEnd::TableCell) => match stack.last_mut() {
            Some(Frame::TableRow { cells }) | Some(Frame::TableHead { cells }) => {
                cells.push(content);
            }
            _ => {}
        },
        (Frame::TableRow { cells }, TagEnd::TableRow) => {
            for frame in stack.iter_mut().rev() {
                if let Frame::Table { rows, .. } = frame {
                    rows.push(cells);
                    break;
                }
            }
        }
        (Frame::TableHead { cells }, TagEnd::TableHead) => {
            for frame in stack.iter_mut().rev() {
                if let Frame::Table { rows, .. } = frame {
                    rows.push(cells);
                    break;
                }
            }
        }
        (Frame::Table { rows, alignments }, TagEnd::Table) => {
            let el = build_table_element(rows, alignments, options);
            push_block(stack, Block::Element(el));
        }
        // HtmlBlock and anything routed to Discard.
        (Frame::Discard, _) => {}
        (_, _) => {}
    }
}

pub(super) fn wrap_inline(tag: &str, content: Vec<Inline>) -> Inline {
    Inline::Element(Element {
        sigil: Sigil::named(tag),
        placement: Placement::Inline,
        args: None,
        content: Some(wrap_inline_content(content)),
        value: None,
        id: None,
        connects: Vec::new(),
        span: Span::dummy(),
    })
}

fn inline_target(stack: &mut [Frame]) -> Option<&mut Vec<Inline>> {
    match stack.last_mut()? {
        Frame::Paragraph(v) => Some(v),
        Frame::Heading(_, v) => Some(v),
        Frame::Emphasis(v) => Some(v),
        Frame::Strong(v) => Some(v),
        Frame::Strikeout(v) => Some(v),
        Frame::Link { inlines, .. } => Some(inlines),
        Frame::Image { alt, .. } => Some(alt),
        Frame::Item { content, .. } => Some(content),
        Frame::BlockQuote(content) => Some(content),
        Frame::TableCell { content } => Some(content),
        _ => None,
    }
}

/// Append a bare inline event (text, code span, emphasis/strong/link
/// result, break) to whichever frame is currently accumulating inlines.
/// List items can receive these directly (CommonMark "tight" lists put
/// inline content straight under `Item`, with no `Paragraph` wrapper).
fn push_inline(stack: &mut [Frame], inline: Inline) {
    let Some(target) = inline_target(stack) else {
        return;
    };
    if let (Inline::Text(new), Some(Inline::Text(prev))) = (&inline, target.last_mut()) {
        prev.value.push_str(&new.value);
        return;
    }
    target.push(inline);
}

/// Append a completed block into whatever the new stack top is, adapting
/// its shape: a `Root` frame takes it as-is, while `BlockQuote`/`Item`
/// frames only hold inlines, so paragraphs/headings get their content
/// joined in (space-separated -- multi-paragraph quotes/items are a
/// documented lossy case), plain elements (code blocks, nested quotes,
/// `hr`, ...) come along as a single `Inline::Element`, and a nested
/// list becomes the item's own `sublist` (`BlockQuote`, which has no
/// `sublist` concept, flattens it into text instead).
fn push_block(stack: &mut [Frame], block: Block) {
    match stack.last_mut() {
        Some(Frame::Blocks(v)) => v.push(block),
        Some(Frame::BlockQuote(content)) => merge_block_into(content, block),
        Some(Frame::Item {
            content, sublist, ..
        }) => match block {
            Block::List(l) => {
                // At most one nested list per item, matching
                // `ListItem.sublist`'s single slot (see
                // `tomet_ast::ListItem`) -- a second one directly under
                // the same item (rare, and arguably not well-defined
                // CommonMark) is dropped rather than silently
                // overwriting the first.
                if sublist.is_none() {
                    *sublist = Some(l);
                }
            }
            other => merge_block_into(content, other),
        },
        _ => {}
    }
}

/// Flattens an `Element.content` (`Vec<Block>` now) down to one inline
/// run, by folding each block in with [`merge_block_into`] -- used where
/// this importer already flattens surrounding structure to a single
/// inline run (blockquotes, a heading/list merged into one) and now needs
/// to do the same to a nested element's own content, which is no longer
/// `Vec<Inline>` by construction.
fn blocks_to_flat_inlines(blocks: Vec<Block>) -> Vec<Inline> {
    let mut out = Vec::new();
    for block in blocks {
        merge_block_into(&mut out, block);
    }
    out
}

fn merge_block_into(content: &mut Vec<Inline>, block: Block) {
    match block {
        Block::Paragraph(p) => extend_spaced(content, p.content),
        // A heading merged into flattened blockquote/item content splices
        // its title text directly in, same as a paragraph -- not wrapped
        // as a nested `Inline::Element`, which is what the generic
        // `Block::Element` arm would do.
        Block::Element(el) if classify_std_lenient(&el) == ElementKind::Heading => extend_spaced(
            content,
            blocks_to_flat_inlines(el.content.unwrap_or_default()),
        ),
        // A list merged into flattened blockquote content (blockquotes
        // have no `sublist` concept, unlike `Item`) has each of its
        // items' content joined in the same way -- see the module doc.
        Block::List(list) => {
            for item in list.items {
                extend_spaced(
                    content,
                    blocks_to_flat_inlines(item.element.content.unwrap_or_default()),
                );
            }
        }
        Block::Element(el) => {
            if !content.is_empty() {
                content.push(Inline::Text(Text::new(" ", Span::dummy())));
            }
            content.push(Inline::Element(el));
        }
        Block::Section(sec) => {
            extend_spaced(content, sec.title);
            for child in sec.blocks {
                merge_block_into(content, child);
            }
        }
    }
}

fn extend_spaced(content: &mut Vec<Inline>, more: Vec<Inline>) {
    if !content.is_empty() && !more.is_empty() {
        content.push(Inline::Text(Text::new(" ", Span::dummy())));
    }
    content.extend(more);
}
