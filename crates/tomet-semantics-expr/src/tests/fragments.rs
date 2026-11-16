//! Fragments, emoji and unicode.

use super::*;

/// The link element inside a one-paragraph fragment, as its `args` string.
fn link_target(value: &EvalValue) -> String {
    let EvalValue::Fragment(blocks) = value else {
        panic!("expected a fragment, got {value:?}");
    };
    let [tomet_ast::Block::Paragraph(p)] = blocks.as_slice() else {
        panic!("expected one paragraph, got {blocks:?}");
    };
    let [tomet_ast::Inline::Element(el)] = p.content.as_slice() else {
        panic!("expected one inline element, got {:?}", p.content);
    };
    assert_eq!(el.name, tomet_ast::Name::bare("link"));
    match el.data.args.as_ref().and_then(|args| args.nth_unnamed(0)) {
        Some(tomet_ast::Value::Scalar(Scalar::Text(target))) => target.clone(),
        other => panic!("expected a string target, got {other:?}"),
    }
}

/// The label of the link element inside a one-paragraph fragment, if any.
fn link_label_text(value: &EvalValue) -> Option<String> {
    let EvalValue::Fragment(blocks) = value else {
        panic!("expected a fragment, got {value:?}");
    };
    let [tomet_ast::Block::Paragraph(p)] = blocks.as_slice() else {
        panic!("expected one paragraph, got {blocks:?}");
    };
    let [tomet_ast::Inline::Element(el)] = p.content.as_slice() else {
        panic!("expected one inline element, got {:?}", p.content);
    };
    assert_eq!(el.name, tomet_ast::Name::bare("link"));
    let content = el.content.as_ref()?;
    let [tomet_ast::Block::Paragraph(inner_p)] = content.as_slice() else {
        panic!("expected one paragraph in content, got {content:?}");
    };
    let [tomet_ast::Inline::Text(t)] = inner_p.content.as_slice() else {
        panic!("expected one inline text in content, got {:?}", inner_p.content);
    };
    Some(t.value.clone())
}

#[test]
fn a_document_links_to_itself_as_a_fragment() {
    let unlab = in_vault("tmt(\"docs/a.tmt\").link()").unwrap();
    assert_eq!(link_target(&unlab), "tmt:docs/a.tmt");
    assert_eq!(link_label_text(&unlab), None);

    let lab_text = in_vault("tmt(\"docs/a.tmt\").link(\"Doc A\")").unwrap();
    assert_eq!(link_target(&lab_text), "tmt:docs/a.tmt");
    assert_eq!(link_label_text(&lab_text), Some("Doc A".to_string()));

    let lab_num = in_vault("tmt(\"docs/a.tmt\").link(100)").unwrap();
    assert_eq!(link_target(&lab_num), "tmt:docs/a.tmt");
    assert_eq!(link_label_text(&lab_num), Some("100".to_string()));

    assert_eq!(
        link_target(&in_vault("link(self)").unwrap()),
        "tmt:here.tmt"
    );
    let self_lab = in_vault("link(self, \"Self Note\")").unwrap();
    assert_eq!(link_target(&self_lab), "tmt:here.tmt");
    assert_eq!(link_label_text(&self_lab), Some("Self Note".to_string()));

    let self_num = in_vault("link(self, 42)").unwrap();
    assert_eq!(link_target(&self_num), "tmt:here.tmt");
    assert_eq!(link_label_text(&self_num), Some("42".to_string()));
}

#[test]
fn an_element_with_an_id_links_to_that_id() {
    let unlab = in_vault("#c1.link()").unwrap();
    assert_eq!(link_target(&unlab), "tmt:here.tmt#c1");
    assert_eq!(link_label_text(&unlab), None);

    let lab = in_vault("#c1.link(\"Card 1\")").unwrap();
    assert_eq!(link_target(&lab), "tmt:here.tmt#c1");
    assert_eq!(link_label_text(&lab), Some("Card 1".to_string()));

    let lab_num = in_vault("#c1.link(77)").unwrap();
    assert_eq!(link_target(&lab_num), "tmt:here.tmt#c1");
    assert_eq!(link_label_text(&lab_num), Some("77".to_string()));

    let intro = in_vault("#intro.link()").unwrap();
    assert_eq!(link_target(&intro), "tmt:here.tmt#intro");
    assert_eq!(link_label_text(&intro), None);

    let intro_lab = in_vault("#intro.link(\"Intro Section\")").unwrap();
    assert_eq!(link_target(&intro_lab), "tmt:here.tmt#intro");
    assert_eq!(link_label_text(&intro_lab), Some("Intro Section".to_string()));
}

