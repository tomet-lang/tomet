use super::*;
use tomet_ast::{Paragraph, Placement, RawText, Sigil, Span, Text};

/// Wraps a flat inline sequence as `Element.content` (`Vec<Block>`
/// now) -- every fixture below is still just one paragraph's worth.
fn wrap_content(inlines: Vec<Inline>) -> Vec<Block> {
    vec![Block::Paragraph(Paragraph::new(inlines, Span::dummy()))]
}

#[test]
fn a_backtick_span_survives_export_unescaped() {
    // Tomet keeps `` `x` `` as literal text (the parser only shields
    // the run from further markup), so it reaches the writer looking
    // like prose. Escaping it turned every `` `spec/` `` in the docs
    // into a literal ``\`spec/\``.
    let doc = tomet_parser::parse_document("地の文に `code` と `spec/` があります。\n").unwrap();
    assert_eq!(
        to_markdown(&doc).trim(),
        "地の文に `code` と `spec/` があります。"
    );
}

#[test]
fn ruby_exports_as_raw_html() {
    let doc = tomet_parser::parse_document("a @ruby[漢字](rt:\"かんじ\") b\n").unwrap();
    assert_eq!(
        to_markdown(&doc).trim(),
        "a <ruby>漢字<rt>かんじ</rt></ruby> b"
    );
}

#[test]
fn an_unpaired_backtick_is_still_escaped() {
    // No closing backtick on the line -- not a span, so it stays an
    // escaped literal, exactly as before.
    let doc = tomet_parser::parse_document("値段は 100` です\n").unwrap();
    assert_eq!(to_markdown(&doc).trim(), "値段は 100\\` です");
}

#[test]
fn a_backtick_span_is_not_escaped_inside_a_table_cell() {
    let doc =
        tomet_parser::parse_document("@table()[\n[ Dir ][ Lang ]\n[ `spec/` ][ 日本語 ]\n]{}\n")
            .unwrap();
    assert!(
        to_markdown(&doc).contains("| `spec/` |"),
        "got: {}",
        to_markdown(&doc)
    );
}

#[test]
fn directives_have_no_markdown_output() {
    // `settings` and the binding element joined `BUILTIN_KINDS` after
    // this list was written, so they used to fall through to the
    // generic passthrough and emit a `<div data-tm-kind="settings">`.
    //
    // This list is also what caught `@import` splitting into `@use`
    // and `@include`: dropping `import` from `BUILTIN_KINDS` made it
    // a `Custom` kind again, and the div came straight back.
    for src in [
        "@settings(file:project.settings.tmt)\n",
        "@use(deck)\n",
        "@include(./chapter.tmt)\n",
        "@meta{type: note}\n",
    ] {
        let doc = tomet_parser::parse_document(src).unwrap();
        assert_eq!(to_markdown(&doc).trim(), "", "for {src:?}");
    }
}

#[test]
fn adjacent_meta_blocks_have_no_visible_output() {
    // Hand-built `Document` (doesn't go through `tomet_parser::
    // parse_document`, so it's independent of the parser's own
    // paragraph-continuation rules -- `tomet-parser` no longer
    // folds adjacent `@meta(...)` elements with no blank line between
    // them into one `Block::Paragraph`; each becomes its own
    // `Block::Element` now, see `document.rs`'s `Stop::Paragraph`).
    // Kept as a regression test for the shape this crate must still
    // handle correctly if it ever *does* show up (e.g. a
    // hand-authored `Document`, or a future grammar change): a
    // `Block::Paragraph` containing only no-output elements plus
    // inter-element whitespace text must not leave a stray blank line
    // behind in the exported Markdown.
    fn meta_element(tag: &str) -> Element {
        Element {
            sigil: Sigil::named("meta"),
            placement: Placement::Inline,
            args: Some(Value::String(tag.to_string())),
            content: None,
            value: Some(ElementValue::from_map(Value::Map(vec![(
                "key".to_string(),
                Value::String("value".to_string()),
            )]))),
            id: None,
            connects: Vec::new(),
            span: Span::dummy(),
        }
    }
    let doc = Document {
        blocks: vec![
            Block::Paragraph(Paragraph::new(
                vec![
                    Inline::Element(meta_element("json")),
                    Inline::Text(Text::new(" ", Span::dummy())),
                    Inline::Element(meta_element("yaml")),
                    Inline::Text(Text::new(" ", Span::dummy())),
                    Inline::Element(meta_element("toml")),
                ],
                Span::dummy(),
            )),
            Block::Element(heading_element(
                1,
                vec![Inline::Text(Text::new("next", Span::dummy()))],
            )),
        ],
        span: Span::dummy(),
    };
    assert_eq!(to_markdown(&doc), "# next\n\n");
}

