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
        parse_file("::kind(config, 2)\n::meta{ id: x, title: T }\n\n=[ Title ]\n\nbody\n").unwrap();
    assert_eq!(doc.header.len(), 2);
    let kind = &doc.header[0];
    assert_eq!(kind.name, Name::bare("kind"));
    assert!(kind.data.args.is_some());
    let meta = &doc.header[1];
    assert_eq!(meta.name, Name::bare("meta"));
    assert!(meta.data.value.is_some());
    assert_eq!(doc.body.len(), 1);
    assert!(matches!(doc.body[0], Block::Section(_)));
}

#[test]
fn a_declaration_takes_the_same_groups_as_an_element() {
    let doc = parse_file("::x(a)[ b ]{ c: 1 }#(i)\n").unwrap();
    assert_eq!(doc.header.len(), 1);
    let d = &doc.header[0];
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
        let err = parse_file(src).unwrap_err();
        assert!(
            err.message.contains("takes no `:connect`"),
            "{src:?}: {}",
            err.message
        );
    }
}

#[test]
fn the_next_double_colon_line_is_not_a_connect_of_the_one_before() {
    let doc = parse_file("::kind(config)\n::meta{ id: x }\n").unwrap();
    assert_eq!(doc.header.len(), 2);
    assert!(doc.body.is_empty());
}

#[test]
fn the_declaration_does_not_depend_on_its_name() {
    // the parser is vocabulary-free: any name is read, and a later pass judges it
    let doc = parse_file("::character(birth: 1990)\n").unwrap();
    assert_eq!(doc.header[0].name, Name::bare("character"));
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
        let doc = parse_file(src).unwrap();
        assert!(doc.header.is_empty(), "{src:?} -> {doc:?}");
    }
}

#[test]
fn a_declaration_inside_a_code_fence_is_code() {
    let doc = parse_file("```tmt\n::kind(config)\n```\n").unwrap();
    assert!(doc.header.is_empty());
    assert_eq!(doc.body.len(), 1);
}

/// The parser knows no vocabulary, so `@kind` is an element like any other
/// to it. That a declaration must be written `::kind` is validation's call
/// (`WrongSigil`), not the grammar's.
#[test]
fn the_parser_does_not_refuse_a_declaration_name_after_an_at_sign() {
    let doc = parse_file(";@kind(config)\n;@meta{ id: x }\n").unwrap();
    assert_eq!(element(&doc.body[0]).name, Name::bare("kind"));
    assert_eq!(element(&doc.body[1]).name, Name::bare("meta"));
}

#[test]
fn the_span_covers_the_double_colon() {
    let src = "::kind(config)\n";
    let doc = parse_file(src).unwrap();
    let d = &doc.header[0];
    assert_eq!(&src[d.span.start..d.span.end], "::kind(config)");
}

#[test]
fn a_broken_declaration_is_an_error_and_not_a_paragraph() {
    for src in [
        "::kind(config\n",
        "::meta{ a: 1, a: 2 }\n",
        "::kind(config) trailing\n",
    ] {
        assert!(parse_file(src).is_err(), "{src:?}");
    }
}

#[test]
fn a_trailing_comment_may_follow_a_declaration() {
    let doc = parse_file("::kind(config) // the kind\n").unwrap();
    assert_eq!(doc.header.len(), 1);
}

#[test]
fn a_declaration_with_text_after_it_on_its_line_is_an_error() {
    let err = parse_file("::meta{a: 1} text\n").expect_err("a declaration stands alone");
    assert!(err.message.contains("stands alone"), "{}", err.message);
    // Where the text after the declaration begins.
    assert_eq!(err.offset, 13);
}

#[test]
fn a_double_colon_that_opens_no_declaration_is_not_an_error_either_way() {
    // No group after the name: it is prose, not a declaration with something after it.
    assert!(parse_file("::memo is how a path starts\n").is_ok());
    // Inside a `|` run a line may begin with one.
    assert!(parse_file("@quote\n| ::meta{a: 1} text\n").is_ok());
}
