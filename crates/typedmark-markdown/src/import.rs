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
//! run (`Element::area` is `Vec<Inline>`, not `Vec<Block>`). HTML blocks
//! and inline HTML are dropped; hard breaks collapse to a space.

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use typedmark_ast::{
    Block, Document, Element, ElementValue, Heading, Inline, ListItem, Sigil, Value,
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
    let parser = Parser::new_ext(src, Options::empty());
    let mut stack: Vec<Frame> = vec![Frame::Blocks(Vec::new())];

    for event in parser {
        match event {
            Event::Start(tag) => stack.push(start_frame(tag)),
            Event::End(tag_end) => end_frame(&mut stack, tag_end),
            Event::Text(text) => match stack.last_mut() {
                Some(Frame::CodeBlock { text: buf, .. }) => buf.push_str(&text),
                Some(Frame::Discard) => {}
                _ => push_inline(&mut stack, Inline::Text(text.into_string())),
            },
            Event::Code(code) => {
                if !matches!(stack.last(), Some(Frame::Discard)) {
                    push_inline(&mut stack, Inline::Text(format!("`{code}`")));
                }
            }
            Event::Html(_) | Event::InlineHtml(_) => {}
            Event::SoftBreak => push_inline(&mut stack, Inline::Text(" ".to_string())),
            Event::HardBreak => push_inline(&mut stack, Inline::Text(" ".to_string())),
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
    match root {
        Frame::Blocks(blocks) => Document { blocks },
        _ => Document::default(),
    }
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
        (Frame::Paragraph(inlines), TagEnd::Paragraph) => {
            push_block(stack, Block::Paragraph(inlines))
        }
        (Frame::Heading(level, inlines), TagEnd::Heading(_)) => push_block(
            stack,
            Block::Heading(Heading {
                level,
                content: inlines,
                attrs: None,
            }),
        ),
        (Frame::BlockQuote(area), TagEnd::BlockQuote(_)) => {
            let el = Element {
                sigil: Sigil::Type("blockquote".to_string()),
                input: None,
                area: Some(area),
                value: None,
            };
            push_block(stack, Block::Element(el));
        }
        (Frame::CodeBlock { lang, mut text }, TagEnd::CodeBlock) => {
            if text.ends_with('\n') {
                text.pop();
            }
            let input = if lang.is_empty() {
                None
            } else {
                Some(Value::Map(vec![("lang".to_string(), Value::String(lang))]))
            };
            let el = Element {
                sigil: Sigil::Type("pre".to_string()),
                input,
                area: None,
                value: Some(ElementValue::Data(Value::String(text))),
            };
            push_block(stack, Block::Element(el));
        }
        (Frame::List { ordered, items }, TagEnd::List(_)) => {
            push_block(stack, Block::List { ordered, items })
        }
        (Frame::Item { content, extra }, TagEnd::Item) => match stack.last_mut() {
            Some(Frame::List { items, .. }) => {
                items.push(ListItem { content });
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
                input: Some(Value::Map(vec![("url".to_string(), Value::String(dest))])),
                area: Some(inlines),
                value: None,
            };
            push_inline(stack, Inline::Element(el));
        }
        (Frame::Image { dest, alt }, TagEnd::Image) => {
            let key = if dest.contains("://") { "url" } else { "file" };
            let el = Element {
                sigil: Sigil::Type("embed".to_string()),
                input: Some(Value::Map(vec![(key.to_string(), Value::String(dest))])),
                area: Some(alt),
                value: None,
            };
            push_inline(stack, Inline::Element(el));
        }
        // HtmlBlock and anything routed to Discard.
        (Frame::Discard, _) => {}
        (_, _) => {}
    }
}

fn wrap_inline(tag: &str, area: Vec<Inline>) -> Inline {
    Inline::Element(Element {
        sigil: Sigil::Type(tag.to_string()),
        input: None,
        area: Some(area),
        value: None,
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
        Frame::BlockQuote(area) => Some(area),
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
        prev.push_str(new);
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
        Some(Frame::BlockQuote(area)) => merge_block_into(area, None, block),
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
        Block::Paragraph(inlines) => extend_spaced(content, inlines),
        Block::Heading(h) => extend_spaced(content, h.content),
        Block::Element(el) => {
            if !content.is_empty() {
                content.push(Inline::Text(" ".to_string()));
            }
            content.push(Inline::Element(el));
        }
        Block::List { items, .. } => match extra_items {
            Some(extra) => extra.extend(items),
            None => {
                for item in items {
                    extend_spaced(content, item.content);
                }
            }
        },
    }
}

fn extend_spaced(content: &mut Vec<Inline>, more: Vec<Inline>) {
    if !content.is_empty() && !more.is_empty() {
        content.push(Inline::Text(" ".to_string()));
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
                assert_eq!(h.content, vec![Inline::Text("Title".to_string())]);
            }
            other => panic!("expected heading, got {other:?}"),
        }
        assert_eq!(
            doc.blocks[1],
            Block::Paragraph(vec![Inline::Text("Hello world.".to_string())])
        );
    }

    #[test]
    fn emphasis_and_strong() {
        let doc = from_markdown("a *em* b **strong** c\n");
        match &doc.blocks[0] {
            Block::Paragraph(inlines) => {
                let kinds: Vec<_> = inlines
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
            Block::List { ordered, items } => {
                assert!(!ordered);
                assert_eq!(items[0].content, vec![Inline::Text("one".to_string())]);
                assert_eq!(items[1].content, vec![Inline::Text("two".to_string())]);
            }
            other => panic!("expected list, got {other:?}"),
        }
    }

    #[test]
    fn ordered_list() {
        let doc = from_markdown("1. one\n2. two\n");
        match &doc.blocks[0] {
            Block::List { ordered, .. } => assert!(ordered),
            other => panic!("expected list, got {other:?}"),
        }
    }

    #[test]
    fn nested_list_flattens_to_sibling_items() {
        let doc = from_markdown("- a\n  - b\n- c\n");
        match &doc.blocks[0] {
            Block::List { items, .. } => {
                let texts: Vec<_> = items
                    .iter()
                    .map(|i| match i.content.as_slice() {
                        [Inline::Text(t)] => t.clone(),
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
            Block::Paragraph(inlines) => match &inlines[0] {
                Inline::Element(el) => {
                    assert_eq!(el.sigil, Sigil::At(None));
                    assert_eq!(
                        el.input,
                        Some(Value::Map(vec![(
                            "url".to_string(),
                            Value::String("https://example.com".to_string())
                        )]))
                    );
                    assert_eq!(el.area, Some(vec![Inline::Text("Wiki".to_string())]));
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
            Block::Paragraph(inlines) => match &inlines[0] {
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
            Block::Paragraph(inlines) => match &inlines[0] {
                Inline::Element(el) => {
                    assert_eq!(el.sigil, Sigil::Type("embed".to_string()));
                    assert_eq!(
                        el.input,
                        Some(Value::Map(vec![(
                            "file".to_string(),
                            Value::String("assets/pic.png".to_string())
                        )]))
                    );
                    assert_eq!(el.area, Some(vec![Inline::Text("a cat".to_string())]));
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
                assert_eq!(el.sigil, Sigil::Type("pre".to_string()));
                assert_eq!(
                    el.input,
                    Some(Value::Map(vec![(
                        "lang".to_string(),
                        Value::String("rust".to_string())
                    )]))
                );
                assert_eq!(
                    el.value,
                    Some(ElementValue::Data(Value::String(
                        "fn main() {}".to_string()
                    )))
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
                assert_eq!(el.area, Some(vec![Inline::Text("quoted text".to_string())]));
            }
            other => panic!("expected blockquote element, got {other:?}"),
        }
    }

    #[test]
    fn inline_code_span_survives_as_backticked_text() {
        let doc = from_markdown("call `foo()` now\n");
        assert_eq!(
            doc.blocks[0],
            Block::Paragraph(vec![Inline::Text("call `foo()` now".to_string())])
        );
    }

    #[test]
    fn html_block_is_dropped() {
        let doc = from_markdown("<div>raw html</div>\n\nreal paragraph\n");
        assert_eq!(doc.blocks.len(), 1);
        assert_eq!(
            doc.blocks[0],
            Block::Paragraph(vec![Inline::Text("real paragraph".to_string())])
        );
    }
}
