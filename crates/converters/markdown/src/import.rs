//! CommonMark -> `typedmark_ast::Document`, folding `pulldown-cmark`'s
//! event stream directly (no intermediate tree) using an explicit frame
//! stack, one frame per currently-open tag.
//!
//! Constructs that map cleanly reuse `typedmark_ast` shapes that already
//! exist for TypedMark's own native shorthand (`em`/`strong`/`hr`, flat
//! `List`). Constructs with no native equivalent (fenced/indented code,
//! block quotes) go through the generic `<T>` element escape hatch
//! (`pre`, `blockquote`) documented in `docs/commonmark-support.md`.
//!
//! Two things are structurally lossy on import, both documented there:
//! nested lists are flattened into the enclosing list as sibling items
//! (`ListItem` has no slot for children), and block quotes containing
//! more than one block get their content joined into a single inline
//! run (`Element::content` is `Vec<Inline>`, not `Vec<Block>`). HTML blocks
//! and inline HTML are dropped; hard breaks collapse to a space.

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use typedmark_ast::{
    Block, Document, Element, Heading, Inline, List, ListItem, Paragraph, Sigil, Span, Text, Value,
};

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
        marker: Option<String>,
    },
    List {
        ordered: bool,
        items: Vec<ListItem>,
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
}

pub fn from_markdown(src: &str) -> Document {
    from_markdown_with_options(src, &ImportOptions::default())
}

pub fn from_markdown_with_options(src: &str, options: &ImportOptions) -> Document {
    let (frontmatter, markdown_body) = extract_yaml_frontmatter(src);

    let mut options_flags = Options::empty();
    options_flags.insert(Options::ENABLE_TABLES);
    options_flags.insert(Options::ENABLE_TASKLISTS);
    options_flags.insert(Options::ENABLE_GFM);
    let parser = Parser::new_ext(markdown_body, options_flags);
    let mut stack: Vec<Frame> = vec![Frame::Blocks(Vec::new())];

    if let Some(entries) = frontmatter {
        let mut meta_el = Element::new(Sigil::At(Some("meta".to_string())));
        meta_el.value = Some(typedmark_ast::ElementValue::Data(Value::Map(entries)));
        push_block(&mut stack, Block::Element(meta_el));
    }

    for event in parser {
        match event {
            Event::Start(tag) => stack.push(start_frame(tag)),
            Event::End(tag_end) => end_frame(&mut stack, tag_end, options.adjust_table_width),
            Event::TaskListMarker(checked) => {
                if let Some(Frame::Item { marker, .. }) = stack.last_mut() {
                    *marker = Some(if checked {
                        "x".to_string()
                    } else {
                        " ".to_string()
                    });
                }
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
                Block::Element(Element::new(Sigil::Type("hr".to_string()))),
            ),
            _ => {}
        }
    }

    let root = stack.pop().expect("root frame always present");
    let mut doc = match root {
        Frame::Blocks(blocks) => Document::new(blocks, Span::dummy()),
        _ => Document::default(),
    };
    post_process_document_wikilinks(&mut doc);
    doc
}

fn enclose_sigils_in_backticks(text: &str) -> String {
    if !text.contains('@')
        && !text.contains('<')
        && !text.contains('$')
        && !text.contains('^')
        && !text.contains('/')
        && !text.contains('*')
    {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + 8);
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '@' => out.push_str("`@`"),
            '$' => out.push_str("`$`"),
            '^' => out.push_str("`^`"),
            '<' => {
                if chars.peek() == Some(&'>') {
                    chars.next();
                    out.push_str("`<>`");
                } else {
                    let mut look = chars.clone();
                    let ident = eat_ident_str(&mut look);
                    if !ident.is_empty()
                        && look.next() == Some('>')
                        && matches!(
                            look.peek(),
                            Some(&'(') | Some(&'[') | Some(&'{') | Some(&':')
                        )
                    {
                        out.push_str("`<`");
                    } else {
                        out.push('<');
                    }
                }
            }
            '>' => out.push('>'),
            '/' => {
                if chars.peek() == Some(&'/') && !out.ends_with(':') {
                    let mut slashes = String::from("/");
                    while chars.peek() == Some(&'/') {
                        slashes.push(chars.next().unwrap());
                    }
                    out.push('`');
                    out.push_str(&slashes);
                    out.push('`');
                } else if chars.peek() == Some(&'*') {
                    chars.next();
                    out.push_str("`/*`");
                } else {
                    out.push('/');
                }
            }
            '*' => {
                if chars.peek() == Some(&'/') {
                    chars.next();
                    out.push_str("`*/`");
                } else {
                    out.push('*');
                }
            }
            c => out.push(c),
        }
    }
    out
}

