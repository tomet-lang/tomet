use super::*;

/// Every sigil takes every group opener.
///
/// "Does a group start here?" has one answer, but each sigil still asks it twice --
/// once in a recognizer that decides dispatch, once in the parser
/// that consumes -- and nothing in the types forces the two to
/// agree. This table is what forces it.
///
/// It is the guard `181b542` did not have. From `2a99315` until then,
/// `-` was missing `[` and `{` from both of its asks, so `-()[ x ]`
/// was an item and `-[ x ]` was a paragraph, and nothing said so.
#[test]
fn every_sigil_takes_every_group_opener() {
    // The sigil as written, the element the construct produces, and whether it is a nexus --
    // which stands on `(...)` or `[...]` and refuses a `{...}` for a group of its own.
    const SIGILS: &[(&str, &str, bool)] = &[
        ("@memo", "memo", true),
        ("=", "section", false),
        ("-", "ul", false),
        ("-.", "ol", false),
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
        let doc = parse_file(src).ok()?;
        if let Some(d) = doc.header.first() {
            return Some(d.name.name.clone());
        }
        match doc.body.first()? {
            Block::Element(el) => Some(el.name.name.clone()),
            Block::Section(_) => Some("section".into()),
            Block::Paragraph(_) => None,
            Block::List(list) => Some(if list.ordered { "ol" } else { "ul" }.into()),
            Block::Interp(_) | Block::Hole(_) | Block::TypeSeat(_) => None,
        }
    }

    let mut unexpected_failures = Vec::new();
    let mut closed_gaps = Vec::new();

    for (sigil, expected, nexus) in SIGILS {
        for (opener, group) in OPENERS {
            let src = format!("{sigil}{group}\n");
            if *nexus && *opener == '{' {
                assert!(
                    parse_file(&src).is_err(),
                    "{sigil} must not stand on a lone {{...}}: {src:?}"
                );
                continue;
            }
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
    // Regression: the content does not break at the *first* literal `]`,
    // which would corrupt the rest of it as stray trailing text. Applies
    // to every ordinary (non-raw) `[content]`, not just an opt-in one.
    let doc =
        parse_file("@caution[\nline one\nline two with * and [brackets] inside\n]\n").unwrap();
    assert_eq!(doc.body.len(), 1);
    match &doc.body[0] {
        Block::Element(el) => {
            assert_eq!(
                el.content,
                wrap(vec![
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
    assert!(parse_file("@caution[ has an [ that never closes\n]\n").is_err());
}

#[test]
fn empty_paren_and_brace_groups_are_a_deliberately_empty_map() {
    // `()`/`{}` written out (as opposed to the group being omitted
    // entirely, which leaves `args`/`value` as `None`) is a valid,
    // deliberately-empty map. `()` is also what a nexus with nothing to say is written as.
    let doc = parse_file("@T()\n").unwrap();
    match &doc.body[0] {
        Block::Element(el) => {
            assert_eq!(el.data.args, Some(Value::Map(vec![])));
            assert_eq!(el.content, None);
        }
        other => panic!("expected an element, got {other:?}"),
    }

    let doc = parse_file("::meta()\n").unwrap();
    assert_eq!(doc.header.len(), 1);
    let d = &doc.header[0];
    assert_eq!(d.data.args, Some(Value::Map(vec![])));

    // `{}` is a group of its own, and empty, behind a primary group.
    let doc = parse_file("@T(){}\n").unwrap();
    match &doc.body[0] {
        Block::Element(el) => {
            assert_eq!(el.data.args, Some(Value::Map(vec![])));
            assert_eq!(el.data.value, Some(Group::from_map(Value::Map(vec![]))));
        }
        other => panic!("expected an element, got {other:?}"),
    }
}

#[test]
fn a_second_group_written_against_the_first_is_an_error() {
    // A repeated group is an error. It must not fall through: the
    // group would stay where it stood and be read as prose, which would
    // also cost the element its block placement, so `@T(a:1)(b:2)`
    // would quietly become a paragraph.
    let err = parse_file("@T(a:1)(b:2)\n").expect_err("a second (args) group should not parse");
    assert!(
        err.message.contains("a second `(args)` group"),
        "{}",
        err.message
    );
}

#[test]
fn every_kind_of_group_written_twice_against_itself_is_an_error() {
    // The offset is where the second group begins; the message names the kind.
    for (src, kind, at) in [
        ("@T(a:1)(b:2)\n", "a second `(args)` group", 7),
        ("@T[a][b]\n", "a second `[content]` group", 5),
        ("@T(){a:1}{b:2}\n", "a second `{value}` group", 9),
        ("::meta(){a:1}{b:2}\n", "a second `{value}` group", 13),
        ("a @T(a:1)(b:2) b\n", "a second `(args)` group", 9),
        ("@T()#(a)#(b)\n", "a second `#(...)`", 8),
    ] {
        let err = parse_file(src).expect_err(src);
        assert!(err.message.contains(kind), "`{src}`: {}", err.message);
        assert_eq!(err.offset, at, "`{src}`: {}", err.message);
    }
}

#[test]
fn a_second_group_apart_from_the_first_is_not_an_error() {
    // Apart by a space it is prose; on the next line it is prose too. Only against the first it is a mistake.
    for src in [
        "@T(a:1) (b:2)\n",
        "@T(a:1)\n(b:2)\n",
        "- (a)(b)\n",
        "=[ t ](a)(b)\n",
    ] {
        assert!(parse_file(src).is_ok(), "`{src}` should parse");
    }
}

#[test]
fn a_parenthesis_after_an_element_is_prose() {
    // The narrowing that keeps ordinary sentences working: a group has
    // to be written *against* the element to count as a second one.
    // `@file(x) (it was ...)` is how `docs/.writ.tmt` and
    // `tmtroot/agents.tmt` both write, and both parse.
    let doc = parse_file("@T(a:1) (b:2)\n").unwrap();
    assert_eq!(doc.body.len(), 1);
    let Block::Paragraph(p) = &doc.body[0] else {
        panic!("expected a paragraph, got {:?}", doc.body[0]);
    };
    assert_eq!(p.content.len(), 2);
    assert_eq!(&p.content[1], &Inline::Text(" (b:2)".into()));
    match &p.content[0] {
        Inline::Element(el) => {
            assert_eq!(
                el.data.args,
                Some(Value::Map(vec![("a".into(), Value::Int(1))]))
            );
        }
        other => panic!("expected an element, got {other:?}"),
    }
}

#[test]
fn a_comment_between_groups_does_not_detach_the_next_group() {
    // Regression: a `//`/`/* */` comment between an element's groups
    // is inside the whitespace/newline gap tolerance, so it does not
    // end the element early and leave the next group as
    // unrelated trailing text.
    let doc = parse_file("@T(a:1) // note\n[content]\n").unwrap();
    assert_eq!(doc.body.len(), 1);
    match &doc.body[0] {
        Block::Element(el) => {
            assert_eq!(
                el.data.args,
                Some(Value::Map(vec![("a".into(), Value::Int(1))]))
            );
            assert_eq!(el.content, wrap(vec![Inline::Text("content".into())]));
        }
        other => panic!("expected an element with both groups, got {other:?}"),
    }
}
