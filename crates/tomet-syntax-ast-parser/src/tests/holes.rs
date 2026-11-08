//! Holes (`<?matcher(name, args){options}>[content]`), read by `parse_file` like
//! any other syntax. See `tmtroot/spec/syntax/hole.tmt`.

use super::*;
use tomet_ast::{Hole, Name};

/// The one paragraph's inline run of a document.
fn inlines(src: &str) -> Vec<Inline> {
    let doc = parse_file(src).unwrap();
    match <[Block; 1]>::try_from(doc.body).expect("one block") {
        [Block::Paragraph(p)] => p.content,
        [other] => panic!("expected one paragraph, got {other:?}"),
    }
}

/// The one block-level hole of a document.
fn hole(src: &str) -> Hole {
    let doc = parse_file(src).unwrap();
    match <[Block; 1]>::try_from(doc.body).expect("one block") {
        [Block::Hole(hole)] => hole,
        [other] => panic!("expected one hole, got {other:?}"),
    }
}

/// The one block element of a document.
fn element(src: &str) -> Element {
    let doc = parse_file(src).unwrap();
    match <[Block; 1]>::try_from(doc.body).expect("one block") {
        [Block::Element(el)] => el,
        [other] => panic!("expected one element, got {other:?}"),
    }
}

/// A run of text and holes as a short string: `"adr-" <?int> "-" <?> ".md"`.
fn shape(items: &[Inline]) -> String {
    let parts: Vec<String> = items
        .iter()
        .map(|item| match item {
            Inline::Text(t) => format!("{:?}", t.value),
            Inline::Hole(hole) => match &hole.matcher {
                Some(m) => format!("<?{m}>"),
                None => "<?>".to_string(),
            },
            other => panic!("unexpected inline {other:?}"),
        })
        .collect();
    parts.join(" ")
}

fn matcher(name: &str) -> Option<Name> {
    Some(Name::bare(name))
}

fn string(s: &str) -> Value {
    Value::String(s.to_string())
}