fn eat_ident_str(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut s = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_alphanumeric() || c == '_' || c == '-' {
            s.push(c);
            chars.next();
        } else {
            break;
        }
    }
    s
}

fn extract_yaml_frontmatter(src: &str) -> (Option<Vec<(String, Value)>>, &str) {
    let trimmed = src.trim_start();
    if !trimmed.starts_with("---") {
        return (None, src);
    }

    let rest = &trimmed[3..];
    if !rest.starts_with('\n') && !rest.starts_with("\r\n") {
        return (None, src);
    }

    let end_pos = if let Some(pos) = rest.find("\n---") {
        pos
    } else if let Some(pos) = rest.find("\n...") {
        pos
    } else {
        return (None, src);
    };

    let yaml_text = &rest[..end_pos];
    let closing_slice = &rest[end_pos..];
    let after_closing_idx =
        if closing_slice.starts_with("\n---") || closing_slice.starts_with("\n...") {
            end_pos + 4
        } else if closing_slice.starts_with("\r\n---") || closing_slice.starts_with("\r\n...") {
            end_pos + 5
        } else {
            return (None, src);
        };

    let remaining_src = rest[after_closing_idx..].trim_start_matches(|c| c == '\r' || c == '\n');

    if let Ok(serde_yaml::Value::Mapping(map)) =
        serde_yaml::from_str::<serde_yaml::Value>(yaml_text)
    {
        let entries: Vec<(String, Value)> = map
            .into_iter()
            .map(|(k, v)| (yaml_key_to_string(k), yaml_to_value(v)))
            .collect();
        if !entries.is_empty() {
            return (Some(entries), remaining_src);
        }
    }

    (None, remaining_src)
}

fn yaml_to_value(v: serde_yaml::Value) -> Value {
    match v {
        serde_yaml::Value::Null => Value::Null,
        serde_yaml::Value::Bool(b) => Value::Bool(b),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else {
                Value::Float(n.as_f64().unwrap_or_default())
            }
        }
        serde_yaml::Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.starts_with("[[") && trimmed.ends_with("]]") && trimmed.len() > 4 {
                let inner = &trimmed[2..trimmed.len() - 2];
                let target = if let Some((t, _)) = inner.split_once('|') {
                    t.trim()
                } else {
                    inner.trim()
                };
                Value::Map(vec![(
                    "wiki".to_string(),
                    Value::String(target.to_string()),
                )])
            } else {
                Value::String(s)
            }
        }
        serde_yaml::Value::Sequence(items) => {
            Value::Seq(items.into_iter().map(yaml_to_value).collect())
        }
        serde_yaml::Value::Mapping(map) => Value::Map(
            map.into_iter()
                .map(|(k, v)| (yaml_key_to_string(k), yaml_to_value(v)))
                .collect(),
        ),
        serde_yaml::Value::Tagged(tagged) => yaml_to_value(tagged.value),
    }
}

fn yaml_key_to_string(k: serde_yaml::Value) -> String {
    match k {
        serde_yaml::Value::String(s) => s,
        serde_yaml::Value::Bool(b) => b.to_string(),
        serde_yaml::Value::Number(n) => n.to_string(),
        serde_yaml::Value::Null => "null".to_string(),
        other => format!("{other:?}"),
    }
}

