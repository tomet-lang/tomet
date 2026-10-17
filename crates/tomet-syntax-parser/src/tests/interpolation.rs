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
        InterpExprKind::Id(name) => format!("Id({name})"),
        InterpExprKind::Literal(Literal::Int(i)) => format!("Int({i})"),
        InterpExprKind::Literal(Literal::Float(f)) => format!("Float({f})"),
        InterpExprKind::Literal(Literal::String(s)) => format!("String({s:?})"),
        InterpExprKind::Literal(Literal::Bool(b)) => format!("Bool({b})"),
        InterpExprKind::Literal(Literal::Null) => "Null".to_string(),
        InterpExprKind::Binary { op, lhs, rhs } => {
            format!("({} {} {})", describe(lhs), op.symbol(), describe(rhs))
        }
        InterpExprKind::Group(inner) => format!("Group({})", describe(inner)),
        InterpExprKind::DocId { doc, id } => format!("DocId({}, {id})", describe(doc)),
        InterpExprKind::Addr { text, raw } => format!("Addr({text:?}, raw={raw})"),
        InterpExprKind::Lambda { params, body } => {
            format!("Lambda([{}], {})", params.join(", "), describe(body))
        }
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
fn parses_hash_id_interpolation() {
    let doc1 = parse_document("${#intro}\n").unwrap();
    assert_eq!(describe(interp_expr(&doc1.blocks[0])), "Id(intro)");

    let doc2 = parse_document("${#(intro)}\n").unwrap();
    assert_eq!(describe(interp_expr(&doc2.blocks[0])), "Id(intro)");

    let doc3 = parse_document("${#intro.prop.tags}\n").unwrap();
    assert_eq!(
        describe(interp_expr(&doc3.blocks[0])),
        "Member(Member(Id(intro), prop), tags)"
    );

    let doc4 = parse_document("${#intro.prop.tags.item(0)}\n").unwrap();
    assert_eq!(
        describe(interp_expr(&doc4.blocks[0])),
        "Call(Member(Member(Member(Id(intro), prop), tags), item), [Int(0)])"
    );
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
            let content = first_para(el.content.as_ref().expect("content"));
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

/// Parses `${src}` as a standalone line and returns its description.
fn described(src: &str) -> String {
    let doc = parse_document(&format!("${{{src}}}\n")).unwrap();
    describe(interp_expr(&doc.blocks[0]))
}

/// Parses `${src}` and prints the expression back with `Display`.
fn printed(src: &str) -> String {
    let doc = parse_document(&format!("${{{src}}}\n")).unwrap();
    interp_expr(&doc.blocks[0]).to_string()
}

#[test]
fn true_false_and_null_are_literals() {
    assert_eq!(described("true"), "Bool(true)");
    assert_eq!(described("false"), "Bool(false)");
    assert_eq!(described("null"), "Null");
}

#[test]
fn a_word_that_only_starts_like_a_literal_is_still_a_name() {
    // They are values, not keywords: `nullable` and `true-ish` are names.
    assert_eq!(described("nullable"), "Identifier(nullable)");
    assert_eq!(described("true-ish"), "Identifier(true-ish)");
}

#[test]
fn parentheses_group_and_are_kept_in_the_tree() {
    assert_eq!(described("(a)"), "Group(Identifier(a))");
    assert_eq!(
        described("(a || b) && c"),
        "(Group((Identifier(a) || Identifier(b))) && Identifier(c))"
    );
}

#[test]
fn all_six_comparisons_parse() {
    let cases = [
        ("a == b", "(Identifier(a) == Identifier(b))"),
        ("a != b", "(Identifier(a) != Identifier(b))"),
        ("a < b", "(Identifier(a) < Identifier(b))"),
        ("a <= b", "(Identifier(a) <= Identifier(b))"),
        ("a > b", "(Identifier(a) > Identifier(b))"),
        ("a >= b", "(Identifier(a) >= Identifier(b))"),
    ];
    for (src, expected) in cases {
        assert_eq!(described(src), expected, "source: {src}");
    }
}

#[test]
fn comparison_needs_no_spaces_to_be_unambiguous() {
    // Only `-` is part of an identifier; none of the comparison symbols is.
    assert_eq!(described("a>=1"), "(Identifier(a) >= Int(1))");
    assert_eq!(described("a==b"), "(Identifier(a) == Identifier(b))");
}

#[test]
fn comparison_binds_tighter_than_and_which_binds_tighter_than_or() {
    assert_eq!(
        described("a == 1 && b != 2 || c"),
        "(((Identifier(a) == Int(1)) && (Identifier(b) != Int(2))) || Identifier(c))"
    );
    assert_eq!(
        described("a || b && c"),
        "(Identifier(a) || (Identifier(b) && Identifier(c)))"
    );
}

#[test]
fn postfix_binds_tighter_than_any_operator() {
    assert_eq!(
        described("d.meta.draft == true"),
        "(Member(Member(Identifier(d), meta), draft) == Bool(true))"
    );
    assert_eq!(
        described("exists(d.meta.title) && d.meta.title.contains(\"x\")"),
        "(Call(Identifier(exists), [Member(Member(Identifier(d), meta), title)]) && \
         Call(Member(Member(Member(Identifier(d), meta), title), contains), [String(\"x\")]))"
    );
}

#[test]
fn comparisons_do_not_chain() {
    let err = parse_document("${a < b < c}\n").unwrap_err();
    assert!(err.to_string().contains("chained"), "got: {err}");
    // Grouping one side is the way out.
    assert_eq!(
        described("(a < b) == c"),
        "(Group((Identifier(a) < Identifier(b))) == Identifier(c))"
    );
}

#[test]
fn a_lone_equals_ampersand_or_bar_is_an_error_that_names_the_fix() {
    for (src, hint) in [("${a = b}", "=="), ("${a & b}", "&&"), ("${a | b}", "||")] {
        let err = parse_document(&format!("{src}\n")).unwrap_err();
        assert!(err.to_string().contains(hint), "{src}: got {err}");
    }
}

#[test]
fn a_lambda_takes_one_parameter_without_parentheses() {
    assert_eq!(
        described("d => d.meta.draft"),
        "Lambda([d], Member(Member(Identifier(d), meta), draft))"
    );
}

#[test]
fn a_lambda_takes_several_parameters_in_parentheses() {
    assert_eq!(
        described("(a, b) => a >= b"),
        "Lambda([a, b], (Identifier(a) >= Identifier(b)))"
    );
    assert_eq!(described("(a) => a"), "Lambda([a], Identifier(a))");
}

#[test]
fn a_lambda_body_runs_to_the_end_of_the_argument() {
    assert_eq!(
        described("f(d => d.x, \"desc\")"),
        "Call(Identifier(f), [Lambda([d], Member(Identifier(d), x)), String(\"desc\")])"
    );
    assert_eq!(
        described("xs.filter(d => d.n > 1 && d.m).sort(d => d.k, \"desc\")"),
        "Call(Member(Call(Member(Identifier(xs), filter), \
         [Lambda([d], ((Member(Identifier(d), n) > Int(1)) && Member(Identifier(d), m)))]), sort), \
         [Lambda([d], Member(Identifier(d), k)), String(\"desc\")])"
    );
}

#[test]
fn a_lambda_head_is_not_confused_with_a_named_argument_or_a_group() {
    assert_eq!(
        described("f(x: 1)"),
        "Call(Identifier(f), [NamedArg(x, Int(1))])"
    );
    assert_eq!(described("(x)"), "Group(Identifier(x))");
    assert_eq!(
        described("f(x => x, y: 2)"),
        "Call(Identifier(f), [Lambda([x], Identifier(x)), NamedArg(y, Int(2))])"
    );
}

#[test]
fn the_arrow_is_not_a_comparison() {
    // `=>` is the lambda arrow; `>=` is a comparison. A stray arrow in an
    // operand position is a plain syntax error, not a silent comparison.
    assert!(parse_document("${a => }\n").is_err());
    assert_eq!(described("a >= b"), "(Identifier(a) >= Identifier(b))");
}

#[test]
fn operators_work_inside_the_braceless_call_form() {
    let doc = parse_document("$f(a == b, c && d)\n").unwrap();
    assert_eq!(
        describe(interp_expr(&doc.blocks[0])),
        "Call(Identifier(f), [(Identifier(a) == Identifier(b)), (Identifier(c) && Identifier(d))])"
    );
}

#[test]
fn display_gives_back_the_canonical_spelling() {
    let cases = [
        ("a>=1", "a >= 1"),
        ("a  ==   b", "a == b"),
        ("(a||b)&&c", "(a || b) && c"),
        ("d=>d.x", "d => d.x"),
        ("(a,b)=>a<b", "(a, b) => a < b"),
        ("true", "true"),
        ("null", "null"),
        ("xs.filter(d => d.n != null)", "xs.filter(d => d.n != null)"),
    ];
    for (src, expected) in cases {
        assert_eq!(printed(src), expected, "source: {src}");
        // The canonical spelling is a fixed point.
        assert_eq!(printed(expected), expected, "not stable: {expected}");
    }
}

#[test]
fn an_operator_inside_a_table_cell_does_not_split_the_cell() {
    let doc = parse_document("@table\n|[ a ][ ${x || y} ]\n|[ b ][ ${p && q} ]\n").unwrap();
    let rendered = format!("{doc:?}");
    assert!(
        rendered.contains("Or") || rendered.contains("||"),
        "got: {rendered}"
    );
}

#[test]
fn a_hash_after_a_document_expression_picks_an_element_in_it() {
    assert_eq!(described("self#intro"), "DocId(Identifier(self), intro)");
    assert_eq!(
        described("tmt(\"docs/a.tmt\")#intro"),
        "DocId(Call(Identifier(tmt), [String(\"docs/a.tmt\")]), intro)"
    );
    // The parenthesized id is the same id.
    assert_eq!(described("self#(intro)"), "DocId(Identifier(self), intro)");
}

#[test]
fn steps_after_a_document_id_are_members_and_calls_on_the_element() {
    assert_eq!(
        described("tmt(\"a.tmt\")#intro.prop.tags.item(2)"),
        "Call(Member(Member(Member(DocId(Call(Identifier(tmt), [String(\"a.tmt\")]), intro), prop), tags), item), [Int(2)])"
    );
    assert_eq!(
        described("self#item(0)"),
        "Call(DocId(Identifier(self), item), [Int(0)])"
    );
}

#[test]
fn the_leading_hash_is_still_an_id_on_its_own() {
    // `#intro` is `self#intro`; the parser keeps the short form as `Id` and
    // leaves the meaning to the evaluator.
    assert_eq!(
        described("#intro.prop.tags"),
        "Member(Member(Id(intro), prop), tags)"
    );
}

#[test]
fn a_hash_after_a_space_is_not_a_document_boundary() {
    assert!(parse_document("${self #intro}\n").is_err());
}

#[test]
fn a_document_id_prints_back_without_spaces() {
    assert_eq!(
        printed("tmt(\"a.tmt\")#(intro).prop"),
        "tmt(\"a.tmt\")#intro.prop"
    );
    assert_eq!(printed("self#intro"), "self#intro");
}

#[test]
fn addr_reads_a_link_style_address_as_raw_text() {
    assert_eq!(
        described("addr(tmt:docs/a.tmt#intro)"),
        "Addr(\"tmt:docs/a.tmt#intro\", raw=true)"
    );
    // The in-document steps ride along inside the same text.
    assert_eq!(
        described("addr(tmt:docs/a.tmt#intro.prop.tags.item(2))"),
        "Addr(\"tmt:docs/a.tmt#intro.prop.tags.item(2)\", raw=true)"
    );
    // `ident:` is a named argument everywhere else, and a hyphen is part of an
    // identifier; none of that applies inside `addr(`.
    assert_eq!(
        described("addr(dir:docs/my-notes)"),
        "Addr(\"dir:docs/my-notes\", raw=true)"
    );
}

#[test]
fn addr_with_no_document_on_the_left_is_the_current_document() {
    assert_eq!(described("addr(#intro)"), "Addr(\"#intro\", raw=true)");
}

#[test]
fn addr_takes_a_quoted_form_for_text_the_raw_form_cannot_carry() {
    assert_eq!(
        described("addr(\"tmt:docs/a (copy).tmt#intro\")"),
        "Addr(\"tmt:docs/a (copy).tmt#intro\", raw=false)"
    );
    // Nested parentheses balance in the raw form; an unbalanced one needs quotes.
    assert_eq!(
        described("addr(tmt:docs/a(1).tmt)"),
        "Addr(\"tmt:docs/a(1).tmt\", raw=true)"
    );
}

#[test]
fn addr_can_be_followed_by_steps_and_used_as_an_argument() {
    assert_eq!(
        described("addr(dir:docs).docs"),
        "Member(Addr(\"dir:docs\", raw=true), docs)"
    );
    assert_eq!(
        described("include(addr(tmt:docs/a.tmt#intro), heading: false)"),
        "Call(Identifier(include), [Addr(\"tmt:docs/a.tmt#intro\", raw=true), NamedArg(heading, Bool(false))])"
    );
}

#[test]
fn addr_works_in_the_braceless_call_form() {
    let doc = parse_document("$include(addr(tmt:docs/a.tmt))\n").unwrap();
    assert_eq!(
        describe(interp_expr(&doc.blocks[0])),
        "Call(Identifier(include), [Addr(\"tmt:docs/a.tmt\", raw=true)])"
    );
    let doc = parse_document("$addr(dir:docs)\n").unwrap();
    assert_eq!(
        describe(interp_expr(&doc.blocks[0])),
        "Addr(\"dir:docs\", raw=true)"
    );
}

#[test]
fn addr_that_is_unterminated_or_empty_is_an_error() {
    assert!(parse_document("${addr(tmt:docs/a.tmt}\n").is_err());
    assert!(parse_document("${addr()}\n").is_err());
    assert!(parse_document("${addr(\"\")}\n").is_err());
}

#[test]
fn a_method_named_addr_is_an_ordinary_call() {
    // Only a bare `addr(` is raw. `x.addr()` is a method on a string value.
    assert_eq!(
        described("#note.prop.source.addr()"),
        "Call(Member(Member(Member(Id(note), prop), source), addr), [])"
    );
}

#[test]
fn addr_prints_back_in_the_spelling_it_was_written() {
    assert_eq!(
        printed("addr(tmt:docs/a.tmt#intro)"),
        "addr(tmt:docs/a.tmt#intro)"
    );
    assert_eq!(
        printed("addr(\"tmt:docs/a (copy).tmt\")"),
        "addr(\"tmt:docs/a (copy).tmt\")"
    );
    assert_eq!(printed("addr( tmt:docs/a.tmt )"), "addr(tmt:docs/a.tmt)");
}
