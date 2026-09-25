use super::*;

/// `${...}` parses to a real `Element` (`Sigil::Dollar`, `value:
/// Some(ElementValue::Interp(expr))`) -- same shape as `@name{value}`,
/// not a bespoke `Inline` variant. A standalone `${x}` line therefore
/// collapses to `Block::Element` exactly like a standalone
/// `@meta{...}` does (`document.rs::parse_paragraph`'s one-element
/// collapse), which is why these tests destructure `Block::Element`
/// rather than `Block::Paragraph`.
fn interp_expr(block: &Block) -> &InterpExpr {
    match block {
        Block::Element(el) => {
            assert_eq!(el.sigil, Sigil::Dollar);
            match &el.value {
                Some(ElementValue::Interp(expr)) => expr,
                other => panic!("expected ElementValue::Interp, got {other:?}"),
            }
        }
        other => {
            panic!("expected a standalone ${{...}} to collapse to Block::Element, got {other:?}")
        }
    }
}

/// Renders an `InterpExpr` to a canonical, span-free string (e.g.
/// `Call(Identifier(sum), [Identifier(a)])`) so tests can assert
/// against a plain string instead of hand-building an expected tree
/// with matching spans (which `derive(PartialEq)` would otherwise
/// require field-for-field, spans included).
fn describe(expr: &InterpExpr) -> String {
    match &expr.kind {
        InterpExprKind::Identifier(name) => format!("Identifier({name})"),
        InterpExprKind::Literal(Literal::Int(i)) => format!("Int({i})"),
        InterpExprKind::Literal(Literal::Float(f)) => format!("Float({f})"),
        InterpExprKind::Literal(Literal::String(s)) => format!("String({s:?})"),
        InterpExprKind::Call { callee, args } => {
            let args: Vec<_> = args.iter().map(describe).collect();
            format!("Call({}, [{}])", describe(callee), args.join(", "))
        }
        InterpExprKind::Member { object, member } => {
            format!("Member({}, {member})", describe(object))
        }
        InterpExprKind::NamedArg { name, value } => {
            format!("NamedArg({name}, {})", describe(value))
        }
    }
}

#[test]
fn parses_bare_identifier_interpolation() {
    let doc = parse_document("${id}\n").unwrap();
    assert_eq!(describe(interp_expr(&doc.blocks[0])), "Identifier(id)");
}

#[test]
fn parses_dotted_member_interpolation() {
    let doc = parse_document("${a.b.c}\n").unwrap();
    assert_eq!(
        describe(interp_expr(&doc.blocks[0])),
        "Member(Member(Identifier(a), b), c)"
    );
}

#[test]
fn parses_call_interpolation_with_nested_call() {
    let doc = parse_document("${sum(a, mul(b, c))}\n").unwrap();
    assert_eq!(
        describe(interp_expr(&doc.blocks[0])),
        "Call(Identifier(sum), [Identifier(a), Call(Identifier(mul), [Identifier(b), Identifier(c)])])"
    );
}

#[test]
fn parses_member_access_on_a_calls_result() {
    // The whole point of `Member`/`Call` both wrapping `Box<InterpExpr>`
    // instead of a flat `Path` + a bare-`String` callee: `b(x).id`
    // (access a member of a call's return value) needs a `Call` node
    // nested *inside* a `Member`'s `object`, which a flat `Path`
    // couldn't represent at all.
    let doc = parse_document("${b(x).id}\n").unwrap();
    assert_eq!(
        describe(interp_expr(&doc.blocks[0])),
        "Member(Call(Identifier(b), [Identifier(x)]), id)"
    );
}

#[test]
fn parses_call_on_a_members_result() {
    // The other direction: `a.b(x)` -- call member `b` of `a`.
    let doc = parse_document("${a.b(x)}\n").unwrap();
    assert_eq!(
        describe(interp_expr(&doc.blocks[0])),
        "Call(Member(Identifier(a), b), [Identifier(x)])"
    );
}

#[test]
fn parses_literals_in_interpolation() {
    let cases = [
        ("${1}", "Int(1)"),
        ("${1.5}", "Float(1.5)"),
        ("${-3}", "Int(-3)"),
        ("${\"hi\"}", "String(\"hi\")"),
    ];
    for (src, expected) in cases {
        let doc = parse_document(&format!("{src}\n")).unwrap();
        assert_eq!(
            describe(interp_expr(&doc.blocks[0])),
            expected,
            "source: {src}"
        );
    }
}

