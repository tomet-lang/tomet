//! Macros: text macros, block macros `::macro(name: [ ... ])`, and element macros
//! `::macro(name: @el[ ... ])`.

use super::*;

#[test]
fn a_macro_fills_positional_and_named_placeholders() {
    let mut macros = HashMap::new();
    macros.insert(
        "gh".to_string(),
        Value::Scalar(Scalar::Text("https://x/issues/$(1)".to_string())),
    );
    macros.insert(
        "who".to_string(),
        Value::Scalar(Scalar::Text("hi $(name)".to_string())),
    );
    let run = |src: &str| run_with(src, &EvalValue::Scalar(Scalar::None), &macros);
    assert_eq!(run("macro.gh(12)").unwrap(), text("https://x/issues/12"));
    assert_eq!(run("macro.who(name: \"ann\")").unwrap(), text("hi ann"));
    assert_eq!(
        run("macro.nope(1)"),
        Err(EvalError::UnknownFunction("macro.nope".to_string()))
    );
}

#[test]
fn macro_bare_word_argument_is_unknown_name() {
    let mut macros = HashMap::new();
    macros.insert(
        "gh".to_string(),
        Value::Scalar(Scalar::Text("https://x/issues/$(1)".to_string())),
    );
    let run = |src: &str| run_with(src, &EvalValue::Scalar(Scalar::None), &macros);
    assert_eq!(
        run("macro.gh(tomet)"),
        Err(EvalError::UnknownName("tomet".to_string()))
    );
}

#[test]
fn macro_non_placeholder_in_template_treated_as_plain_text() {
    let mut macros = HashMap::new();
    macros.insert(
        "env".to_string(),
        Value::Scalar(Scalar::Text("$(HOME)/bin:$(PATH:-/bin):$()".to_string())),
    );
    let run = |src: &str| run_with(src, &EvalValue::Scalar(Scalar::None), &macros);
    assert_eq!(
        run("macro.env(HOME: \"/root\")").unwrap(),
        text("/root/bin:$(PATH:-/bin):$()")
    );
}

#[test]
fn a_macro_cannot_be_named_without_being_called_and_has_no_bare_form() {
    let mut macros = HashMap::new();
    macros.insert(
        "gh".to_string(),
        Value::Scalar(Scalar::Text("x".to_string())),
    );
    let run = |src: &str| run_with(src, &EvalValue::Scalar(Scalar::None), &macros);
    assert!(run("macro.gh").is_err());
    // The bare name is not a macro and not a function.
    assert_eq!(
        run("gh(12)"),
        Err(EvalError::UnknownFunction("gh".to_string()))
    );
}

#[test]
fn named_arguments_outside_a_call_are_rejected() {
    // `NamedArg` at the top level only parses; it means nothing there.
    assert!(out("heading: false").is_err());
}

/// The `[ ... ]` value of the first entry of the `::macro` declaration in `src`.
fn block_macro(src: &str) -> Value {
    let file = tomet_ast_parser::parse_file(src).expect("parses");
    let Some(args) = &file.header[0].data.args else {
        panic!("expected `(name: template)` entries");
    };
    args.nth(0).expect("first entry").value().clone()
}

fn macros_of(entries: &[(&str, Value)]) -> HashMap<String, Value> {
    entries
        .iter()
        .map(|(name, template)| (name.to_string(), template.clone()))
        .collect()
}

#[test]
fn a_block_macro_is_its_blocks_with_the_places_filled_in() {
    let macros = macros_of(&[
        (
            "card",
            block_macro("::macro(card: [\n@card(title: $(1))[ $(2) ]\n])\n"),
        ),
        (
            "notice",
            block_macro("::macro(notice: [ Notice: $(msg) ])\n"),
        ),
    ]);
    let run = |src: &str| run_with(src, &EvalValue::Scalar(Scalar::None), &macros);

    let EvalValue::Fragment(blocks) = run("macro.card(\"Welcome\", \"Hello\")").unwrap() else {
        panic!("a block macro is a fragment");
    };
    let [tomet_ast::Block::Element(card)] = &blocks[..] else {
        panic!("expected the card, got {blocks:?}");
    };
    assert_eq!(
        card.data.args,
        Some(tomet_ast::Tuple::from_named([(
            "title".to_string(),
            Value::Scalar(Scalar::Text("Welcome".to_string()))
        )]))
    );
    let Some([tomet_ast::Block::Paragraph(p)]) = card.content.as_deref() else {
        panic!("expected one paragraph, got {:?}", card.content);
    };
    assert!(matches!(&p.content[..], [tomet_ast::Inline::Text(t)] if t.value == "Hello"));

    let EvalValue::Fragment(blocks) = run("macro.notice(msg: \"Important\")").unwrap() else {
        panic!("a block macro is a fragment");
    };
    let [tomet_ast::Block::Paragraph(p)] = &blocks[..] else {
        panic!("expected a paragraph, got {blocks:?}");
    };
    assert!(
        matches!(&p.content[..], [tomet_ast::Inline::Text(a), tomet_ast::Inline::Text(b)]
            if a.value == "Notice: " && b.value == "Important"),
        "{:?}",
        p.content
    );
}

#[test]
fn a_number_given_to_a_block_macro_stays_a_number_in_data() {
    let macros = macros_of(&[("n", block_macro("::macro(n: [\n@meter(of: $(1))\n])\n"))]);
    let run = |src: &str| run_with(src, &EvalValue::Scalar(Scalar::None), &macros);
    let EvalValue::Fragment(blocks) = run("macro.n(7)").unwrap() else {
        panic!("a block macro is a fragment");
    };
    let [tomet_ast::Block::Element(el)] = &blocks[..] else {
        panic!("expected an element, got {blocks:?}");
    };
    assert_eq!(
        el.data.args,
        Some(tomet_ast::Tuple::from_named([(
            "of".to_string(),
            Value::Scalar(Scalar::Int(7))
        )]))
    );
}

