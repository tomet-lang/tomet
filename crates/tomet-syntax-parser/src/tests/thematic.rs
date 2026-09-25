use super::*;

#[test]
fn parses_thematic_break() {
    let doc = parse_document("---\n").unwrap();
    match &doc.blocks[0] {
        Block::Element(el) => {
            assert_eq!(el.sigil, Sigil::named("hr"));
            assert_eq!(el.content, None);
        }
        other => panic!("expected hr element, got {other:?}"),
    }
}

#[test]
fn parses_thematic_break_with_more_than_three_dashes() {
    let doc = parse_document("-----\n").unwrap();
    match &doc.blocks[0] {
        Block::Element(el) => assert_eq!(el.sigil, Sigil::named("hr")),
        other => panic!("expected hr element, got {other:?}"),
    }
}

#[test]
fn parses_titled_thematic_break() {
    let doc = parse_document("---[ Title ]---\n").unwrap();
    match &doc.blocks[0] {
        Block::Element(el) => {
            assert_eq!(el.sigil, Sigil::named("hr"));
            assert_eq!(el.content, Some(vec![Inline::Text("Title".into())]));
        }
        other => panic!("expected hr element, got {other:?}"),
    }
}

#[test]
fn titled_thematic_break_dash_runs_need_not_match_in_length() {
    let doc = parse_document("-----[ Title ]---\n").unwrap();
    match &doc.blocks[0] {
        Block::Element(el) => {
            assert_eq!(el.content, Some(vec![Inline::Text("Title".into())]));
        }
        other => panic!("expected hr element, got {other:?}"),
    }
}

#[test]
fn titled_thematic_break_needs_three_or_more_closing_dashes() {
    assert!(parse_document("---[ Title ]--\n").is_err());
}

#[test]
fn titled_thematic_break_rejects_trailing_junk() {
    assert!(parse_document("---[ Title ]--- stray text\n").is_err());
}

#[test]
fn a_dash_run_not_followed_by_a_bracket_is_plain_text() {
    // `---<embed>---` doesn't commit to the titled form (no `[` right
    // after the dashes) and isn't a plain break either (trailing
    // content isn't just whitespace) -- it's ordinary paragraph text.
    let doc = parse_document("---@embed---\n").unwrap();
    match &doc.blocks[0] {
        Block::Paragraph(p) => {
            assert_eq!(&p.content, &vec![Inline::Text("---@embed---".into())]);
        }
        other => panic!("expected paragraph, got {other:?}"),
    }
}
