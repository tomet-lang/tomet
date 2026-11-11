//! The `<…>` box in the CST: which texts are one, and what a name tag lowers to. (A hole is
//! `holes`'s, and a type seat `type_seats`'s.)
//! See `tmtroot/spec/syntax/slot.tmt`.

use super::*;
use tomet_cst::SyntaxKind as K;

/// The text of every box of `src`, in document order (an outer one first). Also
/// checks the tree is lossless, which every case here depends on.
fn boxes(src: &str) -> Vec<String> {
    let root = tomet_cst_parser::parse_cst(src);
    assert_eq!(root.text().to_string(), src, "lossless: {src:?}");
    root.descendants()
        .filter(|n| n.kind() == K::SLOT)
        .map(|n| n.text().to_string())
        .collect()
}

#[test]
fn a_name_tag_is_a_box() {
    for (src, tag) in [
        ("--out <PATH>", "<PATH>"),
        ("<ファイル>を開く", "<ファイル>"),
        ("<config-vault>", "<config-vault>"),
        ("<a_b>", "<a_b>"),
        // Right after a word too.
        ("Vec<T>", "<T>"),
        ("x<y>z", "<y>"),
    ] {
        assert_eq!(boxes(src), [tag], "boxes in `{src}`");
    }
}

#[test]
fn a_marked_box_is_a_box() {
    assert_eq!(
        boxes("adr-<?int(n)>-<?(slug)>.md"),
        ["<?int(n)>", "<?(slug)>"]
    );
    assert_eq!(boxes("<?>"), ["<?>"]);
    assert_eq!(boxes("<?int>"), ["<?int>"]);
    assert_eq!(boxes("<?int(n){min: 1}>"), ["<?int(n){min: 1}>"]);
    assert_eq!(boxes("<?tandr.heading(h)>"), ["<?tandr.heading(h)>"]);
}

#[test]
fn a_type_seat_is_a_box() {
    assert_eq!(boxes("<$uuid>"), ["<$uuid>"]);
    assert_eq!(boxes("<$uuid()>"), ["<$uuid()>"]);
    assert_eq!(
        boxes("id <$nanoid(length: 8)> at <$date(default: auto)>"),
        ["<$nanoid(length: 8)>", "<$date(default: auto)>"]
    );
}

#[test]
fn a_type_seat_that_does_not_fit_is_text() {
    for src in [
        "<$>",
        "<$ x>",
        "<$5>",
        "<$x >",
        "<$x",
        "<$x.y>",
        "<$x{a: 1}>",
        "<$x[t]>",
        "<$x(a)(b)>",
    ] {
        assert_eq!(boxes(src), Vec::<String>::new(), "`{src}` has no box");
        assert!(parse_file(src).is_ok(), "`{src}` is never an error");
    }
}

#[test]
fn a_shape_that_does_not_fit_is_text() {
    for src in [
        "a < b",
        "x<=1",
        "1<2",
        "<->",
        "<-x>",
        "<>",
        "<3>",
        "<a b>",
        "a<b と c>d",
        "x<y&&z>w",
        "<",
        ">",
        "a > b",
        // An unclosed or loose head.
        "<?int",
        "<?int(n",
        "<?int and <?name",
        "<? int>",
        "<?int (n)>",
    ] {
        assert_eq!(boxes(src), Vec::<String>::new(), "`{src}` has no box");
        assert!(parse_file(src).is_ok(), "`{src}` is never an error");
    }
}

#[test]
fn html_shaped_text_is_not_a_box() {
    // `!` and `/` are never marks, so these stay text and raise nothing.
    for src in ["<!doctype html>", "<!-- x -->", "</x>", "<x/>", "<br/>"] {
        assert_eq!(boxes(src), Vec::<String>::new(), "`{src}` has no box");
        assert!(parse_file(src).is_ok(), "`{src}` is never an error");
    }
}

#[test]
fn only_a_hole_takes_content() {
    assert_eq!(boxes("<PATH>[x]"), ["<PATH>"], "a tag, then text");
    assert_eq!(boxes("<$x>[x]"), ["<$x>"], "a seat, then text");
    assert_eq!(boxes("<?>[x]"), ["<?>[x]"]);
    assert_eq!(
        boxes("<?(id)>[adr-<?int(n)>]"),
        ["<?(id)>[adr-<?int(n)>]", "<?int(n)>"]
    );
}

#[test]
fn a_closer_inside_a_group_or_a_string_closes_nothing() {
    for src in [
        r#"<?regex("a>b")>"#,
        r#"<$nanoid(prefix: ">")>"#,
        "<$x(a > b)>",
        "<?alt(a, ([x>y]))>",
        "<?(n){min: 1}>",
        "<?(n){gt: 1 > 0}>",
    ] {
        assert_eq!(boxes(src), [src], "one box in `{src}`");
    }
}

#[test]
fn a_tag_lowers_to_text() {
    // Tomet gives a name tag no meaning, so the AST has no node for it.
    let src = "--out <PATH> と <ファイル>";
    let doc = parse_file(src).unwrap();
    let [Block::Paragraph(p)] = doc.body.as_slice() else {
        panic!("one paragraph, got {:?}", doc.body);
    };
    assert_eq!(p.content.len(), 1, "one run of text");
    assert!(matches!(&p.content[0], Inline::Text(t) if t.value == src));
}

#[test]
fn a_tag_in_a_value_is_its_text() {
    let doc = parse_file("@opt(<name>)").unwrap();
    let [Block::Element(el)] = doc.body.as_slice() else {
        panic!("one element, got {:?}", doc.body);
    };
    assert_eq!(el.data.args, Some(Value::String("<name>".into())));

    let doc = parse_file("@opt(key: <name>)").unwrap();
    let [Block::Element(el)] = doc.body.as_slice() else {
        panic!("one element, got {:?}", doc.body);
    };
    assert_eq!(
        el.data.args,
        Some(Value::Map(vec![(
            "key".into(),
            Value::String("<name>".into())
        )]))
    );
}
