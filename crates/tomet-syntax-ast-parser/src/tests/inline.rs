use super::*;

#[test]
fn parses_emphasis_and_strong() {
    let doc = parse_file("a *em* b **strong** c _em2_ d __strong2__\n").unwrap();
    match &doc.body[0] {
        Block::Paragraph(p) => {
            let kinds: Vec<_> = p
                .content
                .iter()
                .filter_map(|i| match i {
                    Inline::Element(el) => Some((el.name.clone(), el.content.clone())),
                    _ => None,
                })
                .collect();
            assert_eq!(
                kinds,
                vec![
                    (Name::bare("em"), wrap(vec![Inline::Text("em".into())])),
                    (
                        Name::bare("strong"),
                        wrap(vec![Inline::Text("strong".into())])
                    ),
                    (Name::bare("em"), wrap(vec![Inline::Text("em2".into())])),
                    (
                        Name::bare("strong"),
                        wrap(vec![Inline::Text("strong2".into())])
                    ),
                ]
            );
        }
        other => panic!("expected paragraph, got {other:?}"),
    }
}

#[test]
fn underscore_does_not_trigger_inside_a_word() {
    let doc = parse_file("foo_bar_baz\n").unwrap();
    match &doc.body[0] {
        Block::Paragraph(p) => {
            assert_eq!(&p.content, &vec![Inline::Text("foo_bar_baz".into())]);
        }
        other => panic!("expected paragraph, got {other:?}"),
    }
}

