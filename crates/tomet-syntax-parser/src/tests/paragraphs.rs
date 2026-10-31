use super::*;

#[test]
fn an_element_on_a_continuation_line_stays_in_the_paragraph() {
    // An element trigger used to end a paragraph early, so a sentence
    // wrapped across lines was torn into separate blocks the moment a
    // line happened to start with one. A continuation line is not
    // block context, so the element belongs to the running text; a
    // blank line is what opens a block.
    let doc = parse_document("text\n@note{key:value}\n").unwrap();
    assert_eq!(doc.blocks.len(), 1);
    match &doc.blocks[0] {
        Block::Paragraph(p) => {
            assert_eq!(p.content.len(), 3);
            // The newline is a `SoftBreak`, the way any wrapped line is
            // now -- not folded into the text as a space.
            assert_eq!(&p.content[0], &Inline::Text("text".into()));
            assert_eq!(&p.content[1], &sb());
            match &p.content[2] {
                Inline::Element(el) => {
                    assert_eq!(el.name, Name::bare("note"));
                    assert_eq!(el.placement, tomet_ast::Placement::Inline);
                }
                other => panic!("expected an element, got {other:?}"),
            }
        }
        other => panic!("expected one paragraph, got {other:?}"),
    }

    // With the blank line, it is a block.
    let doc = parse_document("text\n\n@note{key:value}\n").unwrap();
    assert_eq!(doc.blocks.len(), 2);
    match &doc.blocks[1] {
        Block::Element(el) => {
            assert_eq!(el.name, Name::bare("note"));
            assert_eq!(el.placement, tomet_ast::Placement::Block);
        }
        other => panic!("expected an element, got {other:?}"),
    }
}

#[test]
fn adjacent_element_lines_are_one_paragraph() {
    // An element is running text like any other: `@note{...}` then
    // `@memo{...}` on consecutive lines fold into one paragraph, with a soft
    // break between them. A blank line or a `;` is what makes them blocks.
    let doc = parse_document("@note{ type:#settings }\n@memo{ key:value }\n").unwrap();
    assert_eq!(doc.blocks.len(), 1);
    match &doc.blocks[0] {
        Block::Paragraph(p) => {
            assert_eq!(p.content.len(), 3);
            assert!(matches!(&p.content[0], Inline::Element(el) if el.name == Name::bare("note")));
            assert!(matches!(&p.content[1], Inline::SoftBreak(_)));
            assert!(matches!(&p.content[2], Inline::Element(el) if el.name == Name::bare("memo")));
        }
        other => panic!("expected one paragraph, got {other:?}"),
    }

    for src in [
        "@note{ type:#settings }\n\n@memo{ key:value }\n",
        "@note{ type:#settings }\n;@memo{ key:value }\n",
    ] {
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.blocks.len(), 2, "{src:?}");
        assert!(matches!(&doc.blocks[0], Block::Element(_)), "{src:?}");
        assert!(matches!(&doc.blocks[1], Block::Element(_)), "{src:?}");
    }
}

#[test]
fn text_after_an_element_line_folds_into_it_unless_a_semicolon_breaks() {
    let doc = parse_document("@note{ a: 1 }\ntext\n").unwrap();
    assert_eq!(doc.blocks.len(), 1);
    assert!(matches!(&doc.blocks[0], Block::Paragraph(p) if p.content.len() == 3));

    let doc = parse_document("@note{ a: 1 }\n;text\n").unwrap();
    assert_eq!(doc.blocks.len(), 2);
    assert!(matches!(&doc.blocks[0], Block::Element(_)));
    assert!(matches!(&doc.blocks[1], Block::Paragraph(_)));
}

#[test]
fn an_interpolation_is_a_block_only_when_it_stands_alone_on_its_line() {
    let doc = parse_document("${a}\n").unwrap();
    assert!(matches!(&doc.blocks[0], Block::Interp(_)));

    // Anything else on the line, or a following line that folds, makes it
    // running text.
    for src in ["${a} text\n", "${a} ${b}\n", "${a}\n${b}\n", "${a}\ntext\n"] {
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.blocks.len(), 1, "{src:?}");
        assert!(matches!(&doc.blocks[0], Block::Paragraph(_)), "{src:?}");
    }

    let doc = parse_document(";${a}\n;${b}\n").unwrap();
    assert_eq!(doc.blocks.len(), 2);
    assert!(doc.blocks.iter().all(|b| matches!(b, Block::Interp(_))));
}

#[test]
fn a_declaration_after_an_element_line_stays_a_block() {
    let doc = parse_document("@note{ a: 1 }\n::kind(config)\n").unwrap();
    assert_eq!(doc.blocks.len(), 2);
    assert!(matches!(&doc.blocks[0], Block::Element(_)));
    assert!(matches!(&doc.blocks[1], Block::Declaration(_)));
}

