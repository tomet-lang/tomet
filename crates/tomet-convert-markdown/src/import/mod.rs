//! CommonMark -> `tomet_ast::Document`, folding `pulldown-cmark`'s
//! event stream directly (no intermediate tree) using an explicit frame
//! stack, one frame per currently-open tag.
//!
//! Constructs that map cleanly reuse `tomet_ast` shapes that already
//! exist for Tomet's own native shorthand (`em`/`strong`/`hr`, list
//! elements -- see `Element::list`/`Element::list_item`). Constructs with
//! no native equivalent (fenced/indented code, block quotes) go through
//! the generic `<T>` element escape hatch (`pre`, `blockquote`)
//! documented in `docs/commonmark-support.md`.
//!
//! One thing is structurally lossy on import, documented there: a block
//! quote containing more than one block gets its content joined into a
//! single inline run (`Element::content` is `Vec<Inline>`, not
//! `Vec<Block>`), so a list inside a quote has its items' content
//! flattened into that run too. A nested list under a plain (non-quote)
//! list item is not lossy -- it's kept as that item's own `children`. HTML
//! blocks and inline HTML are dropped; hard breaks collapse to a space.
//!
//! YAML frontmatter extraction lives in `frontmatter`; `[[wiki]]`/bare-URL
//! detection and sigil-escaping (both post-processing passes over
//! already-imported inline text) live in `wikilink`. This file keeps the
//! core pulldown-cmark event-stream state machine (the part that's
//! genuinely one cohesive piece -- frame push/pop per event).

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use tomet_ast::{
    Block, Document, Element, ElementValue, Inline, Paragraph, Sigil, Span, Text, Value,
};
use tomet_semantics::{ElementKind, classify, list_ordered};
use tomet_tree::{element_list, element_list_item, element_new};

mod frontmatter;
mod wikilink;

enum Frame {
    /// Top-level document, and the fallback container for anything that
    /// doesn't need special shape-adaptation (only ever the root here).
    Blocks(Vec<Block>),
    Paragraph(Vec<Inline>),
    Heading(u8, Vec<Inline>),
    /// A block quote's content, already flattened to inlines as blocks
    /// close inside it (see `merge_block_into`).
    BlockQuote(Vec<Inline>),
    /// A list item's content. `extra` collects items promoted out of a
    /// nested list (flattening) so `List`'s End handler can splice them
    /// in as siblings right after this item.
    Item {
        content: Vec<Inline>,
        children: Vec<Block>,
        marker: Option<Value>,
    },
    List {
        ordered: bool,
        items: Vec<Element>,
    },
    Emphasis(Vec<Inline>),
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
    let parser = Parser::new_ext(&preprocessed_body, options_flags);
    let mut stack: Vec<Frame> = vec![Frame::Blocks(Vec::new())];

