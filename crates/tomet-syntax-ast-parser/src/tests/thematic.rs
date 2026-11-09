use super::*;

#[test]
fn parses_thematic_break() {
    let doc = parse_file("---\n").unwrap();
    match &doc.body[0] {
        Block::Element(el) => {
            assert_eq!(el.name, Name::bare("hr"));
            assert_eq!(el.content, None);
        }
        other => panic!("expected hr element, got {other:?}"),
    }
}

#[test]
fn parses_thematic_break_with_more_than_three_dashes() {
    let doc = parse_file("-----\n").unwrap();
    match &doc.body[0] {
        Block::Element(el) => assert_eq!(el.name, Name::bare("hr")),
        other => panic!("expected hr element, got {other:?}"),
    }
}

#[test]
fn parses_titled_thematic_break() {
    let doc = parse_file("---[ Title ]---\n").unwrap();
    match &doc.body[0] {
        Block::Element(el) => {
            assert_eq!(el.name, Name::bare("hr"));
            assert_eq!(el.content, wrap(vec![Inline::Text("Title".into())]));
        }
        other => panic!("expected hr element, got {other:?}"),
    }
}

#[test]
fn titled_thematic_break_dash_runs_need_not_match_in_length() {
    let doc = parse_file("-----[ Title ]---\n").unwrap();
    match &doc.body[0] {
        Block::Element(el) => {
            assert_eq!(el.content, wrap(vec![Inline::Text("Title".into())]));
        }
        other => panic!("expected hr element, got {other:?}"),
    }
}

#[test]
fn titled_thematic_break_needs_three_or_more_closing_dashes() {
    assert!(parse_file("---[ Title ]--\n").is_err());
}

#[test]
fn titled_thematic_break_rejects_trailing_junk() {
    assert!(parse_file("---[ Title ]--- stray text\n").is_err());
}

/// The content of the `hr` that opens `src`.
fn hr_content(src: &str) -> Option<Vec<Block>> {
    match &parse_file(src).unwrap().body[0] {
        Block::Element(el) => {
            assert_eq!(el.name, Name::bare("hr"), "{src:?}");
            el.content.clone()
        }
        other => panic!("expected hr element, got {other:?}"),
    }
}

#[test]
fn titled_thematic_break_takes_pipe_lines_for_its_title() {
    // `|` is `[ ]` without the brackets, so it is the same node.
    assert_eq!(
        hr_content("---\n| Title\n---\n"),
        wrap(vec![Inline::Text("Title".into())])
    );
    assert_eq!(
        hr_content("---\n| Title\n---\n"),
        hr_content("---[ Title ]---\n")
    );
    // Several lines are one paragraph, as in the bracketed form.
    assert_eq!(
        hr_content("---\n| one\n| two\n---\n"),
        hr_content("---[ one\ntwo ]---\n")
    );
    // The closing fence is 3 or more dashes, indented or not, and the file may end on it.
    assert_eq!(
        hr_content("-----\n| Title\n   -----"),
        hr_content("---[ Title ]---\n")
    );
}

#[test]
fn titled_thematic_break_in_pipe_form_must_be_closed() {
    for src in [
        "---\n| Title\n",
        "---\n| Title",
        "---\n| Title\n\nafter\n",
        "---\n| Title\nafter\n",
        "---\n| Title\n--\n",
    ] {
        let err = parse_file(src).unwrap_err();
        assert!(
            err.message.contains("close the titled thematic break"),
            "{src:?}: {}",
            err.message
        );
    }
    assert!(parse_file("---\n| Title\n--- stray text\n").is_err());
}

#[test]
fn a_pipe_line_only_titles_a_break_it_directly_follows() {
    // A blank line in between: a plain break, then a paragraph that happens to start with `|`.
    let doc = parse_file("---\n\n| x\n").unwrap();
    assert!(matches!(&doc.body[0], Block::Element(el) if el.content.is_none()));
    assert!(matches!(&doc.body[1], Block::Paragraph(_)));
}

#[test]
fn pipe_form_breaks_pair_up_left_to_right() {
    // The second `---` closes the first break; it does not open another one.
    let doc = parse_file("---\n| a\n---\n| b\n---\n").unwrap();
    assert_eq!(doc.body.len(), 3);
    assert!(matches!(&doc.body[0], Block::Element(el) if el.content.is_some()));
    assert!(matches!(&doc.body[1], Block::Paragraph(_)));
    assert!(matches!(&doc.body[2], Block::Element(el) if el.content.is_none()));
}

#[test]
fn a_dash_run_not_followed_by_a_bracket_is_plain_text() {
    // `---<embed>---` doesn't commit to the titled form (no `[` right
    // after the dashes) and isn't a plain break either (trailing
    // content isn't just whitespace) -- it's ordinary paragraph text.
    let doc = parse_file("---@embed---\n").unwrap();
    match &doc.body[0] {
        Block::Paragraph(p) => {
            assert_eq!(&p.content, &vec![Inline::Text("---@embed---".into())]);
        }
        other => panic!("expected paragraph, got {other:?}"),
    }
}
