use super::*;

#[test]
fn parses_caret_reference_elements() {
    let doc = parse_document("See ^(note1) and ^footnote(note2).\n").unwrap();
    let Block::Paragraph(p) = &doc.blocks[0] else {
        panic!()
    };
    // "See ", Element(^(note1)), " and ", Element(^footnote(note2)), "."
    assert_eq!(p.content.len(), 5);
    assert_eq!(p.content[0], Inline::Text("See ".into()));

    let Inline::Element(caret1) = &p.content[1] else {
        panic!()
    };
    assert_eq!(caret1.sigil, Sigil::Caret(None));
    assert!(caret1.args.is_some());

    assert_eq!(p.content[2], Inline::Text(" and ".into()));

    let Inline::Element(caret2) = &p.content[3] else {
        panic!()
    };
    assert_eq!(
        caret2.sigil,
        Sigil::Caret(Some(tomet_ast::Name::bare("footnote")))
    );
    assert!(caret2.args.is_some());

    assert_eq!(p.content[4], Inline::Text(".".into()));
}

#[test]
fn caret_without_parens_is_plain_text() {
    let doc = parse_document("Math x^2 and bare ^word here.\n").unwrap();
    let Block::Paragraph(p) = &doc.blocks[0] else {
        panic!()
    };
    assert_eq!(
        p.content,
        vec![Inline::Text("Math x^2 and bare ^word here.".into())]
    );
}
