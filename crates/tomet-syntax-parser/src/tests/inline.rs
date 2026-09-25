use super::*;

#[test]
fn parses_emphasis_and_strong() {
    let doc = parse_document("a *em* b **strong** c _em2_ d __strong2__\n").unwrap();
    match &doc.blocks[0] {
        Block::Paragraph(p) => {
            let kinds: Vec<_> = p
                .content
                .iter()
                .filter_map(|i| match i {
                    Inline::Element(el) => Some((el.sigil.clone(), el.content.clone())),
                    _ => None,
                })
                .collect();
            assert_eq!(
                kinds,
                vec![
                    (Sigil::named("em"), Some(vec![Inline::Text("em".into())])),
                    (
                        Sigil::named("strong"),
                        Some(vec![Inline::Text("strong".into())])
                    ),
                    (Sigil::named("em"), Some(vec![Inline::Text("em2".into())])),
                    (
                        Sigil::named("strong"),
                        Some(vec![Inline::Text("strong2".into())])
                    ),
                ]
            );
        }
        other => panic!("expected paragraph, got {other:?}"),
    }
}

#[test]
fn underscore_does_not_trigger_inside_a_word() {
    let doc = parse_document("foo_bar_baz\n").unwrap();
    match &doc.blocks[0] {
        Block::Paragraph(p) => {
            assert_eq!(&p.content, &vec![Inline::Text("foo_bar_baz".into())]);
        }
        other => panic!("expected paragraph, got {other:?}"),
    }
}

#[test]
fn unmatched_delimiter_falls_back_to_literal_text() {
    let doc = parse_document("this *word never closes\n").unwrap();
    match &doc.blocks[0] {
        Block::Paragraph(p) => {
            assert_eq!(
                &p.content,
                &vec![Inline::Text("this *word never closes".into())]
            );
        }
        other => panic!("expected paragraph, got {other:?}"),
    }
}

/// The document's first paragraph, rendered as plain text -- folding
/// each `SoftBreak` the way a renderer (e.g. the printer) does, via
/// `softbreak_join`, rather than asserting the paragraph parses to a
/// single merged `Text` (it no longer does; see `Inline::SoftBreak`).
fn paragraph_text(src: &str) -> String {
    let doc = parse_document(src).unwrap();
    match &doc.blocks[0] {
        Block::Paragraph(p) => {
            let items = &p.content;
            let mut out = String::new();
            for (idx, item) in items.iter().enumerate() {
                match item {
                    Inline::Text(t) => out.push_str(&t.value),
                    Inline::SoftBreak(_) => {
                        let before = out.chars().last();
                        let after = items.get(idx + 1).and_then(Inline::first_char);
                        out.push_str(tomet_ast::softbreak_join(before, after));
                    }
                    other => panic!("expected only text/softbreaks, got {other:?}"),
                }
            }
            out
        }
        other => panic!("expected paragraph, got {other:?}"),
    }
}

#[test]
fn a_fold_between_two_wide_characters_joins_with_nothing() {
    assert_eq!(
        paragraph_text("日本語の段落を手で折ると、\nここに空白は入らない。\n"),
        "日本語の段落を手で折ると、ここに空白は入らない。"
    );
}

#[test]
fn a_fold_between_two_narrow_characters_still_joins_with_a_space() {
    assert_eq!(
        paragraph_text("an English paragraph folded\nby hand keeps its space\n"),
        "an English paragraph folded by hand keeps its space"
    );
}

#[test]
fn a_fold_with_a_narrow_character_on_either_side_keeps_the_space() {
    assert_eq!(paragraph_text("日本語\nEnglish\n"), "日本語 English");
    assert_eq!(paragraph_text("English\n日本語\n"), "English 日本語");
}

#[test]
fn ambiguous_width_counts_as_narrow_at_a_fold() {
    // `→` is East Asian Ambiguous. Calling it wide needs a locale the
    // parser does not have, so it stays narrow and the fold keeps its
    // space.
    assert_eq!(paragraph_text("矢印→\nあ\n"), "矢印→ あ");
}

// `parses_the_repo_spec_examples` lives in the `tomet-tests` package
// now -- it reads the shared corpus, which this crate no longer
// reaches out of its own directory for.

#[test]
fn bare_at_is_plain_text_when_not_an_element() {
    let doc = parse_document("contact me@example.com please\n").unwrap();
    match &doc.blocks[0] {
        Block::Paragraph(p) => {
            assert_eq!(
                &p.content,
                &vec![Inline::Text("contact me@example.com please".into())]
            );
        }
        other => panic!("expected paragraph, got {other:?}"),
    }
}
