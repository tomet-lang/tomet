//! Type seats (`<$type(args)>`), read by `parse_file` like any other syntax. See the `TypeSeat`
//! node of `tomet-ast`.

use super::interpolation::describe;
use super::*;
use tomet_ast::TypeSeat;

/// The one block-level seat of a document.
fn block_seat(src: &str) -> TypeSeat {
    let doc = parse_file(src).unwrap();
    match <[Block; 1]>::try_from(doc.body).expect("one block") {
        [Block::TypeSeat(seat)] => seat,
        [other] => panic!("expected one type seat, got {other:?}"),
    }
}

/// The one paragraph's inline run of a document.
fn inlines(src: &str) -> Vec<Inline> {
    let doc = parse_file(src).unwrap();
    match <[Block; 1]>::try_from(doc.body).expect("one block") {
        [Block::Paragraph(p)] => p.content,
        [other] => panic!("expected one paragraph, got {other:?}"),
    }
}

/// The type a seat holds, as a span-free string.
fn written(seat: &TypeSeat) -> String {
    describe(&seat.expr)
}

fn error_of(src: &str) -> String {
    parse_file(src)
        .expect_err(&format!("{src:?} is refused"))
        .message
}

#[test]
fn a_seat_holds_the_type_as_the_expression_it_is_written() {
    for (src, expected) in [
        ("<$uuid>", "Identifier(uuid)"),
        ("<$date>", "Identifier(date)"),
        (
            "<$date(default: auto)>",
            "Call(Identifier(date), [NamedArg(default, Identifier(auto))])",
        ),
        (
            "<$list(string, length: 6)>",
            "Call(Identifier(list), [Identifier(string), NamedArg(length, Int(6))])",
        ),
        (
            r#"<$nanoid(length: 8, prefix: "note-", default: auto)>"#,
            r#"Call(Identifier(nanoid), [NamedArg(length, Int(8)), NamedArg(prefix, String("note-")), NamedArg(default, Identifier(auto))])"#,
        ),
        // A type inside a type.
        (
            "<$list(list(int), length: 2)>",
            "Call(Identifier(list), [Call(Identifier(list), [Identifier(int)]), NamedArg(length, Int(2))])",
        ),
        // The arguments are expressions: which are meaningful is not the AST's to say.
        (
            "<$x(a + b)>",
            "Call(Identifier(x), [(Identifier(a) + Identifier(b))])",
        ),
    ] {
        assert_eq!(written(&block_seat(src)), expected, "{src:?}");
    }
}

#[test]
fn a_name_and_a_call_with_no_arguments_are_two_written_forms() {
    assert_eq!(written(&block_seat("<$uuid>")), "Identifier(uuid)");
    assert_eq!(
        written(&block_seat("<$uuid()>")),
        "Call(Identifier(uuid), [])"
    );
    // And print as they were written.
    assert_eq!(block_seat("<$uuid>").to_string(), "<$uuid>");
    assert_eq!(block_seat("<$uuid()>").to_string(), "<$uuid()>");
}

#[test]
fn a_seat_prints_as_the_source_that_reads_back_as_it() {
    for src in [
        "<$uuid>",
        "<$uuid()>",
        "<$date(default: auto)>",
        "<$list(string, length: 6)>",
        r#"<$nanoid(length: 8, prefix: "note-", default: auto)>"#,
    ] {
        let seat = block_seat(src);
        assert_eq!(seat.to_string(), src);
        assert_eq!(block_seat(&seat.to_string()), seat);
    }
    // White space and line breaks inside the arguments are not kept: the canonical form is.
    assert_eq!(
        block_seat("<$list(\n  string,\n  length:   6\n)>").to_string(),
        "<$list(string, length: 6)>"
    );
}

