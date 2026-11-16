use super::*;
use tomet_ast::{Block, Element, File, Inline, Value};
use tomet_scalar::Scalar;

fn element_of(file: &File) -> &Element {
    match &file.body[0] {
        Block::Element(el) => el,
        other => panic!("expected an element, got {other:?}"),
    }
}

fn names(el: &Element, direct_only: bool) -> Vec<String> {
    let mut out = Vec::new();
    for_each_descendant(el, direct_only, |node| {
        out.push(
            node.element_view()
                .map(|v| v.name.name.clone())
                .unwrap_or_default(),
        );
    });
    out
}

#[test]
fn direct_only_stops_at_the_first_level() {
    let file = tomet_ast_parser::parse_file("@outer[ @mid[ @inner[ text ] ] ]\n").unwrap();
    let outer = element_of(&file);
    assert_eq!(names(outer, true), vec!["mid"]);
}

#[test]
fn every_descendant_walks_every_depth() {
    let file = tomet_ast_parser::parse_file("@outer[ @mid[ @inner[ text ] ] ]\n").unwrap();
    let outer = element_of(&file);
    assert_eq!(names(outer, false), vec!["mid", "inner"]);
}

#[test]
fn descendant_walk_never_visits_the_element_itself() {
    let file = tomet_ast_parser::parse_file("@outer[ @mid[ text ] ]\n").unwrap();
    let outer = element_of(&file);
    assert!(!names(outer, false).contains(&"outer".to_string()));
}

/// `<` for entering and `>` for leaving, `P` for a paragraph and `E` for an element.
fn scope_events(src: &str, enter_answer: Visit) -> String {
    let file = tomet_ast_parser::parse_file(src).unwrap();
    let events = std::cell::RefCell::new(String::new());
    let label = |node: Node<'_>| match node {
        Node::Block(Block::Paragraph(_)) => Some('P'),
        Node::Block(Block::Element(_)) | Node::Inline(Inline::Element(_)) => Some('E'),
        _ => None,
    };
    for block in &file.body {
        let _ = walk_node_scoped(
            Node::Block(block),
            Place::Top,
            &mut |node, _| {
                if let Some(c) = label(node) {
                    let mut events = events.borrow_mut();
                    events.push('<');
                    events.push(c);
                    if c == 'E' {
                        return enter_answer;
                    }
                }
                Visit::Descend
            },
            &mut |node, _| {
                if let Some(c) = label(node) {
                    let mut events = events.borrow_mut();
                    events.push('>');
                    events.push(c);
                }
            },
        );
    }
    events.into_inner()
}

#[test]
fn a_scoped_walk_leaves_a_node_after_everything_it_holds() {
    // The paragraph inside the inline element is told apart from the two around it by
    // when it is left, which is what a walker that only sees entries cannot do.
    assert_eq!(
        scope_events("a @em[ x ] b\n", Visit::Descend),
        "<P<E<P>P>E>P"
    );
}

#[test]
fn a_scoped_walk_leaves_a_node_it_was_told_to_skip() {
    assert_eq!(scope_events("a @em[ x ] b\n", Visit::Skip), "<P<E>E>P");
}

/// The `[...]` value of `::macro(m: [ ... ])` in `src`.
fn macro_body(file: &mut File) -> &mut Value {
    let Some(args) = &mut file.header[0].data.args else {
        panic!("expected `(name: template)` entries");
    };
    args.nth_mut(0).expect("entry").value_mut()
}

/// Two blocks: an element on a line of its own, then a `$(2)` on a line of its own.
const BODY: &str = "::macro(m: [\n@card(title: $(1), n: $(3))[ hi $(name) ]\n\n$(2)\n])\n";

fn interpolations(value: &Value) -> Vec<String> {
    let mut found = Vec::new();
    for_each_interpolation_in_value(value, |i| found.push(i.expr.to_string()));
    found
}