fn heading_element(level: i64, content: Vec<Inline>) -> Element {
    Element {
        sigil: Sigil::named("heading"),
        placement: Placement::Block,
        args: Some(Value::Int(level)),
        content: Some(wrap_content(content)),
        value: None,
        id: None,
        connects: Vec::new(),
        span: Span::dummy(),
    }
}

#[test]
fn heading_and_paragraph() {
    let doc = Document {
        blocks: vec![
            Block::Element(heading_element(
                2,
                vec![Inline::Text(Text::new("Title", Span::dummy()))],
            )),
            Block::Paragraph(Paragraph::new(
                vec![Inline::Text(Text::new("Hello.", Span::dummy()))],
                Span::dummy(),
            )),
        ],
        span: Span::dummy(),
    };
    assert_eq!(to_markdown(&doc), "## Title\n\nHello.\n\n");
}

#[test]
fn nested_inline_heading_does_not_export_as_bare_hash_line() {
    // `@heading(2)[...]` nested inside a paragraph (inline position)
    // must not emit a bare `## text` mid-paragraph -- that wouldn't
    // parse back as a heading on re-import anyway. It falls through to
    // generic/custom rendering instead (raw HTML passthrough, per this
    // module's documented fallback for constructs with no CommonMark
    // form).
    let doc = Document {
        blocks: vec![Block::Paragraph(Paragraph::new(
            vec![
                Inline::Text(Text::new("before ", Span::dummy())),
                Inline::Element(heading_element(
                    2,
                    vec![Inline::Text(Text::new("Nested", Span::dummy()))],
                )),
                Inline::Text(Text::new(" after", Span::dummy())),
            ],
            Span::dummy(),
        ))],
        span: Span::dummy(),
    };
    let md = to_markdown(&doc);
    assert!(!md.contains("## Nested"), "got: {md:?}");
}

#[test]
fn bullet_and_ordered_list() {
    let doc = Document {
        blocks: vec![Block::List(tomet_tree::list(
            true,
            vec![
                tomet_tree::list_item(
                    Sigil::Bare,
                    wrap_content(vec![Inline::Text(Text::new("one", Span::dummy()))]),
                    None,
                    None,
                    None,
                    Vec::new(),
                    None,
                    Span::dummy(),
                ),
                tomet_tree::list_item(
                    Sigil::Bare,
                    wrap_content(vec![Inline::Text(Text::new("two", Span::dummy()))]),
                    None,
                    None,
                    None,
                    Vec::new(),
                    None,
                    Span::dummy(),
                ),
            ],
            Span::dummy(),
        ))],
        span: Span::dummy(),
    };
    assert_eq!(to_markdown(&doc), "1. one\n2. two\n\n");
}

#[test]
fn emphasis_and_strong() {
    let doc = Document {
        blocks: vec![Block::Paragraph(Paragraph::new(
            vec![
                Inline::Element(Element {
                    sigil: Sigil::named("em"),
                    placement: Placement::Inline,
                    args: None,
                    content: Some(wrap_content(vec![Inline::Text(Text::new(
                        "a",
                        Span::dummy(),
                    ))])),
                    value: None,
                    id: None,
                    connects: Vec::new(),
                    span: Span::dummy(),
                }),
                Inline::Text(Text::new(" ", Span::dummy())),
                Inline::Element(Element {
                    sigil: Sigil::named("strong"),
                    placement: Placement::Inline,
                    args: None,
                    content: Some(wrap_content(vec![Inline::Text(Text::new(
                        "b",
                        Span::dummy(),
                    ))])),
                    value: None,
                    id: None,
                    connects: Vec::new(),
                    span: Span::dummy(),
                }),
            ],
            Span::dummy(),
        ))],
        span: Span::dummy(),
    };
    assert_eq!(to_markdown(&doc), "*a* **b**\n\n");
}

