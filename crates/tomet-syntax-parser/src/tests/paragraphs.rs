use super::*;

#[test]
fn an_element_on_a_continuation_line_stays_in_the_paragraph() {
    // An element trigger used to end a paragraph early, so a sentence
    // wrapped across lines was torn into separate blocks the moment a
    // line happened to start with one. A continuation line is not
    // block context, so the element belongs to the running text; a
    // blank line is what opens a block.
    let doc = parse_document("text\n@meta{key:value}\n").unwrap();
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
                    assert_eq!(el.sigil, Sigil::named("meta"));
                    assert_eq!(el.placement, tomet_ast::Placement::Inline);
                }
                other => panic!("expected an element, got {other:?}"),
            }
        }
        other => panic!("expected one paragraph, got {other:?}"),
    }

    // With the blank line, it is a block.
    let doc = parse_document("text\n\n@meta{key:value}\n").unwrap();
    assert_eq!(doc.blocks.len(), 2);
    match &doc.blocks[1] {
        Block::Element(el) => {
            assert_eq!(el.sigil, Sigil::named("meta"));
            assert_eq!(el.placement, tomet_ast::Placement::Block);
        }
        other => panic!("expected an element, got {other:?}"),
    }
}

#[test]
fn adjacent_elements_with_no_blank_line_isolate_by_default() {
    // `docs/spec/syntax.tmt`'s `##[ 継続 ]`: a bare element that opens
    // its own line isolates as its own block by default, whether or
    // not a blank line follows -- `@meta{...}` immediately followed
    // by `@settings{...}` (no blank line, no `\`) stays two
    // independent `Block::Element`s, not one `Block::Paragraph`.
    // `docs/examples/dirs.tmt`'s file listing depends on this.
    let doc = parse_document("@meta{ type:#settings }\n@settings{ key:value }\n").unwrap();
    assert_eq!(doc.blocks.len(), 2);
    match (&doc.blocks[0], &doc.blocks[1]) {
        (Block::Element(a), Block::Element(b)) => {
            assert_eq!(a.sigil, Sigil::named("meta"));
            assert_eq!(b.sigil, Sigil::named("settings"));
        }
        other => panic!("expected two elements, got {other:?}"),
    }
}

#[test]
fn a_leading_continuation_joins_adjacent_elements() {
    // The `\` override (`docs/spec/syntax.tmt`'s `##[ 継続 ]`) for the
    // case above: with it, `@settings{...}` joins `@meta{...}`'s
    // paragraph instead of standing alone.
    let doc = parse_document("@meta{ type:#settings }\n\\@settings{ key:value }\n").unwrap();
    assert_eq!(doc.blocks.len(), 1);
    match &doc.blocks[0] {
        Block::Paragraph(p) => {
            let elements: Vec<_> = p
                .content
                .iter()
                .filter_map(|inline| match inline {
                    Inline::Element(el) => Some(el.sigil.clone()),
                    _ => None,
                })
                .collect();
            assert_eq!(
                elements,
                vec![Sigil::named("meta"), Sigil::named("settings")]
            );
        }
        other => panic!("expected one paragraph, got {other:?}"),
    }
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
            assert_eq!(el.sigil, Sigil::named("raw"));
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
            assert_eq!(hr.sigil, Sigil::named("hr"));
            assert_eq!(&b.content, &vec![Inline::Text("more".into())]);
        }
        other => panic!("expected paragraph, hr, paragraph, got {other:?}"),
    }
}

#[test]
fn a_leading_continuation_joins_a_thematic_break_into_the_previous_paragraph() {
    // The parser stays kind-oblivious about what a leading `\`
    // (`docs/spec/syntax.tmt`'s `##[ 継続 ]`) joins: popping a
    // thematic break (`Block::Element`) and demoting it to
    // `Placement::Inline` works the same as for any other kind.
    // `tomet-semantics::shape_mismatch` is what would flag this
    // particular join as wrong, since `hr` always requires `Block`.
    let doc = parse_document("text\n\n---\n\\more\n").unwrap();
    assert_eq!(doc.blocks.len(), 2);
    match (&doc.blocks[0], &doc.blocks[1]) {
        (Block::Paragraph(a), Block::Paragraph(b)) => {
            assert_eq!(&a.content, &vec![Inline::Text("text".into())]);
            match b.content.first() {
                Some(Inline::Element(hr)) => assert_eq!(hr.sigil, Sigil::named("hr")),
                other => panic!("expected hr as the paragraph's first item, got {other:?}"),
            }
            match b.content.last() {
                Some(Inline::Text(t)) => assert_eq!(t.value, "more"),
                other => panic!("expected trailing text, got {other:?}"),
            }
        }
        other => panic!("expected two paragraphs, got {other:?}"),
    }
}
