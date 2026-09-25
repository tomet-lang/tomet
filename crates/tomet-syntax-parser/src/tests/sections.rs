use super::*;

#[test]
fn parses_nested_section_tree() {
    let src = "= Intro\nIntro text.\n\n== Background\nBackground text.\n\n=== Details\nDetail text.\n\n== Goals\nGoal text.\n\n= Next Chapter\nNext text.\n";
    let doc = parse_document(src).unwrap();
    // Top level has two sections: "Intro" and "Next Chapter"
    assert_eq!(doc.blocks.len(), 2);

    // First section: Intro
    let Block::Section(intro) = &doc.blocks[0] else {
        panic!("expected intro section");
    };
    assert_eq!(intro.level, 1);
    assert_eq!(intro.title, vec![Inline::Text("Intro".into())]);
    // intro contains: Paragraph("Intro text."), Section("Background"), Section("Goals")
    assert_eq!(intro.blocks.len(), 3);
    assert!(matches!(&intro.blocks[0], Block::Paragraph(_)));

    // Background
    let Block::Section(bg) = &intro.blocks[1] else {
        panic!("expected background section");
    };
    assert_eq!(bg.level, 2);
    assert_eq!(bg.title, vec![Inline::Text("Background".into())]);
    // bg contains: Paragraph("Background text."), Section("Details")
    assert_eq!(bg.blocks.len(), 2);
    let Block::Section(details) = &bg.blocks[1] else {
        panic!("expected details section");
    };
    assert_eq!(details.level, 3);
    assert_eq!(details.title, vec![Inline::Text("Details".into())]);
    assert_eq!(details.blocks.len(), 1);

    // Goals
    let Block::Section(goals) = &intro.blocks[2] else {
        panic!("expected goals section");
    };
    assert_eq!(goals.level, 2);
    assert_eq!(goals.title, vec![Inline::Text("Goals".into())]);
    assert_eq!(goals.blocks.len(), 1);

    // Second section: Next Chapter
    let Block::Section(next) = &doc.blocks[1] else {
        panic!("expected next chapter section");
    };
    assert_eq!(next.level, 1);
    assert_eq!(next.title, vec![Inline::Text("Next Chapter".into())]);
    assert_eq!(next.blocks.len(), 1);
}

#[test]
fn section_sugar_and_bracket_forms_match() {
    let sugar = parse_document("= Hello World\nParagraph\n").unwrap();
    let bracket = parse_document("=[ Hello World ]\nParagraph\n").unwrap();
    assert_eq!(sugar.blocks.len(), 1);
    assert_eq!(bracket.blocks.len(), 1);

    let Block::Section(s_sec) = &sugar.blocks[0] else {
        panic!()
    };
    let Block::Section(b_sec) = &bracket.blocks[0] else {
        panic!()
    };

    assert_eq!(s_sec.level, b_sec.level);
    assert_eq!(s_sec.title, b_sec.title);
    assert_eq!(s_sec.blocks.len(), b_sec.blocks.len());
}