fn find_next_url(text: &str) -> Option<(usize, usize)> {
    let mut search_from = 0;
    while search_from < text.len() {
        let rest = &text[search_from..];
        let rel_http = rest.find("http://");
        let rel_https = rest.find("https://");
        let rel_mailto = rest.find("mailto:");

        let first_rel = match (rel_http, rel_https, rel_mailto) {
            (None, None, None) => return None,
            (a, b, c) => [a, b, c].into_iter().flatten().min().unwrap(),
        };

        let idx = search_from + first_rel;

        if idx > 0 {
            let prev_char = text[..idx].chars().next_back().unwrap();
            if prev_char.is_alphanumeric() {
                search_from = idx + 1;
                continue;
            }
        }

        let url_sub = &text[idx..];
        let end_rel = url_sub
            .find(|c: char| c.is_whitespace() || c < ' ')
            .unwrap_or(url_sub.len());

        let url_end = idx + end_rel;
        return Some((idx, url_end));
    }
    None
}

fn parse_urls_and_wikilinks(text: &str) -> Vec<Inline> {
    let mut result = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        let url_match = find_next_url(remaining);
        let wiki_start = remaining.find("[[");

        match (url_match, wiki_start) {
            (Some((u_start, u_end)), None) => {
                if u_start > 0 {
                    let seg = enclose_sigils_in_backticks(&remaining[..u_start]);
                    if !seg.is_empty() {
                        result.push(Inline::Text(Text::new(seg, Span::dummy())));
                    }
                }
                let url_str = &remaining[u_start..u_end];
                let mut el = Element::new(Sigil::At(None));
                el.args = Some(Value::Map(vec![(
                    "url".to_string(),
                    Value::String(url_str.to_string()),
                )]));
                result.push(Inline::Element(el));
                remaining = &remaining[u_end..];
            }
            (None, Some(start_idx)) => {
                parse_one_wikilink(&mut result, &mut remaining, start_idx);
            }
            (Some((u_start, u_end)), Some(start_idx)) => {
                if u_start < start_idx {
                    if u_start > 0 {
                        let seg = enclose_sigils_in_backticks(&remaining[..u_start]);
                        if !seg.is_empty() {
                            result.push(Inline::Text(Text::new(seg, Span::dummy())));
                        }
                    }
                    let url_str = &remaining[u_start..u_end];
                    let mut el = Element::new(Sigil::At(None));
                    el.args = Some(Value::Map(vec![(
                        "url".to_string(),
                        Value::String(url_str.to_string()),
                    )]));
                    result.push(Inline::Element(el));
                    remaining = &remaining[u_end..];
                } else {
                    parse_one_wikilink(&mut result, &mut remaining, start_idx);
                }
            }
            (None, None) => {
                let seg = enclose_sigils_in_backticks(remaining);
                if !seg.is_empty() {
                    result.push(Inline::Text(Text::new(seg, Span::dummy())));
                }
                break;
            }
        }
    }

    result
}

fn parse_one_wikilink(result: &mut Vec<Inline>, remaining: &mut &str, start_idx: usize) {
    if let Some(end_idx) = remaining[start_idx + 2..].find("]]") {
        let actual_end_idx = start_idx + 2 + end_idx;

        let is_embed = start_idx > 0 && remaining.as_bytes()[start_idx - 1] == b'!';
        let text_end_idx = if is_embed { start_idx - 1 } else { start_idx };

        if text_end_idx > 0 {
            let segment = enclose_sigils_in_backticks(&remaining[..text_end_idx]);
            if !segment.is_empty() {
                result.push(Inline::Text(Text::new(segment, Span::dummy())));
            }
        }

        let sigil = if is_embed {
            Sigil::Type("embed".to_string())
        } else {
            Sigil::At(None)
        };

        let inner = &remaining[start_idx + 2..actual_end_idx];
        let wikilink_el = if let Some((target, display)) = inner.split_once('|') {
            let target = target.trim();
            let display = display.trim();
            let mut el = Element::new(sigil);
            el.args = Some(Value::Map(vec![(
                "wiki".to_string(),
                Value::String(target.to_string()),
            )]));
            el.content = Some(vec![Inline::Text(Text::new(
                enclose_sigils_in_backticks(display),
                Span::dummy(),
            ))]);
            el
        } else {
            let target = inner.trim();
            let mut el = Element::new(sigil);
            el.args = Some(Value::Map(vec![(
                "wiki".to_string(),
                Value::String(target.to_string()),
            )]));
            el
        };

        result.push(Inline::Element(wikilink_el));
        *remaining = &remaining[actual_end_idx + 2..];
    } else {
        let segment = enclose_sigils_in_backticks(&remaining[..start_idx + 2]);
        if !segment.is_empty() {
            result.push(Inline::Text(Text::new(segment, Span::dummy())));
        }
        *remaining = &remaining[start_idx + 2..];
    }
}