#[test]
fn a_greater_than_in_the_arguments_does_not_end_the_seat() {
    let seat = block_seat(r#"<$nanoid(prefix: ">", length: 3)>"#);
    assert_eq!(
        written(&seat),
        r#"Call(Identifier(nanoid), [NamedArg(prefix, String(">")), NamedArg(length, Int(3))])"#
    );
    let seat = block_seat("<$x(a > b)>");
    assert_eq!(
        written(&seat),
        "Call(Identifier(x), [(Identifier(a) > Identifier(b))])"
    );
}

#[test]
fn a_block_seat_spans_the_box() {
    let src = "text\n\n<$list(string)>\n";
    let doc = parse_file(src).unwrap();
    let Some(Block::TypeSeat(seat)) = doc.body.last() else {
        panic!("expected a seat, got {:?}", doc.body);
    };
    assert!(seat.span.exact_eq(&Span::new(6, 21)));
    assert_eq!(&src[seat.span.start..seat.span.end], "<$list(string)>");
    // The type's own spans are in the document too.
    assert_eq!(
        &src[seat.expr.span.start..seat.expr.span.end],
        "list(string)"
    );
}

#[test]
fn a_seat_sits_in_running_text() {
    let items = inlines("id: <$uuid> and <$date(default: auto)> end");
    let [
        Inline::Text(a),
        Inline::TypeSeat(first),
        Inline::Text(b),
        Inline::TypeSeat(second),
        Inline::Text(c),
    ] = items.as_slice()
    else {
        panic!("expected text and two seats, got {items:?}");
    };
    assert_eq!(
        (a.value.as_str(), b.value.as_str(), c.value.as_str()),
        ("id: ", " and ", " end")
    );
    assert_eq!(written(first), "Identifier(uuid)");
    assert_eq!(
        written(second),
        "Call(Identifier(date), [NamedArg(default, Identifier(auto))])"
    );
    // Starting the line, with more text after it, it starts a paragraph.
    let items = inlines("<$uuid> is the id");
    assert!(matches!(
        items.as_slice(),
        [Inline::TypeSeat(_), Inline::Text(_)]
    ));
}

#[test]
fn a_seat_takes_no_content_after_it() {
    // `<$x>[t]` is a seat and the text `[t]`.
    let items = inlines("<$x>[t]");
    let [Inline::TypeSeat(seat), Inline::Text(text)] = items.as_slice() else {
        panic!("expected a seat and text, got {items:?}");
    };
    assert_eq!(written(seat), "Identifier(x)");
    assert_eq!(text.value, "[t]");
    // Nor a `{value}`, an id or an argument group.
    for after in ["{a: 1}", "#(id)", "(a)"] {
        let items = inlines(&format!("<$x>{after}"));
        assert!(
            matches!(items.as_slice(), [Inline::TypeSeat(_), Inline::Text(t)] if t.value == after),
            "{after:?}: {items:?}"
        );
    }
}

#[test]
fn a_seat_sits_where_a_value_goes() {
    let doc = parse_file("@card(<$date>)\n").unwrap();
    let [Block::Element(card)] = doc.body.as_slice() else {
        panic!("expected one element, got {:?}", doc.body);
    };
    let Some(Value::TypeSeat(seat)) = &card.data.args else {
        panic!("expected a seat as the argument, got {:?}", card.data.args);
    };
    assert_eq!(written(seat), "Identifier(date)");

    let doc = parse_file("@card(created: <$date(default: auto)>, n: <$int>)\n").unwrap();
    let [Block::Element(card)] = doc.body.as_slice() else {
        panic!("expected one element, got {:?}", doc.body);
    };
    let Some(Value::Map(args)) = &card.data.args else {
        panic!("expected named arguments, got {:?}", card.data.args);
    };
    assert!(matches!(&args[0].1, Value::TypeSeat(_)));
    assert!(matches!(&args[1].1, Value::TypeSeat(_)));

    // In a sequence and in a value group's pair.
    let doc = parse_file("@card((<$int>, <$string>)){ tags: <$list(string)> }\n").unwrap();
    let [Block::Element(card)] = doc.body.as_slice() else {
        panic!("expected one element, got {:?}", doc.body);
    };
    let Some(Value::Seq(items)) = &card.data.args else {
        panic!("expected a sequence, got {:?}", card.data.args);
    };
    assert!(items.iter().all(|v| matches!(v, Value::TypeSeat(_))));
    let Some(Group(entries)) = &card.data.value else {
        panic!("expected a value group");
    };
    assert!(matches!(&entries[0], tomet_ast::Entry::Pair(k, Value::TypeSeat(_)) if k == "tags"));
}

#[test]
fn a_seat_sits_in_the_content_of_an_element() {
    let doc = parse_file("@card[ <$string> ]\n").unwrap();
    let [Block::Element(card)] = doc.body.as_slice() else {
        panic!("expected one element, got {:?}", doc.body);
    };
    // Alone on its line inside the group, it is a block; `[ <$string> ]` is one line of text.
    let content = card.content.as_deref().unwrap();
    assert!(
        matches!(content, [Block::TypeSeat(_)])
            || matches!(content, [Block::Paragraph(p)] if matches!(p.content.as_slice(), [Inline::TypeSeat(_)])),
        "{content:?}"
    );

    let doc = parse_file("@card[\n  <$string>\n\n  more <$int>\n]\n").unwrap();
    let [Block::Element(card)] = doc.body.as_slice() else {
        panic!("expected one element, got {:?}", doc.body);
    };
    let content = card.content.as_deref().unwrap();
    assert!(matches!(content[0], Block::TypeSeat(_)), "{content:?}");
    assert!(matches!(&content[1], Block::Paragraph(_)));
}

#[test]
fn a_seat_in_a_heading_title_is_inline() {
    for src in ["=[ <$string> ]\n", "= <$string>\n"] {
        let doc = parse_file(src).unwrap();
        let [Block::Section(sec)] = doc.body.as_slice() else {
            panic!("expected a section, got {:?}", doc.body);
        };
        assert!(
            matches!(sec.title.as_slice(), [Inline::TypeSeat(seat)] if written(seat) == "Identifier(string)"),
            "{src:?}: {:?}",
            sec.title
        );
    }
    let doc = parse_file("== Name: <$string(default: none)>\n").unwrap();
    let [Block::Section(sec)] = doc.body.as_slice() else {
        panic!("expected a section");
    };
    assert!(matches!(
        sec.title.as_slice(),
        [Inline::Text(_), Inline::TypeSeat(_)]
    ));
}

#[test]
fn a_seat_in_a_list_item_is_inline() {
    let doc = parse_file("- <$int>\n- name <$string>\n").unwrap();
    let [Block::List(list)] = doc.body.as_slice() else {
        panic!("expected a list, got {:?}", doc.body);
    };
    let mut seats = 0;
    for item in &list.items {
        for block in &item.content {
            if let Block::Paragraph(p) = block {
                seats += p
                    .content
                    .iter()
                    .filter(|i| matches!(i, Inline::TypeSeat(_)))
                    .count();
            }
            if matches!(block, Block::TypeSeat(_)) {
                seats += 1;
            }
        }
    }
    assert_eq!(seats, 2);
}

/// How many type seats `parse_file` finds anywhere in `src`.
fn seats_in(src: &str) -> usize {
    let doc = parse_file(src).unwrap();
    let mut count = 0;
    tomet_tree::for_each_type_seat(&doc, |_| count += 1);
    count
}

#[test]
fn a_seat_is_found_wherever_content_goes() {
    for (src, expected) in [
        ("- <$int>", 1),
        ("- <$list(string, length: 2)> (a)", 1),
        ("== <$nanoid(length: 8)>", 1),
        ("=[ <$x(a: 1)> ]#(id)", 1),
        ("=[ <$string> ]", 1),
        ("@card[\n  <$string>\n]", 1),
        ("@card[\n  line <$int>\n\n  line <$date>\n]", 2),
        ("@callout\n| <$int> text\n| <$date>", 2),
        ("@ns.thing(a: [<$int>], b: <$date>)", 2),
        ("::meta(created: <$datetime(default: auto)>)", 1),
        ("*emphasis <$int>*", 1),
    ] {
        assert_eq!(seats_in(src), expected, "seats in `{src}`");
    }
}

#[test]
fn a_seat_is_not_an_element_nor_a_hole_nor_an_interpolation() {
    let doc = parse_file("<$meta>\n\n<$list(string)>\n").unwrap();
    let (mut elements, mut holes, mut interps) = (0, 0, 0);
    tomet_tree::for_each_element(&doc, |_| elements += 1);
    tomet_tree::for_each_hole(&doc, |_| holes += 1);
    tomet_tree::for_each_interpolation(&doc, |_| interps += 1);
    assert_eq!((elements, holes, interps), (0, 0, 0));
    assert_eq!(seats_in("<$meta>\n\n<$list(string)>\n"), 2);
}

#[test]
fn what_is_not_a_seat_is_text() {
    for src in [
        "cost <$5>",
        "<$ x>",
        "<$x >",
        "<$x",
        "<$>",
        "a <$ b",
        "<$x.y>",
        "<$x{a: 1}>",
        "<$x[t]>",
        "<$x#(id)>",
        "<$x(a)(b)>",
    ] {
        let doc = parse_file(src).unwrap_or_else(|e| panic!("`{src}` is never an error: {e}"));
        let mut seats = 0;
        tomet_tree::for_each_type_seat(&doc, |_| seats += 1);
        assert_eq!(seats, 0, "`{src}` has no seat");
    }
    // Escaped, a seat is text.
    let items = inlines("write \\<$date> for a date");
    assert!(
        matches!(items.as_slice(), [Inline::Text(t)] if t.value == "write <$date> for a date"),
        "{items:?}"
    );
}

/// A `(` that does not close is no seat, and it is not text: `$x(` reads as the head of an
/// interpolation whose arguments never end, so the document is refused, as `$x(a` is.
#[test]
fn a_seat_whose_paren_does_not_close_is_an_error_and_not_text() {
    for src in ["<$x(a>", "<$x(", "<$x(\"a>", "x <$x(a> y", "@a(<$x(a>)"] {
        assert!(parse_file(src).is_err(), "`{src}` is an error");
    }
    assert!(parse_file("$x(a").is_err(), "the same text without the `<`");
}

#[test]
fn a_hole_and_a_name_tag_stay_what_they_were() {
    let items = inlines("a <?int> b <PATH> c <$int>");
    let kinds: Vec<&str> = items
        .iter()
        .map(|i| match i {
            Inline::Text(_) => "text",
            Inline::Hole(_) => "hole",
            Inline::TypeSeat(_) => "seat",
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    // The name tag is text, in the run around it.
    assert_eq!(kinds, ["text", "hole", "text", "seat"]);
    assert!(matches!(&items[2], Inline::Text(t) if t.value == " b <PATH> c "));
}

#[test]
fn a_seat_that_is_not_a_type_is_refused_and_says_what_it_holds() {
    for src in ["<$true>", "<$null>", "<$false(a)>"] {
        let message = error_of(src);
        assert!(
            message.contains("a type seat holds a type"),
            "{src:?}: {message}"
        );
    }
}

#[test]
fn arguments_that_are_not_an_expression_are_refused() {
    for src in ["<$x(a b)>", "<$x(1 +)>", "<$x(,)>"] {
        let _ = error_of(src);
    }
}

#[test]
fn a_seat_is_no_entry_of_a_value_group() {
    let message = error_of("@card(){ <$x> }\n");
    assert!(message.contains("type seat"), "{message}");
    let message = error_of("@card(){\n  <$x>\n}\n");
    assert!(message.contains("type seat"), "{message}");
}

#[test]
fn a_seat_in_a_value_file_is_a_value() {
    let value = parse_value("a: <$date>, b: (<$int>)").unwrap();
    let Value::Map(entries) = value else {
        panic!("expected a map");
    };
    assert!(matches!(&entries[0].1, Value::TypeSeat(_)));
    assert!(
        matches!(&entries[1].1, Value::Seq(items) if matches!(items.as_slice(), [Value::TypeSeat(_)]))
    );

    let file = parse_data_file("::kind(blueprint)\n\ncreated: <$date(default: auto)>\n").unwrap();
    let Value::Map(entries) = &file.body else {
        panic!("expected a map");
    };
    let Value::TypeSeat(seat) = &entries[0].1 else {
        panic!("expected a seat, got {:?}", entries[0].1);
    };
    // Its place is in the file, not in the body.
    let src = "::kind(blueprint)\n\ncreated: <$date(default: auto)>\n";
    assert_eq!(
        &src[seat.span.start..seat.span.end],
        "<$date(default: auto)>"
    );
}

#[test]
fn list_seats_with_sequence_defaults() {
    for (src, expected_desc, expected_print) in [
        (
            r#"<$list(string, default: ("a", "b"))>"#,
            r#"Call(Identifier(list), [Identifier(string), NamedArg(default, Seq([String("a"), String("b")]))])"#,
            r#"<$list(string, default: ("a", "b"))>"#,
        ),
        (
            "<$list(string, default: ())>",
            "Call(Identifier(list), [Identifier(string), NamedArg(default, Seq([]))])",
            "<$list(string, default: ())>",
        ),
        (
            r#"<$list(string, default: ("a",))>"#,
            r#"Call(Identifier(list), [Identifier(string), NamedArg(default, Seq([String("a")]))])"#,
            r#"<$list(string, default: ("a",))>"#,
        ),
        (
            r#"<$list(string, default: ("a"))>"#,
            r#"Call(Identifier(list), [Identifier(string), NamedArg(default, Group(String("a")))])"#,
            r#"<$list(string, default: ("a"))>"#,
        ),
    ] {
        let seat = block_seat(src);
        assert_eq!(written(&seat), expected_desc, "{src}");
        assert_eq!(seat.to_string(), expected_print, "{src}");
    }
}

#[test]
fn unclosed_type_seat_paren_is_refused() {
    for src in ["<$x(a>", "<$x(", "<$x(\"a>"] {
        assert!(parse_file(src).is_err(), "{src:?} must be refused");
    }
}