    if let Some(entries) = frontmatter {
        let mut meta_el = element_new(Sigil::At(Some("meta".to_string())));
        meta_el.value = Some(tomet_ast::ElementValue::Data(Value::Map(entries)));
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
                        Inline::Text(Text::new(format!("`{code}`"), Span::dummy())),
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
            Event::SoftBreak => {
                push_inline(&mut stack, Inline::Text(Text::new("\n", Span::dummy())))
            }
            Event::HardBreak => {
                push_inline(&mut stack, Inline::Text(Text::new("\n", Span::dummy())))
            }
            Event::Rule => push_block(
                &mut stack,
                Block::Element(element_new(Sigil::Type("hr".to_string()))),
            ),
            _ => {}
        }
    }

    let root = stack.pop().expect("root frame always present");
    let mut doc = match root {
        Frame::Blocks(blocks) => Document::new(blocks, Span::dummy()),
        _ => Document::default(),
    };
    wikilink::post_process_document_wikilinks(&mut doc);
    doc
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
            children: Vec::new(),
            marker: None,
        },
        Tag::Emphasis => Frame::Emphasis(Vec::new()),
        Tag::Strong => Frame::Strong(Vec::new()),
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
            Block::Element(Element {
                sigil: Sigil::At(Some("heading".to_string())),
                // Imported headings never carry `id`/`cssclass` -- CommonMark
                // has nothing to import them from.
                args: Some(Value::Int(level as i64)),
                content: Some(inlines),
                children: None,
                value: None,
                span: Span::dummy(),
            }),
        ),
        (Frame::BlockQuote(mut content), TagEnd::BlockQuote(_)) => {
            let mut is_callout = false;
            let mut variant = String::new();
            let mut title = None;

            if let Some(first_inline) = content.first_mut() {
                if let Inline::Text(t) = first_inline {
                    let val_trimmed = t.value.trim_start();
                    if val_trimmed.starts_with("[!") {
                        if let Some(end_bracket) = val_trimmed.find(']') {
                            let kind_str = val_trimmed[2..end_bracket].trim().to_lowercase();
                            if !kind_str.is_empty() {
                                is_callout = true;
                                variant = kind_str;
                                let after_bracket = &val_trimmed[end_bracket + 1..];
                                let (first_line, rest_lines) = match after_bracket.find('\n') {
                                    Some(idx) => (&after_bracket[..idx], &after_bracket[idx + 1..]),
                                    None => (after_bracket, ""),
                                };
                                let clean_title = first_line
                                    .trim_start_matches(|c| {
                                        c == ' ' || c == '-' || c == '+' || c == '|'
                                    })
                                    .trim();
                                if !clean_title.is_empty() {
                                    title = Some(clean_title.to_string());
                                }
                                t.value = rest_lines.trim_start_matches(['\r', '\n']).to_string();
                            }
                        }
                    }
                }
            }

            if is_callout {
                while let Some(first_inline) = content.first() {
                    if let Inline::Text(t) = first_inline {
                        if t.value.trim().is_empty() {
                            content.remove(0);
                            continue;
                        }
                    }
                    break;
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
                    sigil: Sigil::Type("callout".to_string()),
                    args: Some(args),
                    content: Some(content),
                    children: None,
                    value: None,
                    span: Span::dummy(),
                };
                push_block(stack, Block::Element(el));
            } else {
                let el = Element {
                    sigil: Sigil::Type("blockquote".to_string()),
                    args: None,
                    content: Some(content),
                    children: None,
                    value: None,
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
                sigil: Sigil::Type("codeblock".to_string()),
                args,
                content: Some(vec![Inline::Text(Text::new(text, Span::dummy()))]),
                children: None,
                value: None,
                span: Span::dummy(),
            };
            push_block(stack, Block::Element(el));
        }
        (Frame::List { ordered, items }, TagEnd::List(_)) => push_block(
            stack,
            Block::Element(element_list(ordered, items, Span::dummy())),
        ),
        (
            Frame::Item {
                mut content,
                children,
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
                if marker.is_none() {
                    if let Some(Inline::Text(t)) = content.first_mut() {
                        let s = t.value.trim_start();
                        if s.starts_with('(') {
                            if let Some(close_idx) = s.find(')') {
                                if close_idx >= 1 && s[close_idx..].starts_with(") ") {
                                    let inner = &s[1..close_idx];
                                    marker = Some(Value::String(inner.to_string()));
                                    let remainder = s[close_idx + 2..].to_string();
                                    t.value = remainder;
                                }
                            }
                        } else if s.starts_with('[') {
                            if let Some(close_idx) = s.find(']') {
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
                    }
                }
                if let Some(Inline::Text(t)) = content.first() {
                    if t.value.is_empty() && content.len() > 1 {
                        content.remove(0);
                    }
                }
                items.push(element_list_item(
                    content,
                    marker,
                    None,
                    children,
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
        (Frame::Link { dest, inlines }, TagEnd::Link) => {
            // `target_scheme` (downstream, in `tomet-semantics`)
            // classifies `dest` by its own shape (`scheme://...` -> url,
            // otherwise -> file) once this is rendered/queried -- no key
            // choice needed here, unlike the old per-kind key scheme.
            let el = Element {
                sigil: Sigil::At(Some("link".to_string())),
                args: Some(Value::Map(vec![(
                    "target".to_string(),
                    Value::String(dest),
                )])),
                content: Some(inlines),
                children: None,
                value: None,
                span: Span::dummy(),
            };
            push_inline(stack, Inline::Element(el));
        }
        (Frame::Image { dest, alt }, TagEnd::Image) => {
            let content = if alt.is_empty() { None } else { Some(alt) };
            let el = Element {
                sigil: Sigil::Type("embed".to_string()),
                args: Some(Value::Map(vec![(
                    "target".to_string(),
                    Value::String(dest),
                )])),
                content,
                children: None,
                value: None,
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

fn char_display_width(c: char) -> usize {
    if c.is_ascii() {
        1
    } else {
        // CJK fullwidth characters, emojis, etc. count as 2 columns
        2
    }
}

fn inlines_display_width(inlines: &[Inline]) -> usize {
    let mut len = 0;
    for inline in inlines {
        match inline {
            Inline::Text(t) => {
                let trimmed = t.value.trim();
                for c in trimmed.chars() {
                    len += char_display_width(c);
                }
            }
            Inline::Element(el) => {
                if let Some(content) = &el.content {
                    len += inlines_display_width(content);
                }
            }
        }
    }
    len
}

fn build_table_element(
    rows: Vec<Vec<Vec<Inline>>>,
    alignments: Vec<pulldown_cmark::Alignment>,
    options: &ImportOptions,
) -> Element {
    let mut content_inlines = Vec::new();
    if rows.is_empty() {
        let mut el = element_new(Sigil::At(Some("table".to_string())));
        el.content = Some(content_inlines);
        return el;
    }

    let mut col_count = 0;
    for row in &rows {
        col_count = col_count.max(row.len());
    }

    let default_align = options.table_align.as_deref().unwrap_or("left");

    let mode = options
        .table_adjust_width_mode
        .as_deref()
        .unwrap_or(if options.adjust_table_width { "true" } else { "false" });

    let max_col_width_limit = options.table_max_col_width.unwrap_or(20);

    let mut max_lens = vec![0usize; col_count];
    for row in &rows {
        for (c_idx, cell) in row.iter().enumerate() {
            let len = inlines_display_width(cell);
            max_lens[c_idx] = max_lens[c_idx].max(len);
        }
    }

    let mut target_widths: Vec<Option<usize>> = vec![None; col_count];
    let mut auto_align_active = true;

    for (c_idx, &max_len) in max_lens.iter().enumerate() {
        match mode {
            "true" | "all" => {
                target_widths[c_idx] = Some(max_len.max(1));
            }
            "auto" => {
                if auto_align_active && max_len <= max_col_width_limit {
                    target_widths[c_idx] = Some(max_len.max(1));
                } else {
                    auto_align_active = false;
                    target_widths[c_idx] = None;
                }
            }
            _ => {
                target_widths[c_idx] = None;
            }
        }
    }

    content_inlines.push(Inline::Text(Text::new("\n", Span::dummy())));

    for row in &rows {
        for c_idx in 0..col_count {
            let cell_inlines = row.get(c_idx).cloned().unwrap_or_default();
            let cell_len = inlines_display_width(&cell_inlines);

            let col_align = alignments
                .get(c_idx)
                .map(|a| match a {
                    pulldown_cmark::Alignment::Right => "right",
                    pulldown_cmark::Alignment::Center => "center",
                    pulldown_cmark::Alignment::Left => "left",
                    pulldown_cmark::Alignment::None => default_align,
                })
                .unwrap_or(default_align);

            let (left_spaces, right_spaces) = if let Some(target_w) = target_widths[c_idx] {
                let target_width = target_w + 2;
                let extra = if target_width > cell_len {
                    target_width - cell_len
                } else {
                    2
                };
                match col_align {
                    "right" => {
                        let left = extra.saturating_sub(1);
                        let right = 1;
                        (left, right)
                    }
                    "center" => {
                        let left = extra / 2;
                        let right = extra - left;
                        (left, right)
                    }
                    _ => {
                        // "left"
                        let left = 1;
                        let right = extra.saturating_sub(1);
                        (left, right)
                    }
                }
            } else {
                (1, 1)
            };

            let left_str = " ".repeat(left_spaces);
            let right_str = " ".repeat(right_spaces);

            content_inlines.push(Inline::Text(Text::new(
                format!("[{left_str}"),
                Span::dummy(),
            )));
            content_inlines.extend(cell_inlines);
            content_inlines.push(Inline::Text(Text::new(
                format!("{right_str}]"),
                Span::dummy(),
            )));
        }
        content_inlines.push(Inline::Text(Text::new("\n", Span::dummy())));
    }

    let mut el = element_new(Sigil::At(Some("table".to_string())));
    el.content = Some(content_inlines);
    el
}

fn wrap_inline(tag: &str, content: Vec<Inline>) -> Inline {
    Inline::Element(Element {
        sigil: Sigil::Type(tag.to_string()),
        args: None,
        content: Some(content),
        children: None,
        value: None,
        span: Span::dummy(),
    })
}

fn preprocess_markdown_tables(src: &str) -> String {
    let mut out = String::with_capacity(src.len() + 32);
    for line in src.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('|') && trimmed.contains("[[") && trimmed.contains('|') {
            let mut in_wikilink = false;
            let mut chars = line.chars().peekable();
            while let Some(ch) = chars.next() {
                if ch == '[' && chars.peek() == Some(&'[') {
                    out.push('[');
                    out.push(chars.next().unwrap());
                    in_wikilink = true;
                } else if in_wikilink && ch == ']' && chars.peek() == Some(&']') {
                    out.push(']');
                    out.push(chars.next().unwrap());
                    in_wikilink = false;
                } else if in_wikilink && ch == '|' && !out.ends_with('\\') {
                    out.push('\\');
                    out.push('|');
                } else {
                    out.push(ch);
                }
            }
            out.push('\n');
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !src.ends_with('\n') && out.ends_with('\n') {
        out.pop();
    }
    out
}

fn inline_target(stack: &mut [Frame]) -> Option<&mut Vec<Inline>> {
    match stack.last_mut()? {
        Frame::Paragraph(v) => Some(v),
        Frame::Heading(_, v) => Some(v),
        Frame::Emphasis(v) => Some(v),
        Frame::Strong(v) => Some(v),
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
/// Adjacent `Text` nodes are merged (e.g. plain text either side of an
/// inline code span) rather than left as separate fragments.
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
/// list's items are either promoted as siblings (`Item`) or flattened
/// into text (`BlockQuote`, which has no sibling-item concept).
fn push_block(stack: &mut [Frame], block: Block) {
    match stack.last_mut() {
        Some(Frame::Blocks(v)) => v.push(block),
        Some(Frame::BlockQuote(content)) => merge_block_into(content, block),
        Some(Frame::Item {
            content, children, ..
        }) => {
            let is_list = matches!(&block, Block::Element(el) if list_ordered(el).is_some());
            if is_list {
                children.push(block);
            } else {
                merge_block_into(content, block);
            }
        }
        _ => {}
    }
}

fn merge_block_into(content: &mut Vec<Inline>, block: Block) {
    match block {
        Block::Paragraph(p) => extend_spaced(content, p.content),
        // A heading merged into flattened blockquote/item content splices
        // its title text directly in, same as a paragraph -- not wrapped
        // as a nested `Inline::Element`, which is what the generic
        // `Block::Element` arm below would do.
        Block::Element(el) if classify(&el) == ElementKind::Heading => {
            extend_spaced(content, el.content.unwrap_or_default())
        }
        // A list merged into flattened blockquote content (blockquotes
        // have no sibling-`children` concept, unlike `Item`) has each of
        // its items' content joined in the same way -- see the module doc.
        Block::Element(mut el) if list_ordered(&el).is_some() => {
            if let Some(ElementValue::Children(items)) = el.value.take() {
                for item in items {
                    extend_spaced(content, item.content.unwrap_or_default());
                }
            }
        }
        Block::Element(el) => {
            if !content.is_empty() {
                content.push(Inline::Text(Text::new(" ", Span::dummy())));
            }
            content.push(Inline::Element(el));
        }
    }
}

fn extend_spaced(content: &mut Vec<Inline>, more: Vec<Inline>) {
    if !content.is_empty() && !more.is_empty() {
        content.push(Inline::Text(Text::new(" ", Span::dummy())));
    }
    content.extend(more);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_semantics::list_items;

    #[test]
    fn heading_and_paragraph() {
        let doc = from_markdown("# Title\n\nHello world.\n");
        assert_eq!(doc.blocks.len(), 2);
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(classify(el), ElementKind::Heading);
                assert_eq!(tomet_semantics::heading_level(el), Some(1));
                assert_eq!(
                    el.content,
                    Some(vec![Inline::Text(Text::new("Title", Span::dummy()))])
                );
            }
            other => panic!("expected heading, got {other:?}"),
        }
        assert_eq!(
            doc.blocks[1],
            Block::Paragraph(Paragraph::new(
                vec![Inline::Text(Text::new("Hello world.", Span::dummy()))],
                Span::dummy()
            ))
        );
    }

    #[test]
    fn emphasis_and_strong() {
        let doc = from_markdown("a *em* b **strong** c\n");
        match &doc.blocks[0] {
            Block::Paragraph(p) => {
                let kinds: Vec<_> = p
                    .content
                    .iter()
                    .filter_map(|i| match i {
                        Inline::Element(el) => Some(el.sigil.clone()),
                        _ => None,
                    })
                    .collect();
                assert_eq!(
                    kinds,
                    vec![
                        Sigil::Type("em".to_string()),
                        Sigil::Type("strong".to_string())
                    ]
                );
            }
            other => panic!("expected paragraph, got {other:?}"),
        }
    }

    #[test]
    fn flat_bullet_list() {
        let doc = from_markdown("- one\n- two\n");
        match &doc.blocks[0] {
            Block::Element(list) => {
                assert_eq!(list_ordered(list), Some(false));
                let items = list_items(list);
                assert_eq!(
                    items[0].content,
                    Some(vec![Inline::Text(Text::new("one", Span::dummy()))])
                );
                assert_eq!(
                    items[1].content,
                    Some(vec![Inline::Text(Text::new("two", Span::dummy()))])
                );
            }
            other => panic!("expected list, got {other:?}"),
        }
    }

    #[test]
    fn ordered_list() {
        let doc = from_markdown("1. one\n2. two\n");
        match &doc.blocks[0] {
            Block::Element(list) => assert_eq!(list_ordered(list), Some(true)),
            other => panic!("expected list, got {other:?}"),
        }
    }

    #[test]
    fn nested_list_preserves_children_hierarchy() {
        let doc = from_markdown("- a\n  - b\n- c\n");
        match &doc.blocks[0] {
            Block::Element(list) => {
                let items = list_items(list);
                assert_eq!(items.len(), 2);
                assert_eq!(
                    items[0].content,
                    Some(vec![Inline::Text(Text::new("a", Span::dummy()))])
                );
                let children = items[0].children.as_ref().expect("nested sub-list");
                assert_eq!(children.len(), 1);
                let Block::Element(sub) = &children[0] else {
                    panic!("expected sub-list");
                };
                let sub_items = list_items(sub);
                assert_eq!(sub_items.len(), 1);
                assert_eq!(
                    sub_items[0].content,
                    Some(vec![Inline::Text(Text::new("b", Span::dummy()))])
                );
                assert_eq!(
                    items[1].content,
                    Some(vec![Inline::Text(Text::new("c", Span::dummy()))])
                );
            }
            other => panic!("expected list, got {other:?}"),
        }
    }

    #[test]
    fn link() {
        let doc = from_markdown("[Wiki](https://example.com)\n");
        match &doc.blocks[0] {
            Block::Paragraph(p) => match &p.content[0] {
                Inline::Element(el) => {
                    assert_eq!(el.sigil, Sigil::At(Some("link".to_string())));
                    assert_eq!(
                        el.args,
                        Some(Value::Map(vec![(
                            "target".to_string(),
                            Value::String("https://example.com".to_string())
                        )]))
                    );
                    assert_eq!(
                        el.content,
                        Some(vec![Inline::Text(Text::new("Wiki", Span::dummy()))])
                    );
                }
                other => panic!("expected element, got {other:?}"),
            },
            other => panic!("expected paragraph, got {other:?}"),
        }
    }

    #[test]
    fn autolink_is_a_plain_link_element() {
        let doc = from_markdown("<https://example.com>\n");
        match &doc.blocks[0] {
            Block::Paragraph(p) => match &p.content[0] {
                Inline::Element(el) => assert_eq!(el.sigil, Sigil::At(Some("link".to_string()))),
                other => panic!("expected element, got {other:?}"),
            },
            other => panic!("expected paragraph, got {other:?}"),
        }
    }

    #[test]
    fn image_becomes_embed_element() {
        let doc = from_markdown("![a cat](assets/pic.png)\n");
        match &doc.blocks[0] {
            Block::Paragraph(p) => match &p.content[0] {
                Inline::Element(el) => {
                    assert_eq!(el.sigil, Sigil::Type("embed".to_string()));
                    assert_eq!(
                        el.args,
                        Some(Value::Map(vec![(
                            "target".to_string(),
                            Value::String("assets/pic.png".to_string())
                        )]))
                    );
                    assert_eq!(
                        el.content,
                        Some(vec![Inline::Text(Text::new("a cat", Span::dummy()))])
                    );
                }
                other => panic!("expected element, got {other:?}"),
            },
            other => panic!("expected paragraph, got {other:?}"),
        }
    }

    #[test]
    fn thematic_break() {
        let doc = from_markdown("---\n");
        match &doc.blocks[0] {
            Block::Element(el) => assert_eq!(el.sigil, Sigil::Type("hr".to_string())),
            other => panic!("expected hr element, got {other:?}"),
        }
    }

    #[test]
    fn fenced_code_block() {
        let doc = from_markdown("```rust\nfn main() {}\n```\n");
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(el.sigil, Sigil::Type("codeblock".to_string()));
                assert_eq!(
                    el.args,
                    Some(Value::Map(vec![(
                        "lang".to_string(),
                        Value::String("rust".to_string())
                    )]))
                );
                assert_eq!(
                    el.content,
                    Some(vec![Inline::Text(Text::new("fn main() {}", Span::dummy()))])
                );
            }
            other => panic!("expected pre element, got {other:?}"),
        }
    }

    #[test]
    fn simple_block_quote() {
        let doc = from_markdown("> quoted text\n");
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(el.sigil, Sigil::Type("blockquote".to_string()));
                assert_eq!(el.args, None);
                assert_eq!(
                    el.content,
                    Some(vec![Inline::Text(Text::new("quoted text", Span::dummy()))])
                );
            }
            other => panic!("expected blockquote element, got {other:?}"),
        }
    }

    #[test]
    fn heading_nested_inside_a_block_quote_splices_its_title_text_in() {
        // A multi-block quote's content is already flattened into one
        // inline run on import (module doc). A heading block hitting
        // `merge_block_into`'s dedicated `Heading` arm must splice its
        // title text directly in, the same way a paragraph does -- not
        // end up wrapped as a nested `Inline::Element(@heading(...))`,
        // which is what the generic `Block::Element` arm would produce.
        let doc = from_markdown("> # Quoted Title\n>\n> more text\n");
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(el.sigil, Sigil::Type("blockquote".to_string()));
                let content = el.content.as_ref().expect("content");
                assert!(
                    !content
                        .iter()
                        .any(|i| matches!(i, Inline::Element(inner) if classify(inner) == ElementKind::Heading)),
                    "heading should have been spliced as text, not nested as an element: {content:?}"
                );
                assert!(
                    content
                        .iter()
                        .any(|i| matches!(i, Inline::Text(t) if t.value.contains("Quoted Title"))),
                    "expected the heading's title text to appear in the flattened content: {content:?}"
                );
            }
            other => panic!("expected blockquote element, got {other:?}"),
        }
    }

    #[test]
    fn inline_code_span_survives_as_backticked_text() {
        let doc = from_markdown("call `foo()` now\n");
        assert_eq!(
            doc.blocks[0],
            Block::Paragraph(Paragraph::new(
                vec![Inline::Text(Text::new("call `foo()` now", Span::dummy()))],
                Span::dummy()
            ))
        );
    }

    #[test]
    fn html_block_is_dropped() {
        let doc = from_markdown("<div>raw html</div>\n\nreal paragraph\n");
        assert_eq!(doc.blocks.len(), 1);
        assert_eq!(
            doc.blocks[0],
            Block::Paragraph(Paragraph::new(
                vec![Inline::Text(Text::new("real paragraph", Span::dummy()))],
                Span::dummy()
            ))
        );
    }

    #[test]
    fn wikilink_converts_to_tomet_element() {
        let src = "Check [[name]] and [[name|display]] here.\n";
        let doc = from_markdown(src);
        assert_eq!(doc.blocks.len(), 1);
        let Block::Paragraph(p) = &doc.blocks[0] else {
            panic!("expected paragraph");
        };
        assert_eq!(p.content.len(), 5);
        assert_eq!(
            p.content[0],
            Inline::Text(Text::new("Check ", Span::dummy()))
        );

        // [[name]] -> @link(target:ref:name)
        let Inline::Element(el1) = &p.content[1] else {
            panic!("expected element 1");
        };
        assert_eq!(el1.sigil, Sigil::At(Some("link".to_string())));
        assert_eq!(
            el1.args,
            Some(Value::Map(vec![(
                "target".to_string(),
                Value::String("ref:name".to_string())
            )]))
        );
        assert_eq!(el1.content, None);

        assert_eq!(
            p.content[2],
            Inline::Text(Text::new(" and ", Span::dummy()))
        );

        // [[name|display]] -> @link[display](target:ref:name)
        let Inline::Element(el2) = &p.content[3] else {
            panic!("expected element 2");
        };
        assert_eq!(el2.sigil, Sigil::At(Some("link".to_string())));
        assert_eq!(
            el2.args,
            Some(Value::Map(vec![(
                "target".to_string(),
                Value::String("ref:name".to_string())
            )]))
        );
        assert_eq!(
            el2.content,
            Some(vec![Inline::Text(Text::new("display", Span::dummy()))])
        );

        assert_eq!(
            p.content[4],
            Inline::Text(Text::new(" here.", Span::dummy()))
        );
    }

    #[test]
    fn table_converts_to_tomet_element() {
        let src = "| col1 | col2 |\n| --- | --- |\n| val1 | val2 |\n";
        let doc = from_markdown(src);
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(el.sigil, Sigil::At(Some("table".to_string())));
                assert!(el.content.is_some());
            }
            other => panic!("expected table element, got {other:?}"),
        }
    }

    #[test]
    fn test_nested_list_import() {
        let md = "- a\n  - b\n";
        let doc = from_markdown(md);
        assert_eq!(doc.blocks.len(), 1);
        let Block::Element(list) = &doc.blocks[0] else {
            panic!("expected list");
        };
        let items = list_items(list);
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].content,
            Some(vec![Inline::Text(Text::new("a", Span::dummy()))])
        );
        let children = items[0].children.as_ref().expect("nested sub-list");
        assert_eq!(children.len(), 1);
        let Block::Element(sub_list) = &children[0] else {
            panic!("expected sub-list");
        };
        let sub_items = list_items(sub_list);
        assert_eq!(sub_items.len(), 1);
        assert_eq!(
            sub_items[0].content,
            Some(vec![Inline::Text(Text::new("b", Span::dummy()))])
        );
    }

    #[test]
    fn table_converts_default_without_width_adjustment() {
        let src = "| title | sdfasdf |\n| --- | --- |\n| title | sdfddfasdf |\n";
        let doc = from_markdown(src);
        let Block::Element(el) = &doc.blocks[0] else {
            panic!("expected element");
        };
        let content = el.content.as_ref().unwrap();
        let cell1_prefix = match &content[1] {
            Inline::Text(t) => &t.value,
            _ => "",
        };
        assert_eq!(cell1_prefix, "[ ");
    }

    #[test]
    fn table_converts_with_width_adjustment_option() {
        let src = "| title | sdfasdf |\n| --- | --- |\n| title | sdfddfasdf |\n";
        let opts = ImportOptions {
            adjust_table_width: true,
            table_adjust_width_mode: Some("true".to_string()),
            table_max_col_width: None,
            table_align: None,
        };
        let doc = from_markdown_with_options(src, &opts);
        let Block::Element(el) = &doc.blocks[0] else {
            panic!("expected element");
        };
        let content = el.content.as_ref().unwrap();
        // Row 0 cell 1 ("sdfasdf" len 7 vs max len 10 "sdfddfasdf"): target_width 12, extra 5 -> left 2, right 3
        let row0_cell1_suffix = match &content[6] {
            Inline::Text(t) => &t.value,
            _ => "",
        };
        assert_eq!(row0_cell1_suffix, "    ]");
    }

    #[test]
    fn table_converts_with_auto_width_adjustment_stops_at_wide_column() {
        let src = "| 殻 | n | suborbitals |\n| --- | --- | --- |\n| K殻 | 1 | 1s+2s+2p+3d (very long) |\n| L殻 | 2 | 2s+2p |\n";
        let opts = ImportOptions {
            adjust_table_width: true,
            table_adjust_width_mode: Some("auto".to_string()),
            table_max_col_width: Some(5),
            table_align: None,
        };
        let doc = from_markdown_with_options(src, &opts);
        let Block::Element(el) = &doc.blocks[0] else {
            panic!("expected element");
        };
        let content = el.content.as_ref().unwrap();
        // Col 0 (殻): display width 2 vs K殻 width 3 (<= 5) -> padded!
        // Col 1 (n): len 1 (<= 5) -> padded!
        // Col 2 (suborbitals): len > 5 -> NOT padded (stays "[ " and " ]")
        let col0_header_suffix = match &content[3] {
            Inline::Text(t) => &t.value,
            _ => "",
        };
        assert_eq!(col0_header_suffix, "  ]"); // 1 base + 1 extra space on right
    }

    #[test]
    fn table_converts_with_markdown_alignments() {
        let src = "| left | center | right |\n| :--- | :---: | ---: |\n| 1 | 2 | 3 |\n| 100 | 200 | 300 |\n";
        let opts = ImportOptions {
            adjust_table_width: true,
            table_adjust_width_mode: Some("true".to_string()),
            table_max_col_width: Some(20),
            table_align: Some("left".to_string()),
        };
        let doc = from_markdown_with_options(src, &opts);
        let Block::Element(el) = &doc.blocks[0] else {
            panic!("expected element");
        };
        let content = el.content.as_ref().unwrap();
        let rendered: String = content
            .iter()
            .map(|inl| match inl {
                Inline::Text(t) => t.value.clone(),
                _ => String::new(),
            })
            .collect();
        eprintln!("MARKDOWN IMPORT TABLE:\n{rendered}");
        assert!(rendered.contains("[ 1    ][   2    ][     3 ]"));
    }

    #[test]
    fn text_with_sigil_chars_encloses_them_in_backticks() {
        let md = "Contact @user at <foo> or $100 and <> with 2 * 3 ^ 2 and **.\n";
        let doc = from_markdown(md);
        assert_eq!(doc.blocks.len(), 1);
        let Block::Paragraph(p) = &doc.blocks[0] else {
            panic!("expected paragraph");
        };
        assert_eq!(
            p.content,
            vec![Inline::Text(Text::new(
                "Contact `@`user at <foo> or `$`100 and `<>` with 2 * 3 `^` 2 and **.",
                Span::dummy()
            ))]
        );
    }

    #[test]
    fn checkbox_task_list_markers_and_custom_markers_import_as_typed_markers() {
        let md = "- [x] task1\n- [ ] task2\n- [c] con item\n- [p] pro item\n- (x) task3\n";
        let doc = from_markdown(md);
        assert_eq!(doc.blocks.len(), 1);
        let Block::Element(list) = &doc.blocks[0] else {
            panic!("expected list");
        };
        let items = list_items(list);
        assert_eq!(items.len(), 5);
        assert_eq!(items[0].args, Some(Value::String("x".to_string())));
        assert_eq!(
            items[0].content,
            Some(vec![Inline::Text(Text::new("task1", Span::dummy()))])
        );
        assert_eq!(items[1].args, Some(Value::String(" ".to_string())));
        assert_eq!(
            items[1].content,
            Some(vec![Inline::Text(Text::new("task2", Span::dummy()))])
        );
        assert_eq!(items[2].args, Some(Value::String("c".to_string())));
        assert_eq!(
            items[2].content,
            Some(vec![Inline::Text(Text::new("con item", Span::dummy()))])
        );
        assert_eq!(items[3].args, Some(Value::String("p".to_string())));
        assert_eq!(
            items[3].content,
            Some(vec![Inline::Text(Text::new("pro item", Span::dummy()))])
        );
        assert_eq!(items[4].args, Some(Value::String("x".to_string())));
        assert_eq!(
            items[4].content,
            Some(vec![Inline::Text(Text::new("task3", Span::dummy()))])
        );
    }

    #[test]
    fn embed_wikilinks_and_images_import_as_embed_elements() {
        let md = "![[name|display]] and ![display](_path)\n";
        let doc = from_markdown(md);
        assert_eq!(doc.blocks.len(), 1);
        let Block::Paragraph(p) = &doc.blocks[0] else {
            panic!("expected paragraph");
        };
        let Inline::Element(embed1) = &p.content[0] else {
            panic!("expected embed element 1");
        };
        assert_eq!(embed1.sigil, Sigil::Type("embed".to_string()));
        assert_eq!(
            embed1.args,
            Some(Value::Map(vec![(
                "target".to_string(),
                Value::String("name".to_string())
            )]))
        );
        assert_eq!(
            embed1.content,
            Some(vec![Inline::Text(Text::new("display", Span::dummy()))])
        );

        let Inline::Element(embed2) = &p.content[2] else {
            panic!("expected embed element 2");
        };
        assert_eq!(embed2.sigil, Sigil::Type("embed".to_string()));
        assert_eq!(
            embed2.args,
            Some(Value::Map(vec![(
                "target".to_string(),
                Value::String("_path".to_string())
            )]))
        );
        assert_eq!(
            embed2.content,
            Some(vec![Inline::Text(Text::new("display", Span::dummy()))])
        );
    }

    #[test]
    fn test_wikilink_with_at_prefix_not_enclosed_in_backticks() {
        let md = "Check [[@file_name]] and [[@Templater|display]]\n";
        let doc = from_markdown(md);
        assert_eq!(doc.blocks.len(), 1);
        let Block::Paragraph(p) = &doc.blocks[0] else {
            panic!("expected paragraph");
        };

        let Inline::Element(el1) = &p.content[1] else {
            panic!("expected element 1");
        };
        assert_eq!(el1.sigil, Sigil::At(Some("link".to_string())));
        assert_eq!(
            el1.args,
            Some(Value::Map(vec![(
                "target".to_string(),
                Value::String("ref:@file_name".to_string())
            )]))
        );

        let Inline::Element(el2) = &p.content[3] else {
            panic!("expected element 2");
        };
        assert_eq!(el2.sigil, Sigil::At(Some("link".to_string())));
        assert_eq!(
            el2.args,
            Some(Value::Map(vec![(
                "target".to_string(),
                Value::String("ref:@Templater".to_string())
            )]))
        );
    }

    #[test]
    fn test_multiline_paragraph_linebreaks_preserved() {
        let md = "Line 1\nLine 2\nLine 3\n";
        let doc = from_markdown(md);
        assert_eq!(doc.blocks.len(), 1);
        let Block::Paragraph(p) = &doc.blocks[0] else {
            panic!("expected paragraph");
        };
        assert_eq!(
            p.content,
            vec![Inline::Text(Text::new(
                "Line 1\nLine 2\nLine 3",
                Span::dummy()
            ))]
        );
    }

    #[test]
    fn test_obsidian_callout_blockquote_import() {
        let md = "> [!info] 2025/04/29 11:09\n> コレさすがに草www\n> お前なら[[$2025-04-26|どうするんだ]]？\n";
        let doc = from_markdown(md);
        assert_eq!(doc.blocks.len(), 1);
        let Block::Element(el) = &doc.blocks[0] else {
            panic!("expected element");
        };
        assert_eq!(el.sigil, Sigil::Type("callout".to_string()));
        assert_eq!(
            el.args,
            Some(Value::Map(vec![
                ("variant".to_string(), Value::String("info".to_string())),
                (
                    "title".to_string(),
                    Value::String("2025/04/29 11:09".to_string())
                ),
            ]))
        );

        let md_plain = "> Plain quote\n";
        let doc_plain = from_markdown(md_plain);
        let Block::Element(el_plain) = &doc_plain.blocks[0] else {
            panic!("expected plain quote element");
        };
        assert_eq!(el_plain.sigil, Sigil::Type("blockquote".to_string()));
        assert_eq!(el_plain.args, None);
    }

    #[test]
    fn test_comment_sigils_enclosed_in_backticks() {
        let md = "Comment // double slash and /// triple slash and /* block start */ block end and single / slash.\n";
        let doc = from_markdown(md);
        let Block::Paragraph(p) = &doc.blocks[0] else {
            panic!("expected paragraph");
        };
        let Inline::Text(t) = &p.content[0] else {
            panic!("expected text");
        };
        assert_eq!(
            t.value,
            "Comment `//` double slash and `///` triple slash and `/*` block start `*/` block end and single / slash."
        );
    }

    #[test]
    fn test_callout_with_empty_leading_line_import() {
        let md = "> [!done]+ 06:49\n>\n> なんか、[[@美|きれい]]にいろいろ書きたいなって思っちゃうけど、だめよな\n> 過去のことを書きたくなるけど、今のほうがいいよな。\n";
        let doc = from_markdown(md);
        let Block::Element(el) = &doc.blocks[0] else {
            panic!("expected element");
        };
        let content = el.content.as_ref().unwrap();
        let Inline::Text(t) = &content[0] else {
            panic!("expected text");
        };
        assert!(
            t.value.starts_with("なんか、"),
            "callout content should start directly with text, got: {:?}",
            t.value
        );
    }

    #[test]
    fn test_bare_url_with_query_param_no_backticks() {
        let md = "- http://127.0.0.1:8888/search?lang=ja&q=<query>\n- <redacted-internal-url>/search?lang=ja&q=<query>\n";
        let doc = from_markdown(md);
        let exported = crate::export::to_markdown(&doc);
        assert_eq!(exported.trim(), md.trim());
    }

    #[test]
    fn test_bare_angle_brackets_not_escaped() {
        let md = "asdfasdfdsf>\n<foo>\n";
        let doc = from_markdown(md);
        let exported = crate::export::to_markdown(&doc);
        assert!(
            !exported.contains("`>`"),
            "should never wrap > in backticks: {exported}"
        );
        assert!(
            !exported.contains("`<`"),
            "should not wrap < in backticks when not element: {exported}"
        );
        assert_eq!(exported.trim(), md.trim());
    }
}