#[test]
fn holes_sit_in_running_text() {
    let items = inlines("adr-<?int(n)>-<?(slug)>.md");
    assert_eq!(shape(&items), r#""adr-" <?int> "-" <?> ".md""#);
}

#[test]
fn a_single_argument_is_the_value_itself() {
    let Inline::Hole(hole) = &inlines("a<?int(n)>")[1] else {
        panic!("expected a hole");
    };
    assert_eq!(hole.matcher, matcher("int"));
    assert_eq!(hole.data.args, Some(string("n")));
}

#[test]
fn an_empty_group_is_an_empty_map() {
    let hole = hole("<?()>");
    assert_eq!(hole.matcher, None);
    assert_eq!(hole.data.args, Some(Value::Map(Vec::new())));
    assert_eq!(hole.placement, Placement::Block);
}

#[test]
fn the_parens_may_be_left_out() {
    // The box ends the hole, so a matcher needs no `( )` to be told from prose.
    let named = hole("<?int>");
    assert_eq!(named.matcher, matcher("int"));
    assert_eq!(named.data.args, None);

    let bare = hole("<?>");
    assert_eq!(bare.matcher, None);
    assert_eq!(bare.data.args, None);
}

#[test]
fn the_matcher_may_be_left_out() {
    let hole = hole("<?(n)>");
    assert_eq!(hole.matcher, None);
    assert_eq!(hole.data.args, Some(string("n")));
}

#[test]
fn content_is_a_sub_pattern() {
    let hole = hole("<?(id)>[adr-<?int(n)>-<?(slug)>]");
    assert_eq!(hole.matcher, None);
    assert_eq!(hole.data.args, Some(string("id")));
    let content = hole.content.expect("content");
    assert_eq!(shape(first_para(&content)), r#""adr-" <?int> "-" <?>"#);
}

#[test]
fn several_arguments_are_unkeyed_entries() {
    let hole = hole(r#"<?int(n, "x")>"#);
    assert_eq!(hole.matcher, matcher("int"));
    assert_eq!(
        hole.data.args,
        Some(Value::Map(vec![
            (String::new(), string("n")),
            (String::new(), string("x")),
        ]))
    );
}

#[test]
fn options_are_the_value_group() {
    let hole = hole("<?int(n){min: 1}>");
    let Some(Group(entries)) = &hole.data.value else {
        panic!("expected a value group, got {:?}", hole.data.value);
    };
    assert_eq!(entries.len(), 1);
}

#[test]
fn a_content_literal_is_a_value() {
    let hole = hole("<?alt(doc, ([adr-<?int(n)>], [rfc-<?int(n)>]))>");
    let Some(Value::Map(entries)) = &hole.data.args else {
        panic!("expected a map, got {:?}", hole.data.args);
    };
    assert_eq!(entries[0].1, string("doc"));
    let Value::Seq(branches) = &entries[1].1 else {
        panic!("expected a sequence, got {:?}", entries[1].1);
    };
    assert_eq!(branches.len(), 2);
    assert!(branches.iter().all(|b| matches!(b, Value::Blocks(_))));
}

#[test]
fn a_content_literal_is_a_value_in_positional_position_too() {
    let hole = hole("<?many(items, [ab])>");
    let Some(Value::Map(entries)) = &hole.data.args else {
        panic!("expected a map, got {:?}", hole.data.args);
    };
    assert_eq!(entries[0].1, string("items"));
    assert!(matches!(entries[1].1, Value::Blocks(_)));
}

#[test]
fn a_matcher_may_be_namespaced() {
    let hole = hole("<?tandr.heading(h)>");
    assert_eq!(
        hole.matcher,
        Some(Name {
            namespace: Some("tandr".into()),
            name: "heading".into(),
        })
    );
}

#[test]
fn a_hole_sits_inside_an_element() {
    let link = element("@link(url: <?(u)>)[ <?(text)> ]");
    assert_eq!(link.name, Name::bare("link"));
    let Some(Value::Map(args)) = &link.data.args else {
        panic!("expected args, got {:?}", link.data.args);
    };
    assert_eq!(args[0].0, "url");
    let Value::Hole(url) = &args[0].1 else {
        panic!("expected a hole, got {:?}", args[0].1);
    };
    assert_eq!(url.matcher, None);
    let content = link.content.expect("content");
    assert_eq!(shape(first_para(&content)), "<?>");
}

#[test]
fn a_box_that_does_not_close_is_text() {
    for src in [
        "<?int and <?name",
        "<?int",
        "<?int(n",
        "<? int>",
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
fn the_old_spelling_is_text() {
    // `?name(...)` is not a spelling; a `?` is an ordinary character.
    for src in ["?(x)", "adr-?int(n)", "本当に?(はい)", "?()", "?int(n)"] {
        let doc = parse_file(src).unwrap();
        let [Block::Paragraph(p)] = doc.body.as_slice() else {
            panic!("`{src}` should be one paragraph, got {:?}", doc.body);
        };
        assert_eq!(p.content.len(), 1, "`{src}` is one run of text");
        assert!(matches!(&p.content[0], Inline::Text(t) if t.value == src));
    }
}

#[test]
fn a_head_may_span_lines_like_an_elements_groups() {
    let hole = hole("<?alt(\n  a,\n  b\n){\n  min: 1\n}>");
    assert_eq!(hole.matcher, matcher("alt"));
    assert!(matches!(&hole.data.args, Some(Value::Map(e)) if e.len() == 2));
    assert!(hole.data.value.is_some());
}

#[test]
fn a_hole_takes_no_id() {
    // `#(id)` after a hole is the text it looks like; the hole ends at its `>`.
    assert_eq!(shape(&inlines("<?(n)>#(x)")), r##"<?> "#(x)""##);
}

#[test]
fn a_matcher_is_not_an_element_name() {
    // `<?meta>` is a hole with the matcher `meta`; it is not the `meta` element, and
    // is not reached by the walkers that visit elements.
    let doc = parse_file("<?meta>").unwrap();
    let mut elements = 0;
    tomet_tree::for_each_element(&doc, |_| elements += 1);
    assert_eq!(elements, 0);
    assert_eq!(hole("<?meta>").matcher, matcher("meta"));
}

/// How many holes `parse_file` finds anywhere in `src`.
fn holes_in(src: &str) -> usize {
    let doc = parse_file(src).unwrap();
    let mut count = 0;
    tomet_tree::for_each_hole(&doc, |_| count += 1);
    count
}

#[test]
fn a_hole_is_found_wherever_content_goes() {
    for (src, expected) in [
        ("- <?int(n)>", 1),
        ("- first\n- <?(second)>", 1),
        ("=[ <?(title)> ]", 1),
        ("@card[\n  <?(body)>\n]", 1),
        ("@card[\n  line <?(a)>\n\n  line <?(b)>\n]", 2),
        ("@callout\n| <?(a)> text\n| <?(b)>", 2),
        ("@ns.thing(a: [<?(x)>], b: <?(y)>)", 2),
        ("@meta(type: <?(kind)>)", 1),
        ("*emphasis <?(x)>*", 1),
    ] {
        assert_eq!(holes_in(src), expected, "holes in `{src}`");
    }
}

#[test]
fn a_quoted_argument_is_never_formatting() {
    // A name the content grammar would read as formatting (`__init__.py` is
    // `strong(init)` + `.py`) is written as a quoted argument instead.
    let first = hole(r#"<?string("__init__.py")>"#);
    assert_eq!(first.matcher, matcher("string"));
    assert_eq!(first.data.args, Some(string("__init__.py")));

    let second = hole(r#"<?string("_private_.rs")>"#);
    assert_eq!(second.data.args, Some(string("_private_.rs")));
}