#[test]
fn link_rejects_bad_labels() {
    assert!(matches!(
        in_vault("self.link(none)"),
        Err(EvalError::TypeMismatch { got: "none", .. })
    ));
    assert!(matches!(
        in_vault("self.link((1, 2))"),
        Err(EvalError::TypeMismatch { got: "tuple", .. })
    ));
    assert!(matches!(
        in_vault("self.link(self)"),
        Err(EvalError::TypeMismatch { got: "document", .. })
    ));
    assert!(matches!(
        in_vault("#intro.link(none)"),
        Err(EvalError::TypeMismatch { got: "none", .. })
    ));
    assert!(matches!(
        in_vault("#intro.link((1, 2))"),
        Err(EvalError::TypeMismatch { got: "tuple", .. })
    ));
    assert!(matches!(
        in_vault("link(self, none)"),
        Err(EvalError::TypeMismatch { got: "none", .. })
    ));
    assert!(matches!(
        in_vault("link(self, (1, 2))"),
        Err(EvalError::TypeMismatch { got: "tuple", .. })
    ));
    assert!(matches!(
        in_vault("link(self, self)"),
        Err(EvalError::TypeMismatch { got: "document", .. })
    ));
    assert!(matches!(
        in_vault("link(self, self.link())"),
        Err(EvalError::TypeMismatch { got: "fragment", .. })
    ));
}

#[test]
fn link_wrong_argument_counts() {
    assert_eq!(
        in_vault("self.link(\"a\", \"b\")"),
        Err(EvalError::WrongArgCount {
            function: ".link".to_string(),
            expected: "0 to 1".to_string(),
            got: 2,
        })
    );
    assert_eq!(
        in_vault("#intro.link(\"a\", \"b\")"),
        Err(EvalError::WrongArgCount {
            function: ".link".to_string(),
            expected: "0 to 1".to_string(),
            got: 2,
        })
    );
    assert_eq!(
        in_vault("link()"),
        Err(EvalError::WrongArgCount {
            function: "link".to_string(),
            expected: "1 to 2".to_string(),
            got: 0,
        })
    );
    assert_eq!(
        in_vault("link(self, \"a\", \"b\")"),
        Err(EvalError::WrongArgCount {
            function: "link".to_string(),
            expected: "1 to 2".to_string(),
            got: 3,
        })
    );
}

#[test]
fn link_inside_map_lambda() {
    let resolver = MemoryResolver::new()
        .with_source(
            "here.tmt",
            "::meta(lang: \"en\", lang_name: \"English\")\n",
        )
        .unwrap()
        .with_source(
            "locales/ja/readme.tmt",
            "::meta(lang: \"ja\", lang_name: \"日本語\")\n",
        )
        .unwrap();

    let links = in_resolver(
        &resolver,
        "vault::docs.map(d => d.link(d::meta.lang_name))",
    )
    .unwrap();

    let EvalValue::Tuple(items) = links else {
        panic!("expected a tuple");
    };
    let results: Vec<(String, Option<String>)> = items
        .unnamed()
        .map(|v| (link_target(v), link_label_text(v)))
        .collect();

    assert_eq!(
        results,
        vec![
            ("tmt:here.tmt".to_string(), Some("English".to_string())),
            (
                "tmt:locales/ja/readme.tmt".to_string(),
                Some("日本語".to_string())
            ),
        ]
    );
}

