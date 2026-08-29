mod codeblock;
mod document;
mod element;
mod embedded_format;
mod error;
mod heading;
mod inline;
mod interp;
mod list;
mod value;

pub use document::parse_document;
pub use error::{Error, Result};
pub use value::parse_value;

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_ast::{
        Block, Element, ElementValue, Inline, InterpExpr, InterpExprKind, Literal, Sigil, Value,
    };
    use tomet_semantics::{ElementKind, classify, heading_level, list_items, list_ordered};

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
            Block::Element(el) => {
                assert_eq!(classify(el), ElementKind::Heading);
                assert_eq!(heading_level(el), Some(1));
                assert_eq!(el.content, Some(vec![Inline::Text("Hello".into())]));
                assert_eq!(
                    el.value,
                    Some(ElementValue::Data(Value::Map(vec![(
                        "id".into(),
                        Value::String("header1".into())
                    )])))
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

    /// A list item's trailing `{value}` attrs, if any -- mirrors how
    /// `Element::list_item` stores them under `value` as `ElementValue::Data`.
    fn item_attrs(item: &Element) -> Option<Value> {
        match &item.value {
            Some(ElementValue::Data(v)) => Some(v.clone()),
            _ => None,
        }
    }

    #[test]
    fn parses_list() {
        let doc = parse_document("- one\n- two\n").unwrap();
        match &doc.blocks[0] {
            Block::Element(list) => {
                assert_eq!(list_ordered(list), Some(false));
                let items = list_items(list);
                assert_eq!(items.len(), 2);
                assert_eq!(items[0].content, Some(vec![Inline::Text("one".into())]));
                assert_eq!(items[1].content, Some(vec![Inline::Text("two".into())]));
            }
            other => panic!("expected list, got {other:?}"),
        }
    }

    #[test]
    fn parses_list_value_markers_and_trailing_attrs() {
        let doc =
            parse_document("- (T) todo {tag: dev}\n- (\"?\") question {id: task1}\n").unwrap();
        match &doc.blocks[0] {
            Block::Element(list) => {
                assert_eq!(list_ordered(list), Some(false));
                let items = list_items(list);
                assert_eq!(items.len(), 2);

                assert_eq!(items[0].content, Some(vec![Inline::Text("todo".into())]));
                assert_eq!(items[0].args, Some(Value::String("T".into())));
                assert_eq!(
                    item_attrs(&items[0]),
                    Some(Value::Map(vec![(
                        "tag".into(),
                        Value::String("dev".into())
                    )]))
                );

                assert_eq!(
                    items[1].content,
                    Some(vec![Inline::Text("question".into())])
                );
                assert_eq!(items[1].args, Some(Value::String("?".into())));
                assert_eq!(
                    item_attrs(&items[1]),
                    Some(Value::Map(vec![(
                        "id".into(),
                        Value::String("task1".into())
                    )]))
                );
            }
            other => panic!("expected list, got {other:?}"),
        }
    }

    #[test]
    fn list_item_ending_in_an_element_does_not_error_on_a_trailing_brace() {
        // `element.rs::parse_element`'s main loop always lets an element
        // claim an adjacent *bare*, colon-less `{value}` group as its
        // own -- just whitespace/one blank line, no colon needed --
        // *before* the list item's own trailing-attrs guess
        // (`list.rs::peek_trailing_attrs`) ever gets a say. Previously
        // (before `allow_colon_connect` existed) this shape failed
        // outright with "expected '{'" regardless of whether a colon was
        // present: the attrs guess found the line's one `{`, assumed it
        // belonged to the item, told inline parsing to stop right before
        // it, but the element inside kept going and swallowed straight
        // past that point anyway, leaving nothing at the assumed
        // position to re-parse as the item's own attrs.
        //
        // Now the two shapes mean different things, on purpose (see
        // `element::parse_element`'s `allow_colon_connect` doc comment,
        // and `docs/ja/specifications/syntax.tmt`'s `##[ コネクト ]`,
        // which had flagged exactly this as an unimplemented idea): a
        // *bare* `{...}` still always belongs to the element (matches
        // how the same input already behaves with no list item involved
        // at all), but a *colon-prefixed* `:{...}` is reserved for the
        // item itself, specifically so `id:breakfast` here can be the
        // item's own metadata rather than forced onto `@link`.
        let bare = "- @link(ref:x) {id:breakfast}\n";
        let doc = parse_document(bare).unwrap_or_else(|e| panic!("{bare:?} failed: {e}"));
        match &doc.blocks[0] {
            Block::Element(list) => {
                let items = list_items(list);
                assert_eq!(items.len(), 1, "{bare:?}");
                assert_eq!(item_attrs(&items[0]), None, "{bare:?}");
                match items[0].content.as_deref() {
                    Some([Inline::Element(link)]) => {
                        assert_eq!(link.sigil, Sigil::At(Some("link".into())), "{bare:?}");
                        assert_eq!(
                            link.value,
                            Some(ElementValue::Data(Value::Map(vec![(
                                "id".into(),
                                Value::String("breakfast".into())
                            )]))),
                            "{bare:?}"
                        );
                    }
                    other => panic!("{bare:?}: expected a single @link element, got {other:?}"),
                }
            }
            other => panic!("{bare:?}: expected list, got {other:?}"),
        }

        for src in [
            "- @link(ref:x) :{id:breakfast}\n",
            "- @link(ref:@other) :{id:breakfast}\n",
        ] {
            let doc = parse_document(src).unwrap_or_else(|e| panic!("{src:?} failed: {e}"));
            match &doc.blocks[0] {
                Block::Element(list) => {
                    let items = list_items(list);
                    assert_eq!(items.len(), 1, "{src:?}");
                    assert_eq!(
                        item_attrs(&items[0]),
                        Some(Value::Map(vec![(
                            "id".into(),
                            Value::String("breakfast".into())
                        )])),
                        "{src:?}"
                    );
                    // The connector (` :`) left no trace in the item's
                    // own content -- just the `@link` element, nothing
                    // else.
                    match items[0].content.as_deref() {
                        Some([Inline::Element(link)]) => {
                            assert_eq!(link.sigil, Sigil::At(Some("link".into())), "{src:?}");
                            assert_eq!(link.value, None, "{src:?}");
                        }
                        other => panic!("{src:?}: expected a single @link element, got {other:?}"),
                    }
                }
                other => panic!("{src:?}: expected list, got {other:?}"),
            }
        }
    }

    #[test]
    fn list_item_colon_connect_supports_an_empty_braced_value() {
        // The `docs/ja/specifications/syntax.tmt` `##[ コネクト ]` example
        // this feature implements (`- () xxxxxx :{}`) uses an *empty*
        // `{}` for its item-level attrs -- `heading::parse_braced_value`
        // (shared by heading and list-item attrs) used to have no
        // special case for that, unlike `parse_paren_value`/
        // `element.rs::parse_value_group` right next to it, and errored
        // ("expected a value") instead of producing an empty map,
        // silently falling through to treating the whole line as
        // ordinary text with no attrs at all.
        let doc = parse_document("- () xxxxxx :{}\n").unwrap();
        match &doc.blocks[0] {
            Block::Element(list) => {
                let items = list_items(list);
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].content, Some(vec![Inline::Text("xxxxxx".into())]));
                assert_eq!(item_attrs(&items[0]), Some(Value::Map(vec![])));
            }
            other => panic!("expected list, got {other:?}"),
        }
    }

    #[test]
    fn list_marker_supports_explicit_key_value() {
        let doc = parse_document("- (color: red, priority: high) content\n").unwrap();
        match &doc.blocks[0] {
            Block::Element(list) => {
                assert_eq!(
                    list_items(list)[0].args,
                    Some(Value::Map(vec![
                        ("color".into(), Value::String("red".into())),
                        ("priority".into(), Value::String("high".into())),
                    ]))
                );
            }
            other => panic!("expected list, got {other:?}"),
        }
    }

    #[test]
    fn list_marker_quoted_scalar_avoids_colon_misparse() {
        let doc = parse_document("- (\"12:01\") woke up\n").unwrap();
        match &doc.blocks[0] {
            Block::Element(list) => {
                assert_eq!(
                    list_items(list)[0].args,
                    Some(Value::String("12:01".into()))
                );
            }
            other => panic!("expected list, got {other:?}"),
        }
    }

    #[test]
    fn bracket_content_is_never_a_marker() {
        // `[...]` after a list marker has no special meaning at all (the
        // checkbox form is gone) -- it's just literal text.
        let doc = parse_document("- [T] content\n").unwrap();
        match &doc.blocks[0] {
            Block::Element(list) => {
                let items = list_items(list);
                assert_eq!(items[0].args, None);
                assert_eq!(
                    items[0].content,
                    Some(vec![Inline::Text("[T] content".into())])
                );
            }
            other => panic!("expected list, got {other:?}"),
        }
    }

    #[test]
    fn parses_ordered_list() {
        let doc = parse_document("-. one\n-. two\n").unwrap();
        match &doc.blocks[0] {
            Block::Element(list) => {
                assert_eq!(list_ordered(list), Some(true));
                let items = list_items(list);
                assert_eq!(items.len(), 2);
                assert_eq!(items[0].content, Some(vec![Inline::Text("one".into())]));
                assert_eq!(items[1].content, Some(vec![Inline::Text("two".into())]));
            }
            other => panic!("expected list, got {other:?}"),
        }
    }

    #[test]
    fn mixed_list_markers_split_into_separate_lists() {
        let doc = parse_document("- one\n-. two\n").unwrap();
        assert_eq!(doc.blocks.len(), 2);
        match (&doc.blocks[0], &doc.blocks[1]) {
            (Block::Element(l1), Block::Element(l2))
                if list_ordered(l1) == Some(false) && list_ordered(l2) == Some(true) => {}
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
        parse_document(include_str!("../../../docs/tests/readme.ja.tmt")).unwrap();
        parse_document(include_str!(
            "../../../docs/tests/tmt/examples/image.meta.tmt"
        ))
        .unwrap();
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

    /// `${...}` parses to a real `Element` (`Sigil::Dollar`, `value:
    /// Some(ElementValue::Interp(expr))`) -- same shape as `@name{value}`,
    /// not a bespoke `Inline` variant. A standalone `${x}` line therefore
    /// collapses to `Block::Element` exactly like a standalone
    /// `@meta{...}` does (`document.rs::parse_paragraph`'s one-element
    /// collapse), which is why these tests destructure `Block::Element`
    /// rather than `Block::Paragraph`.
    fn interp_expr(block: &Block) -> &InterpExpr {
        match block {
            Block::Element(el) => {
                assert_eq!(el.sigil, Sigil::Dollar);
                match &el.value {
                    Some(ElementValue::Interp(expr)) => expr,
                    other => panic!("expected ElementValue::Interp, got {other:?}"),
                }
            }
            other => panic!(
                "expected a standalone ${{...}} to collapse to Block::Element, got {other:?}"
            ),
        }
    }

    /// Renders an `InterpExpr` to a canonical, span-free string (e.g.
    /// `Call(Identifier(sum), [Identifier(a)])`) so tests can assert
    /// against a plain string instead of hand-building an expected tree
    /// with matching spans (which `derive(PartialEq)` would otherwise
    /// require field-for-field, spans included).
    fn describe(expr: &InterpExpr) -> String {
        match &expr.kind {
            InterpExprKind::Identifier(name) => format!("Identifier({name})"),
            InterpExprKind::Literal(Literal::Int(i)) => format!("Int({i})"),
            InterpExprKind::Literal(Literal::Float(f)) => format!("Float({f})"),
            InterpExprKind::Literal(Literal::String(s)) => format!("String({s:?})"),
            InterpExprKind::Call { callee, args } => {
                let args: Vec<_> = args.iter().map(describe).collect();
                format!("Call({}, [{}])", describe(callee), args.join(", "))
            }
            InterpExprKind::Member { object, member } => {
                format!("Member({}, {member})", describe(object))
            }
        }
    }

    #[test]
    fn parses_bare_identifier_interpolation() {
        let doc = parse_document("${id}\n").unwrap();
        assert_eq!(describe(interp_expr(&doc.blocks[0])), "Identifier(id)");
    }

    #[test]
    fn parses_dotted_member_interpolation() {
        let doc = parse_document("${a.b.c}\n").unwrap();
        assert_eq!(
            describe(interp_expr(&doc.blocks[0])),
            "Member(Member(Identifier(a), b), c)"
        );
    }

    #[test]
    fn parses_call_interpolation_with_nested_call() {
        let doc = parse_document("${sum(a, mul(b, c))}\n").unwrap();
        assert_eq!(
            describe(interp_expr(&doc.blocks[0])),
            "Call(Identifier(sum), [Identifier(a), Call(Identifier(mul), [Identifier(b), Identifier(c)])])"
        );
    }

    #[test]
    fn parses_member_access_on_a_calls_result() {
        // The whole point of `Member`/`Call` both wrapping `Box<InterpExpr>`
        // instead of a flat `Path` + a bare-`String` callee: `b(x).id`
        // (access a member of a call's return value) needs a `Call` node
        // nested *inside* a `Member`'s `object`, which a flat `Path`
        // couldn't represent at all.
        let doc = parse_document("${b(x).id}\n").unwrap();
        assert_eq!(
            describe(interp_expr(&doc.blocks[0])),
            "Member(Call(Identifier(b), [Identifier(x)]), id)"
        );
    }

    #[test]
    fn parses_call_on_a_members_result() {
        // The other direction: `a.b(x)` -- call member `b` of `a`.
        let doc = parse_document("${a.b(x)}\n").unwrap();
        assert_eq!(
            describe(interp_expr(&doc.blocks[0])),
            "Call(Member(Identifier(a), b), [Identifier(x)])"
        );
    }

    #[test]
    fn parses_literals_in_interpolation() {
        let cases = [
            ("${1}", "Int(1)"),
            ("${1.5}", "Float(1.5)"),
            ("${-3}", "Int(-3)"),
            ("${\"hi\"}", "String(\"hi\")"),
        ];
        for (src, expected) in cases {
            let doc = parse_document(&format!("{src}\n")).unwrap();
            assert_eq!(
                describe(interp_expr(&doc.blocks[0])),
                expected,
                "source: {src}"
            );
        }
    }

    #[test]
    fn interpolation_allows_inline_whitespace() {
        let doc = parse_document("${ sum(a, b) }\n").unwrap();
        assert_eq!(
            describe(interp_expr(&doc.blocks[0])),
            "Call(Identifier(sum), [Identifier(a), Identifier(b)])"
        );

        let doc = parse_document("${ a . b }\n").unwrap();
        assert_eq!(
            describe(interp_expr(&doc.blocks[0])),
            "Member(Identifier(a), b)"
        );
    }

    #[test]
    fn unclosed_interpolation_is_an_error() {
        assert!(parse_document("${id\n").is_err());
    }

    #[test]
    fn empty_interpolation_is_an_error() {
        assert!(parse_document("${}\n").is_err());
    }

    #[test]
    fn interpolation_with_bad_leading_char_is_an_error() {
        assert!(parse_document("${+x}\n").is_err());
    }

    #[test]
    fn dollar_not_followed_by_brace_is_plain_text() {
        let doc = parse_document("costs $5 or $ {x} today\n").unwrap();
        match &doc.blocks[0] {
            Block::Paragraph(p) => {
                assert_eq!(
                    &p.content,
                    &vec![Inline::Text("costs $5 or $ {x} today".into())]
                );
            }
            other => panic!("expected paragraph, got {other:?}"),
        }
    }

    #[test]
    fn interp_trigger_on_next_line_does_not_end_paragraph() {
        // Unlike `<T>`/`@name`, `${...}` is deliberately NOT added to
        // `Stop::Paragraph`'s block-trigger disjunction (see
        // `document.rs::parse_inline_seq`): a `${...}` embedded mid-prose
        // (with text before/after it on the same line) stays inline
        // content in the running paragraph. This case has trailing text
        // after `${id}`, so it doesn't hit the single-element collapse
        // `interp_expr` above relies on -- content.len() > 1.
        let doc = parse_document("first line\n${id} second line\n").unwrap();
        match &doc.blocks[0] {
            Block::Paragraph(p) => {
                assert_eq!(p.content.len(), 3);
                assert!(matches!(p.content[0], Inline::Text(_)));
                assert!(matches!(p.content[1], Inline::Element(_)));
                assert!(matches!(p.content[2], Inline::Text(_)));
            }
            other => panic!("expected a single paragraph, got {other:?}"),
        }
    }

    #[test]
    fn interpolation_inside_element_content_and_heading() {
        let doc = parse_document("<memo>[ total: ${sum(a, b)} ]\n\n#[ ${x} ]\n").unwrap();
        match &doc.blocks[0] {
            Block::Element(el) => {
                let content = el.content.as_ref().expect("content");
                assert!(content.iter().any(|i| matches!(
                    i,
                    Inline::Element(e) if e.sigil == Sigil::Dollar
                )));
            }
            other => panic!("expected element, got {other:?}"),
        }
        match &doc.blocks[1] {
            Block::Element(el) => {
                assert_eq!(classify(el), ElementKind::Heading);
                let content = el.content.as_ref().expect("content");
                assert!(content.iter().any(|i| matches!(
                    i,
                    Inline::Element(e) if e.sigil == Sigil::Dollar
                )));
            }
            other => panic!("expected heading, got {other:?}"),
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
            (Block::Element(a), Block::Element(b))
                if classify(a) == ElementKind::Heading && classify(b) == ElementKind::Heading =>
            {
                assert_eq!(a.content, Some(vec![Inline::Text("one".into())]));
                assert_eq!(b.content, Some(vec![Inline::Text("two".into())]));
            }
            other => panic!("expected two headings, got {other:?}"),
        }
    }

    #[test]
    fn indented_line_comment_is_recognized_at_block_level() {
        let doc = parse_document("#[ one ]\n\n  // indented note\n\n#[ two ]\n").unwrap();
        assert_eq!(doc.blocks.len(), 2);
        assert!(
            matches!(&doc.blocks[0], Block::Element(el) if classify(el) == ElementKind::Heading)
        );
        assert!(
            matches!(&doc.blocks[1], Block::Element(el) if classify(el) == ElementKind::Heading)
        );
    }

    #[test]
    fn indented_block_comment_is_recognized_at_block_level() {
        let doc = parse_document("#[ one ]\n\n  /* indented note */\n\n#[ two ]\n").unwrap();
        assert_eq!(doc.blocks.len(), 2);
        assert!(
            matches!(&doc.blocks[0], Block::Element(el) if classify(el) == ElementKind::Heading)
        );
        assert!(
            matches!(&doc.blocks[1], Block::Element(el) if classify(el) == ElementKind::Heading)
        );
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
        // (no blank line between them, as `docs/docs.settings.tmt` itself is
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
                assert_eq!(p.content.len(), 3);
                assert_eq!(p.content[0], Inline::Text("see ".into()));
                if let Inline::Element(el) = &p.content[1] {
                    assert_eq!(el.sigil, Sigil::At(None));
                    assert_eq!(
                        el.args,
                        Some(Value::Map(vec![(
                            "url".to_string(),
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
                assert_eq!(el.content, Some(vec![Inline::Text("fn main() {}".into())]));
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
                assert_eq!(&p.content, &vec![Inline::Text("call `foo()` now".into())]);
            }
            other => panic!("expected a paragraph, got {other:?}"),
        }
    }

    #[test]
    fn empty_paren_and_brace_groups_are_a_deliberately_empty_map() {
        // `()`/`{}` written out (as opposed to the group being omitted
        // entirely, which leaves `args`/`value` as `None`) is a valid,
        // deliberately-empty map -- matches the spec's own `@meta{}` etc.
        let doc = parse_document("<T>()\n").unwrap();
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(el.args, Some(Value::Map(vec![])));
                assert_eq!(el.content, None);
            }
            other => panic!("expected an element, got {other:?}"),
        }

        let doc = parse_document("@meta{}\n").unwrap();
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(el.value, Some(ElementValue::Data(Value::Map(vec![]))));
            }
            other => panic!("expected an element, got {other:?}"),
        }
    }

    #[test]
    fn a_second_args_group_is_a_duplicate_group_error() {
        let err = parse_document("<T>(a:1)(b:2)\n").unwrap_err();
        assert!(err.message.contains("duplicate"), "got: {err:?}");
    }

    #[test]
    fn a_comment_between_groups_does_not_detach_the_next_group() {
        // Regression: a `//`/`/* */` comment between an element's groups
        // used to fall outside the whitespace/newline gap tolerance,
        // ending the element early and leaving the next group as
        // unrelated trailing text.
        let doc = parse_document("<T>(a:1) // note\n[content]\n").unwrap();
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(el.args, Some(Value::Map(vec![("a".into(), Value::Int(1))])));
                assert_eq!(el.content, Some(vec![Inline::Text("content".into())]));
            }
            other => panic!("expected an element with both groups, got {other:?}"),
        }
    }

    #[test]
    fn a_paragraph_does_not_swallow_a_following_fenced_code_block() {
        // Regression: a fenced code block (or thematic break) directly
        // after a paragraph line, with no blank line between them, used to
        // be absorbed as lazy-continuation paragraph text instead of
        // starting its own block.
        let doc = parse_document("text\n```js\ncode\n```\n").unwrap();
        assert_eq!(doc.blocks.len(), 2);
        match (&doc.blocks[0], &doc.blocks[1]) {
            (Block::Paragraph(p), Block::Element(el)) => {
                assert_eq!(&p.content, &vec![Inline::Text("text".into())]);
                assert_eq!(el.sigil, Sigil::Type("codeblock".into()));
            }
            other => panic!("expected paragraph then codeblock, got {other:?}"),
        }
    }

    #[test]
    fn a_paragraph_does_not_swallow_a_following_thematic_break() {
        let doc = parse_document("text\n---\nmore\n").unwrap();
        assert_eq!(doc.blocks.len(), 3);
        match (&doc.blocks[0], &doc.blocks[1], &doc.blocks[2]) {
            (Block::Paragraph(a), Block::Element(hr), Block::Paragraph(b)) => {
                assert_eq!(&a.content, &vec![Inline::Text("text".into())]);
                assert_eq!(hr.sigil, Sigil::Type("hr".into()));
                assert_eq!(&b.content, &vec![Inline::Text("more".into())]);
            }
            other => panic!("expected paragraph, hr, paragraph, got {other:?}"),
        }
    }

    #[test]
    fn heading_supports_inline_colon_connection() {
        let doc = parse_document("#[ Overview ]:{ id: intro, tag: main }\n").unwrap();
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(classify(el), ElementKind::Heading);
                assert_eq!(
                    el.value,
                    Some(ElementValue::Data(Value::Map(vec![
                        ("id".into(), Value::String("intro".into())),
                        ("tag".into(), Value::String("main".into())),
                    ])))
                );
            }
            other => panic!("expected heading, got {other:?}"),
        }
    }

    #[test]
    fn element_supports_inline_colon_connection() {
        let doc = parse_document("<task>[ Task A ]:{ id: taskA, priority: high }\n").unwrap();
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(el.sigil, Sigil::Type("task".into()));
                assert_eq!(
                    el.value,
                    Some(ElementValue::Data(Value::Map(vec![
                        ("id".into(), Value::String("taskA".into())),
                        ("priority".into(), Value::String("high".into())),
                    ])))
                );
            }
            other => panic!("expected element, got {other:?}"),
        }
    }

    #[test]
    fn remote_id_target_element_supports_colon_connection() {
        let doc = parse_document("<id:taskA>:{ priority: high, tag: dev }\n").unwrap();
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(el.sigil, Sigil::Type("id:taskA".into()));
                assert_eq!(
                    el.value,
                    Some(ElementValue::Data(Value::Map(vec![
                        ("priority".into(), Value::String("high".into())),
                        ("tag".into(), Value::String("dev".into())),
                    ])))
                );
            }
            other => panic!("expected remote id element, got {other:?}"),
        }
    }

    #[test]
    fn bare_url_autolink_strips_trailing_punctuation() {
        let doc = parse_document("Check https://example.com/foo! and https://example.com/bar.\n")
            .unwrap();
        match &doc.blocks[0] {
            Block::Paragraph(p) => {
                assert_eq!(p.content.len(), 5);
                assert_eq!(p.content[0], Inline::Text("Check ".into()));
                if let Inline::Element(el) = &p.content[1] {
                    assert_eq!(
                        el.args,
                        Some(Value::Map(vec![(
                            "url".into(),
                            Value::String("https://example.com/foo".into())
                        )]))
                    );
                } else {
                    panic!("expected url element");
                }
                assert_eq!(p.content[2], Inline::Text("! and ".into()));
                if let Inline::Element(el) = &p.content[3] {
                    assert_eq!(
                        el.args,
                        Some(Value::Map(vec![(
                            "url".into(),
                            Value::String("https://example.com/bar".into())
                        )]))
                    );
                } else {
                    panic!("expected url element");
                }
                assert_eq!(p.content[4], Inline::Text(".".into()));
            }
            other => panic!("expected paragraph, got {other:?}"),
        }
    }

    #[test]
    fn bare_url_autolink_handles_parentheses() {
        let doc =
            parse_document("Visit (https://example.com/foo) or https://example.com/path(bar)\n")
                .unwrap();
        match &doc.blocks[0] {
            Block::Paragraph(p) => {
                assert_eq!(p.content[0], Inline::Text("Visit (".into()));
                if let Inline::Element(el) = &p.content[1] {
                    assert_eq!(
                        el.args,
                        Some(Value::Map(vec![(
                            "url".into(),
                            Value::String("https://example.com/foo".into())
                        )]))
                    );
                } else {
                    panic!("expected url element");
                }
                assert_eq!(p.content[2], Inline::Text(") or ".into()));
                if let Inline::Element(el) = &p.content[3] {
                    assert_eq!(
                        el.args,
                        Some(Value::Map(vec![(
                            "url".into(),
                            Value::String("https://example.com/path(bar)".into())
                        )]))
                    );
                } else {
                    panic!("expected url element");
                }
            }
            other => panic!("expected paragraph, got {other:?}"),
        }
    }

    #[test]
    fn bare_url_autolink_supports_http_https_mailto() {
        let doc = parse_document("http://a.com https://b.com mailto:user@example.com\n").unwrap();
        match &doc.blocks[0] {
            Block::Paragraph(p) => {
                assert_eq!(p.content.len(), 5);
                if let Inline::Element(el) = &p.content[0] {
                    assert_eq!(
                        el.args,
                        Some(Value::Map(vec![(
                            "url".into(),
                            Value::String("http://a.com".into())
                        )]))
                    );
                }
                assert_eq!(p.content[1], Inline::Text(" ".into()));
                if let Inline::Element(el) = &p.content[2] {
                    assert_eq!(
                        el.args,
                        Some(Value::Map(vec![(
                            "url".into(),
                            Value::String("https://b.com".into())
                        )]))
                    );
                }
                assert_eq!(p.content[3], Inline::Text(" ".into()));
                if let Inline::Element(el) = &p.content[4] {
                    assert_eq!(
                        el.args,
                        Some(Value::Map(vec![(
                            "url".into(),
                            Value::String("mailto:user@example.com".into())
                        )]))
                    );
                }
            }
            other => panic!("expected paragraph, got {other:?}"),
        }
    }

    #[test]
    fn code_span_does_not_autolink_bare_url() {
        let doc = parse_document("`https://example.com`\n").unwrap();
        match &doc.blocks[0] {
            Block::Paragraph(p) => {
                assert_eq!(
                    p.content,
                    vec![Inline::Text("`https://example.com`".into())]
                );
            }
            other => panic!("expected paragraph, got {other:?}"),
        }
    }

    #[test]
    fn bare_absolute_path_parses_as_a_plain_scalar() {
        // Group B of docs/reviews/2026-08-22-link-reference-uri-schemes.md:
        // a leading `/` can never start a map key, so this is unambiguous.
        assert_eq!(
            parse_value("/readme.md").unwrap(),
            Value::String("/readme.md".into())
        );
        let doc = parse_document("@(/etc/hosts)[Hosts]\n").unwrap();
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(el.args, Some(Value::String("/etc/hosts".into())));
            }
            other => panic!("expected element, got {other:?}"),
        }
    }

    #[test]
    fn bare_scheme_uri_parses_as_one_scalar_not_a_map() {
        // Group B: `scheme://...` at a "fresh value" position (no `key:`
        // wrapper) used to be misread as `Map([(scheme, "//...")])` --
        // `://` immediately after the identifier now forces the whole
        // thing to be read as one scalar instead.
        assert_eq!(
            parse_value("https://example.com/path").unwrap(),
            Value::String("https://example.com/path".into())
        );
        assert_eq!(
            parse_value("file://some/where").unwrap(),
            Value::String("file://some/where".into())
        );
        let doc = parse_document("@(https://example.com)[Site]\n").unwrap();
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(el.args, Some(Value::String("https://example.com".into())));
            }
            other => panic!("expected element, got {other:?}"),
        }
    }

    #[test]
    fn explicit_key_colon_scheme_uri_is_still_a_map() {
        // The disambiguation only fires at a fresh-value position -- a
        // real `key: value` entry (e.g. `url: https://...`, tested above
        // in `preserves_colon_in_url_values`) is unaffected since it never
        // goes through `parse_map_body_or_scalar` for its value half.
        assert_eq!(
            parse_value("url:https://example.com").unwrap(),
            Value::Map(vec![(
                "url".into(),
                Value::String("https://example.com".into())
            )])
        );
    }

    #[test]
    fn bare_url_autolink_preserves_query_param_placeholder() {
        let doc = parse_document("http://127.0.0.1:8888/search?lang=ja&q=<query>\n").unwrap();
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(
                    el.args,
                    Some(Value::Map(vec![(
                        "url".into(),
                        Value::String("http://127.0.0.1:8888/search?lang=ja&q=<query>".into())
                    )]))
                );
            }
            other => panic!("expected element, got {other:?}"),
        }
    }
}
