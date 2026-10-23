use super::*;

fn element(block: &Block) -> &Element {
    let Block::Element(el) = block else {
        panic!("expected an element block, got {block:?}")
    };
    el
}

#[test]
fn a_double_colon_line_is_a_document_declaration() {
    let doc =
        parse_document("::kind(config, 2)\n::meta{ id: x, title: T }\n\n=[ Title ]\n\nbody\n")
            .unwrap();
    let kind = element(&doc.blocks[0]);
    assert_eq!(kind.sigil, Sigil::document("kind"));
    assert_eq!(kind.placement, Placement::Block);
    assert!(kind.args.is_some());
    let meta = element(&doc.blocks[1]);
    assert_eq!(meta.sigil, Sigil::document("meta"));
    assert_eq!(meta.placement, Placement::Block);
    assert!(meta.value.is_some());
    assert!(matches!(doc.blocks[2], Block::Section(_)));
}

#[test]
fn a_declaration_takes_the_same_groups_as_an_element() {
    let doc = parse_document("::x(a)[ b ]{ c: 1 }#(i):rule(allow:list(k))\n").unwrap();
    let el = element(&doc.blocks[0]);
    assert_eq!(el.sigil, Sigil::document("x"));
    assert!(el.args.is_some());
    assert!(el.content.is_some());
    assert!(el.value.is_some());
    assert!(el.id.is_some());
    assert_eq!(el.connects.len(), 1);
    assert_eq!(el.connects[0].sigil, Sigil::named("rule"));
}

#[test]
fn the_next_double_colon_line_is_not_a_connect_of_the_one_before() {
    let doc = parse_document("::kind(config)\n::meta{ id: x }\n").unwrap();
    assert_eq!(doc.blocks.len(), 2);
    assert!(element(&doc.blocks[0]).connects.is_empty());
}

#[test]
fn a_single_colon_line_after_one_is_still_a_connect() {
    let doc = parse_document("::kind(config)\n:rule(allow:list(k))\n").unwrap();
    let el = element(&doc.blocks[0]);
    assert_eq!(el.connects.len(), 1);
}

#[test]
fn the_declaration_does_not_depend_on_its_name() {
    // the parser is vocabulary-free: any name is read, and a later pass judges it
    let doc = parse_document("::character(birth: 1990)\n").unwrap();
    assert_eq!(element(&doc.blocks[0]).sigil, Sigil::document("character"));
}

#[test]
fn a_double_colon_that_starts_no_declaration_stays_text() {
    for src in [
        "std::vec is a type\n",
        "text ::kind(x) in the middle\n",
        "::\n",
        "::(x)\n",
        "::1abc(x)\n",
        ":::kind(x)\n",
        "a::kind(x)\n",
    ] {
        let doc = parse_document(src).unwrap();
        assert!(
            doc.blocks
                .iter()
                .all(|b| !matches!(b, Block::Element(el) if el.sigil.is_document())),
            "{src:?} -> {doc:?}"
        );
    }
}

#[test]
fn a_declaration_inside_a_code_fence_is_code() {
    let doc = parse_document("```tmt\n::kind(config)\n```\n").unwrap();
    assert_eq!(doc.blocks.len(), 1);
    assert!(!element(&doc.blocks[0]).sigil.is_document());
}

/// The parser knows no vocabulary, so `@kind` is an element like any other
/// to it. That a declaration must be written `::kind` is validation's call
/// (`WrongSigil`), not the grammar's.
#[test]
fn the_parser_does_not_refuse_a_declaration_name_after_an_at_sign() {
    let doc = parse_document("@kind(config)\n@meta{ id: x }\n").unwrap();
    assert_eq!(element(&doc.blocks[0]).sigil, Sigil::named("kind"));
    assert_eq!(element(&doc.blocks[1]).sigil, Sigil::named("meta"));
}

#[test]
fn the_span_covers_the_double_colon() {
    let src = "::kind(config)\n";
    let doc = parse_document(src).unwrap();
    let el = element(&doc.blocks[0]);
    assert_eq!(&src[el.span.start..el.span.end], "::kind(config)");
}

#[test]
fn a_broken_declaration_is_an_error_and_not_a_paragraph() {
    for src in [
        "::kind(config\n",
        "::meta{ a: 1, a: 2 }\n",
        "::kind(config) trailing\n",
    ] {
        assert!(parse_document(src).is_err(), "{src:?}");
    }
}

#[test]
fn a_trailing_comment_may_follow_a_declaration() {
    let doc = parse_document("::kind(config) // the kind\n").unwrap();
    assert!(element(&doc.blocks[0]).sigil.is_document());
}