#[test]
fn join_one_paragraph_fragments() {
    let val = in_vault("(self.link(\"Here\"), #intro.link(\"Intro\")).join(\" | \")").unwrap();
    let EvalValue::Fragment(blocks) = val else {
        panic!("expected a fragment, got {val:?}");
    };
    let [tomet_ast::Block::Paragraph(p)] = blocks.as_slice() else {
        panic!("expected one paragraph, got {blocks:?}");
    };
    assert_eq!(p.content.len(), 3);
    match &p.content[0] {
        tomet_ast::Inline::Element(el) => {
            assert_eq!(el.name, tomet_ast::Name::bare("link"));
            assert_eq!(
                link_label_text(&EvalValue::Fragment(vec![tomet_ast::Block::Paragraph(
                    tomet_ast::Paragraph::new(vec![p.content[0].clone()], tomet_span::Span::default())
                )])),
                Some("Here".to_string())
            );
        }
        other => panic!("expected inline element, got {other:?}"),
    }
    match &p.content[1] {
        tomet_ast::Inline::Text(t) => assert_eq!(t.value, " | "),
        other => panic!("expected inline text, got {other:?}"),
    }
    match &p.content[2] {
        tomet_ast::Inline::Element(el) => {
            assert_eq!(el.name, tomet_ast::Name::bare("link"));
            assert_eq!(
                link_label_text(&EvalValue::Fragment(vec![tomet_ast::Block::Paragraph(
                    tomet_ast::Paragraph::new(vec![p.content[2].clone()], tomet_span::Span::default())
                )])),
                Some("Intro".to_string())
            );
        }
        other => panic!("expected inline element, got {other:?}"),
    }

    // A single fragment:
    let single = in_vault("(self.link(\"Alone\"),).join(\" | \")").unwrap();
    let EvalValue::Fragment(blocks) = single else {
        panic!("expected a fragment");
    };
    let [tomet_ast::Block::Paragraph(p)] = blocks.as_slice() else {
        panic!("expected one paragraph");
    };
    assert_eq!(p.content.len(), 1);

    // Empty tuple returns empty text scalar:
    let empty = in_vault("().join(\" | \")").unwrap();
    assert_eq!(empty, EvalValue::Scalar(Scalar::Text(String::new())));
}

#[test]
fn join_mismatches_with_indices() {
    // Mixed: scalar then fragment
    let err = in_vault("(\"text\", self.link()).join(\" | \")").unwrap_err();
    match err {
        EvalError::TypeMismatch { what, got, .. } => {
            assert!(what.contains("1"), "expected index 1, got {what}");
            assert_eq!(got, "fragment");
        }
        other => panic!("expected TypeMismatch, got {other:?}"),
    }

    // Mixed: fragment then scalar
    let err = in_vault("(self.link(), \"text\").join(\" | \")").unwrap_err();
    match err {
        EvalError::TypeMismatch { what, got, .. } => {
            assert!(what.contains("1"), "expected index 1, got {what}");
            assert_eq!(got, "string");
        }
        other => panic!("expected TypeMismatch, got {other:?}"),
    }

    // Non-scalar non-fragment
    let err = in_vault("(self, self).join(\" | \")").unwrap_err();
    match err {
        EvalError::TypeMismatch { what, got, .. } => {
            assert!(what.contains("0"), "expected index 0, got {what}");
            assert_eq!(got, "document");
        }
        other => panic!("expected TypeMismatch, got {other:?}"),
    }

    // Fragment of zero blocks
    let zero_blocks = EvalValue::Fragment(vec![]);
    let err = with_var("(var.f,).join(\" | \")", map(vec![("f", zero_blocks)])).unwrap_err();
    match err {
        EvalError::TypeMismatch { what, got, .. } => {
            assert!(what.contains("0"), "expected index 0, got {what}");
            assert_eq!(got, "fragment of zero blocks");
        }
        other => panic!("expected TypeMismatch, got {other:?}"),
    }

    // Fragment of several blocks
    let span = tomet_span::Span::default();
    let multi_blocks = EvalValue::Fragment(vec![
        tomet_ast::Block::Paragraph(tomet_ast::Paragraph::new(vec![], span)),
        tomet_ast::Block::Paragraph(tomet_ast::Paragraph::new(vec![], span)),
    ]);
    let err = with_var("(var.f,).join(\" | \")", map(vec![("f", multi_blocks)])).unwrap_err();
    match err {
        EvalError::TypeMismatch { what, got, .. } => {
            assert!(what.contains("0"), "expected index 0, got {what}");
            assert_eq!(got, "fragment of several blocks");
        }
        other => panic!("expected TypeMismatch, got {other:?}"),
    }

    // Non-paragraph block
    let section = EvalValue::Fragment(vec![tomet_ast::Block::Section(tomet_ast::Section {
        level: 1,
        title: vec![],
        data: tomet_ast::Data::default(),
        blocks: vec![],
        id: None,
        connects: vec![],
        span,
    })]);
    let err = with_var("(var.f,).join(\" | \")", map(vec![("f", section)])).unwrap_err();
    match err {
        EvalError::TypeMismatch { what, got, .. } => {
            assert!(what.contains("0"), "expected index 0, got {what}");
            assert_eq!(got, "non-paragraph block");
        }
        other => panic!("expected TypeMismatch, got {other:?}"),
    }
}