#[test]
fn parses_section_with_decorative_trailing_equals() {
    // Bracket form with trailing equals (no space and with space)
    let doc1 = parse_document("==[ Title ]==\nParagraph\n").unwrap();
    let Block::Section(s1) = &doc1.blocks[0] else {
        panic!()
    };
    assert_eq!(s1.level, 2);
    assert_eq!(s1.title, vec![Inline::Text("Title".into())]);
    assert_eq!(s1.blocks.len(), 1);
    let Block::Paragraph(p1) = &s1.blocks[0] else {
        panic!()
    };
    assert_eq!(p1.content, vec![Inline::Text("Paragraph".into())]);

    let doc2 = parse_document("==[ Title ] ==\nParagraph\n").unwrap();
    let Block::Section(s2) = &doc2.blocks[0] else {
        panic!()
    };
    assert_eq!(s2.title, vec![Inline::Text("Title".into())]);
    assert_eq!(s2.blocks.len(), 1);

    // Bracket form with attrs
    let doc3 = parse_document("==[ Title ]{ id: intro }==\nParagraph\n").unwrap();
    let Block::Section(s3) = &doc3.blocks[0] else {
        panic!()
    };
    assert_eq!(s3.title, vec![Inline::Text("Title".into())]);
    assert!(s3.value.is_some());
    assert_eq!(s3.blocks.len(), 1);

    let doc4 = parse_document("==[ Title ]=={ id: intro }\nParagraph\n").unwrap();
    let Block::Section(s4) = &doc4.blocks[0] else {
        panic!()
    };
    assert_eq!(s4.title, vec![Inline::Text("Title".into())]);
    assert!(s4.value.is_some());
    assert_eq!(s4.blocks.len(), 1);

    // Sugar form with trailing equals
    let doc5 = parse_document("== Title ==\nParagraph\n").unwrap();
    let Block::Section(s5) = &doc5.blocks[0] else {
        panic!()
    };
    assert_eq!(s5.level, 2);
    assert_eq!(s5.title, vec![Inline::Text("Title".into())]);
    assert_eq!(s5.blocks.len(), 1);

    let doc6 = parse_document("== Title ===\nParagraph\n").unwrap();
    let Block::Section(s6) = &doc6.blocks[0] else {
        panic!()
    };
    assert_eq!(s6.title, vec![Inline::Text("Title".into())]);
    assert_eq!(s6.blocks.len(), 1);

    // Heading with internal '='
    let doc7 = parse_document("== Math: 1 + 1 = 2 ==\nParagraph\n").unwrap();
    let Block::Section(s7) = &doc7.blocks[0] else {
        panic!()
    };
    assert_eq!(s7.title, vec![Inline::Text("Math: 1 + 1 = 2".into())]);
    assert_eq!(s7.blocks.len(), 1);

    let doc8 = parse_document("==[ Math: 1 + 1 = 2 ]==\nParagraph\n").unwrap();
    let Block::Section(s8) = &doc8.blocks[0] else {
        panic!()
    };
    assert_eq!(s8.title, vec![Inline::Text("Math: 1 + 1 = 2".into())]);
    assert_eq!(s8.blocks.len(), 1);
}

#[test]
fn parses_tag_sugar_syntax() {
    let doc = parse_document("Prose with #(rust, parser, tomet) tags.\n").unwrap();
    let Block::Paragraph(p) = &doc.blocks[0] else {
        panic!()
    };
    assert_eq!(p.content.len(), 3);
    assert_eq!(p.content[0], Inline::Text("Prose with ".into()));

    let Inline::Element(tag_el) = &p.content[1] else {
        panic!("expected tag element, got {:?}", p.content[1]);
    };
    assert_eq!(tag_el.sigil, Sigil::named("tag"));
    assert_eq!(
        tag_el.args,
        Some(Value::Map(vec![
            ("".into(), Value::String("rust".into())),
            ("".into(), Value::String("parser".into())),
            ("".into(), Value::String("tomet".into())),
        ]))
    );
    assert_eq!(p.content[2], Inline::Text(" tags.".into()));
}

#[test]
fn double_equal_is_plain_text_not_mark() {
    let doc = parse_document("This is ==not highlighted== text.\n").unwrap();
    let Block::Paragraph(p) = &doc.blocks[0] else {
        panic!()
    };
    // Contains no Element, just Text
    assert_eq!(
        p.content,
        vec![Inline::Text("This is ==not highlighted== text.".into())]
    );
}

#[test]
fn old_hash_heading_is_plain_paragraph() {
    let doc = parse_document("# Old Heading\n\n#[ Another Heading ]\n").unwrap();
    assert_eq!(doc.blocks.len(), 2);
    assert!(matches!(&doc.blocks[0], Block::Paragraph(_)));
    assert!(matches!(&doc.blocks[1], Block::Paragraph(_)));
}