#[test]
fn unmatched_delimiter_falls_back_to_literal_text() {
    let doc = parse_file("this *word never closes\n").unwrap();
    match &doc.body[0] {
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
    let doc = parse_file(src).unwrap();
    match &doc.body[0] {
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
    let doc = parse_file("contact me@example.com please\n").unwrap();
    match &doc.body[0] {
        Block::Paragraph(p) => {
            assert_eq!(
                &p.content,
                &vec![Inline::Text("contact me@example.com please".into())]
            );
        }
        other => panic!("expected paragraph, got {other:?}"),
    }
}

#[test]
fn element_or_span_inside_quotes_in_prose_is_read_as_element_or_span() {
    let doc = parse_file("He said \"@note[important] and *bold text* here\"\n").unwrap();
    let Block::Paragraph(p) = &doc.body[0] else {
        panic!("expected paragraph");
    };
    let has_el = p
        .content
        .iter()
        .any(|i| matches!(i, Inline::Element(e) if e.name == Name::bare("note")));
    let has_em = p
        .content
        .iter()
        .any(|i| matches!(i, Inline::Element(e) if e.name == Name::bare("em")));
    assert!(has_el, "should parse @note element inside quotes in prose");
    assert!(
        has_em,
        "should parse *bold text* span inside quotes in prose"
    );
}

#[test]
fn delimiters_and_names_read_as_before() {
    let doc = parse_file("__strong__ snake_case_name @my_name(1) ::a-b(2)\n").unwrap();
    let Block::Paragraph(p) = &doc.body[0] else {
        panic!("expected paragraph");
    };
    let has_strong = p
        .content
        .iter()
        .any(|i| matches!(i, Inline::Element(e) if e.name == Name::bare("strong")));
    assert!(has_strong, "expected __strong__ to be parsed as strong");
    let el = p
        .content
        .iter()
        .find_map(|i| match i {
            Inline::Element(e) if e.name == Name::bare("my_name") => Some(e),
            _ => None,
        })
        .expect("found @my_name");
    assert_eq!(el.connects.len(), 1);
    assert_eq!(el.connects[0].name, Name::bare("a-b"));
}

#[test]
fn the_cst_and_the_ast_agree_on_inline_structure() {
    use tomet_cst::SyntaxKind as K;
    fn names(inlines: &[Inline], out: &mut Vec<String>) {
        for i in inlines {
            if let Inline::Element(e) = i {
                out.push(e.name.name.clone());
                for b in e.content.iter().flatten() {
                    if let Block::Paragraph(p) = b {
                        names(&p.content, out);
                    }
                }
            }
        }
    }
    for src in [
        "a *em* b **strong** c ~~gone~~ `code` https://a.b/c\n",
        "**a *b* c**\n",
        "`*not em*` and _em_\n",
        "x _two words_ y\n",
    ] {
        let doc = parse_file(src).unwrap();
        let mut ast = Vec::new();
        for b in &doc.body {
            if let Block::Paragraph(p) = b {
                names(&p.content, &mut ast);
            }
        }
        ast.sort();
        let mut cst: Vec<String> = Vec::new();
        let root = tomet_cst_parser::parse_cst(src);
        for (kind, name) in [
            (K::EMPHASIS, "em"),
            (K::STRONG, "strong"),
            (K::STRIKEOUT, "strikeout"),
            (K::CODE_SPAN, "raw"),
            (K::AUTOLINK, "link"),
        ] {
            let count = root.descendants().filter(|n| n.kind() == kind).count();
            cst.extend(std::iter::repeat_n(name.to_string(), count));
        }
        cst.sort();
        assert_eq!(cst, ast, "{src:?}");
    }
}

#[test]
fn general_backslash_escapes_lower_to_literal_text() {
    assert_eq!(
        paragraph_text(
            "\\; \\*em\\* \\**strong\\** \\`code\\` \\// \\/* \\@name(x) \\${x} \\<slot> \\\\\n"
        ),
        "; *em* **strong** `code` // /* @name(x) ${x} <slot> \\"
    );
    assert_eq!(paragraph_text("\\*em*\n"), "*em*");
    assert_eq!(paragraph_text("*em\\*\n"), "*em*");
    assert_eq!(paragraph_text("\\**strong**\n"), "**strong**");
    assert_eq!(paragraph_text("**strong\\**\n"), "**strong**");
    assert_eq!(
        paragraph_text("\\::kind(x)\n\\===[ H ]\n\\---\n\\- item\n"),
        "::kind(x) ===[ H ] --- - item"
    );
    // Unescaped backslash before non-punctuation stays a literal backslash.
    assert_eq!(
        paragraph_text("a\\b and C:\\Users\\name and a\\ b\n"),
        "a\\b and C:\\Users\\name and a\\ b"
    );
    // `\\*em*` escapes the backslash and parses `*em*` as emphasis.
    let doc = parse_file("\\\\*em*\n").unwrap();
    let Block::Paragraph(p) = &doc.body[0] else {
        panic!("expected paragraph");
    };
    assert_eq!(p.content.len(), 2);
    assert_eq!(p.content[0], Inline::Text("\\".into()));
    assert!(matches!(&p.content[1], Inline::Element(e) if e.name == Name::bare("em")));

    // Escaped brackets inside `@note[...]` stay text.
    let doc = parse_file("@note[ a \\[ b \\] c ]\n").unwrap();
    let Block::Element(el) = &doc.body[0] else {
        panic!("expected element");
    };
    assert_eq!(el.content, wrap(vec![Inline::Text("a [ b ] c".into())]));

    // Escaped trailing groups on headings and list items stay part of the text.
    let doc =
        parse_file("= Heading \\#(id) \\:connect(x)\n\n- item \\#(id) \\:connect(x)\n").unwrap();
    let Block::Section(sec) = &doc.body[0] else {
        panic!("expected section");
    };
    assert_eq!(sec.id, None);
    assert!(sec.connects.is_empty());
    assert_eq!(
        sec.title,
        vec![Inline::Text("Heading #(id) :connect(x)".into())]
    );
    let Block::List(list) = &sec.blocks[0] else {
        panic!("expected list");
    };
    assert_eq!(list.items[0].id, None);
    assert!(list.items[0].connects.is_empty());
    assert_eq!(
        list.items[0].content,
        vec![Block::Paragraph(tomet_ast::Paragraph::new(
            vec![Inline::Text("item #(id) :connect(x)".into())],
            Span::dummy(),
        ))]
    );
}