#[test]
fn link_round_trips() {
    let el = Element {
        sigil: Sigil::named("link"),
        placement: Placement::Inline,
        args: Some(Value::Map(vec![(
            "target".to_string(),
            Value::String("https://example.com".to_string()),
        )])),
        content: Some(wrap_content(vec![Inline::Text(Text::new(
            "Wiki",
            Span::dummy(),
        ))])),
        value: None,
        id: None,
        connects: Vec::new(),
        span: Span::dummy(),
    };
    let doc = Document {
        blocks: vec![Block::Paragraph(Paragraph::new(
            vec![Inline::Element(el)],
            Span::dummy(),
        ))],
        span: Span::dummy(),
    };
    assert_eq!(to_markdown(&doc), "[Wiki](https://example.com)\n\n");
}

#[test]
fn embed_becomes_image() {
    let el = Element {
        sigil: Sigil::named("embed"),
        placement: Placement::Inline,
        args: Some(Value::Map(vec![(
            "target".to_string(),
            Value::String("pic.png".to_string()),
        )])),
        content: Some(wrap_content(vec![Inline::Text(Text::new(
            "a cat",
            Span::dummy(),
        ))])),
        value: None,
        id: None,
        connects: Vec::new(),
        span: Span::dummy(),
    };
    let doc = Document {
        blocks: vec![Block::Paragraph(Paragraph::new(
            vec![Inline::Element(el)],
            Span::dummy(),
        ))],
        span: Span::dummy(),
    };
    assert_eq!(to_markdown(&doc), "![a cat](pic.png)\n\n");
}

#[test]
fn raw_block_uses_fence_and_lang() {
    let el = Element {
        sigil: Sigil::named("raw"),
        placement: Placement::Block,
        args: Some(Value::Map(vec![(
            "lang".to_string(),
            Value::String("rust".to_string()),
        )])),
        content: Some(wrap_content(vec![Inline::Text(Text::new(
            "fn main() {}",
            Span::dummy(),
        ))])),
        value: None,
        id: None,
        connects: Vec::new(),
        span: Span::dummy(),
    };
    let doc = Document {
        blocks: vec![Block::Element(el)],
        span: Span::dummy(),
    };
    assert_eq!(to_markdown(&doc), "```rust\nfn main() {}\n```\n\n");
}

#[test]
fn raw_inline_uses_backticks() {
    let el = Element {
        sigil: Sigil::named("raw"),
        placement: Placement::Inline,
        args: None,
        content: Some(wrap_content(vec![Inline::Raw(RawText::new(
            "foo()",
            Span::dummy(),
        ))])),
        value: None,
        id: None,
        connects: Vec::new(),
        span: Span::dummy(),
    };
    let doc = Document {
        blocks: vec![Block::Paragraph(Paragraph::new(
            vec![
                Inline::Text(Text::new("call ", Span::dummy())),
                Inline::Element(el),
                Inline::Text(Text::new(" now", Span::dummy())),
            ],
            Span::dummy(),
        ))],
        span: Span::dummy(),
    };
    assert_eq!(to_markdown(&doc), "call `foo()` now\n\n");
}

#[test]
fn thematic_break() {
    let doc = Document {
        blocks: vec![Block::Element(tomet_tree::element_new(Sigil::named("hr")))],
        span: Span::dummy(),
    };
    assert_eq!(to_markdown(&doc), "---\n\n");
}

#[test]
fn config_element_exports_as_nothing() {
    let mut el = tomet_tree::element_new(Sigil::named("config"));
    el.args = Some(Value::Map(vec![(
        "format".to_string(),
        Value::String("json".to_string()),
    )]));
    let doc = Document {
        blocks: vec![Block::Element(el)],
        span: Span::dummy(),
    };
    assert_eq!(to_markdown(&doc), "");
}

/// The pair this asserts is the point: a bold line, a blank line, then
/// the rule. `Title\n---` -- what this used to produce, and what this
/// test used to lock in -- is a setext heading, so the divider came out
/// as an `<h2>`.
#[test]
fn titled_thematic_break_is_not_a_setext_heading() {
    let mut el = tomet_tree::element_new(Sigil::named("hr"));
    el.content = Some(wrap_content(vec![Inline::Text(Text::new(
        "Title",
        Span::dummy(),
    ))]));
    let doc = Document {
        blocks: vec![Block::Element(el)],
        span: Span::dummy(),
    };
    assert_eq!(to_markdown(&doc), "**Title**\n\n---\n\n");
}