fn post_process_document_wikilinks(doc: &mut Document) {
    for block in &mut doc.blocks {
        post_process_block_wikilinks(block);
    }
}

fn post_process_block_wikilinks(block: &mut Block) {
    match block {
        Block::Paragraph(p) => {
            p.content = post_process_inlines_wikilinks(std::mem::take(&mut p.content));
        }
        Block::Heading(h) => {
            h.content = post_process_inlines_wikilinks(std::mem::take(&mut h.content));
        }
        Block::List(list) => {
            for item in &mut list.items {
                item.content = post_process_inlines_wikilinks(std::mem::take(&mut item.content));
                for child in &mut item.children {
                    post_process_block_wikilinks(child);
                }
            }
        }
        Block::Element(el) => {
            post_process_element_wikilinks(el);
        }
    }
}

fn post_process_element_wikilinks(el: &mut Element) {
    if matches!(&el.sigil, Sigil::Type(name) if name == "codeblock") {
        return;
    }
    if let Some(content) = el.content.take() {
        el.content = Some(post_process_inlines_wikilinks(content));
    }
}

fn post_process_inlines_wikilinks(inlines: Vec<Inline>) -> Vec<Inline> {
    let mut new_inlines = Vec::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => {
                new_inlines.extend(parse_urls_and_wikilinks(&t.value));
            }
            Inline::Element(mut el) => {
                post_process_element_wikilinks(&mut el);
                new_inlines.push(Inline::Element(el));
            }
        }
    }
    new_inlines
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
        Tag::Table(_) => Frame::Table { rows: Vec::new() },
        Tag::TableHead => Frame::TableHead { cells: Vec::new() },
        Tag::TableRow => Frame::TableRow { cells: Vec::new() },
        Tag::TableCell => Frame::TableCell {
            content: Vec::new(),
        },
        // HtmlBlock and anything gated behind unset Options (footnotes, strikethrough, etc.).
        _ => Frame::Discard,
    }
}

