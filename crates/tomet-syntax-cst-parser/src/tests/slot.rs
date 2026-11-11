//! The box in the tree: which texts are a type seat (`<$type(args)>`), where it closes, and
//! what it takes. The hole and the name tag are the ast-parser's `slots` and `holes` tests'.

use tomet_cst::{AstNode, CstSlot, CstSlotMark, SyntaxKind as K, SyntaxNode};

use crate::parse_cst;

/// The box nodes of `src`, in document order. Also checks the tree is lossless, which every
/// case here depends on.
fn slots(src: &str) -> Vec<SyntaxNode> {
    let root = parse_cst(src);
    assert_eq!(root.text().to_string(), src, "lossless: {src:?}");
    root.descendants().filter(|n| n.kind() == K::SLOT).collect()
}

/// The text of the type seats of `src`.
fn seats(src: &str) -> Vec<String> {
    slots(src)
        .into_iter()
        .filter(|n| CstSlot::cast(n.clone()).unwrap().mark() == CstSlotMark::TypeSeat)
        .map(|n| n.text().to_string())
        .collect()
}

/// The kinds of the tokens and nodes directly in `node`, in order.
fn parts(node: &SyntaxNode) -> Vec<K> {
    node.children_with_tokens().map(|e| e.kind()).collect()
}

#[test]
fn a_type_seat_is_the_mark_a_name_and_optional_arguments() {
    let cases = [
        ("<$uuid>", vec![K::LT, K::DOLLAR, K::NAME, K::GT]),
        ("<$uuid()>", vec![K::LT, K::DOLLAR, K::NAME, K::ARGS, K::GT]),
        (
            "<$date(default: auto)>",
            vec![K::LT, K::DOLLAR, K::NAME, K::ARGS, K::GT],
        ),
        (
            "<$list(string, length: 6)>",
            vec![K::LT, K::DOLLAR, K::NAME, K::ARGS, K::GT],
        ),
        (
            r#"<$nanoid(length: 8, prefix: "note-", default: auto)>"#,
            vec![K::LT, K::DOLLAR, K::NAME, K::ARGS, K::GT],
        ),
        // A name has digits, `_` and `-`, and starts with a letter or `_`.
        ("<$_a-b_2>", vec![K::LT, K::DOLLAR, K::NAME, K::GT]),
    ];
    for (src, expected) in cases {
        let found = slots(src);
        assert_eq!(found.len(), 1, "one box in {src:?}");
        assert_eq!(found[0].text().to_string(), src);
        assert_eq!(parts(&found[0]), expected, "{src:?}");
        assert_eq!(
            CstSlot::cast(found[0].clone()).unwrap().mark(),
            CstSlotMark::TypeSeat
        );
    }
}

#[test]
fn the_mark_tells_the_three_boxes_apart() {
    let marks: Vec<CstSlotMark> = slots("<PATH> <?int> <$int>")
        .into_iter()
        .map(|n| CstSlot::cast(n).unwrap().mark())
        .collect();
    assert_eq!(
        marks,
        [
            CstSlotMark::NameTag,
            CstSlotMark::Hole,
            CstSlotMark::TypeSeat
        ]
    );
}

#[test]
fn a_type_seat_is_a_form_of_the_tree_and_a_name_tag_is_not() {
    // As a block, alone on its line.
    let root = parse_cst("<$uuid>\n");
    let [form] = root.children().collect::<Vec<_>>().try_into().unwrap();
    assert_eq!(form.kind(), K::BLOCK_FORM);
    assert_eq!(
        form.children().map(|n| n.kind()).collect::<Vec<_>>(),
        [K::SLOT]
    );

    // In running text.
    let root = parse_cst("id <$uuid> and <PATH>\n");
    let para = root
        .descendants()
        .find(|n| n.kind() == K::PARAGRAPH)
        .unwrap();
    let kinds: Vec<K> = para.children().map(|n| n.kind()).collect();
    assert_eq!(kinds, [K::INLINE_FORM, K::SLOT], "a name tag stands bare");

    // As a value.
    let root = parse_cst("@a(<$date>)\n");
    let args = root.descendants().find(|n| n.kind() == K::ARGS).unwrap();
    let item = args.children().next().unwrap();
    assert_eq!(item.kind(), K::SEQ_ITEM);
    let form = item.children().next().unwrap();
    assert_eq!(form.kind(), K::INLINE_FORM);
    assert_eq!(seats("@a(k: <$uuid>)\n"), ["<$uuid>"]);
}

