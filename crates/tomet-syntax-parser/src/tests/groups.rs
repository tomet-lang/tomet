use super::*;

/// Every sigil takes every group opener.
///
/// "Does a group start here?" has one answer now
/// (`element::opens_group`), but each sigil still asks it twice --
/// once in a recognizer that decides dispatch, once in the parser
/// that consumes -- and nothing in the types forces the two to
/// agree. This table is what forces it.
///
/// It is the guard `181b542` did not have. From `2a99315` until then,
/// `-` was missing `[` and `{` from both of its asks, so `-()[ x ]`
/// was an item and `-[ x ]` was a paragraph, and nothing said so.
#[test]
fn every_sigil_takes_every_group_opener() {
    // The sigil as written, and the element the construct produces.
    const SIGILS: &[(&str, &str)] = &[
        ("@memo", "memo"),
        ("=", "section"),
        ("-", "ul"),
        ("-.", "ol"),
    ];
    // One opener each, written so the construct closes on its line.
    const OPENERS: &[(char, &str)] = &[
        ('(', "(a: 1)"),
        ('[', "[ x ]"),
        ('{', "{ a: 1 }"),
        ('|', "| x"),
    ];
    // Openers a sigil does not take yet. Listed rather than skipped,
    // so closing one forces its line out of here. Empty since
    // `-` and `-.` learned `(`.
    const KNOWN_GAPS: &[(&str, char)] = &[];

    fn produced(src: &str) -> Option<String> {
        let doc = parse_document(src).ok()?;
        match doc.blocks.first()? {
            Block::Element(el) => el.sigil.name().map(|n| n.name.clone()),
            Block::Section(_) => Some("section".into()),
            Block::Paragraph(_) => None,
        }
    }

    let mut unexpected_failures = Vec::new();
    let mut closed_gaps = Vec::new();

    for (sigil, expected) in SIGILS {
        for (opener, group) in OPENERS {
            let src = format!("{sigil}{group}\n");
            let got = produced(&src);
            let takes_it = got.as_deref() == Some(*expected);
            let is_gap = KNOWN_GAPS.contains(&(sigil, *opener));

            match (takes_it, is_gap) {
                (false, false) => unexpected_failures.push(format!(
                    "{sigil} does not take {opener}: {src:?} -> {got:?}"
                )),
                (true, true) => closed_gaps.push(format!("{sigil} now takes {opener}")),
                _ => {}
            }
        }
    }

    assert!(
        unexpected_failures.is_empty(),
        "a sigil's recognizer and its parser disagree about an opener:\n  {}",
        unexpected_failures.join("\n  ")
    );
    assert!(
        closed_gaps.is_empty(),
        "listed in KNOWN_GAPS but no longer a gap -- delete the line:\n  {}",
        closed_gaps.join("\n  ")
    );
}

#[test]
fn a_bare_bracket_pair_inside_a_content_no_longer_truncates_it() {
    // Regression for the memo-content-fidelity fix: `Stop::Bracket`
    // used to break at the *first* literal `]`, corrupting the rest
    // of the content as stray trailing text. Applies to every ordinary
    // (non-raw) `[content]`, not just an opt-in one.
    let doc =
        parse_document("@caution[\nline one\nline two with * and [brackets] inside\n]\n").unwrap();
    assert_eq!(doc.blocks.len(), 1);
    match &doc.blocks[0] {
        Block::Element(el) => {
            assert_eq!(
                el.content,
                Some(vec![
                    Inline::Text("line one".into()),
                    sb(),
                    Inline::Text("line two with * and [brackets] inside".into()),
                ])
            );
        }
        other => panic!("expected an element, got {other:?}"),
    }
}

#[test]
fn an_unbalanced_bracket_inside_a_content_still_errors() {
    assert!(parse_document("@caution[ has an [ that never closes\n]\n").is_err());
}

#[test]
fn empty_paren_and_brace_groups_are_a_deliberately_empty_map() {
    // `()`/`{}` written out (as opposed to the group being omitted
    // entirely, which leaves `args`/`value` as `None`) is a valid,
    // deliberately-empty map -- matches the spec's own `@meta{}` etc.
    let doc = parse_document("@T()\n").unwrap();
    match &doc.blocks[0] {
        Block::Element(el) => {
            assert_eq!(el.args, Some(Value::Map(vec![])));
            assert_eq!(el.content, None);
        }
        other => panic!("expected an element, got {other:?}"),
    }

    let doc = parse_document("@meta{}\n").unwrap();
    match &doc.blocks[0] {
        Block::Element(el) => {
            assert_eq!(el.value, Some(ElementValue::from_map(Value::Map(vec![]))));
        }
        other => panic!("expected an element, got {other:?}"),
    }
}

#[test]
fn a_second_group_written_against_the_first_is_an_error() {
    // `docs/spec/syntax.tmt` calls a repeated group an error. It used
    // to fall through instead: the group stayed where it stood and was
    // read as prose, which also cost the element its block placement,
    // so `@T(a:1)(b:2)` quietly became a paragraph.
    let err = parse_document("@T(a:1)(b:2)\n").expect_err("a second (args) group should not parse");
    assert!(
        err.message.contains("a second `(args)` group"),
        "{}",
        err.message
    );
}

#[test]
fn a_parenthesis_after_an_element_is_prose() {
    // The narrowing that keeps ordinary sentences working: a group has
    // to be written *against* the element to count as a second one.
    // `@file(x) (it was ...)` is how `docs/.writ.tmt` and
    // `tmtroot/agents.tmt` both write, and both parse.
    let doc = parse_document("@T(a:1) (b:2)\n").unwrap();
    assert_eq!(doc.blocks.len(), 1);
    let Block::Paragraph(p) = &doc.blocks[0] else {
        panic!("expected a paragraph, got {:?}", doc.blocks[0]);
    };
    assert_eq!(p.content.len(), 2);
    assert_eq!(&p.content[1], &Inline::Text(" (b:2)".into()));
    match &p.content[0] {
        Inline::Element(el) => {
            assert_eq!(el.args, Some(Value::Map(vec![("a".into(), Value::Int(1))])));
        }
        other => panic!("expected an element, got {other:?}"),
    }
}

#[test]
fn a_comment_between_groups_does_not_detach_the_next_group() {
    // Regression: a `//`/`/* */` comment between an element's groups
    // used to fall outside the whitespace/newline gap tolerance,
    // ending the element early and leaving the next group as
    // unrelated trailing text.
    let doc = parse_document("@T(a:1) // note\n[content]\n").unwrap();
    assert_eq!(doc.blocks.len(), 1);
    match &doc.blocks[0] {
        Block::Element(el) => {
            assert_eq!(el.args, Some(Value::Map(vec![("a".into(), Value::Int(1))])));
            assert_eq!(el.content, Some(vec![Inline::Text("content".into())]));
        }
        other => panic!("expected an element with both groups, got {other:?}"),
    }
}
