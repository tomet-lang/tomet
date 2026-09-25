use super::*;

#[test]
fn line_comment_alone_produces_no_blocks() {
    let doc = parse_document("// just a note\n").unwrap();
    assert_eq!(doc.blocks, vec![]);
}

#[test]
fn line_comment_without_trailing_newline_is_not_an_error() {
    let doc = parse_document("// trailing note, no newline").unwrap();
    assert_eq!(doc.blocks, vec![]);
}

#[test]
fn line_comment_is_discarded_between_blocks() {
    // A comment line splits adjacent block-level constructs exactly
    // like a blank line already does (e.g. two `-` runs separated by a
    // blank line are two `Block::List`s, not one) -- it doesn't merge
    // into either neighbor, it just produces no block of its own.
    let doc = parse_document("=[ one ]\n// skip this\n=[ two ]\n").unwrap();
    assert_eq!(doc.blocks.len(), 2);
    match (&doc.blocks[0], &doc.blocks[1]) {
        (Block::Section(a), Block::Section(b)) => {
            assert_eq!(a.title, vec![Inline::Text("one".into())]);
            assert_eq!(b.title, vec![Inline::Text("two".into())]);
        }
        other => panic!("expected two sections, got {other:?}"),
    }
}

#[test]
fn indented_line_comment_is_recognized_at_block_level() {
    let doc = parse_document("=[ one ]\n\n  // indented note\n\n=[ two ]\n").unwrap();
    assert_eq!(doc.blocks.len(), 2);
    assert!(matches!(&doc.blocks[0], Block::Section(_)));
    assert!(matches!(&doc.blocks[1], Block::Section(_)));
}

#[test]
fn indented_block_comment_is_recognized_at_block_level() {
    let doc = parse_document("=[ one ]\n\n  /* indented note */\n\n=[ two ]\n").unwrap();
    assert_eq!(doc.blocks.len(), 2);
    assert!(matches!(&doc.blocks[0], Block::Section(_)));
    assert!(matches!(&doc.blocks[1], Block::Section(_)));
}

#[test]
fn line_comment_interrupts_a_paragraph_with_no_blank_line_before_it() {
    // Same as `#`/list markers already do in the lazy-continuation
    // check -- a comment line ends the paragraph even with no blank
    // line separating them, rather than being swallowed as running
    // text.
    let doc = parse_document("text\n// a comment, not more paragraph text\nmore\n").unwrap();
    assert_eq!(doc.blocks.len(), 2);
    match (&doc.blocks[0], &doc.blocks[1]) {
        (Block::Paragraph(a), Block::Paragraph(b)) => {
            assert_eq!(&a.content, &vec![Inline::Text("text".into())]);
            assert_eq!(&b.content, &vec![Inline::Text("more".into())]);
        }
        other => panic!("expected two paragraphs, got {other:?}"),
    }
}

#[test]
fn indented_line_comment_interrupts_a_paragraph_with_no_blank_line_before_it() {
    let doc = parse_document("text\n  // indented, still interrupts\nmore\n").unwrap();
    assert_eq!(doc.blocks.len(), 2);
    match (&doc.blocks[0], &doc.blocks[1]) {
        (Block::Paragraph(a), Block::Paragraph(b)) => {
            assert_eq!(&a.content, &vec![Inline::Text("text".into())]);
            assert_eq!(&b.content, &vec![Inline::Text("more".into())]);
        }
        other => panic!("expected two paragraphs, got {other:?}"),
    }
}

#[test]
fn block_comment_spans_multiple_lines_and_blank_lines() {
    let doc = parse_document(
            "before\n\n/* this whole\nchunk, including\n\na blank line and #[ not a heading ]\nis discarded */\n\nafter\n",
        )
        .unwrap();
    assert_eq!(doc.blocks.len(), 2);
    match (&doc.blocks[0], &doc.blocks[1]) {
        (Block::Paragraph(a), Block::Paragraph(b)) => {
            assert_eq!(&a.content, &vec![Inline::Text("before".into())]);
            assert_eq!(&b.content, &vec![Inline::Text("after".into())]);
        }
        other => panic!("expected two paragraphs, got {other:?}"),
    }
}

#[test]
fn unterminated_block_comment_at_block_level_is_an_error() {
    assert!(parse_document("/* never closed\n").is_err());
}

#[test]
fn inline_block_comment_is_removed_from_paragraph_text() {
    let doc = parse_document("keep /* drop this */ also keep\n").unwrap();
    match &doc.blocks[0] {
        Block::Paragraph(p) => {
            // The comment's source is excluded entirely, and the text
            // flushed before it and after it are merged back into one
            // `Text` (`push_text` in `inline.rs`) -- two adjacent
            // `Text`s with nothing between them would otherwise be
            // indistinguishable from a genuine `SoftBreak`, which
            // deliberately does not rely on bare adjacency to mean
            // anything.
            assert_eq!(&p.content, &vec![Inline::Text("keep  also keep".into())]);
        }
        other => panic!("expected paragraph, got {other:?}"),
    }
}

#[test]
fn inline_block_comment_works_inside_content() {
    let doc = parse_document("@caution[ keep /* drop */ this ]\n").unwrap();
    match &doc.blocks[0] {
        Block::Element(el) => {
            assert_eq!(el.content, Some(vec![Inline::Text("keep  this".into())]));
        }
        other => panic!("expected element, got {other:?}"),
    }
}

#[test]
fn unterminated_inline_block_comment_is_an_error() {
    assert!(parse_document("keep /* never closes\n").is_err());
}

