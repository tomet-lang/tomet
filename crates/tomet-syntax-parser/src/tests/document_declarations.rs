use super::*;

fn element(block: &Block) -> &Element {
    let Block::Element(el) = block else {
        panic!("expected an element block, got {block:?}")
    };
    el
}

fn declaration(block: &Block) -> &Declaration {
    let Block::Declaration(declaration) = block else {
        panic!("expected a declaration block, got {block:?}")
    };
    declaration
}

#[test]
fn a_double_colon_line_is_a_document_declaration() {
    let doc =
        parse_document("::kind(config, 2)\n::meta{ id: x, title: T }\n\n=[ Title ]\n\nbody\n")
            .unwrap();
    let kind = declaration(&doc.blocks[0]);
    assert_eq!(kind.name, Name::bare("kind"));
    assert!(kind.data.args.is_some());
    let meta = declaration(&doc.blocks[1]);
    assert_eq!(meta.name, Name::bare("meta"));
    assert!(meta.data.value.is_some());
    assert!(matches!(doc.blocks[2], Block::Section(_)));
}

#[test]
fn a_declaration_takes_the_same_groups_as_an_element() {
    let doc = parse_document("::x(a)[ b ]{ c: 1 }#(i)\n").unwrap();
    let d = declaration(&doc.blocks[0]);
    assert_eq!(d.name, Name::bare("x"));
    assert!(d.data.args.is_some());
    assert!(d.content.is_some());
    assert!(d.data.value.is_some());
    assert!(d.id.is_some());
}

#[test]
fn a_declaration_takes_no_connect() {
    // A connect is metadata of an element; a declaration is attached to
    // nothing. Written on the same line or on the next, it is refused.
    for src in [
        "::x(a):rule(allow:list(k))\n",
        "::kind(config)\n:rule(allow:list(k))\n",
    ] {
        let err = parse_document(src).unwrap_err();
        assert!(
            err.message.contains("takes no `:connect`"),
            "{src:?}: {}",
            err.message
        );
    }
}

#[test]
fn the_next_double_colon_line_is_not_a_connect_of_the_one_before() {
    let doc = parse_document("::kind(config)\n::meta{ id: x }\n").unwrap();
    assert_eq!(doc.blocks.len(), 2);
    assert!(matches!(doc.blocks[0], Block::Declaration(_)));
    assert!(matches!(doc.blocks[1], Block::Declaration(_)));
}

#[test]
fn the_declaration_does_not_depend_on_its_name() {
    // the parser is vocabulary-free: any name is read, and a later pass judges it
    let doc = parse_document("::character(birth: 1990)\n").unwrap();
    assert_eq!(declaration(&doc.blocks[0]).name, Name::bare("character"));
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
                .all(|b| !matches!(b, Block::Declaration(_))),
            "{src:?} -> {doc:?}"
        );
    }
}

#[test]
fn a_declaration_inside_a_code_fence_is_code() {
    let doc = parse_document("```tmt\n::kind(config)\n```\n").unwrap();
    assert_eq!(doc.blocks.len(), 1);
    assert!(!matches!(doc.blocks[0], Block::Declaration(_)));
}

/// The parser knows no vocabulary, so `@kind` is an element like any other
/// to it. That a declaration must be written `::kind` is validation's call
/// (`WrongSigil`), not the grammar's.
#[test]
fn the_parser_does_not_refuse_a_declaration_name_after_an_at_sign() {
    let doc = parse_document("@kind(config)\n@meta{ id: x }\n").unwrap();
    assert_eq!(element(&doc.blocks[0]).name, Name::bare("kind"));
    assert_eq!(element(&doc.blocks[1]).name, Name::bare("meta"));
}

#[test]
fn the_span_covers_the_double_colon() {
    let src = "::kind(config)\n";
    let doc = parse_document(src).unwrap();
    let d = declaration(&doc.blocks[0]);
    assert_eq!(&src[d.span.start..d.span.end], "::kind(config)");
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
    assert!(matches!(doc.blocks[0], Block::Declaration(_)));
}
