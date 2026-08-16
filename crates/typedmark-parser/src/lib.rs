mod document;
mod embedded_format;
mod error;
mod value;

pub use document::parse_document;
pub use error::{Error, Result};
pub use value::parse_value;

#[cfg(test)]
mod tests {
    use super::*;
    use typedmark_ast::{Block, ElementValue, Inline, Sigil, Value};

    #[test]
    fn parses_flat_map() {
        let v = parse_value("title: value\ntags: [a, b]").unwrap();
        assert_eq!(
            v,
            Value::Map(vec![
                ("title".into(), Value::String("value".into())),
                (
                    "tags".into(),
                    Value::Seq(vec![Value::String("a".into()), Value::String("b".into())])
                ),
            ])
        );
    }

    #[test]
    fn parses_scalars() {
        assert_eq!(parse_value("42").unwrap(), Value::Int(42));
        assert_eq!(parse_value("-3.5").unwrap(), Value::Float(-3.5));
        assert_eq!(parse_value("true").unwrap(), Value::Bool(true));
        assert_eq!(parse_value("null").unwrap(), Value::Null);
        assert_eq!(parse_value("hello").unwrap(), Value::String("hello".into()));
    }

    #[test]
    fn preserves_colon_in_url_values() {
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
    fn parses_heading_with_attrs() {
        let doc = parse_document("#[ Hello ]{ id:header1 }\n").unwrap();
        match &doc.blocks[0] {
            Block::Heading(h) => {
                assert_eq!(h.level, 1);
                assert_eq!(h.content, vec![Inline::Text("Hello".into())]);
                assert_eq!(
                    h.attrs,
                    Some(Value::Map(vec![(
                        "id".into(),
                        Value::String("header1".into())
                    )]))
                );
            }
            other => panic!("expected heading, got {other:?}"),
        }
    }

    #[test]
    fn parses_link_with_order_free_groups() {
        let a = parse_document("@(url:https://example.com)[Wiki]").unwrap();
        let b = parse_document("@[Wiki](url:https://example.com)").unwrap();
        assert_eq!(a, b);
        match &a.blocks[0] {
            Block::Element(el) => {
                assert_eq!(el.sigil, Sigil::At(None));
                assert_eq!(
                    el.args,
                    Some(Value::Map(vec![(
                        "url".into(),
                        Value::String("https://example.com".into())
                    )]))
                );
                assert_eq!(el.content, Some(vec![Inline::Text("Wiki".into())]));
            }
            other => panic!("expected element, got {other:?}"),
        }
    }

    #[test]
    fn parses_links_container_with_bare_children() {
        let doc = parse_document("@links {\n  (1)[ note ]\n  (anotation1)[ note ]\n}\n").unwrap();
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(el.sigil, Sigil::At(Some("links".into())));
                match &el.value {
                    Some(ElementValue::Children(children)) => {
                        assert_eq!(children.len(), 2);
                        assert_eq!(children[0].sigil, Sigil::Bare);
                        assert_eq!(children[0].args, Some(Value::Int(1)));
                        assert_eq!(children[0].content, Some(vec![Inline::Text("note".into())]));
                        assert_eq!(children[1].args, Some(Value::String("anotation1".into())));
                    }
                    other => panic!("expected children, got {other:?}"),
                }
            }
            other => panic!("expected element, got {other:?}"),
        }
    }

    #[test]
    fn parses_caution_as_typed_element_not_bare_bracket() {
        let doc = parse_document("<caution>[ be careful ]\n").unwrap();
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(el.sigil, Sigil::Type("caution".into()));
                assert_eq!(el.content, Some(vec![Inline::Text("be careful".into())]));
            }
            other => panic!("expected element, got {other:?}"),
        }
    }

    #[test]
    fn parses_list() {
        let doc = parse_document("- one\n- two\n").unwrap();
        match &doc.blocks[0] {
            Block::List(list) => {
                assert!(!list.ordered);
                assert_eq!(list.items.len(), 2);
                assert_eq!(list.items[0].content, vec![Inline::Text("one".into())]);
                assert_eq!(list.items[1].content, vec![Inline::Text("two".into())]);
            }
            other => panic!("expected list, got {other:?}"),
        }
    }

    #[test]
    fn parses_list_generic_markers_and_trailing_attrs() {
        let doc = parse_document(
            "- ( ) item {tag: dev}\n- [x] done {id: task1}\n- (T) todo\n- (?) question\n",
        )
        .unwrap();
        match &doc.blocks[0] {
            Block::List(list) => {
                assert!(!list.ordered);
                assert_eq!(list.items.len(), 4);

                assert_eq!(list.items[0].content, vec![Inline::Text("item".into())]);
                assert_eq!(list.items[0].marker, Some(" ".to_string()));
                assert_eq!(
                    list.items[0].attrs,
                    Some(Value::Map(vec![(
                        "tag".into(),
                        Value::String("dev".into())
                    )]))
                );

                assert_eq!(list.items[1].content, vec![Inline::Text("done".into())]);
                assert_eq!(list.items[1].marker, Some("x".to_string()));

                assert_eq!(list.items[2].content, vec![Inline::Text("todo".into())]);
                assert_eq!(list.items[2].marker, Some("T".to_string()));

                assert_eq!(list.items[3].content, vec![Inline::Text("question".into())]);
                assert_eq!(list.items[3].marker, Some("?".to_string()));
            }
            other => panic!("expected list, got {other:?}"),
        }
    }

    #[test]
    fn parses_ordered_list() {
        let doc = parse_document("-. one\n-. two\n").unwrap();
        match &doc.blocks[0] {
            Block::List(list) => {
                assert!(list.ordered);
                assert_eq!(list.items.len(), 2);
                assert_eq!(list.items[0].content, vec![Inline::Text("one".into())]);
                assert_eq!(list.items[1].content, vec![Inline::Text("two".into())]);
            }
            other => panic!("expected list, got {other:?}"),
        }
    }

    #[test]
    fn mixed_list_markers_split_into_separate_lists() {
        let doc = parse_document("- one\n-. two\n").unwrap();
        assert_eq!(doc.blocks.len(), 2);
        match (&doc.blocks[0], &doc.blocks[1]) {
            (Block::List(l1), Block::List(l2)) if !l1.ordered && l2.ordered => {}
            other => panic!("expected two separate lists, got {other:?}"),
        }
    }

    #[test]
    fn parses_thematic_break() {
        let doc = parse_document("---\n").unwrap();
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(el.sigil, Sigil::Type("hr".into()));
                assert_eq!(el.content, None);
            }
            other => panic!("expected hr element, got {other:?}"),
        }
    }

    #[test]
    fn parses_thematic_break_with_more_than_three_dashes() {
        let doc = parse_document("-----\n").unwrap();
        match &doc.blocks[0] {
            Block::Element(el) => assert_eq!(el.sigil, Sigil::Type("hr".into())),
            other => panic!("expected hr element, got {other:?}"),
        }
    }

    #[test]
    fn parses_titled_thematic_break() {
        let doc = parse_document("---[ Title ]---\n").unwrap();
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(el.sigil, Sigil::Type("hr".into()));
                assert_eq!(el.content, Some(vec![Inline::Text("Title".into())]));
            }
            other => panic!("expected hr element, got {other:?}"),
        }
    }

    #[test]
    fn titled_thematic_break_dash_runs_need_not_match_in_length() {
        let doc = parse_document("-----[ Title ]---\n").unwrap();
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(el.content, Some(vec![Inline::Text("Title".into())]));
            }
            other => panic!("expected hr element, got {other:?}"),
        }
    }

    #[test]
    fn titled_thematic_break_needs_three_or_more_closing_dashes() {
        assert!(parse_document("---[ Title ]--\n").is_err());
    }

    #[test]
    fn titled_thematic_break_rejects_trailing_junk() {
        assert!(parse_document("---[ Title ]--- stray text\n").is_err());
    }

    #[test]
    fn a_dash_run_not_followed_by_a_bracket_is_plain_text() {
        // `---<embed>---` doesn't commit to the titled form (no `[` right
        // after the dashes) and isn't a plain break either (trailing
        // content isn't just whitespace) -- it's ordinary paragraph text.
        let doc = parse_document("---<embed>---\n").unwrap();
        match &doc.blocks[0] {
            Block::Paragraph(p) => {
                assert_eq!(&p.content, &vec![Inline::Text("---<embed>---".into())]);
            }
            other => panic!("expected paragraph, got {other:?}"),
        }
    }

    #[test]
    fn parses_emphasis_and_strong_and_mark() {
        let doc = parse_document("a *em* b **strong** c _em2_ d __strong2__ e ==mark==\n").unwrap();
        match &doc.blocks[0] {
            Block::Paragraph(p) => {
                let kinds: Vec<_> = p
                    .content
                    .iter()
                    .filter_map(|i| match i {
                        Inline::Element(el) => Some((el.sigil.clone(), el.content.clone())),
                        _ => None,
                    })
                    .collect();
                assert_eq!(
                    kinds,
                    vec![
                        (
                            Sigil::Type("em".into()),
                            Some(vec![Inline::Text("em".into())])
                        ),
                        (
                            Sigil::Type("strong".into()),
                            Some(vec![Inline::Text("strong".into())])
                        ),
                        (
                            Sigil::Type("em".into()),
                            Some(vec![Inline::Text("em2".into())])
                        ),
                        (
                            Sigil::Type("strong".into()),
                            Some(vec![Inline::Text("strong2".into())])
                        ),
                        (
                            Sigil::Type("mark".into()),
                            Some(vec![Inline::Text("mark".into())])
                        ),
                    ]
                );
            }
            other => panic!("expected paragraph, got {other:?}"),
        }
    }

    #[test]
    fn underscore_does_not_trigger_inside_a_word() {
        let doc = parse_document("foo_bar_baz\n").unwrap();
        match &doc.blocks[0] {
            Block::Paragraph(p) => {
                assert_eq!(&p.content, &vec![Inline::Text("foo_bar_baz".into())]);
            }
            other => panic!("expected paragraph, got {other:?}"),
        }
    }

    #[test]
    fn unmatched_delimiter_falls_back_to_literal_text() {
        let doc = parse_document("this *word never closes\n").unwrap();
        match &doc.blocks[0] {
            Block::Paragraph(p) => {
                assert_eq!(
                    &p.content,
                    &vec![Inline::Text("this *word never closes".into())]
                );
            }
            other => panic!("expected paragraph, got {other:?}"),
        }
    }

    #[test]
    fn parses_the_repo_spec_examples() {
        parse_document(include_str!("../../../docs/readme.ja.tm")).unwrap();
        parse_document(include_str!("../../../docs/tmt/examples/image_meta.tm")).unwrap();
    }

    #[test]
    fn bare_at_is_plain_text_when_not_an_element() {
        let doc = parse_document("contact me@example.com please\n").unwrap();
        match &doc.blocks[0] {
            Block::Paragraph(p) => {
                assert_eq!(
                    &p.content,
                    &vec![Inline::Text("contact me@example.com please".into())]
                );
            }
            other => panic!("expected paragraph, got {other:?}"),
        }
    }

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
        let doc = parse_document("#[ one ]\n// skip this\n#[ two ]\n").unwrap();
        assert_eq!(doc.blocks.len(), 2);
        match (&doc.blocks[0], &doc.blocks[1]) {
            (Block::Heading(a), Block::Heading(b)) => {
                assert_eq!(a.content, vec![Inline::Text("one".into())]);
                assert_eq!(b.content, vec![Inline::Text("two".into())]);
            }
            other => panic!("expected two headings, got {other:?}"),
        }
    }

    #[test]
    fn indented_line_comment_is_recognized_at_block_level() {
        let doc = parse_document("#[ one ]\n\n  // indented note\n\n#[ two ]\n").unwrap();
        assert_eq!(doc.blocks.len(), 2);
        assert!(matches!(&doc.blocks[0], Block::Heading(_)));
        assert!(matches!(&doc.blocks[1], Block::Heading(_)));
    }

    #[test]
    fn indented_block_comment_is_recognized_at_block_level() {
        let doc = parse_document("#[ one ]\n\n  /* indented note */\n\n#[ two ]\n").unwrap();
        assert_eq!(doc.blocks.len(), 2);
        assert!(matches!(&doc.blocks[0], Block::Heading(_)));
        assert!(matches!(&doc.blocks[1], Block::Heading(_)));
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
    fn an_element_trigger_ends_a_paragraph_with_no_blank_line_before_it() {
        // Same as `#`/list markers/comments already do in the lazy-continuation
        // check -- a `<T>`/`@name` element trigger on the next line ends the
        // paragraph even with no blank line separating them, rather than
        // being folded into one `Block::Paragraph` together with the
        // previous line. (An element trigger followed by *more* running
        // text on its own next line is a separate case, unaffected here --
        // that's still ordinary lazy continuation, since only a following
        // element trigger ends a paragraph early, not a preceding one.)
        let doc = parse_document("text\n@meta{key:value}\n").unwrap();
        assert_eq!(doc.blocks.len(), 2);
        match (&doc.blocks[0], &doc.blocks[1]) {
            (Block::Paragraph(a), Block::Element(el)) => {
                assert_eq!(&a.content, &vec![Inline::Text("text".into())]);
                assert_eq!(el.sigil, Sigil::At(Some("meta".into())));
            }
            other => panic!("expected paragraph then element, got {other:?}"),
        }
    }

    #[test]
    fn adjacent_elements_with_no_blank_line_become_separate_blocks() {
        // Regression: `@meta{...}` immediately followed by `@settings{...}`
        // (no blank line between them, as `docs/docs.settings.tm` itself is
        // written) used to lazily continue into one `Block::Paragraph` --
        // it must now become two independent `Block::Element`s instead.
        let doc = parse_document("@meta{ type:@settings }\n@settings{ key:value }\n").unwrap();
        assert_eq!(doc.blocks.len(), 2);
        match (&doc.blocks[0], &doc.blocks[1]) {
            (Block::Element(a), Block::Element(b)) => {
                assert_eq!(a.sigil, Sigil::At(Some("meta".into())));
                assert_eq!(b.sigil, Sigil::At(Some("settings".into())));
            }
            other => panic!("expected two elements, got {other:?}"),
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
                // The comment's source is excluded entirely, but the text
                // flushed before it and after it stay as separate `Inline`
                // chunks (flushing doesn't merge adjacent text runs).
                assert_eq!(
                    &p.content,
                    &vec![
                        Inline::Text("keep ".into()),
                        Inline::Text(" also keep".into())
                    ]
                );
            }
            other => panic!("expected paragraph, got {other:?}"),
        }
    }

    #[test]
    fn inline_block_comment_works_inside_content() {
        let doc = parse_document("<caution>[ keep /* drop */ this ]\n").unwrap();
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(
                    el.content,
                    Some(vec![
                        Inline::Text("keep ".into()),
                        Inline::Text(" this".into())
                    ])
                );
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
                assert_eq!(
                    &p.content,
                    &vec![Inline::Text("see https://example.com for more".into())]
                );
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
        let doc = parse_document("<caution>[ keep // drop this\n]\n").unwrap();
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(el.content, Some(vec![Inline::Text("keep ".into())]));
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
        let doc = parse_document("<caution>[\n  keep\n  // drop this line\n  keep2\n]\n").unwrap();
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(
                    el.content,
                    Some(vec![
                        Inline::Text("keep ".into()),
                        Inline::Text(" keep2".into())
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
                assert_eq!(el.sigil, Sigil::At(Some("config".into())));
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
                    Some(ElementValue::Data(Value::Map(vec![(
                        "key".into(),
                        Value::String("value".into())
                    )])))
                );
            }
            other => panic!("expected an element, got {other:?}"),
        }
    }

    #[test]
    fn own_line_comment_works_inside_a_sequence() {
        let v = parse_value("[\n  a,\n  // a note\n  b,\n]").unwrap();
        assert_eq!(
            v,
            Value::Seq(vec![Value::String("a".into()), Value::String("b".into())])
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

    #[test]
    fn a_bare_bracket_pair_inside_a_content_no_longer_truncates_it() {
        // Regression for the memo-content-fidelity fix: `Stop::Bracket`
        // used to break at the *first* literal `]`, corrupting the rest
        // of the content as stray trailing text. Applies to every ordinary
        // (non-raw) `[content]`, not just an opt-in one.
        let doc =
            parse_document("<caution>[\nline one\nline two with * and [brackets] inside\n]\n")
                .unwrap();
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(
                    el.content,
                    Some(vec![Inline::Text(
                        "line one line two with * and [brackets] inside".into()
                    )])
                );
            }
            other => panic!("expected an element, got {other:?}"),
        }
    }

    #[test]
    fn an_unbalanced_bracket_inside_a_content_still_errors() {
        assert!(parse_document("<caution>[ has an [ that never closes\n]\n").is_err());
    }

    #[test]
    fn content_raw_preserves_brackets_and_newlines_losslessly() {
        let doc = parse_document(
            "<memo>(content:raw)[\nline one\nline two with * and [brackets] inside\n]\n",
        )
        .unwrap();
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(
                    el.content,
                    Some(vec![Inline::Text(
                        "\nline one\nline two with * and [brackets] inside\n".into()
                    )])
                );
            }
            other => panic!("expected an element, got {other:?}"),
        }
    }

    #[test]
    fn content_raw_is_not_confused_by_an_apostrophe() {
        // The reason `content:raw` can't reuse codeblock's quote-aware
        // matcher as-is: free-form prose has no guarantee its `'`/`"`
        // occurrences are balanced the way real source code's are.
        let doc = parse_document("<memo>(content:raw)[don't forget [this]]\n").unwrap();
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(
                    el.content,
                    Some(vec![Inline::Text("don't forget [this]".into())])
                );
            }
            other => panic!("expected an element, got {other:?}"),
        }
    }

    #[test]
    fn an_unrecognized_content_value_falls_back_to_ordinary_prose() {
        // Mirrors `format`'s unknown-value fallback: `content:raw` is the
        // only recognized value, anything else (or no `content` key at all)
        // parses as normal prose, so line breaks still collapse per the
        // usual lazy-continuation rule.
        let doc = parse_document("<memo>(content:literal)[\nline one\nline two\n]\n").unwrap();
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(
                    el.content,
                    Some(vec![Inline::Text("line one line two".into())])
                );
            }
            other => panic!("expected an element, got {other:?}"),
        }
    }

    #[test]
    fn parses_a_fenced_code_block_with_lang() {
        let doc = parse_document("```rust\nfn main() {}\n```\n").unwrap();
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(el.sigil, Sigil::Type("codeblock".into()));
                assert_eq!(
                    el.args,
                    Some(Value::Map(vec![(
                        "lang".to_string(),
                        Value::String("rust".to_string())
                    )]))
                );
                assert_eq!(
                    el.content,
                    Some(vec![Inline::Text("fn main() {}".into())])
                );
            }
            other => panic!("expected a codeblock element, got {other:?}"),
        }
    }

    #[test]
    fn parses_a_fenced_code_block_without_lang() {
        let doc = parse_document("```\nplain\n```\n").unwrap();
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(el.sigil, Sigil::Type("codeblock".into()));
                assert_eq!(el.args, None);
                assert_eq!(el.content, Some(vec![Inline::Text("plain".into())]));
            }
            other => panic!("expected a codeblock element, got {other:?}"),
        }
    }

    #[test]
    fn fenced_code_block_and_bracket_codeblock_produce_the_same_ast() {
        let fenced = parse_document("```rust\nfn main() {}\n```\n").unwrap();
        let bracket = parse_document("<codeblock>(lang:rust)[fn main() {}]\n").unwrap();
        match (&fenced.blocks[0], &bracket.blocks[0]) {
            (Block::Element(a), Block::Element(b)) => {
                assert_eq!(a.sigil, b.sigil);
                assert_eq!(a.args, b.args);
                assert_eq!(a.content, b.content);
                assert_eq!(a.value, b.value);
            }
            other => panic!("expected two codeblock elements, got {other:?}"),
        }
    }

    #[test]
    fn fenced_code_block_closing_fence_needs_at_least_the_opening_backtick_count() {
        // A closing fence with fewer backticks than the opening one doesn't
        // close it -- it's just consumed as ordinary body content, same as
        // CommonMark.
        let doc = parse_document("````\ncode\n```\nmore code\n````\n").unwrap();
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(
                    el.content,
                    Some(vec![Inline::Text("code\n```\nmore code".into())])
                );
            }
            other => panic!("expected a codeblock element, got {other:?}"),
        }
    }

    #[test]
    fn fenced_code_block_closing_fence_can_have_more_backticks_than_opening() {
        let doc = parse_document("```\ncode\n`````\n").unwrap();
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(el.content, Some(vec![Inline::Text("code".into())]));
            }
            other => panic!("expected a codeblock element, got {other:?}"),
        }
    }

    #[test]
    fn fenced_code_block_body_can_contain_short_backtick_runs() {
        let doc = parse_document("```\nsee `foo` and ``bar``\n```\n").unwrap();
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(
                    el.content,
                    Some(vec![Inline::Text("see `foo` and ``bar``".into())])
                );
            }
            other => panic!("expected a codeblock element, got {other:?}"),
        }
    }

    #[test]
    fn unterminated_fenced_code_block_runs_to_eof_without_error() {
        // Deliberately asymmetric with `<codeblock>[...]`'s hard EOF error
        // (see `parse_fenced_code_block`'s doc comment) -- matches
        // CommonMark's own spec for an unclosed fence.
        let doc = parse_document("```\nline one\nline two").unwrap();
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(
                    el.content,
                    Some(vec![Inline::Text("line one\nline two".into())])
                );
            }
            other => panic!("expected a codeblock element, got {other:?}"),
        }
    }

    #[test]
    fn unterminated_inline_backtick_falls_back_to_literal_text() {
        // Regression: a single unterminated '`' used to swallow everything
        // up to the next stray backtick anywhere later in the source,
        // across paragraph boundaries, collapsing newlines to spaces along
        // the way. It must now fall back to a literal '`' and let the
        // paragraph end normally at the blank line.
        let doc = parse_document("keep `this open\n\nnext paragraph\n").unwrap();
        assert_eq!(doc.blocks.len(), 2);
        match (&doc.blocks[0], &doc.blocks[1]) {
            (Block::Paragraph(a), Block::Paragraph(b)) => {
                assert_eq!(&a.content, &vec![Inline::Text("keep `this open".into())]);
                assert_eq!(&b.content, &vec![Inline::Text("next paragraph".into())]);
            }
            other => panic!("expected two paragraphs, got {other:?}"),
        }
    }

    #[test]
    fn backtick_span_still_closes_normally_on_the_same_line() {
        let doc = parse_document("call `foo()` now\n").unwrap();
        match &doc.blocks[0] {
            Block::Paragraph(p) => {
                assert_eq!(
                    &p.content,
                    &vec![Inline::Text("call `foo()` now".into())]
                );
            }
            other => panic!("expected a paragraph, got {other:?}"),
        }
    }
}