#[test]
fn a_block_macro_checks_its_arguments_as_a_text_macro_does() {
    let macros = macros_of(&[
        (
            "card",
            block_macro("::macro(card: [\n@card(title: $(1))[ $(2) ]\n])\n"),
        ),
        (
            "notice",
            block_macro("::macro(notice: [ Notice: $(msg) ])\n"),
        ),
        ("none", block_macro("::macro(none: [ plain ])\n")),
    ]);
    let run = |src: &str| run_with(src, &EvalValue::Scalar(Scalar::None), &macros);
    assert_eq!(
        run("macro.card(\"only one\")"),
        Err(EvalError::WrongArgCount {
            function: "macro.card".to_string(),
            expected: "2".to_string(),
            got: 1
        })
    );
    assert_eq!(
        run("macro.notice()"),
        Err(EvalError::message(
            "macro.notice expects named argument `msg`"
        ))
    );
    assert_eq!(
        run("macro.none(1)"),
        Err(EvalError::WrongArgCount {
            function: "macro.none".to_string(),
            expected: "0".to_string(),
            got: 1
        })
    );
    assert!(matches!(run("macro.none()"), Ok(EvalValue::Fragment(_))));
}

#[test]
fn a_template_that_is_neither_a_string_blocks_nor_element_cannot_be_called() {
    let macros = macros_of(&[("three", Value::Scalar(Scalar::Int(3)))]);
    let run = |src: &str| run_with(src, &EvalValue::Scalar(Scalar::None), &macros);
    let Err(error) = run("macro.three()") else {
        panic!("a number is not a template");
    };
    assert!(error.to_string().contains("macro.three"), "{error}");
}

#[test]
fn an_element_macro_fills_args_and_content_into_a_block_element_fragment() {
    let macros = macros_of(&[
        (
            "generated_by",
            block_macro(
                "::macro(generated_by: @comment[ Generated from $(1). Edit that, then `tomet export .`. ])\n",
            ),
        ),
        (
            "card",
            block_macro("::macro(card: @card(title: $(1), count: $(n))[ $(2) ])\n"),
        ),
    ]);
    let run = |src: &str| run_with(src, &EvalValue::Scalar(Scalar::None), &macros);

    let EvalValue::Fragment(blocks) = run("macro.generated_by(\"tmtroot/agents.tmt\")").unwrap()
    else {
        panic!("an element macro is a fragment");
    };
    let [tomet_ast::Block::Element(comment)] = &blocks[..] else {
        panic!("expected the comment element, got {blocks:?}");
    };
    assert_eq!(comment.name.to_string(), "comment");
    assert_eq!(comment.placement, tomet_ast::Placement::Block);
    let Some([tomet_ast::Block::Paragraph(p)]) = comment.content.as_deref() else {
        panic!("expected one paragraph, got {:?}", comment.content);
    };
    assert!(
        matches!(
            &p.content[..],
            [
                tomet_ast::Inline::Text(a),
                tomet_ast::Inline::Text(b),
                tomet_ast::Inline::Text(c),
                tomet_ast::Inline::Element(code),
                tomet_ast::Inline::Text(d),
            ] if a.value.contains("Generated from ")
                && b.value == "tmtroot/agents.tmt"
                && c.value.contains("Edit that, then ")
                && code.name.to_string() == "raw"
                && d.value == "."
        ),
        "{:?}",
        p.content
    );

    let EvalValue::Fragment(blocks) = run("macro.card(\"Welcome\", \"Hello\", n: 7)").unwrap()
    else {
        panic!("an element macro is a fragment");
    };
    let [tomet_ast::Block::Element(card)] = &blocks[..] else {
        panic!("expected the card element, got {blocks:?}");
    };
    assert_eq!(card.placement, tomet_ast::Placement::Block);
    assert_eq!(
        card.data.args,
        Some(tomet_ast::Tuple::from_named([
            (
                "title".to_string(),
                Value::Scalar(Scalar::Text("Welcome".to_string()))
            ),
            ("count".to_string(), Value::Scalar(Scalar::Int(7)))
        ]))
    );
    let Some([tomet_ast::Block::Paragraph(p)]) = card.content.as_deref() else {
        panic!("expected one paragraph, got {:?}", card.content);
    };
    assert!(matches!(&p.content[..], [tomet_ast::Inline::Text(t)] if t.value == "Hello"));
}

#[test]
fn an_element_macro_checks_its_arguments() {
    let macros = macros_of(&[
        (
            "card",
            block_macro("::macro(card: @card(title: $(1))[ $(who) ])\n"),
        ),
        ("plain", block_macro("::macro(plain: @comment[ ok ])\n")),
    ]);
    let run = |src: &str| run_with(src, &EvalValue::Scalar(Scalar::None), &macros);
    assert_eq!(
        run("macro.card(who: \"ann\")"),
        Err(EvalError::WrongArgCount {
            function: "macro.card".to_string(),
            expected: "1".to_string(),
            got: 0
        })
    );
    assert_eq!(
        run("macro.card(\"Title\")"),
        Err(EvalError::message(
            "macro.card expects named argument `who`"
        ))
    );
    assert_eq!(
        run("macro.plain(1)"),
        Err(EvalError::WrongArgCount {
            function: "macro.plain".to_string(),
            expected: "0".to_string(),
            got: 1
        })
    );
    assert!(matches!(run("macro.plain()"), Ok(EvalValue::Fragment(_))));
}