#[test]
fn target_expression_evaluates_language_bar() {
    let resolver = MemoryResolver::new()
        .with_source("readme.tmt", "::meta(lang: \"en\", lang_name: \"English\")\n")
        .unwrap()
        .with_source(
            "locales/ja/readme.tmt",
            "::meta(lang: \"ja\", lang_name: \"日本語\")\n",
        )
        .unwrap()
        .with_source(
            "docs/other.tmt",
            "::meta(lang: \"fr\", lang_name: \"Français\")\n",
        )
        .unwrap();

    let expr = r#"vault::docs
        .filter(d => d::filename == "readme.tmt")
        .sort(d => d::meta.lang)
        .map(d => d.link(d::meta.lang_name))
        .join(" | ")"#;

    let result = in_resolver(&resolver, expr).unwrap();
    let EvalValue::Fragment(blocks) = result else {
        panic!("expected a fragment, got {result:?}");
    };
    let [tomet_ast::Block::Paragraph(p)] = blocks.as_slice() else {
        panic!("expected one paragraph, got {blocks:?}");
    };
    assert_eq!(p.content.len(), 3);

    let en_link = &p.content[0];
    let sep = &p.content[1];
    let ja_link = &p.content[2];

    match en_link {
        tomet_ast::Inline::Element(el) => {
            assert_eq!(el.name, tomet_ast::Name::bare("link"));
            let target = match el.data.args.as_ref().and_then(|a| a.nth_unnamed(0)) {
                Some(tomet_ast::Value::Scalar(Scalar::Text(t))) => t.as_str(),
                other => panic!("expected text, got {other:?}"),
            };
            assert_eq!(target, "tmt:readme.tmt");
            let label = link_label_text(&EvalValue::Fragment(vec![tomet_ast::Block::Paragraph(
                tomet_ast::Paragraph::new(vec![en_link.clone()], tomet_span::Span::default()),
            )]));
            assert_eq!(label, Some("English".to_string()));
        }
        other => panic!("expected element, got {other:?}"),
    }

    match sep {
        tomet_ast::Inline::Text(t) => assert_eq!(t.value, " | "),
        other => panic!("expected separator text, got {other:?}"),
    }

    match ja_link {
        tomet_ast::Inline::Element(el) => {
            assert_eq!(el.name, tomet_ast::Name::bare("link"));
            let target = match el.data.args.as_ref().and_then(|a| a.nth_unnamed(0)) {
                Some(tomet_ast::Value::Scalar(Scalar::Text(t))) => t.as_str(),
                other => panic!("expected text, got {other:?}"),
            };
            assert_eq!(target, "tmt:locales/ja/readme.tmt");
            let label = link_label_text(&EvalValue::Fragment(vec![tomet_ast::Block::Paragraph(
                tomet_ast::Paragraph::new(vec![ja_link.clone()], tomet_span::Span::default()),
            )]));
            assert_eq!(label, Some("日本語".to_string()));
        }
        other => panic!("expected element, got {other:?}"),
    }
}