#[test]
fn the_interpolations_under_a_value_include_arguments_and_entries() {
    let mut file = tomet_ast_parser::parse_file(BODY).unwrap();
    // `(args)` is where a descendant walk does not look; here it is the point.
    assert_eq!(
        interpolations(macro_body(&mut file)),
        ["1", "3", "name", "2"]
    );
}

#[test]
fn a_filled_interpolation_is_a_value_in_data_and_text_in_text() {
    let mut file = tomet_ast_parser::parse_file(BODY).unwrap();
    let body = macro_body(&mut file);
    fill_interpolations(body, |i| match i.expr.to_string().as_str() {
        "1" => Some(Value::Scalar(Scalar::Text("Welcome".to_string()))),
        "3" => Some(Value::Scalar(Scalar::Int(5))),
        "name" => Some(Value::Scalar(Scalar::Int(7))),
        "2" => Some(Value::Scalar(Scalar::Text("end".to_string()))),
        _ => None,
    });
    assert!(
        interpolations(body).is_empty(),
        "{:?}",
        interpolations(body)
    );
    let Value::Blocks(blocks) = &*body else {
        panic!("expected blocks");
    };
    let Block::Element(card) = &blocks[0] else {
        panic!("expected the card, got {:?}", blocks[0]);
    };
    let Some(args) = &card.data.args else {
        panic!("expected named arguments, got {:?}", card.data.args);
    };
    assert_eq!(
        args.get("title"),
        Some(&Value::Scalar(Scalar::Text("Welcome".to_string())))
    );
    assert_eq!(
        args.get("n"),
        Some(&Value::Scalar(Scalar::Int(5))),
        "a number stays a number in data"
    );
    let Some([Block::Paragraph(p)]) = card.content.as_deref() else {
        panic!("expected one paragraph, got {:?}", card.content);
    };
    assert!(
        matches!(&p.content[..], [Inline::Text(a), Inline::Text(b)]
            if a.value == "hi " && b.value == "7"),
        "{:?}",
        p.content
    );
    let Block::Paragraph(last) = &blocks[1] else {
        panic!(
            "a block of its own becomes a paragraph, got {:?}",
            blocks[1]
        );
    };
    assert!(matches!(&last.content[..], [Inline::Text(t)] if t.value == "end"));
}

#[test]
fn an_interpolation_nothing_fills_is_left_as_written() {
    let mut file = tomet_ast_parser::parse_file(BODY).unwrap();
    let body = macro_body(&mut file);
    fill_interpolations(body, |i| {
        (i.expr.to_string() == "2").then_some(Value::Scalar(Scalar::Int(1)))
    });
    assert_eq!(interpolations(body), ["1", "3", "name"]);
}

#[test]
fn a_value_that_has_no_text_has_no_place_in_text() {
    let mut file = tomet_ast_parser::parse_file(BODY).unwrap();
    let body = macro_body(&mut file);
    fill_interpolations(body, |_| Some(Value::Tuple(tomet_ast::Tuple::new())));
    // Data takes a map as it is; running text and a block of its own cannot.
    assert_eq!(interpolations(body), ["name", "2"]);
}

#[test]
fn every_interpolation_is_found_wherever_it_sits() {
    let src = "=[ $(title) ]::rule(allow: $(conn))\n\
               \n\
               $(block)\n\
               \n\
               text $(inline) more\n\
               \n\
               @x(a: $(arg), b: ($(nested))){ k: $(val) }[ $(content) ]::as($(connect))\n\
               \n\
               - item $(item)\n";
    let file = tomet_ast_parser::parse_file(src).unwrap();
    let mut found = Vec::new();
    for_each_interpolation(&file, |i| found.push(i.expr.to_string()));
    found.sort();
    let mut expected = vec![
        "title", "conn", "block", "inline", "arg", "nested", "val", "content", "connect", "item",
    ];
    expected.sort();
    assert_eq!(found, expected);
}
/// A seat in every position a value, a block or running text can sit in, named by where.
const SEATS: &str = "\
::meta(k: <$t_decl_value>)

<$t_block>

text <$t_inline> more

