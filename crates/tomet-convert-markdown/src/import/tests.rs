use super::*;

/// Wraps a flat inline sequence as `Element.content` (`Vec<Block>`
/// now) -- every fixture below is still just one paragraph's worth.
fn wrap(inlines: Vec<Inline>) -> Option<Vec<Block>> {
    Some(vec![Block::Paragraph(Paragraph::new(
        inlines,
        Span::dummy(),
    ))])
}

/// The single paragraph's own `Vec<Inline>` inside `Element.content`
/// (`Vec<Block>` now) -- several fixtures below build up content as a
/// flat inline run (table cells) and still index into it that way.
fn para_inlines(blocks: &[Block]) -> &[Inline] {
    match blocks {
        [Block::Paragraph(p)] => &p.content,
        _ => panic!("expected a single paragraph, got {blocks:?}"),
    }
}

#[test]
fn heading_and_paragraph() {
    let doc = from_markdown("# Title\n\nHello world.\n");
    assert_eq!(doc.blocks.len(), 1);
    match &doc.blocks[0] {
        Block::Section(sec) => {
            assert_eq!(sec.level, 1);
            assert_eq!(
                sec.title,
                vec![Inline::Text(Text::new("Title", Span::dummy()))]
            );
            assert_eq!(sec.blocks.len(), 1);
            assert_eq!(
                sec.blocks[0],
                Block::Paragraph(Paragraph::new(
                    vec![Inline::Text(Text::new("Hello world.", Span::dummy()))],
                    Span::dummy()
                ))
            );
        }
        other => panic!("expected section, got {other:?}"),
    }
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
            assert_eq!(kinds, vec![Sigil::named("em"), Sigil::named("strong")]);
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
                list.items[0].element.content,
                wrap(vec![Inline::Text(Text::new("one", Span::dummy()))])
            );
            assert_eq!(
                list.items[1].element.content,
                wrap(vec![Inline::Text(Text::new("two", Span::dummy()))])
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
            let items = &list.items;
            assert_eq!(items.len(), 2);
            assert_eq!(
                items[0].element.content,
                wrap(vec![Inline::Text(Text::new("a", Span::dummy()))])
            );
            let sub = items[0].sublist.as_ref().expect("nested sub-list");
            assert_eq!(sub.items.len(), 1);
            assert_eq!(
                sub.items[0].element.content,
                wrap(vec![Inline::Text(Text::new("b", Span::dummy()))])
            );
            assert_eq!(
                items[1].element.content,
                wrap(vec![Inline::Text(Text::new("c", Span::dummy()))])
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
                assert_eq!(el.sigil, Sigil::named("link"));
                assert_eq!(
                    el.args,
                    Some(Value::Map(vec![(
                        "target".to_string(),
                        Value::String("https://example.com".to_string())
                    )]))
                );
                assert_eq!(
                    el.content,
                    wrap(vec![Inline::Text(Text::new("Wiki", Span::dummy()))])
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
            Inline::Element(el) => assert_eq!(el.sigil, Sigil::named("link")),
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
                assert_eq!(el.sigil, Sigil::named("embed"));
                assert_eq!(
                    el.args,
                    Some(Value::Map(vec![(
                        "target".to_string(),
                        Value::String("assets/pic.png".to_string())
                    )]))
                );
                assert_eq!(
                    el.content,
                    wrap(vec![Inline::Text(Text::new("a cat", Span::dummy()))])
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
        Block::Element(el) => assert_eq!(el.sigil, Sigil::named("hr")),
        other => panic!("expected hr element, got {other:?}"),
    }
}

#[test]
fn fenced_code_block() {
    let doc = from_markdown("```rust\nfn main() {}\n```\n");
    match &doc.blocks[0] {
        Block::Element(el) => {
            assert_eq!(el.sigil, Sigil::named("raw"));
            assert_eq!(
                el.args,
                Some(Value::Map(vec![(
                    "lang".to_string(),
                    Value::String("rust".to_string())
                )]))
            );
            assert_eq!(
                el.content,
                wrap(vec![Inline::Raw(RawText::new(
                    "fn main() {}",
                    Span::dummy()
                ))])
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
            assert_eq!(el.sigil, Sigil::named("quote"));
            assert_eq!(el.args, None);
            assert_eq!(
                el.content,
                wrap(vec![Inline::Text(Text::new("quoted text", Span::dummy()))])
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
            assert_eq!(el.sigil, Sigil::named("quote"));
            let content = para_inlines(el.content.as_ref().expect("content"));
            assert!(
                !content
                    .iter()
                    .any(|i| matches!(i, Inline::Element(inner) if classify_std_lenient(inner) == ElementKind::Heading)),
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
fn inline_code_span_becomes_raw_element() {
    let doc = from_markdown("call `foo()` now\n");
    assert_eq!(
        doc.blocks[0],
        Block::Paragraph(Paragraph::new(
            vec![
                Inline::Text(Text::new("call ", Span::dummy())),
                wrap_inline(
                    "raw",
                    vec![Inline::Raw(RawText::new("foo()", Span::dummy()))],
                ),
                Inline::Text(Text::new(" now", Span::dummy())),
            ],
            Span::dummy()
        ))
    );
}

/// The sigil guard has to leave a code span alone. It is an `@raw` element
/// whose content is `Inline::Raw`, so sigils inside it stay untouched.
#[test]
fn a_code_span_keeps_the_sigils_inside_it() {
    for (src, expected_code) in [
        ("the `//!` module doc\n", "//!"),
        ("the `//` doc\n", "//"),
        ("the `/*` and `*/` pair\n", "/*"),
        ("write to `@x` please\n", "@x"),
    ] {
        let doc = from_markdown(src);
        let Block::Paragraph(p) = &doc.blocks[0] else {
            panic!("expected a paragraph for {src:?}")
        };
        let has_raw = p.content.iter().any(|inl| match inl {
            Inline::Element(el) if el.sigil.is_bare_named("raw") => el
                .content
                .as_ref()
                .is_some_and(|c| match para_inlines(c).first() {
                    Some(Inline::Raw(r)) => r.value == expected_code,
                    _ => false,
                }),
            _ => false,
        });
        assert!(
            has_raw,
            "expected raw element with {expected_code:?} for {src:?}"
        );
    }
}

/// Outside a span the guard still fires -- that is what keeps an
/// imported `//` from reading back as a Tomet comment.
#[test]
fn a_bare_sigil_is_still_protected() {
    let doc = from_markdown("a bare // comment\n");
    let Block::Paragraph(p) = &doc.blocks[0] else {
        panic!("expected a paragraph")
    };
    let Inline::Text(t) = &p.content[0] else {
        panic!("expected text")
    };
    assert_eq!(t.value, "a bare `//` comment");
}

#[test]
fn a_code_span_holding_a_backtick_survives_import() {
    let doc = from_markdown("``a`b``\n");
    let Block::Paragraph(p) = &doc.blocks[0] else {
        panic!("expected a paragraph");
    };
    assert_eq!(
        p.content[0],
        wrap_inline("raw", vec![Inline::Raw(RawText::new("a`b", Span::dummy()))])
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
    assert_eq!(el1.sigil, Sigil::named("link"));
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
    assert_eq!(el2.sigil, Sigil::named("link"));
    assert_eq!(
        el2.args,
        Some(Value::Map(vec![(
            "target".to_string(),
            Value::String("ref:name".to_string())
        )]))
    );
    assert_eq!(
        el2.content,
        wrap(vec![Inline::Text(Text::new("display", Span::dummy()))])
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
            assert_eq!(el.sigil, Sigil::named("table"));
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
    let items = &list.items;
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].element.content,
        wrap(vec![Inline::Text(Text::new("a", Span::dummy()))])
    );
    let sub_list = items[0].sublist.as_ref().expect("nested sub-list");
    let sub_items = &sub_list.items;
    assert_eq!(sub_items.len(), 1);
    assert_eq!(
        sub_items[0].element.content,
        wrap(vec![Inline::Text(Text::new("b", Span::dummy()))])
    );
}

#[test]
fn table_converts_default_without_width_adjustment() {
    let src = "| title | sdfasdf |\n| --- | --- |\n| title | sdfddfasdf |\n";
    let doc = from_markdown(src);
    let Block::Element(el) = &doc.blocks[0] else {
        panic!("expected element");
    };
    let content = para_inlines(el.content.as_ref().unwrap());
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
    let content = para_inlines(el.content.as_ref().unwrap());
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
    let content = para_inlines(el.content.as_ref().unwrap());
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
    let content = para_inlines(el.content.as_ref().unwrap());
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
            // `<` is no longer a Tomet sigil, so it passes through.
            "Contact `@`user at <foo> or `$`100 and <> with 2 * 3 `^` 2 and **.",
            Span::dummy()
        ))]
    );
}

#[test]
fn checkbox_task_list_markers_and_custom_markers_import_as_typed_markers() {
    let md = "- [x] task1\n- [ ] task2\n- [c] con item\n- [p] pro item\n- (x) task3\n";
    let doc = from_markdown(md);
    assert_eq!(doc.blocks.len(), 1);
    let Block::List(list) = &doc.blocks[0] else {
        panic!("expected list");
    };
    let items = &list.items;
    assert_eq!(items.len(), 5);
    assert_eq!(items[0].element.args, Some(Value::String("x".to_string())));
    assert_eq!(
        items[0].element.content,
        wrap(vec![Inline::Text(Text::new("task1", Span::dummy()))])
    );
    assert_eq!(items[1].element.args, Some(Value::String(" ".to_string())));
    assert_eq!(
        items[1].element.content,
        wrap(vec![Inline::Text(Text::new("task2", Span::dummy()))])
    );
    assert_eq!(items[2].element.args, Some(Value::String("c".to_string())));
    assert_eq!(
        items[2].element.content,
        wrap(vec![Inline::Text(Text::new("con item", Span::dummy()))])
    );
    assert_eq!(items[3].element.args, Some(Value::String("p".to_string())));
    assert_eq!(
        items[3].element.content,
        wrap(vec![Inline::Text(Text::new("pro item", Span::dummy()))])
    );
    assert_eq!(items[4].element.args, Some(Value::String("x".to_string())));
    assert_eq!(
        items[4].element.content,
        wrap(vec![Inline::Text(Text::new("task3", Span::dummy()))])
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
    assert_eq!(embed1.sigil, Sigil::named("embed"));
    assert_eq!(
        embed1.args,
        Some(Value::Map(vec![(
            "target".to_string(),
            Value::String("name".to_string())
        )]))
    );
    assert_eq!(
        embed1.content,
        wrap(vec![Inline::Text(Text::new("display", Span::dummy()))])
    );

    let Inline::Element(embed2) = &p.content[2] else {
        panic!("expected embed element 2");
    };
    assert_eq!(embed2.sigil, Sigil::named("embed"));
    assert_eq!(
        embed2.args,
        Some(Value::Map(vec![(
            "target".to_string(),
            Value::String("_path".to_string())
        )]))
    );
    assert_eq!(
        embed2.content,
        wrap(vec![Inline::Text(Text::new("display", Span::dummy()))])
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
    assert_eq!(el1.sigil, Sigil::named("link"));
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
    assert_eq!(el2.sigil, Sigil::named("link"));
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
    // "Preserved" now means kept as their own `SoftBreak` nodes rather
    // than folded into a space -- not, as before `SoftBreak` existed,
    // merged into one `Text` with a literal `'\n'` character. The
    // latter was already a small lie: a real softbreak is whitespace,
    // renderers are free to realize it as a space instead of a newline
    // (see `Inline::SoftBreak`'s doc comment), so baking one spelling
    // into `Text.value` overclaimed what was actually preserved.
    let md = "Line 1\nLine 2\nLine 3\n";
    let doc = from_markdown(md);
    assert_eq!(doc.blocks.len(), 1);
    let Block::Paragraph(p) = &doc.blocks[0] else {
        panic!("expected paragraph");
    };
    assert_eq!(
        p.content,
        vec![
            Inline::Text(Text::new("Line 1", Span::dummy())),
            Inline::SoftBreak(SoftBreak {
                span: Span::dummy()
            }),
            Inline::Text(Text::new("Line 2", Span::dummy())),
            Inline::SoftBreak(SoftBreak {
                span: Span::dummy()
            }),
            Inline::Text(Text::new("Line 3", Span::dummy())),
        ]
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
    assert_eq!(el.sigil, Sigil::named("callout"));
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
    assert_eq!(el_plain.sigil, Sigil::named("quote"));
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
    let content = para_inlines(el.content.as_ref().unwrap());
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
    let md = "- http://127.0.0.1:8888/search?lang=ja&q=@query\n- https://example.com:1011/search?lang=ja&q=@query\n";
    let doc = from_markdown(md);
    let exported = crate::export::to_markdown(&doc);
    assert_eq!(exported.trim(), md.trim());
}

#[test]
fn test_bare_angle_brackets_not_escaped() {
    let md = "asdfasdfdsf>\n#foo\n";
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