#[test]
fn a_paragraph_does_not_swallow_a_following_fenced_code_block() {
    // Regression: a fenced code block (or thematic break) directly
    // after a paragraph line, with no blank line between them, used to
    // be absorbed as lazy-continuation paragraph text instead of
    // starting its own block.
    let doc = parse_document("text\n```js\ncode\n```\n").unwrap();
    assert_eq!(doc.blocks.len(), 2);
    match (&doc.blocks[0], &doc.blocks[1]) {
        (Block::Paragraph(p), Block::Element(el)) => {
            assert_eq!(&p.content, &vec![Inline::Text("text".into())]);
            assert_eq!(el.name, Name::bare("raw"));
        }
        other => panic!("expected paragraph then raw, got {other:?}"),
    }
}

#[test]
fn a_paragraph_does_not_swallow_a_following_thematic_break() {
    // `text`'s own paragraph stops before `---` (a thematic break is
    // one of `paragraph_breaks_here`'s stop conditions), and the
    // thematic break itself isolates as its own block by default
    // (`docs/spec/syntax.tmt`'s `##[ 継続 ]`) -- `more` starts a third,
    // independent paragraph, not a continuation of the break.
    let doc = parse_document("text\n---\nmore\n").unwrap();
    assert_eq!(doc.blocks.len(), 3);
    match (&doc.blocks[0], &doc.blocks[1], &doc.blocks[2]) {
        (Block::Paragraph(a), Block::Element(hr), Block::Paragraph(b)) => {
            assert_eq!(&a.content, &vec![Inline::Text("text".into())]);
            assert_eq!(hr.name, Name::bare("hr"));
            assert_eq!(&b.content, &vec![Inline::Text("more".into())]);
        }
        other => panic!("expected paragraph, hr, paragraph, got {other:?}"),
    }
}

#[test]
fn a_semicolon_at_the_start_of_a_line_breaks_the_paragraph() {
    // `;` opens a new block without a blank line, the way `-` opens a list
    // item: the line before it is not folded into the line after it.
    let doc = parse_document("a\n;b\nc\n").unwrap();
    assert_eq!(doc.blocks.len(), 2);
    match (&doc.blocks[0], &doc.blocks[1]) {
        (Block::Paragraph(a), Block::Paragraph(b)) => {
            assert_eq!(&a.content, &vec![Inline::Text("a".into())]);
            // `c` is a continuation of `b`: only the `;` line starts a block.
            assert_eq!(b.content.len(), 3);
            assert_eq!(&b.content[0], &Inline::Text("b".into()));
            assert_eq!(&b.content[2], &Inline::Text("c".into()));
        }
        other => panic!("expected two paragraphs, got {other:?}"),
    }
}

#[test]
fn semicolons_stand_blocks_of_elements_side_by_side() {
    let doc = parse_document(";@note{ a: 1 }\n;@memo{ b: 2 }\n").unwrap();
    assert_eq!(doc.blocks.len(), 2);
    for (block, name) in doc.blocks.iter().zip(["note", "memo"]) {
        match block {
            Block::Element(el) => {
                assert_eq!(el.name.name, name);
                assert_eq!(el.placement, tomet_ast::Placement::Block);
            }
            other => panic!("expected an element, got {other:?}"),
        }
    }
}

#[test]
fn a_semicolon_inside_a_line_is_text() {
    let doc = parse_document("a;b\n").unwrap();
    assert_eq!(doc.blocks.len(), 1);
    match &doc.blocks[0] {
        Block::Paragraph(p) => assert_eq!(&p.content, &vec![Inline::Text("a;b".into())]),
        other => panic!("expected a paragraph, got {other:?}"),
    }
}

#[test]
fn a_backslash_makes_a_semicolon_at_the_start_of_a_line_text() {
    let doc = parse_document("a\n\\;b\n").unwrap();
    assert_eq!(doc.blocks.len(), 1);
    match &doc.blocks[0] {
        Block::Paragraph(p) => {
            assert_eq!(p.content.len(), 3);
            assert_eq!(&p.content[0], &Inline::Text("a".into()));
            assert_eq!(&p.content[2], &Inline::Text(";b".into()));
        }
        other => panic!("expected one paragraph, got {other:?}"),
    }
}

#[test]
fn a_semicolon_breaks_a_pipe_run_paragraph_too() {
    let doc = parse_document("@quote\n| a\n| ;b\n").unwrap();
    let Block::Element(el) = &doc.blocks[0] else {
        panic!("expected an element, got {:?}", doc.blocks[0]);
    };
    assert_eq!(el.content.as_ref().map(Vec::len), Some(2));
}

#[test]
fn a_bare_name_alone_on_its_line_is_an_element_whatever_follows() {
    // `@memo` with no group is an element when it is the whole of its line
    // (`is_element_start`). The check used to run after the gap before the next
    // group was skipped, so it held only at the end of the file.
    for src in ["@memo\n", "@memo\n\ntext\n", "@memo\n\n@other\n", "@memo // note\n\ntext\n"] {
        let doc = parse_document(src).unwrap();
        assert!(
            matches!(&doc.blocks[0], Block::Element(el) if el.name == Name::bare("memo")),
            "{src:?}: {:?}",
            doc.blocks[0]
        );
    }
    // On a line with something else it is text, as before.
    let doc = parse_document("@memo and more\n").unwrap();
    assert!(matches!(&doc.blocks[0], Block::Paragraph(_)));
}