@host(a: <$t_args>, b: (<$t_seq>)){ d: <$t_pair> }[ <$t_content_block>\n\ntext <$t_content_inline> ]::rule(allow: (<$t_connect>))

=[ title <$t_title> ]{ k: <$t_sec_data> }::rule(allow: (<$t_sec_connect>))

- item <$t_item>
";

const SEAT_ORDER: [&str; 12] = [
    "t_decl_value",
    "t_block",
    "t_inline",
    "t_args",
    "t_seq",
    "t_pair",
    "t_content_block",
    "t_content_inline",
    "t_connect",
    "t_title",
    "t_sec_data",
    "t_item",
];

fn seat_names(file: &File) -> Vec<String> {
    let mut found = Vec::new();
    for_each_type_seat(file, |t| found.push(t.expr.to_string()));
    found
}

#[test]
fn every_type_seat_is_found_wherever_it_sits() {
    let file = tomet_ast_parser::parse_file(SEATS).unwrap();
    let found = seat_names(&file);
    // A `::connect` is reached, and comes last in its owner.
    let mut expected: Vec<&str> = SEAT_ORDER.to_vec();
    expected.insert(9, "t_sec_connect");
    expected.sort_unstable();
    let mut sorted = found.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, expected);
    // Document order, each owner's connects after what else it holds.
    assert_eq!(found[..3], ["t_decl_value", "t_block", "t_inline"]);
    let host = found.iter().position(|n| n == "t_args").unwrap();
    assert_eq!(
        found[host..host + 6],
        [
            "t_args",
            "t_seq",
            "t_pair",
            "t_content_block",
            "t_content_inline",
            "t_connect"
        ]
    );
}

#[test]
fn a_type_seat_is_not_an_element_nor_an_interpolation_to_the_walkers() {
    let file = tomet_ast_parser::parse_file(SEATS).unwrap();
    let mut interpolations = 0;
    for_each_interpolation(&file, |_| interpolations += 1);
    assert_eq!(interpolations, 0);
    let mut elements = Vec::new();
    for_each_element(&file, |el| elements.push(el.name.name.clone()));
    assert_eq!(elements, ["host"]);
}

#[test]
fn the_mutable_walk_reaches_the_type_seats_the_immutable_walks_do() {
    struct Seats(Vec<String>);
    impl Sink for Seats {
        fn inline(&mut self, inline: &mut Inline) {
            if let Inline::TypeSeat(t) = inline {
                self.0.push(t.expr.to_string());
            }
        }
        fn block(&mut self, block: &mut Block) {
            if let Block::TypeSeat(t) = block {
                self.0.push(t.expr.to_string());
            }
        }
        fn value(&mut self, value: &mut Value) {
            if let Value::TypeSeat(t) = value {
                self.0.push(t.expr.to_string());
            }
        }
    }
    let mut file = tomet_ast_parser::parse_file(SEATS).unwrap();
    let mut sink = Seats(Vec::new());
    walk_mut(&mut file, &mut sink, true);
    assert_eq!(sink.0, seat_names(&file));
    assert_eq!(sink.0.len(), SEAT_ORDER.len() + 1);
}

/// Two blocks: an element on a line of its own, then a seat on a line of its own.
const SEAT_BODY: &str =
    "::macro(m: [\n@card(title: <$string>, n: <$int>)[ hi <$name> ]\n\n<$date>\n])\n";

fn seats(value: &Value) -> Vec<String> {
    let mut found = Vec::new();
    for_each_type_seat_in_value(value, |t| found.push(t.expr.to_string()));
    found
}

#[test]
fn the_type_seats_under_a_value_include_arguments_and_entries() {
    let mut file = tomet_ast_parser::parse_file(SEAT_BODY).unwrap();
    assert_eq!(
        seats(macro_body(&mut file)),
        ["string", "int", "name", "date"]
    );
}