#[test]
fn parses_dollar_func_call_syntax() {
    let doc = parse_document("$date(\"YYYY-MM-DD\")\n").unwrap();
    assert_eq!(
        describe(interp_expr(&doc.blocks[0])),
        "Call(Identifier(date), [String(\"YYYY-MM-DD\")])"
    );

    let doc2 = parse_document("$emoji(\"sparkles\")\n").unwrap();
    assert_eq!(
        describe(interp_expr(&doc2.blocks[0])),
        "Call(Identifier(emoji), [String(\"sparkles\")])"
    );

    let doc3 = parse_document("$tm(\"guide/intro\", \"install\")\n").unwrap();
    assert_eq!(
        describe(interp_expr(&doc3.blocks[0])),
        "Call(Identifier(tm), [String(\"guide/intro\"), String(\"install\")])"
    );
}

#[test]
fn bare_dollar_without_parens_is_plain_text() {
    let doc = parse_document("Price is $100 or $PATH or $foo.\n").unwrap();
    match &doc.blocks[0] {
        Block::Paragraph(p) => {
            assert_eq!(
                &p.content,
                &vec![Inline::Text("Price is $100 or $PATH or $foo.".into())]
            );
        }
        other => panic!("expected paragraph, got {other:?}"),
    }
}

#[test]
fn interpolation_allows_inline_whitespace() {
    let doc = parse_document("${ sum(a, b) }\n").unwrap();
    assert_eq!(
        describe(interp_expr(&doc.blocks[0])),
        "Call(Identifier(sum), [Identifier(a), Identifier(b)])"
    );

    let doc = parse_document("${ a . b }\n").unwrap();
    assert_eq!(
        describe(interp_expr(&doc.blocks[0])),
        "Member(Identifier(a), b)"
    );
}

#[test]
fn unclosed_interpolation_is_an_error() {
    assert!(parse_document("${id\n").is_err());
}

#[test]
fn empty_interpolation_is_an_error() {
    assert!(parse_document("${}\n").is_err());
}

#[test]
fn interpolation_with_bad_leading_char_is_an_error() {
    assert!(parse_document("${+x}\n").is_err());
}

#[test]
fn dollar_not_followed_by_brace_is_plain_text() {
    let doc = parse_document("costs $5 or $ {x} today\n").unwrap();
    match &doc.blocks[0] {
        Block::Paragraph(p) => {
            assert_eq!(
                &p.content,
                &vec![Inline::Text("costs $5 or $ {x} today".into())]
            );
        }
        other => panic!("expected paragraph, got {other:?}"),
    }
}

#[test]
fn interp_trigger_on_next_line_does_not_end_paragraph() {
    // Unlike `<T>`/`@name`, `${...}` is deliberately NOT added to
    // `Stop::Paragraph`'s block-trigger disjunction (see
    // `document.rs::parse_inline_seq`): a `${...}` embedded mid-prose
    // (with text before/after it on the same line) stays inline
    // content in the running paragraph. This case has trailing text
    // after `${id}`, so it doesn't hit the single-element collapse
    // `interp_expr` above relies on -- content.len() > 1.
    let doc = parse_document("first line\n${id} second line\n").unwrap();
    match &doc.blocks[0] {
        Block::Paragraph(p) => {
            assert_eq!(p.content.len(), 4);
            assert!(matches!(p.content[0], Inline::Text(_)));
            assert!(matches!(p.content[1], Inline::SoftBreak(_)));
            assert!(matches!(p.content[2], Inline::Element(_)));
            assert!(matches!(p.content[3], Inline::Text(_)));
        }
        other => panic!("expected a single paragraph, got {other:?}"),
    }
}

#[test]
fn interpolation_inside_element_content_and_section() {
    let doc = parse_document("@memo[ total: ${sum(a, b)} ]\n\n=[ ${x} ]\n").unwrap();
    match &doc.blocks[0] {
        Block::Element(el) => {
            let content = el.content.as_ref().expect("content");
            assert!(content.iter().any(|i| matches!(
                i,
                Inline::Element(e) if e.sigil == Sigil::Dollar
            )));
        }
        other => panic!("expected element, got {other:?}"),
    }
    match &doc.blocks[1] {
        Block::Section(sec) => {
            assert!(sec.title.iter().any(|i| matches!(
                i,
                Inline::Element(e) if e.sigil == Sigil::Dollar
            )));
        }
        other => panic!("expected section, got {other:?}"),
    }
}
