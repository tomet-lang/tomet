//! Holes (`?matcher(name, args){options}[content]`), which only `parse_pattern`
//! reads. See `tmtroot/spec/syntax/hole.tmt`.

use super::*;
use tomet_ast::Name;

/// The one paragraph's inline run of a pattern.
fn inlines(src: &str) -> Vec<Inline> {
    let doc = parse_pattern(src).unwrap();
    match <[Block; 1]>::try_from(doc.blocks).expect("one block") {
        [Block::Paragraph(p)] => p.content,
        [other] => panic!("expected one paragraph, got {other:?}"),
    }
}

/// The one block element of a pattern.
fn element(src: &str) -> Element {
    let doc = parse_pattern(src).unwrap();
    match <[Block; 1]>::try_from(doc.blocks).expect("one block") {
        [Block::Element(el)] => el,
        [other] => panic!("expected one element, got {other:?}"),
    }
}

/// A run of text and holes as a short string: `"adr-" ?int "-" ? ".md"`.
fn shape(items: &[Inline]) -> String {
    let parts: Vec<String> = items
        .iter()
        .map(|item| match item {
            Inline::Text(t) => format!("{:?}", t.value),
            Inline::Element(el) => match &el.sigil {
                Sigil::Question(Some(m)) => format!("?{m}"),
                Sigil::Question(None) => "?".to_string(),
                other => panic!("not a hole: {other:?}"),
            },
            other => panic!("unexpected inline {other:?}"),
        })
        .collect();
    parts.join(" ")
}

fn matcher(name: &str) -> Sigil {
    Sigil::Question(Some(Name::bare(name)))
}

fn string(s: &str) -> Value {
    Value::String(s.to_string())
}

