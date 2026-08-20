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
        extra: Vec<ListItem>,
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
    /// HTML blocks/inline HTML -- swallow everything until the matching
    /// End, out of scope for v1 (see module docs).
    Discard,
}

pub fn from_markdown(src: &str) -> Document {
    let (frontmatter, markdown_body) = extract_yaml_frontmatter(src);

    let parser = Parser::new_ext(markdown_body, Options::empty());
    let mut stack: Vec<Frame> = vec![Frame::Blocks(Vec::new())];

    if let Some(entries) = frontmatter {
        let mut meta_el = Element::new(Sigil::At(Some("meta".to_string())));
        meta_el.value = Some(typedmark_ast::ElementValue::Data(Value::Map(entries)));
        push_block(&mut stack, Block::Element(meta_el));
    }

    for event in parser {
        match event {
            Event::Start(tag) => stack.push(start_frame(tag)),
            Event::End(tag_end) => end_frame(&mut stack, tag_end),
            Event::Text(text) => match stack.last_mut() {
                Some(Frame::CodeBlock { text: buf, .. }) => buf.push_str(&text),
                Some(Frame::Discard) => {}
                _ => push_inline(
                    &mut stack,
                    Inline::Text(Text::new(text.into_string(), Span::dummy())),
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
            Event::Html(_) | Event::InlineHtml(_) => {}
            Event::SoftBreak => {
                push_inline(&mut stack, Inline::Text(Text::new(" ", Span::dummy())))
            }
            Event::HardBreak => {
                push_inline(&mut stack, Inline::Text(Text::new(" ", Span::dummy())))
            }
            Event::Rule => push_block(
                &mut stack,
                Block::Element(Element::new(Sigil::Type("hr".to_string()))),
            ),
            // Footnotes/tables/strikethrough/tasklists/math are all gated
            // behind `Options` flags we don't enable, so these shouldn't
            // occur; ignore defensively rather than panic.
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
    let after_closing_idx = if closing_slice.starts_with("\n---") || closing_slice.starts_with("\n...") {
        end_pos + 4
    } else if closing_slice.starts_with("\r\n---") || closing_slice.starts_with("\r\n...") {
        end_pos + 5
    } else {
        return (None, src);
    };

    let remaining_src = rest[after_closing_idx..].trim_start_matches(|c| c == '\r' || c == '\n');

    if let Ok(serde_yaml::Value::Mapping(map)) = serde_yaml::from_str::<serde_yaml::Value>(yaml_text) {
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
        serde_yaml::Value::String(s) => Value::String(s),
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

fn parse_wikilinks(text: &str) -> Vec<Inline> {
    let mut result = Vec::new();
    let mut remaining = text;

    while let Some(start_idx) = remaining.find("[[") {
        if let Some(end_idx) = remaining[start_idx + 2..].find("]]") {
            let actual_end_idx = start_idx + 2 + end_idx;
            if start_idx > 0 {
                result.push(Inline::Text(Text::new(
                    remaining[..start_idx].to_string(),
                    Span::dummy(),
                )));
            }

            let inner = &remaining[start_idx + 2..actual_end_idx];
            let wikilink_el = if let Some((target, display)) = inner.split_once('|') {
                let target = target.trim();
                let display = display.trim();
                let mut el = Element::new(Sigil::At(None));
                el.args = Some(Value::Map(vec![(
                    "wiki".to_string(),
                    Value::String(target.to_string()),
                )]));
                el.content = Some(vec![Inline::Text(Text::new(
                    display.to_string(),
                    Span::dummy(),
                ))]);
                el
            } else {
                let target = inner.trim();
                let mut el = Element::new(Sigil::At(None));
                el.args = Some(Value::Map(vec![(
                    "wiki".to_string(),
                    Value::String(target.to_string()),
                )]));
                el
            };

            result.push(Inline::Element(wikilink_el));
            remaining = &remaining[actual_end_idx + 2..];
        } else {
            break;
        }
    }

    if !remaining.is_empty() {
        result.push(Inline::Text(Text::new(
            remaining.to_string(),
            Span::dummy(),
        )));
    }

    result
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
            }
        }
        Block::Element(el) => {
            post_process_element_wikilinks(el);
        }
    }
}

fn post_process_element_wikilinks(el: &mut Element) {
    if let Some(content) = el.content.take() {
        el.content = Some(post_process_inlines_wikilinks(content));
    }
}

fn post_process_inlines_wikilinks(inlines: Vec<Inline>) -> Vec<Inline> {
    let mut new_inlines = Vec::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => {
                new_inlines.extend(parse_wikilinks(&t.value));
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
            extra: Vec::new(),
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
        // HtmlBlock and anything gated behind unset Options (tables,
        // footnotes, strikethrough, definition lists, metadata blocks).
        _ => Frame::Discard,
    }
}

fn end_frame(stack: &mut Vec<Frame>, tag_end: TagEnd) {
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
        (Frame::BlockQuote(content), TagEnd::BlockQuote(_)) => {
            let el = Element {
                sigil: Sigil::Type("blockquote".to_string()),
                args: Some(Value::String("note".to_string())),
                content: Some(content),
                value: None,
                span: Span::dummy(),
            };
            push_block(stack, Block::Element(el));
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
        (Frame::Item { content, extra }, TagEnd::Item) => match stack.last_mut() {
            Some(Frame::List { items, .. }) => {
                items.push(ListItem::new(content, None, None, Span::dummy()));
                items.extend(extra);
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
            let key = if dest.contains("://") { "url" } else { "file" };
            let el = Element {
                sigil: Sigil::Type("embed".to_string()),
                args: Some(Value::Map(vec![(key.to_string(), Value::String(dest))])),
                content: Some(alt),
                value: None,
                span: Span::dummy(),
            };
            push_inline(stack, Inline::Element(el));
        }
        // HtmlBlock and anything routed to Discard.
        (Frame::Discard, _) => {}
        (_, _) => {}
    }
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
        Some(Frame::BlockQuote(content)) => merge_block_into(content, None, block),
        Some(Frame::Item { content, extra }) => merge_block_into(content, Some(extra), block),
        _ => {}
    }
}

fn merge_block_into(
    content: &mut Vec<Inline>,
    extra_items: Option<&mut Vec<ListItem>>,
    block: Block,
) {
    match block {
        Block::Paragraph(p) => extend_spaced(content, p.content),
        Block::Heading(h) => extend_spaced(content, h.content),
        Block::Element(el) => {
            if !content.is_empty() {
                content.push(Inline::Text(Text::new(" ", Span::dummy())));
            }
            content.push(Inline::Element(el));
        }
        Block::List(list) => match extra_items {
            Some(extra) => extra.extend(list.items),
            None => {
                for item in list.items {
                    extend_spaced(content, item.content);
                }
            }
        },
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
    fn nested_list_flattens_to_sibling_items() {
        let doc = from_markdown("- a\n  - b\n- c\n");
        match &doc.blocks[0] {
            Block::List(list) => {
                let texts: Vec<_> = list
                    .items
                    .iter()
                    .map(|i| match i.content.as_slice() {
                        [Inline::Text(t)] => t.value.clone(),
                        other => panic!("unexpected item content {other:?}"),
                    })
                    .collect();
                assert_eq!(texts, vec!["a", "b", "c"]);
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
                            "file".to_string(),
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
                assert_eq!(el.args, Some(Value::String("note".to_string())));
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
        let Block::Paragraph(p) = &doc.blocks[0] else { panic!("expected paragraph"); };
        assert_eq!(p.content.len(), 5);
        assert_eq!(p.content[0], Inline::Text(Text::new("Check ", Span::dummy())));

        // [[name]] -> @(wiki:name)
        let Inline::Element(el1) = &p.content[1] else { panic!("expected element 1"); };
        assert_eq!(el1.sigil, Sigil::At(None));
        assert_eq!(el1.args, Some(Value::Map(vec![("wiki".to_string(), Value::String("name".to_string()))])));
        assert_eq!(el1.content, None);

        assert_eq!(p.content[2], Inline::Text(Text::new(" and ", Span::dummy())));

        // [[name|display]] -> @[display](wiki:name)
        let Inline::Element(el2) = &p.content[3] else { panic!("expected element 2"); };
        assert_eq!(el2.sigil, Sigil::At(None));
        assert_eq!(el2.args, Some(Value::Map(vec![("wiki".to_string(), Value::String("name".to_string()))])));
        assert_eq!(el2.content, Some(vec![Inline::Text(Text::new("display", Span::dummy()))]));

        assert_eq!(p.content[4], Inline::Text(Text::new(" here.", Span::dummy())));
    }
}