#[test]
fn double_slash_inside_a_url_is_not_treated_as_a_comment() {
    let doc = parse_document("see https://example.com for more\n").unwrap();
    match &doc.blocks[0] {
        Block::Paragraph(p) => {
            assert_eq!(p.content.len(), 3);
            assert_eq!(p.content[0], Inline::Text("see ".into()));
            if let Inline::Element(el) = &p.content[1] {
                assert_eq!(el.sigil, Sigil::named("link"));
                assert_eq!(
                    el.args,
                    Some(Value::Map(vec![(
                        "target".to_string(),
                        Value::String("https://example.com".to_string())
                    )]))
                );
            } else {
                panic!("expected autolink element, got {:?}", p.content[1]);
            }
            assert_eq!(p.content[2], Inline::Text(" for more".into()));
        }
        other => panic!("expected paragraph, got {other:?}"),
    }
}

#[test]
fn inline_double_slash_comment_is_stripped_when_preceded_by_whitespace() {
    let doc = parse_document("aaa // asdasd\n").unwrap();
    match &doc.blocks[0] {
        Block::Paragraph(p) => {
            assert_eq!(&p.content, &vec![Inline::Text("aaa".into())]);
        }
        other => panic!("expected paragraph, got {other:?}"),
    }
}

#[test]
fn inline_double_slash_comment_works_inside_content() {
    let doc = parse_document("@caution[ keep // drop this\n]\n").unwrap();
    match &doc.blocks[0] {
        Block::Element(el) => {
            // The comment's own trailing newline (left in place by
            // `skip_line_comment`, which stops before it) flushes as
            // its own `SoftBreak`, which `trim_edges` then drops as
            // trailing whitespace -- exposing "keep "'s own trailing
            // space as the new true edge, which gets trimmed too. No
            // trailing space survives, unlike before `SoftBreak`
            // existed: back then that lone newline folded straight
            // into a literal `" "` `Text`, and `trim_edges` only ever
            // trimmed `Text`, so it stopped one node short.
            assert_eq!(el.content, Some(vec![Inline::Text("keep".into())]));
        }
        other => panic!("expected element, got {other:?}"),
    }
}

#[test]
fn a_double_slash_line_inside_a_multiline_content_is_a_comment() {
    // Unlike a top-level paragraph, `[content]` content has no block-level
    // dispatch of its own -- a `//` starting a line inside it is only
    // recognized because the preceding newline counts as a boundary,
    // same rule as a same-line trailing comment.
    let doc = parse_document("@caution[\n  keep\n  // drop this line\n  keep2\n]\n").unwrap();
    match &doc.blocks[0] {
        Block::Element(el) => {
            // Two flushes (before/after the elided comment line) each
            // end/start with a break; `push_soft_break` merges them
            // into the one `SoftBreak` a reader actually sees between
            // `keep` and `keep2`, rather than leaving two adjacent ones.
            assert_eq!(
                el.content,
                Some(vec![
                    Inline::Text("keep".into()),
                    sb(),
                    Inline::Text("keep2".into())
                ])
            );
        }
        other => panic!("expected element, got {other:?}"),
    }
}

#[test]
fn own_line_comment_works_inside_a_paren_group() {
    // The original trigger case: a `//` comment on its own line between
    // entries in `@config(...)`'s `(args)` map.
    let doc = parse_document("@config(\n  format:json\n  // a note\n)\n").unwrap();
    match &doc.blocks[0] {
        Block::Element(el) => {
            assert_eq!(el.sigil, Sigil::named("config"));
            assert_eq!(
                el.args,
                Some(Value::Map(vec![(
                    "format".into(),
                    Value::String("json".into())
                )]))
            );
        }
        other => panic!("expected an element, got {other:?}"),
    }
}

#[test]
fn own_line_comment_works_inside_a_lightweight_value_group() {
    let doc = parse_document("@meta{\n  key: value\n  // a note\n}\n").unwrap();
    match &doc.blocks[0] {
        Block::Element(el) => {
            assert_eq!(
                el.value,
                Some(ElementValue::from_map(Value::Map(vec![(
                    "key".into(),
                    Value::String("value".into())
                )])))
            );
        }
        other => panic!("expected an element, got {other:?}"),
    }
}

#[test]
fn own_line_comment_works_inside_a_call() {
    let v = parse_value("list(\n  a,\n  // a note\n  b,\n)").unwrap();
    assert_eq!(
        v,
        Value::Call(
            "list".into(),
            vec![Value::String("a".into()), Value::String("b".into())]
        )
    );
}

#[test]
fn trailing_same_line_comment_after_a_scalar_value_is_stripped() {
    let v = parse_value("key: value // trailing note").unwrap();
    assert_eq!(
        v,
        Value::Map(vec![("key".into(), Value::String("value".into()))])
    );
}

#[test]
fn bare_url_values_are_unaffected_by_trailing_comment_support() {
    // `//` glued directly to preceding text (no whitespace before it)
    // stays literal -- the boundary rule that makes trailing comments
    // safe also protects `https://...`.
    let v = parse_value("url: https://example.com/path").unwrap();
    assert_eq!(
        v,
        Value::Map(vec![(
            "url".into(),
            Value::String("https://example.com/path".into())
        )])
    );
}

#[test]
fn a_bare_scalar_starting_with_double_slash_needs_quoting() {
    // A bare scalar meant to start with a literal `//` (e.g. a
    // protocol-relative URL) is indistinguishable from a comment at
    // that position -- it's read as a (now-empty) value followed by a
    // comment, so it must be quoted instead.
    let quoted = parse_value(r#"path: "//example.com/x""#).unwrap();
    assert_eq!(
        quoted,
        Value::Map(vec![(
            "path".into(),
            Value::String("//example.com/x".into())
        )])
    );
    assert!(parse_value("path: //example.com/x").is_err());
}