#[test]
fn holes_sit_in_running_text() {
    let items = inlines("adr-?int(n)-?(slug).md");
    assert_eq!(shape(&items), r#""adr-" ?int "-" ? ".md""#);
}

#[test]
fn a_single_argument_is_the_value_itself() {
    let Inline::Element(hole) = &inlines("a?int(n)")[1] else {
        panic!("expected a hole");
    };
    assert_eq!(hole.sigil, matcher("int"));
    assert_eq!(hole.args, Some(string("n")));
}

#[test]
fn an_empty_group_is_an_empty_map() {
    let hole = element("?()");
    assert_eq!(hole.sigil, Sigil::Question(None));
    assert_eq!(hole.args, Some(Value::Map(Vec::new())));
    assert_eq!(hole.placement, Placement::Block);
}

#[test]
fn the_matcher_may_be_left_out() {
    let hole = element("?(n)");
    assert_eq!(hole.sigil, Sigil::Question(None));
    assert_eq!(hole.args, Some(string("n")));
}

#[test]
fn content_is_a_sub_pattern() {
    let hole = element("?(id)[adr-?int(n)-?(slug)]");
    assert_eq!(hole.sigil, Sigil::Question(None));
    assert_eq!(hole.args, Some(string("id")));
    let content = hole.content.expect("content");
    assert_eq!(shape(first_para(&content)), r#""adr-" ?int "-" ?"#);
}

#[test]
fn several_arguments_are_unkeyed_entries() {
    let hole = element(r#"?int(n, "x")"#);
    assert_eq!(hole.sigil, matcher("int"));
    assert_eq!(
        hole.args,
        Some(Value::Map(vec![
            (String::new(), string("n")),
            (String::new(), string("x")),
        ]))
    );
}

#[test]
fn options_are_the_value_group() {
    let hole = element("?int(n){min: 1}");
    let Some(ElementValue::Group(entries)) = &hole.value else {
        panic!("expected a value group, got {:?}", hole.value);
    };
    assert_eq!(entries.len(), 1);
}

#[test]
fn a_content_literal_is_a_value() {
    let hole = element("?alt(doc, list([adr-?int(n)], [rfc-?int(n)]))");
    let Some(Value::Map(entries)) = &hole.args else {
        panic!("expected a map, got {:?}", hole.args);
    };
    assert_eq!(entries[0].1, string("doc"));
    let Value::Seq(branches) = &entries[1].1 else {
        panic!("expected list(...), got {:?}", entries[1].1);
    };
    assert_eq!(branches.len(), 2);
    assert!(branches.iter().all(|b| matches!(b, Value::Blocks(_))));
}

#[test]
fn a_content_literal_is_a_value_in_positional_position_too() {
    let hole = element("?many(items, [ab])");
    let Some(Value::Map(entries)) = &hole.args else {
        panic!("expected a map, got {:?}", hole.args);
    };
    assert_eq!(entries[0].1, string("items"));
    assert!(matches!(entries[1].1, Value::Blocks(_)));
}

#[test]
fn a_matcher_may_be_namespaced() {
    let hole = element("?tandr.heading(h)");
    assert_eq!(
        hole.sigil,
        Sigil::Question(Some(Name {
            namespace: Some("tandr".into()),
            name: "heading".into(),
        }))
    );
}

#[test]
fn a_hole_sits_inside_an_element_pattern() {
    let link = element("@link(url: ?(u))[ ?(text) ]");
    assert_eq!(link.sigil, Sigil::named("link"));
    let Some(Value::Map(args)) = &link.args else {
        panic!("expected args, got {:?}", link.args);
    };
    assert_eq!(args[0].0, "url");
    let Value::Element(url) = &args[0].1 else {
        panic!("expected a hole, got {:?}", args[0].1);
    };
    assert_eq!(url.sigil, Sigil::Question(None));
    let content = link.content.expect("content");
    assert_eq!(shape(first_para(&content)), "?");
}

#[test]
fn a_hole_needs_its_parens() {
    for src in [
        "?int and ?name",
        "?int{min: 1}",
        "?int[x]",
        "really? (yes)",
        "what?",
    ] {
        assert_eq!(
            shape(&inlines(src)),
            format!("{src:?}"),
            "`{src}` is text, not a hole"
        );
    }
}

#[test]
fn a_document_never_reads_a_hole() {
    for src in ["?(x)", "adr-?int(n)", "本当に?(はい)", "?()"] {
        let doc = parse_document(src).unwrap();
        let [Block::Paragraph(p)] = doc.blocks.as_slice() else {
            panic!("`{src}` should be one paragraph, got {:?}", doc.blocks);
        };
        assert_eq!(p.content.len(), 1, "`{src}` is one run of text");
        assert!(matches!(&p.content[0], Inline::Text(t) if t.value == src));
    }
}

#[test]
fn a_question_sigil_has_a_matcher_and_no_element_name() {
    let int = matcher("int");
    assert_eq!(int.name(), None);
    assert_eq!(int.matcher(), Some(&Name::bare("int")));
    assert!(int.is_question());
    // `?meta()` is not the `meta` element.
    assert!(!matcher("meta").is_bare_named("meta"));
    assert_eq!(Sigil::Question(None).matcher(), None);
    assert_eq!(Sigil::named("int").matcher(), None);
}

/// How many holes `parse_pattern` finds anywhere in `src`.
fn holes_in(src: &str) -> usize {
    let doc = parse_pattern(src).unwrap();
    let mut count = 0;
    tomet_tree::for_each_element(&doc, |el| {
        if el.sigil.is_question() {
            count += 1;
        }
    });
    count
}

#[test]
fn a_hole_is_found_wherever_content_goes() {
    for (src, expected) in [
        ("- ?int(n)", 1),
        ("- first\n- ?(second)", 1),
        ("=[ ?(title) ]", 1),
        ("@card[\n  ?(body)\n]", 1),
        ("@card[\n  line ?(a)\n\n  line ?(b)\n]", 2),
        ("@callout\n| ?(a) text\n| ?(b)", 2),
        ("@ns.thing(a: [?(x)], b: ?(y))", 2),
        ("@meta{ type: ?(kind) }", 1),
        ("*emphasis ?(x)*", 1),
    ] {
        assert_eq!(holes_in(src), expected, "holes in `{src}`");
    }
}

#[test]
fn a_document_has_no_hole_wherever_content_goes() {
    for src in [
        "- ?int(n)",
        "=[ ?(title) ]",
        "@card[\n  ?(body)\n]",
        "@ns.thing(a: [?(x)])",
        "*emphasis ?(x)*",
    ] {
        let doc = parse_document(src).unwrap();
        let mut count = 0;
        tomet_tree::for_each_element(&doc, |el| {
            if el.sigil.is_question() {
                count += 1;
            }
        });
        assert_eq!(count, 0, "`{src}` read as a document has a hole");
    }
}

#[test]
fn a_quoted_argument_is_never_formatting() {
    // A name the content grammar would read as formatting (`__init__.py` is
    // `strong(init)` + `.py`) is written as a quoted argument instead.
    let hole = element(r#"?string("__init__.py")"#);
    assert_eq!(hole.sigil, matcher("string"));
    assert_eq!(hole.args, Some(string("__init__.py")));

    let hole = element(r#"?string("_private_.rs")"#);
    assert_eq!(hole.args, Some(string("_private_.rs")));
}