#[test]
fn a_type_seat_closes_at_the_first_closer_outside_parentheses_and_strings() {
    for src in [
        "<$list(string, length: 6)>",
        "<$list(list(string), length: 6)>",
        // A `>` inside the arguments closes nothing, nested or in a string.
        "<$x(a > b)>",
        "<$x((a > b), c)>",
        r#"<$nanoid(prefix: ">")>"#,
        r#"<$nanoid(prefix: ")>")>"#,
        // A `)` or a `(` in a string neither closes nor opens.
        r#"<$x(")")>"#,
        r#"<$x("(")>"#,
        // The arguments may run over lines.
        "<$list(\n  string,\n  length: 6\n)>",
    ] {
        assert_eq!(seats(src), [src], "one seat in {src:?}");
    }
    // The first `>` outside them ends the seat: the rest is text.
    assert_eq!(seats("<$x>y>"), ["<$x>"]);
    assert_eq!(seats("<$x(a)> > <$y>"), ["<$x(a)>", "<$y>"]);
}

#[test]
fn what_is_not_a_type_seat_is_no_box() {
    for src in [
        // No name, or not a name.
        "<$>",
        "<$ x>",
        "<$5>",
        "<$-x>",
        "<$(x)>",
        "<$é>",
        "<$x.y>",
        "<$x::y>",
        // Not tight, or not closed.
        "<$x >",
        "<$x",
        "<$x\n>",
        "<$x(a) >",
        // A seat takes no `{value}`, no `[content]`, no `#(id)`, and one `(args)`.
        "<$x{a: 1}>",
        "<$x[t]>",
        "<$x#(id)>",
        "<$x(a)(b)>",
        "<$x(a){b}>",
        "a <$ b",
        // Escaped.
        "\\<$x>",
    ] {
        assert_eq!(seats(src), Vec::<String>::new(), "no seat in {src:?}");
        assert_eq!(slots(src).len(), 0, "no box at all in {src:?}");
    }
}

/// A `(` that does not close is no seat, and not text either: `$x(` is read as the head of an
/// interpolation that never closes, which `parse_file` refuses (`tomet-ast-parser`'s tests).
#[test]
fn an_unclosed_paren_is_no_seat_and_the_rest_reads_as_an_interpolation() {
    for src in ["<$x(a>", "<$x(", "<$x(\"a>", "x <$x(a> y"] {
        assert_eq!(slots(src).len(), 0, "no box in {src:?}");
        let root = parse_cst(src);
        // The `<` is text; what follows it is a form, an interpolation head with its `(` open.
        let form = root
            .descendants()
            .find(|n| n.kind() == K::INLINE_FORM)
            .unwrap_or_else(|| panic!("an interpolation form in {src:?}"));
        assert_eq!(form.text().to_string().chars().next(), Some('$'), "{src:?}");
        let args = form
            .children()
            .find(|n| n.kind() == K::ARGS)
            .unwrap_or_else(|| panic!("an argument group in {src:?}"));
        assert!(
            args.children_with_tokens().all(|e| e.kind() != K::R_PAREN),
            "the `(` of {src:?} is never closed"
        );
    }
}

#[test]
fn a_type_seat_takes_no_content() {
    // A hole takes the `[content]` after its `>`; a seat's `>` ends it.
    let found = slots("<?x>[t]");
    assert_eq!(found[0].text().to_string(), "<?x>[t]");

    for src in ["<$x>[t]\n", "a <$x>[t]\n"] {
        assert_eq!(seats(src), ["<$x>"], "{src:?}");
        let root = parse_cst(src);
        assert!(
            root.descendants().all(|n| n.kind() != K::CONTENT),
            "no content in {src:?}"
        );
    }
    // Alone on its line with text after it, the seat starts a paragraph.
    let root = parse_cst("<$x>[t]\n");
    assert_eq!(
        root.children().map(|n| n.kind()).collect::<Vec<_>>(),
        [K::PARAGRAPH]
    );
}

#[test]
fn the_arguments_of_a_type_seat_are_structured_entries() {
    // The `(args)` is read as arguments, so the entries are structured.
    let found = slots("<$nanoid(length: 8, prefix: \"note-\")>");
    let args = found[0].children().find(|n| n.kind() == K::ARGS).unwrap();
    let entries: Vec<String> = args
        .children()
        .filter(|n| n.kind() == K::MAP_ENTRY)
        .map(|n| n.text().to_string())
        .collect();
    assert_eq!(entries, ["length: 8", "prefix: \"note-\""]);
}
