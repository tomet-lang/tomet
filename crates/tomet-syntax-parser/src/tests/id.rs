use super::*;
use tomet_ast::Id;

#[test]
fn element_reads_a_bare_id_after_its_groups() {
    let doc = parse_document("@memo(a: 1)[ x ]#(myid)\n").unwrap();
    match &doc.blocks[0] {
        Block::Element(el) => assert_eq!(el.id, Some(Id("myid".into()))),
        other => panic!("expected element, got {other:?}"),
    }
}

#[test]
fn element_reads_a_quoted_id_with_spaces() {
    let doc = parse_document("@memo#(\"has spaces\")\n").unwrap();
    match &doc.blocks[0] {
        Block::Element(el) => assert_eq!(el.id, Some(Id("has spaces".into()))),
        other => panic!("expected element, got {other:?}"),
    }
}

#[test]
fn element_reads_a_non_ascii_id() {
    let doc = parse_document("@memo#(かなた)\n").unwrap();
    match &doc.blocks[0] {
        Block::Element(el) => assert_eq!(el.id, Some(Id("かなた".into()))),
        other => panic!("expected element, got {other:?}"),
    }
}

#[test]
fn id_may_come_before_its_groups_too() {
    // Unlike connects, id has no fixed order relative to args/content/value.
    let doc = parse_document("@memo#(myid)(a: 1)[ x ]\n").unwrap();
    match &doc.blocks[0] {
        Block::Element(el) => {
            assert_eq!(el.id, Some(Id("myid".into())));
            assert_eq!(el.args, Some(Value::Map(vec![("a".into(), Value::Int(1))])));
        }
        other => panic!("expected element, got {other:?}"),
    }
}

#[test]
fn id_written_after_a_connect_belongs_to_that_connect_not_the_element() {
    // `:rule(...)`'s own recursive group-read claims an adjacent
    // `#(...)` for itself before control ever returns to `@memo`'s own
    // loop, so there is no ambiguous "did they mean memo's id?" case --
    // writing the id before any connect is the only way to give it to
    // the element itself.
    let doc = parse_document("@memo(a: 1):rule(allow: list(card))#(myid)\n").unwrap();
    match &doc.blocks[0] {
        Block::Element(el) => {
            assert_eq!(el.id, None);
            assert_eq!(el.connects[0].id, Some(Id("myid".into())));
        }
        other => panic!("expected element, got {other:?}"),
    }
}

#[test]
fn a_second_id_is_a_parse_error() {
    let err = parse_document("@memo#(a)#(b)\n").unwrap_err();
    assert!(err.message.contains("at most one"), "got: {}", err.message);
}

#[test]
fn empty_id_group_is_a_parse_error() {
    assert!(parse_document("@memo#()\n").is_err());
}

#[test]
fn section_reads_an_id_in_bracket_form() {
    let doc = parse_document("=[ Intro ]#(intro)\n").unwrap();
    match &doc.blocks[0] {
        Block::Section(s) => assert_eq!(s.id, Some(Id("intro".into()))),
        other => panic!("expected section, got {other:?}"),
    }
}

#[test]
fn section_sugar_reads_a_trailing_id() {
    let doc = parse_document("= Intro #(intro)\n").unwrap();
    match &doc.blocks[0] {
        Block::Section(s) => {
            assert_eq!(s.id, Some(Id("intro".into())));
            assert_eq!(s.title, vec![Inline::Text("Intro".into())]);
        }
        other => panic!("expected section, got {other:?}"),
    }
}

#[test]
fn section_sugar_reads_a_trailing_id_and_attrs_together() {
    let doc = parse_document("= Intro #(intro){ tag: x }\n").unwrap();
    match &doc.blocks[0] {
        Block::Section(s) => {
            assert_eq!(s.id, Some(Id("intro".into())));
            assert_eq!(
                s.value,
                Some(ElementValue::from_map(Value::Map(vec![(
                    "tag".into(),
                    Value::String("x".into())
                )])))
            );
            assert_eq!(s.title, vec![Inline::Text("Intro".into())]);
        }
        other => panic!("expected section, got {other:?}"),
    }
}

#[test]
fn list_item_full_form_reads_an_id() {
    let doc = parse_document("- (x)[ y ]#(item1)\n").unwrap();
    let el = match &doc.blocks[0] {
        Block::Element(el) => el,
        other => panic!("expected element, got {other:?}"),
    };
    let items = list_items(el);
    assert_eq!(items[0].id, Some(Id("item1".into())));
}

#[test]
fn list_item_sugar_reads_a_trailing_id() {
    let doc = parse_document("- plain text #(item1)\n").unwrap();
    let el = match &doc.blocks[0] {
        Block::Element(el) => el,
        other => panic!("expected element, got {other:?}"),
    };
    let items = list_items(el);
    assert_eq!(items[0].id, Some(Id("item1".into())));
    assert_eq!(
        items[0].content,
        Some(vec![Inline::Text("plain text".into())])
    );
}

#[test]
fn list_item_sugar_reads_a_trailing_id_and_attrs_together() {
    let doc = parse_document("- plain text #(item1){ tag: x }\n").unwrap();
    let el = match &doc.blocks[0] {
        Block::Element(el) => el,
        other => panic!("expected element, got {other:?}"),
    };
    let items = list_items(el);
    assert_eq!(items[0].id, Some(Id("item1".into())));
    assert_eq!(
        items[0].value,
        Some(ElementValue::from_map(Value::Map(vec![(
            "tag".into(),
            Value::String("x".into())
        )])))
    );
}

#[test]
fn list_item_sugar_with_no_trailing_group_has_no_id() {
    let doc = parse_document("- plain text\n").unwrap();
    let el = match &doc.blocks[0] {
        Block::Element(el) => el,
        other => panic!("expected element, got {other:?}"),
    };
    let items = list_items(el);
    assert_eq!(items[0].id, None);
    assert_eq!(
        items[0].content,
        Some(vec![Inline::Text("plain text".into())])
    );
}

#[test]
fn hash_not_followed_by_paren_after_an_element_does_not_error() {
    // Regression: `#` immediately after `@meta{...}`'s closing `}` used
    // to be wrongly claimed by `parse_groups`'s id arm (within the same
    // "one newline of gap tolerance" that lets an element's own groups
    // split across lines), erroring on "expected '(' after '#'" instead
    // of leaving an unrelated next line alone.
    let doc = parse_document("@meta{a: 1}\n#[ Title ]\n").unwrap();
    assert_eq!(doc.blocks.len(), 2);
    assert!(matches!(&doc.blocks[0], Block::Element(_)));
    assert!(matches!(&doc.blocks[1], Block::Paragraph(_)));
}

#[test]
fn bare_hash_paren_inside_sugar_text_is_still_plain_text_when_unconfirmed() {
    // Not followed by anything that parses as an id (unterminated), so
    // this must remain ordinary prose rather than erroring the whole line.
    let doc = parse_document("- text #(unterminated\n").unwrap();
    let el = match &doc.blocks[0] {
        Block::Element(el) => el,
        other => panic!("expected element, got {other:?}"),
    };
    let items = list_items(el);
    assert_eq!(items[0].id, None);
    assert_eq!(
        items[0].content,
        Some(vec![Inline::Text("text #(unterminated".into())])
    );
}