#[test]
fn a_filled_type_seat_is_a_value_in_data_and_text_in_text() {
    let mut file = tomet_ast_parser::parse_file(SEAT_BODY).unwrap();
    let body = macro_body(&mut file);
    fill_type_seats(body, |t| match t.expr.to_string().as_str() {
        "string" => Some(Value::Scalar(Scalar::Text("Welcome".to_string()))),
        "int" => Some(Value::Scalar(Scalar::Int(5))),
        "name" => Some(Value::Scalar(Scalar::Int(7))),
        "date" => Some(Value::Scalar(Scalar::Text("end".to_string()))),
        _ => None,
    });
    assert!(seats(body).is_empty(), "{:?}", seats(body));
    let Value::Blocks(blocks) = &*body else {
        panic!("expected blocks");
    };
    let Block::Element(card) = &blocks[0] else {
        panic!("expected the card, got {:?}", blocks[0]);
    };
    let Some(args) = &card.data.args else {
        panic!("expected named arguments, got {:?}", card.data.args);
    };
    assert_eq!(
        args.get("title"),
        Some(&Value::Scalar(Scalar::Text("Welcome".to_string())))
    );
    assert_eq!(
        args.get("n"),
        Some(&Value::Scalar(Scalar::Int(5))),
        "a number stays a number in data"
    );
    let Some([Block::Paragraph(p)]) = card.content.as_deref() else {
        panic!("expected one paragraph, got {:?}", card.content);
    };
    assert!(
        matches!(&p.content[..], [Inline::Text(a), Inline::Text(b)]
            if a.value == "hi " && b.value == "7"),
        "{:?}",
        p.content
    );
    let Block::Paragraph(last) = &blocks[1] else {
        panic!(
            "a block of its own becomes a paragraph, got {:?}",
            blocks[1]
        );
    };
    assert!(matches!(&last.content[..], [Inline::Text(t)] if t.value == "end"));
}

#[test]
fn a_type_seat_nothing_fills_is_left_as_written() {
    let mut file = tomet_ast_parser::parse_file(SEAT_BODY).unwrap();
    let body = macro_body(&mut file);
    fill_type_seats(body, |t| {
        (t.expr.to_string() == "date").then_some(Value::Scalar(Scalar::Int(1)))
    });
    assert_eq!(seats(body), ["string", "int", "name"]);
    // A value with no scalar text has no place in text; data takes it as it is.
    fill_type_seats(body, |_| Some(Value::Tuple(tomet_ast::Tuple::new())));
    assert_eq!(seats(body), ["name"]);
}

#[test]
fn the_type_seats_of_a_file_are_filled_in_the_order_they_are_shown() {
    let mut file = tomet_ast_parser::parse_file(SEATS).unwrap();
    let shown = seat_names(&file);
    let mut asked = Vec::new();
    fill_type_seats_in_file(&mut file, |t| {
        asked.push(t.expr.to_string());
        Some(Value::Scalar(Scalar::Text("x".to_string())))
    });
    assert_eq!(asked, shown);
    assert_eq!(asked.len(), SEAT_ORDER.len() + 1);
    assert_eq!(seat_names(&file), Vec::<String>::new());
}

#[test]
fn a_type_seat_of_a_file_nothing_fills_is_left_as_written() {
    let mut file = tomet_ast_parser::parse_file(SEATS).unwrap();
    fill_type_seats_in_file(&mut file, |t| {
        (t.expr.to_string() != "t_block").then(|| Value::Scalar(Scalar::Text("x".to_string())))
    });
    assert_eq!(seat_names(&file), ["t_block"]);
}

#[test]
fn filling_type_seats_leaves_interpolations_and_the_reverse() {
    let src = "::macro(m: [\n@card(title: <$string>, n: $(1))[ hi <$name> $(2) ]\n])\n";
    let mut file = tomet_ast_parser::parse_file(src).unwrap();
    let body = macro_body(&mut file);
    fill_type_seats(body, |_| Some(Value::Scalar(Scalar::Int(9))));
    assert!(seats(body).is_empty());
    assert_eq!(interpolations(body), ["1", "2"]);
    fill_interpolations(body, |_| Some(Value::Scalar(Scalar::Int(3))));
    assert!(interpolations(body).is_empty());
}