fn end_frame(stack: &mut Vec<Frame>, tag_end: TagEnd, adjust_table_width: bool) {
    let frame = stack.pop().expect("End without matching Start");
    match (frame, tag_end) {
        (Frame::Paragraph(inlines), TagEnd::Paragraph) => push_block(
            stack,
            Block::Paragraph(Paragraph::new(inlines, Span::dummy())),
        ),
        (Frame::Heading(level, inlines), TagEnd::Heading(_)) => push_block(
            stack,
            Block::Heading(Heading::new(level, inlines, None, Span::dummy())),
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
                    value: None,
                    span: Span::dummy(),
                };
                push_block(stack, Block::Element(el));
            } else {
                let el = Element {
                    sigil: Sigil::Type("blockquote".to_string()),
                    args: None,
                    content: Some(content),
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
                value: None,
                span: Span::dummy(),
            };
            push_block(stack, Block::Element(el));
        }
        (Frame::List { ordered, items }, TagEnd::List(_)) => {
            push_block(stack, Block::List(List::new(ordered, items, Span::dummy())))
        }
        (
            Frame::Item {
                mut content,
                children,
                mut marker,
            },
            TagEnd::Item,
        ) => match stack.last_mut() {
            Some(Frame::List { items, .. }) => {
                if marker.is_none() {
                    if let Some(Inline::Text(t)) = content.first_mut() {
                        let s = t.value.trim_start();
                        if s.starts_with('[') || s.starts_with('(') {
                            let close = if s.starts_with('[') { ']' } else { ')' };
                            if let Some(close_idx) = s.find(close) {
                                if close_idx > 1 && s[close_idx..].starts_with(&format!("{close} "))
                                {
                                    marker = Some(s[1..close_idx].to_string());
                                    let remainder = s[close_idx + 2..].to_string();
                                    t.value = remainder;
                                }
                            }
                        }
                    }
                }
                items.push(ListItem::with_children(
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
            let el = Element {
                sigil: Sigil::At(None),
                args: Some(Value::Map(vec![("url".to_string(), Value::String(dest))])),
                content: Some(inlines),
                value: None,
                span: Span::dummy(),
            };
            push_inline(stack, Inline::Element(el));
        }
        (Frame::Image { dest, alt }, TagEnd::Image) => {
            let key = if dest.contains("://") { "url" } else { "path" };
            let content = if alt.is_empty() { None } else { Some(alt) };
            let el = Element {
                sigil: Sigil::Type("embed".to_string()),
                args: Some(Value::Map(vec![(key.to_string(), Value::String(dest))])),
                content,
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
                if let Frame::Table { rows } = frame {
                    rows.push(cells);
                    break;
                }
            }
        }
        (Frame::TableHead { cells }, TagEnd::TableHead) => {
            for frame in stack.iter_mut().rev() {
                if let Frame::Table { rows } = frame {
                    rows.push(cells);
                    break;
                }
            }
        }
        (Frame::Table { rows }, TagEnd::Table) => {
            let el = build_table_element(rows, adjust_table_width);
            push_block(stack, Block::Element(el));
        }
        // HtmlBlock and anything routed to Discard.
        (Frame::Discard, _) => {}
        (_, _) => {}
    }
}

fn build_table_element(rows: Vec<Vec<Vec<Inline>>>, adjust_width: bool) -> Element {
    let mut content_inlines = Vec::new();
    if rows.is_empty() {
        let mut el = Element::new(Sigil::At(Some("table".to_string())));
        el.args = Some(Value::Map(Vec::new()));
        el.content = Some(content_inlines);
        el.value = Some(typedmark_ast::ElementValue::Data(Value::Map(Vec::new())));
        return el;
    }

    let mut col_count = 0;
    for row in &rows {
        col_count = col_count.max(row.len());
    }

    let max_lens = if adjust_width {
        let mut lens = vec![0usize; col_count];
        for row in &rows {
            for (c_idx, cell) in row.iter().enumerate() {
                let len = inlines_plain_len(cell);
                lens[c_idx] = lens[c_idx].max(len);
            }
        }
        Some(lens)
    } else {
        None
    };

    content_inlines.push(Inline::Text(Text::new("\n", Span::dummy())));

    for row in &rows {
        for c_idx in 0..col_count {
            let cell_inlines = row.get(c_idx).cloned().unwrap_or_default();
            let cell_len = inlines_plain_len(&cell_inlines);

            let (left_spaces, right_spaces) = if let Some(lens) = &max_lens {
                let max_len = lens[c_idx].max(1);
                let target_width = max_len + 2;
                let extra = if target_width > cell_len {
                    target_width - cell_len
                } else {
                    2
                };
                let left = extra / 2;
                let right = extra - left;
                (left, right)
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

    let mut el = Element::new(Sigil::At(Some("table".to_string())));
    el.args = Some(Value::Map(Vec::new()));
    el.content = Some(content_inlines);
    el.value = Some(typedmark_ast::ElementValue::Data(Value::Map(Vec::new())));
    el
}

fn inlines_plain_len(inlines: &[Inline]) -> usize {
    let mut len = 0;
    for inline in inlines {
        match inline {
            Inline::Text(t) => len += t.value.trim().chars().count(),
            Inline::Element(el) => {
                if let Some(content) = &el.content {
                    len += inlines_plain_len(content);
                }
            }
        }
    }
    len
}

fn wrap_inline(tag: &str, content: Vec<Inline>) -> Inline {
    Inline::Element(Element {
        sigil: Sigil::Type(tag.to_string()),
        args: None,
        content: Some(content),
        value: None,
        span: Span::dummy(),
    })
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
            if let Block::List(list) = block {
                children.push(Block::List(list));
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
        Block::Heading(h) => extend_spaced(content, h.content),
        Block::Element(el) => {
            if !content.is_empty() {
                content.push(Inline::Text(Text::new(" ", Span::dummy())));
            }
            content.push(Inline::Element(el));
        }
        Block::List(list) => {
            for item in list.items {
                extend_spaced(content, item.content);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_and_paragraph() {
        let doc = from_markdown("# Title\n\nHello world.\n");
        assert_eq!(doc.blocks.len(), 2);
        match &doc.blocks[0] {
            Block::Heading(h) => {
                assert_eq!(h.level, 1);
                assert_eq!(
                    h.content,
                    vec![Inline::Text(Text::new("Title", Span::dummy()))]
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
            Block::List(list) => {
                assert!(!list.ordered);
                assert_eq!(
                    list.items[0].content,
                    vec![Inline::Text(Text::new("one", Span::dummy()))]
                );
                assert_eq!(
                    list.items[1].content,
                    vec![Inline::Text(Text::new("two", Span::dummy()))]
                );
            }
            other => panic!("expected list, got {other:?}"),
        }
    }

    #[test]
    fn ordered_list() {
        let doc = from_markdown("1. one\n2. two\n");
        match &doc.blocks[0] {
            Block::List(list) => assert!(list.ordered),
            other => panic!("expected list, got {other:?}"),
        }
    }

    #[test]
    fn nested_list_preserves_children_hierarchy() {
        let doc = from_markdown("- a\n  - b\n- c\n");
        match &doc.blocks[0] {
            Block::List(list) => {
                assert_eq!(list.items.len(), 2);
                assert_eq!(
                    list.items[0].content,
                    vec![Inline::Text(Text::new("a", Span::dummy()))]
                );
                assert_eq!(list.items[0].children.len(), 1);
                let Block::List(sub) = &list.items[0].children[0] else {
                    panic!("expected sub-list");
                };
                assert_eq!(sub.items.len(), 1);
                assert_eq!(
                    sub.items[0].content,
                    vec![Inline::Text(Text::new("b", Span::dummy()))]
                );
                assert_eq!(
                    list.items[1].content,
                    vec![Inline::Text(Text::new("c", Span::dummy()))]
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
                    assert_eq!(el.sigil, Sigil::At(None));
                    assert_eq!(
                        el.args,
                        Some(Value::Map(vec![(
                            "url".to_string(),
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
                Inline::Element(el) => assert_eq!(el.sigil, Sigil::At(None)),
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
                            "path".to_string(),
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
    fn yaml_frontmatter_converts_to_meta() {
        let src = "---\ntitle: \"Hello\"\nauthor: Alice\ndraft: false\n---\n\n# Main Title\n";
        let doc = from_markdown(src);
        assert_eq!(doc.blocks.len(), 2);
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(el.sigil, Sigil::At(Some("meta".to_string())));
                assert_eq!(
                    el.value,
                    Some(typedmark_ast::ElementValue::Data(Value::Map(vec![
                        ("title".to_string(), Value::String("Hello".to_string())),
                        ("author".to_string(), Value::String("Alice".to_string())),
                        ("draft".to_string(), Value::Bool(false)),
                    ])))
                );
            }
            other => panic!("expected meta element, got {other:?}"),
        }
    }

    #[test]
    fn yaml_frontmatter_with_arrays_converts_to_meta() {
        let src = "---\ntitle: \"Doc\"\ntags:\n  - rust\n  - typedmark\n---\n\n# Main\n";
        let doc = from_markdown(src);
        assert_eq!(doc.blocks.len(), 2);
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(el.sigil, Sigil::At(Some("meta".to_string())));
                assert_eq!(
                    el.value,
                    Some(typedmark_ast::ElementValue::Data(Value::Map(vec![
                        ("title".to_string(), Value::String("Doc".to_string())),
                        (
                            "tags".to_string(),
                            Value::Seq(vec![
                                Value::String("rust".to_string()),
                                Value::String("typedmark".to_string()),
                            ])
                        ),
                    ])))
                );
            }
            other => panic!("expected meta element, got {other:?}"),
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
    fn wikilink_converts_to_typedmark_element() {
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

        // [[name]] -> @(wiki:name)
        let Inline::Element(el1) = &p.content[1] else {
            panic!("expected element 1");
        };
        assert_eq!(el1.sigil, Sigil::At(None));
        assert_eq!(
            el1.args,
            Some(Value::Map(vec![(
                "wiki".to_string(),
                Value::String("name".to_string())
            )]))
        );
        assert_eq!(el1.content, None);

        assert_eq!(
            p.content[2],
            Inline::Text(Text::new(" and ", Span::dummy()))
        );

        // [[name|display]] -> @[display](wiki:name)
        let Inline::Element(el2) = &p.content[3] else {
            panic!("expected element 2");
        };
        assert_eq!(el2.sigil, Sigil::At(None));
        assert_eq!(
            el2.args,
            Some(Value::Map(vec![(
                "wiki".to_string(),
                Value::String("name".to_string())
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
    fn table_converts_to_typedmark_element() {
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
        let Block::List(list) = &doc.blocks[0] else {
            panic!("expected list");
        };
        assert_eq!(list.items.len(), 1);
        assert_eq!(
            list.items[0].content,
            vec![Inline::Text(Text::new("a", Span::dummy()))]
        );
        assert_eq!(list.items[0].children.len(), 1);
        let Block::List(sub_list) = &list.items[0].children[0] else {
            panic!("expected sub-list");
        };
        assert_eq!(sub_list.items.len(), 1);
        assert_eq!(
            sub_list.items[0].content,
            vec![Inline::Text(Text::new("b", Span::dummy()))]
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
        assert_eq!(row0_cell1_suffix, "   ]");
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
    fn task_list_markers_convert_to_item_markers() {
        let md = "- [x] task1\n- [ ] task2\n- (x) task3\n";
        let doc = from_markdown(md);
        assert_eq!(doc.blocks.len(), 1);
        let Block::List(list) = &doc.blocks[0] else {
            panic!("expected list");
        };
        assert_eq!(list.items.len(), 3);
        assert_eq!(list.items[0].marker, Some("x".to_string()));
        assert_eq!(
            list.items[0].content,
            vec![Inline::Text(Text::new("task1", Span::dummy()))]
        );
        assert_eq!(list.items[1].marker, Some(" ".to_string()));
        assert_eq!(
            list.items[1].content,
            vec![Inline::Text(Text::new("task2", Span::dummy()))]
        );
        assert_eq!(list.items[2].marker, Some("x".to_string()));
        assert_eq!(
            list.items[2].content,
            vec![Inline::Text(Text::new("task3", Span::dummy()))]
        );
    }

    #[test]
    fn frontmatter_wikilinks_convert_to_wiki_maps() {
        let src = "---\ntopics:\n  - \"[[@Templater]]\"\n  - \"[[@QuickAdd]]\"\n---\n\n# Title\n";
        let doc = from_markdown(src);
        assert_eq!(doc.blocks.len(), 2);
        let Block::Element(el) = &doc.blocks[0] else {
            panic!("expected meta element");
        };
        assert_eq!(
            el.value,
            Some(typedmark_ast::ElementValue::Data(Value::Map(vec![(
                "topics".to_string(),
                Value::Seq(vec![
                    Value::Map(vec![(
                        "wiki".to_string(),
                        Value::String("@Templater".to_string())
                    )]),
                    Value::Map(vec![(
                        "wiki".to_string(),
                        Value::String("@QuickAdd".to_string())
                    )]),
                ])
            )])))
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
                "wiki".to_string(),
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
                "path".to_string(),
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
        assert_eq!(el1.sigil, Sigil::At(None));
        assert_eq!(
            el1.args,
            Some(Value::Map(vec![(
                "wiki".to_string(),
                Value::String("@file_name".to_string())
            )]))
        );

        let Inline::Element(el2) = &p.content[3] else {
            panic!("expected element 2");
        };
        assert_eq!(el2.sigil, Sigil::At(None));
        assert_eq!(
            el2.args,
            Some(Value::Map(vec![(
                "wiki".to_string(),
                Value::String("@Templater".to_string())
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