#[test]
fn join_20000_fragments_linear_time() {
    let span = tomet_span::Span::default();
    let frag = EvalValue::Fragment(vec![tomet_ast::Block::Paragraph(
        tomet_ast::Paragraph::new(
            vec![tomet_ast::Inline::Text(tomet_ast::Text::new("item", span))],
            span,
        ),
    )]);
    let fragments = vec![frag; 20_000];
    let tuple = tomet_ast::Tuple::from_unnamed(fragments);

    let result = crate::methods::call_method(
        EvalValue::Tuple(tuple),
        "join",
        crate::args::Args {
            positional: vec![EvalValue::Scalar(Scalar::Text(" | ".to_string()))],
            named: vec![],
        },
        &EvalContext {
            resolver: &NoResolver,
            self_doc: DocRef("test.tmt".to_string()),
            macros: &HashMap::new(),
            var: &EvalValue::Scalar(Scalar::None),
            var_unset: &[],
            includes: &[],
            heading_level: 0,
            notes: Default::default(),
        },
    )
    .unwrap();

    let EvalValue::Fragment(blocks) = result else {
        panic!("expected fragment");
    };
    let [tomet_ast::Block::Paragraph(p)] = blocks.as_slice() else {
        panic!("expected one paragraph");
    };
    // 20,000 items + 19,999 separators = 39,999 inlines
    assert_eq!(p.content.len(), 39_999);
    for (i, inline) in p.content.iter().enumerate() {
        if i % 2 == 0 {
            match inline {
                tomet_ast::Inline::Text(t) => assert_eq!(t.value, "item"),
                other => panic!("expected item text at index {i}, got {other:?}"),
            }
        } else {
            match inline {
                tomet_ast::Inline::Text(t) => assert_eq!(t.value, " | "),
                other => panic!("expected separator text at index {i}, got {other:?}"),
            }
        }
    }
}

#[test]
fn an_element_without_an_id_cannot_be_linked() {
    let err = in_vault("self#item(0).link()").unwrap_err();
    assert!(err.to_string().contains("without an id"), "{err}");
}

#[test]
fn link_turns_a_list_of_documents_into_a_list_of_links() {
    let links = in_vault("addr(dir:docs)::docs.map(d => d.link())").unwrap();
    let EvalValue::Tuple(items) = links else {
        panic!("expected a tuple");
    };
    let targets: Vec<String> = items.unnamed().map(link_target).collect();
    assert_eq!(targets, ["tmt:docs/a.tmt", "tmt:docs/sub/b.tmt"]);
}

#[test]
fn link_wants_a_document_or_an_element() {
    assert!(matches!(
        in_vault("link(1)"),
        Err(EvalError::TypeMismatch { got: "int", .. })
    ));
}

#[test]
fn a_fragment_is_data_and_a_reference_is_not() {
    let link = in_vault("self.link()").unwrap();
    assert!(link.to_ast().is_some());
    assert_eq!(in_vault("self").unwrap().to_ast(), None);
}

#[test]
fn emoji_looks_a_name_up_and_refuses_one_it_does_not_know() {
    assert_eq!(out("emoji(\"rocket\")").unwrap(), text("🚀"));
    assert_eq!(out("emoji(\":tada:\")").unwrap(), text("🎉"));
    assert_eq!(out("emoji(\"party\")").unwrap(), text("🎉"));
    let err = out("emoji(\"nope\")").unwrap_err();
    assert!(err.to_string().contains("nope"), "{err}");
    assert!(matches!(
        out("emoji(1)"),
        Err(EvalError::TypeMismatch { got: "int", .. })
    ));
}

#[test]
fn unicode_takes_a_code_point_as_a_number_or_hex_text() {
    assert_eq!(out("unicode(65)").unwrap(), text("A"));
    assert_eq!(out("unicode(\"U+1F680\")").unwrap(), text("🚀"));
    assert_eq!(out("unicode(\"1F680\")").unwrap(), text("🚀"));
    assert_eq!(out("unicode(\"0x41\")").unwrap(), text("A"));
    assert!(out("unicode(\"zzz\")").is_err());
    assert!(out("unicode(-1)").is_err());
    // A surrogate is not a character.
    assert!(out("unicode(55296)").is_err());
    assert!(matches!(
        out("unicode(true)"),
        Err(EvalError::TypeMismatch { got: "bool", .. })
    ));
}

fn zero_element_spans(el: &mut tomet_ast::Element) {
    use tomet_ast::Block;
    el.span = tomet_span::Span::default();
    if let Some(content) = &mut el.content {
        for block in content {
            if let Block::Paragraph(p) = block {
                p.span = tomet_span::Span::default();
                for inline in &mut p.content {
                    if let tomet_ast::Inline::Raw(r) = inline {
                        r.span = tomet_span::Span::default();
                    }
                }
            }
        }
    }
}