#[test]
fn wikilink_exports_to_markdown() {
    let mut el1 = tomet_tree::element_new(Sigil::named("link"));
    el1.args = Some(Value::Map(vec![(
        "target".to_string(),
        Value::String("ref:name".to_string()),
    )]));

    let mut el2 = tomet_tree::element_new(Sigil::named("link"));
    el2.args = Some(Value::Map(vec![(
        "target".to_string(),
        Value::String("ref:name".to_string()),
    )]));
    el2.content = Some(wrap_content(vec![Inline::Text(Text::new(
        "display",
        Span::dummy(),
    ))]));

    let doc = Document {
        blocks: vec![Block::Paragraph(Paragraph::new(
            vec![
                Inline::Element(el1),
                Inline::Text(Text::new(" and ", Span::dummy())),
                Inline::Element(el2),
            ],
            Span::dummy(),
        ))],
        span: Span::dummy(),
    };
    assert_eq!(to_markdown(&doc), "[[name]] and [[name|display]]\n\n");
}

#[test]
fn table_exports_to_markdown() {
    let mut el = tomet_tree::element_new(Sigil::named("table"));
    el.content = Some(wrap_content(vec![Inline::Text(Text::new(
        "[ col1 ][ col2 ]\n[ val1 ][ val2 ]",
        Span::dummy(),
    ))]));
    let doc = Document {
        blocks: vec![Block::Element(el)],
        span: Span::dummy(),
    };
    assert_eq!(
        to_markdown(&doc),
        "| col1 | col2 |\n| --- | --- |\n| val1 | val2 |\n\n"
    );
}

/// This crate does not evaluate `${...}` any more, and this test is
/// what is left of the one that said it did.
///
/// It used to assert the expanded macro. That expansion now happens in
/// `tomet-transform`'s `resolve_interpolations`, before any converter
/// sees the tree, and is pinned there -- because four converters each
/// deciding it produced three different answers for the same document.
///
/// What reaches here is a document nobody prepared, and the only
/// honest rendering of that is what was written.
#[test]
fn an_unprepared_interpolation_renders_as_its_own_source() {
    // The source's own line wrapping survives to the markdown output
    // now (`inline_to_md` renders a `SoftBreak` as a literal newline,
    // not a space -- see its own comment) rather than being folded
    // into one long line, which is what happened while `tomet-parser`
    // still folded a wrapped line into a space before this crate ever
    // saw it.
    let doc =
        tomet_parser::parse_document("Issue: $gh(42)\nFooter: ${copyright}\nMath: ${add(10, 5)}\n")
            .unwrap();
    assert_eq!(
        to_markdown(&doc),
        "Issue: ${gh(42)}\nFooter: ${copyright}\nMath: ${add(10, 5)}\n\n"
    );
}

#[test]
fn inline_footnote_exports_to_markdown() {
    let doc =
        tomet_parser::parse_document("This has a footnote @footnote[a short note] in text.\n")
            .unwrap();
    let md = to_markdown(&doc);
    assert!(
        md.contains("This has a footnote [^1] in text."),
        "got: {md}"
    );
    assert!(md.contains("[^1]: a short note"), "got: {md}");
}

#[test]
fn separated_footnote_with_multiple_carets() {
    let src = "\
Prose A ^(shared).
Prose B ^(shared).

@footnote(shared)[Shared footnote explanation.]
";
    let doc = tomet_parser::parse_document(src).unwrap();
    let md = to_markdown(&doc);
    assert!(md.contains("Prose A [^1]."), "got: {md}");
    assert!(md.contains("Prose B [^1]."), "got: {md}");
    assert!(
        md.contains("[^1]: Shared footnote explanation."),
        "got: {md}"
    );
    // Ensure definition block was not rendered in place
    assert!(!md.contains("@footnote"), "got: {md}");
}

#[test]
fn tag_elements_export_to_markdown() {
    let src = "@tag(rust, tomet) and @tag(spec)\n";
    let doc = tomet_parser::parse_document(src).unwrap();
    let md = to_markdown(&doc);
    assert_eq!(md.trim(), "#rust #tomet and #spec");
}