#[test]
fn parses_headingless_section_syntax() {
    let doc = parse_document("=[ heading ]\n\nsection\n\n=\n\nafter\n").unwrap();
    assert_eq!(doc.blocks.len(), 2);

    // First section: =[ heading ]
    let Block::Section(s1) = &doc.blocks[0] else {
        panic!()
    };
    assert_eq!(s1.level, 1);
    assert_eq!(s1.title, vec![Inline::Text("heading".into())]);
    assert_eq!(s1.blocks.len(), 1);
    let Block::Paragraph(p1) = &s1.blocks[0] else {
        panic!()
    };
    assert_eq!(p1.content, vec![Inline::Text("section".into())]);

    // Second section: bare '='
    let Block::Section(s2) = &doc.blocks[1] else {
        panic!()
    };
    assert_eq!(s2.level, 1);
    assert!(s2.title.is_empty());
    assert_eq!(s2.blocks.len(), 1);
    let Block::Paragraph(p2) = &s2.blocks[0] else {
        panic!()
    };
    assert_eq!(p2.content, vec![Inline::Text("after".into())]);
}

#[test]
fn parses_bare_section_at_eof() {
    let doc = parse_document("=[ heading ]\n\nsection\n\n=\n").unwrap();
    assert_eq!(doc.blocks.len(), 2);

    let Block::Section(s1) = &doc.blocks[0] else {
        panic!()
    };
    assert_eq!(s1.level, 1);
    assert_eq!(s1.title, vec![Inline::Text("heading".into())]);
    assert_eq!(s1.blocks.len(), 1);

    let Block::Section(s2) = &doc.blocks[1] else {
        panic!()
    };
    assert_eq!(s2.level, 1);
    assert!(s2.title.is_empty());
    assert!(s2.blocks.is_empty());
}

#[test]
fn parses_nested_headingless_sections() {
    let src = "=\nIntro\n\n==\nSub\n\n=\nOutro\n";
    let doc = parse_document(src).unwrap();
    assert_eq!(doc.blocks.len(), 2);

    let Block::Section(s1) = &doc.blocks[0] else {
        panic!()
    };
    assert_eq!(s1.level, 1);
    assert!(s1.title.is_empty());
    assert_eq!(s1.blocks.len(), 2); // Paragraph("Intro"), Section(level 2)

    let Block::Section(sub) = &s1.blocks[1] else {
        panic!()
    };
    assert_eq!(sub.level, 2);
    assert!(sub.title.is_empty());
    assert_eq!(sub.blocks.len(), 1);

    let Block::Section(s2) = &doc.blocks[1] else {
        panic!()
    };
    assert_eq!(s2.level, 1);
    assert!(s2.title.is_empty());
    assert_eq!(s2.blocks.len(), 1);
}

#[test]
fn parses_headingless_section_with_comment_and_attrs() {
    let doc1 = parse_document("= // comment\nParagraph\n").unwrap();
    assert_eq!(doc1.blocks.len(), 1);
    let Block::Section(s1) = &doc1.blocks[0] else {
        panic!()
    };
    assert_eq!(s1.level, 1);
    assert!(s1.title.is_empty());

    let doc2 = parse_document("={ id: intro }\nParagraph\n").unwrap();
    assert_eq!(doc2.blocks.len(), 1);
    let Block::Section(s2) = &doc2.blocks[0] else {
        panic!()
    };
    assert_eq!(s2.level, 1);
    assert!(s2.title.is_empty());
    assert!(s2.value.is_some());
}

#[test]
fn bare_equal_with_attached_chars_is_plain_text() {
    let doc = parse_document("=abc is text.\n").unwrap();
    assert_eq!(doc.blocks.len(), 1);
    let Block::Paragraph(p) = &doc.blocks[0] else {
        panic!()
    };
    assert_eq!(p.content, vec![Inline::Text("=abc is text.".into())]);
}