/// Pins that `raw(text, lang: ...)` builds an element structurally equal to a parsed fenced block.
/// The test must fail if the representation of a fenced block ever changes.
#[test]
fn raw_matches_parsed_fenced_block_structurally() {
    use tomet_ast::Block;
    // 1. With lang
    let parsed_file = tomet_ast_parser::parse_file("```tmt\nhello world\n```\n").unwrap();
    let mut parsed_el = match parsed_file.body.into_iter().next().unwrap() {
        Block::Element(el) => el,
        other => panic!("expected Block::Element, got {other:?}"),
    };
    let raw_val = out(r#"raw("hello world", lang: "tmt")"#).unwrap();
    let EvalValue::Fragment(raw_blocks) = raw_val else {
        panic!("expected Fragment");
    };
    let mut raw_el = match <[Block; 1]>::try_from(raw_blocks).unwrap()[0].clone() {
        Block::Element(el) => el,
        other => panic!("expected Block::Element, got {other:?}"),
    };
    zero_element_spans(&mut parsed_el);
    zero_element_spans(&mut raw_el);
    assert_eq!(parsed_el, raw_el);

    // 2. Without lang
    let parsed_no_lang = tomet_ast_parser::parse_file("```\nhello world\n```\n").unwrap();
    let mut parsed_no_lang_el = match parsed_no_lang.body.into_iter().next().unwrap() {
        Block::Element(el) => el,
        other => panic!("expected Block::Element, got {other:?}"),
    };
    let raw_no_lang_val = out(r#"raw("hello world")"#).unwrap();
    let EvalValue::Fragment(raw_no_lang_blocks) = raw_no_lang_val else {
        panic!("expected Fragment");
    };
    let mut raw_no_lang_el = match <[Block; 1]>::try_from(raw_no_lang_blocks).unwrap()[0].clone() {
        Block::Element(el) => el,
        other => panic!("expected Block::Element, got {other:?}"),
    };
    zero_element_spans(&mut parsed_no_lang_el);
    zero_element_spans(&mut raw_no_lang_el);
    assert_eq!(parsed_no_lang_el, raw_no_lang_el);
}

#[test]
fn raw_errors_report_expected_type_and_argument_shape() {
    // No argument
    assert_eq!(
        out("raw()"),
        Err(EvalError::WrongArgCount {
            function: "raw".to_string(),
            expected: "1".to_string(),
            got: 0,
        })
    );

    // Two positional arguments
    assert_eq!(
        out(r#"raw("a", "b")"#),
        Err(EvalError::WrongArgCount {
            function: "raw".to_string(),
            expected: "1".to_string(),
            got: 2,
        })
    );

    // First argument is not a text scalar
    assert_eq!(
        out("raw(none)"),
        Err(EvalError::TypeMismatch {
            what: "`raw`".to_string(),
            expected: "a text scalar".to_string(),
            got: "none",
        })
    );
    assert_eq!(
        out("raw((1, 2))"),
        Err(EvalError::TypeMismatch {
            what: "`raw`".to_string(),
            expected: "a text scalar".to_string(),
            got: "tuple",
        })
    );
    assert_eq!(
        out("raw(1)"),
        Err(EvalError::TypeMismatch {
            what: "`raw`".to_string(),
            expected: "a text scalar".to_string(),
            got: "int",
        })
    );
    assert_eq!(
        in_vault("raw(self)"),
        Err(EvalError::TypeMismatch {
            what: "`raw`".to_string(),
            expected: "a text scalar".to_string(),
            got: "document",
        })
    );
    assert_eq!(
        in_vault("raw(self.link())"),
        Err(EvalError::TypeMismatch {
            what: "`raw`".to_string(),
            expected: "a text scalar".to_string(),
            got: "fragment",
        })
    );
    assert_eq!(
        in_vault("raw(#c1)"),
        Err(EvalError::TypeMismatch {
            what: "`raw`".to_string(),
            expected: "a text scalar".to_string(),
            got: "element",
        })
    );

    // lang is not a text scalar
    assert_eq!(
        out(r#"raw("a", lang: 1)"#),
        Err(EvalError::TypeMismatch {
            what: "`raw`'s `lang:`".to_string(),
            expected: "a text scalar".to_string(),
            got: "int",
        })
    );
    assert_eq!(
        out(r#"raw("a", lang: none)"#),
        Err(EvalError::TypeMismatch {
            what: "`raw`'s `lang:`".to_string(),
            expected: "a text scalar".to_string(),
            got: "none",
        })
    );

    // Any other named argument
    assert_eq!(
        out(r#"raw("a", other: "x")"#),
        Err(EvalError::Message("`raw` takes `lang:`, not `other:`".to_string()))
    );
}
