use super::*;

/// `{$ ... }` parses to an `Interpolation`, not an `Element`. A standalone
/// `$(x)` line is a `Block::Interp` of its own (what it evaluates to may be
/// blocks), which is why these tests destructure `Block::Interp` rather than
/// `Block::Paragraph`.
fn interp_expr(block: &Block) -> &InterpExpr {
    match block {
        Block::Interp(interp) => &interp.expr,
        other => {
            panic!("expected a standalone interpolation to be a Block::Interp, got {other:?}")
        }
    }
}

/// Renders an `InterpExpr` to a canonical, span-free string (e.g.
/// `Call(Identifier(sum), [Identifier(a)])`) so tests can assert
/// against a plain string instead of hand-building an expected tree
/// with matching spans (which `derive(PartialEq)` would otherwise
/// require field-for-field, spans included).
pub(super) fn describe(expr: &InterpExpr) -> String {
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
        InterpExprKind::Seq(items) => {
            let items: Vec<_> = items.iter().map(describe).collect();
            format!("Seq([{}])", items.join(", "))
        }
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
        InterpExprKind::Connect { object, member } => {
            format!("Connect({}, {member})", describe(object))
        }
        InterpExprKind::NamedArg { name, value } => {
            format!("NamedArg({name}, {})", describe(value))
        }
    }
}

#[test]
fn parses_hash_id_interpolation() {
    let doc1 = parse_file("$(#intro)\n").unwrap();
    assert_eq!(describe(interp_expr(&doc1.body[0])), "Id(intro)");

    let doc2 = parse_file("$(#(intro))\n").unwrap();
    assert_eq!(describe(interp_expr(&doc2.body[0])), "Id(intro)");

    let doc3 = parse_file("$(#intro.tags)\n").unwrap();
    assert_eq!(
        describe(interp_expr(&doc3.body[0])),
        "Member(Id(intro), tags)"
    );

    let doc4 = parse_file("{$ #intro.tags.item(0) }\n").unwrap();
    assert_eq!(
        describe(interp_expr(&doc4.body[0])),
        "Call(Member(Member(Id(intro), tags), item), [Int(0)])"
    );
}

#[test]
fn a_double_colon_goes_from_a_node_and_a_dot_reads_inside_it() {
    for (source, expected) in [
        ("$(u1::id)", "Connect(Identifier(u1), id)"),
        ("$(#u1::meta.tag)", "Member(Connect(Id(u1), meta), tag)"),
        (
            "$(self::doc::meta)",
            "Connect(Connect(Identifier(self), doc), meta)",
        ),
        (
            "{$ self::meta.tags.join(\", \") }",
            "Call(Member(Member(Connect(Identifier(self), meta), tags), join), [String(\", \")])",
        ),
        // Space around the sigil is space, as it is around a dot.
        ("{$ a :: b }", "Connect(Identifier(a), b)"),
        (
            "{$ vault::docs.item(0) }",
            "Call(Member(Connect(Identifier(vault), docs), item), [Int(0)])",
        ),
    ] {
        let doc = parse_file(&format!("{source}\n")).unwrap();
        assert_eq!(describe(interp_expr(&doc.body[0])), expected, "{source}");
    }
    // A single colon is still not an operator, and `::` needs a name after it.
    assert!(parse_file("{$ a: }\n").is_err());
    assert!(parse_file("{$ a:: }\n").is_err());
    assert!(parse_file("{$ a::1 }\n").is_err());
    // The printed expression reads back as the same tree.
    let doc = parse_file("$(#u1::meta.tag)\n").unwrap();
    assert_eq!(interp_expr(&doc.body[0]).to_string(), "#u1::meta.tag");
}

#[test]
fn parses_bare_identifier_interpolation() {
    let doc = parse_file("$(id)\n").unwrap();
    assert_eq!(describe(interp_expr(&doc.body[0])), "Identifier(id)");
}

#[test]
fn parses_dotted_member_interpolation() {
    let doc = parse_file("$(a.b.c)\n").unwrap();
    assert_eq!(
        describe(interp_expr(&doc.body[0])),
        "Member(Member(Identifier(a), b), c)"
    );
}

#[test]
fn parses_call_interpolation_with_nested_call() {
    let doc = parse_file("$sum(a, mul(b, c))\n").unwrap();
    assert_eq!(
        describe(interp_expr(&doc.body[0])),
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
    let doc = parse_file("{$ b(x).id }\n").unwrap();
    assert_eq!(
        describe(interp_expr(&doc.body[0])),
        "Member(Call(Identifier(b), [Identifier(x)]), id)"
    );
}

#[test]
fn parses_call_on_a_members_result() {
    // The other direction: `a.b(x)` -- call member `b` of `a`.
    let doc = parse_file("$a.b(x)\n").unwrap();
    assert_eq!(
        describe(interp_expr(&doc.body[0])),
        "Call(Member(Identifier(a), b), [Identifier(x)])"
    );
}

#[test]
fn parses_literals_in_interpolation() {
    let cases = [
        ("$(1)", "Int(1)"),
        ("{$ 1.5 }", "Float(1.5)"),
        ("{$ -3 }", "Int(-3)"),
        ("{$ \"hi\" }", "String(\"hi\")"),
    ];
    for (src, expected) in cases {
        let doc = parse_file(&format!("{src}\n")).unwrap();
        assert_eq!(
            describe(interp_expr(&doc.body[0])),
            expected,
            "source: {src}"
        );
    }
}

#[test]
fn parses_dollar_func_call_syntax() {
    let doc = parse_file("$date(\"YYYY-MM-DD\")\n").unwrap();
    assert_eq!(
        describe(interp_expr(&doc.body[0])),
        "Call(Identifier(date), [String(\"YYYY-MM-DD\")])"
    );

    let doc2 = parse_file("$emoji(\"sparkles\")\n").unwrap();
    assert_eq!(
        describe(interp_expr(&doc2.body[0])),
        "Call(Identifier(emoji), [String(\"sparkles\")])"
    );

    let doc3 = parse_file("$tm(\"guide/intro\", \"install\")\n").unwrap();
    assert_eq!(
        describe(interp_expr(&doc3.body[0])),
        "Call(Identifier(tm), [String(\"guide/intro\"), String(\"install\")])"
    );
}

#[test]
fn bare_dollar_without_parens_is_plain_text() {
    let doc = parse_file("Price is $100 or $PATH or $foo.\n").unwrap();
    match &doc.body[0] {
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
    let doc = parse_file("$sum(a, b)\n").unwrap();
    assert_eq!(
        describe(interp_expr(&doc.body[0])),
        "Call(Identifier(sum), [Identifier(a), Identifier(b)])"
    );

    let doc = parse_file("{$ a . b }\n").unwrap();
    assert_eq!(
        describe(interp_expr(&doc.body[0])),
        "Member(Identifier(a), b)"
    );
}

#[test]
fn unclosed_interpolation_is_an_error() {
    assert!(parse_file("{$ id\n").is_err());
}

#[test]
fn empty_interpolation_is_an_error() {
    assert!(parse_file("{$ }\n").is_err());
}

#[test]
fn interpolation_with_bad_leading_char_is_an_error() {
    assert!(parse_file("{$ +x }\n").is_err());
}

#[test]
fn dollar_not_followed_by_brace_is_plain_text() {
    let doc = parse_file("costs $5 or $ {x} today\n").unwrap();
    match &doc.body[0] {
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
    // `{$ ... }` does not start a block of its own mid-prose: one embedded
    // with text before/after it on the same line stays inline content in
    // the running paragraph. This case has trailing text
    // after `$(id)`, so it is not a standalone line, which is the only
    // place `{$ ... }` becomes a `Block::Interp`.
    let doc = parse_file("first line\n$(id) second line\n").unwrap();
    match &doc.body[0] {
        Block::Paragraph(p) => {
            assert_eq!(p.content.len(), 4);
            assert!(matches!(p.content[0], Inline::Text(_)));
            assert!(matches!(p.content[1], Inline::SoftBreak(_)));
            assert!(matches!(p.content[2], Inline::Interp(_)));
            assert!(matches!(p.content[3], Inline::Text(_)));
        }
        other => panic!("expected a single paragraph, got {other:?}"),
    }
}

#[test]
fn interpolation_inside_element_content_and_section() {
    let doc = parse_file("@memo[ total: $sum(a, b) ]\n\n=[ $(x) ]\n").unwrap();
    match &doc.body[0] {
        Block::Element(el) => {
            let content = first_para(el.content.as_ref().expect("content"));
            assert!(content.iter().any(|i| matches!(i, Inline::Interp(_))));
        }
        other => panic!("expected element, got {other:?}"),
    }
    match &doc.body[1] {
        Block::Section(sec) => {
            assert!(sec.title.iter().any(|i| matches!(i, Inline::Interp(_))));
        }
        other => panic!("expected section, got {other:?}"),
    }
}

/// Parses `{$ src }` as a standalone line and returns its description.
fn described(src: &str) -> String {
    let doc = parse_file(&format!("{{$ {src} }}\n")).unwrap();
    describe(interp_expr(&doc.body[0]))
}

/// Parses `{$ src }` and prints the expression back with `Display`.
fn printed(src: &str) -> String {
    let doc = parse_file(&format!("{{$ {src} }}\n")).unwrap();
    interp_expr(&doc.body[0]).to_string()
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
fn plus_and_minus_are_binary_operators_with_white_space_around_them() {
    assert_eq!(described("a + b"), "(Identifier(a) + Identifier(b))");
    assert_eq!(described("a - b"), "(Identifier(a) - Identifier(b))");
    // Left to right.
    assert_eq!(
        described("a - b - c"),
        "((Identifier(a) - Identifier(b)) - Identifier(c))"
    );
    assert_eq!(described("n - 1"), "(Identifier(n) - Int(1))");
    // A `-` glued to digits is a negative number, also after an operator.
    assert_eq!(described("-3"), "Int(-3)");
    assert_eq!(described("n - -3"), "(Identifier(n) - Int(-3))");
    // A line break is white space.
    assert_eq!(described("a\n  + b"), "(Identifier(a) + Identifier(b))");
}

#[test]
fn a_dash_without_white_space_is_part_of_a_name() {
    assert_eq!(described("daily-note"), "Identifier(daily-note)");
    assert_eq!(described("a-b"), "Identifier(a-b)");
    assert_eq!(
        described("a-b - c-d"),
        "(Identifier(a-b) - Identifier(c-d))"
    );
}

#[test]
fn plus_and_minus_without_white_space_on_both_sides_are_refused_with_the_fix() {
    for src in ["a +b", "a+ b", "a+b", "a -b", "a- b - c", "1 -1", "n -3"] {
        let err = parse_file(&format!("{{$ {src} }}\n")).unwrap_err();
        // `a- b` reads as the name `a-` followed by `b`, which is not an expression;
        // every other one names the white space.
        if src != "a- b - c" {
            assert!(
                err.message.contains("white space on both sides"),
                "{src}: {}",
                err.message
            );
        }
    }
}

#[test]
fn additive_binds_tighter_than_comparison_and_looser_than_postfix() {
    assert_eq!(
        described("a + b == c - d"),
        "((Identifier(a) + Identifier(b)) == (Identifier(c) - Identifier(d)))"
    );
    assert_eq!(
        described("a.n + f(x) - 1"),
        "((Member(Identifier(a), n) + Call(Identifier(f), [Identifier(x)])) - Int(1))"
    );
    assert_eq!(
        described("x => x + 1"),
        "Lambda([x], (Identifier(x) + Int(1)))"
    );
    assert_eq!(
        described("a + 1 > 2 && b"),
        "(((Identifier(a) + Int(1)) > Int(2)) && Identifier(b))"
    );
}

#[test]
fn the_three_spellings_each_take_what_their_role_allows() {
    let spelled = |src: &str| {
        let doc = parse_file(&format!("{src}\n")).unwrap();
        match &doc.body[0] {
            Block::Interp(interp) => interp.to_string(),
            other => panic!("{src}: {other:?}"),
        }
    };
    // A reference, written with or without the gate, is printed as `$(...)`.
    assert_eq!(spelled("$(var.title)"), "$(var.title)");
    assert_eq!(spelled("{$ var.title }"), "$(var.title)");
    assert_eq!(spelled("$(self::filename)"), "$(self::filename)");
    assert_eq!(spelled("$(#intro.tags)"), "$(#intro.tags)");
    assert_eq!(spelled("$(doc#id::meta.key)"), "$(doc#id::meta.key)");
    assert_eq!(spelled("$(1)"), "$(1)");
    // A call of a name is `$name(args)`.
    assert_eq!(spelled("$macro.gh(1)"), "$macro.gh(1)");
    assert_eq!(spelled("{$ macro.gh(1) }"), "$macro.gh(1)");
    assert_eq!(
        spelled("$addr(tmt:docs/a.tmt#intro)"),
        "$addr(tmt:docs/a.tmt#intro)"
    );
    assert_eq!(spelled("$emoji(\"sparkles\")"), "$emoji(\"sparkles\")");
    // Anything else keeps the gate.
    assert_eq!(spelled("{$ a + b }"), "{$ a + b }");
    assert_eq!(spelled("{$ \"text\" }"), "{$ \"text\" }");
    assert_eq!(
        spelled("{$ xs.map(x => x).len() }"),
        "{$ xs.map(x => x).len() }"
    );
    assert_eq!(spelled("{$ -1 }"), "{$ -1 }");
    assert_eq!(spelled("{$ true }"), "{$ true }");
    assert_eq!(spelled("{$ (a) }"), "{$ (a) }");
}

#[test]
fn a_reference_form_refuses_what_is_not_a_reference_and_says_where_it_goes() {
    for src in [
        "$(a + b)",
        "$(f(x))",
        "$(\"s\")",
        "$(x => x)",
        "$(true)",
        "$(-1)",
        "$((a))",
        "$()",
    ] {
        let err = parse_file(&format!("{src}\n")).unwrap_err();
        assert!(
            err.message.contains("reference") || err.message.contains("empty"),
            "{src}: {}",
            err.message
        );
    }
    assert!(
        parse_file("$(a + b)\n")
            .unwrap_err()
            .message
            .contains("{$ ... }")
    );
}

#[test]
fn a_gate_holds_a_closing_brace_that_is_inside_a_string() {
    assert_eq!(described("\"}\" == x"), "(String(\"}\") == Identifier(x))");
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
    let err = parse_file("{$ a < b < c }\n").unwrap_err();
    assert!(err.to_string().contains("chained"), "got: {err}");
    // Grouping one side is the way out.
    assert_eq!(
        described("(a < b) == c"),
        "(Group((Identifier(a) < Identifier(b))) == Identifier(c))"
    );
}

#[test]
fn every_binary_operator_is_read_from_its_own_symbol() {
    // The symbol is the node's own (`BinaryOp::symbol`); the parser has to read each one back as
    // that operator, and nothing else, or a printed expression reads as another.
    for op in tomet_ast::BinaryOp::ALL {
        let doc = parse_file(&format!("{{$ a {} b }}\n", op.symbol())).unwrap();
        match &interp_expr(&doc.body[0]).kind {
            InterpExprKind::Binary { op: read, .. } => {
                assert_eq!(*read, op, "{}", op.symbol());
            }
            other => panic!("{}: read as {other:?}", op.symbol()),
        }
    }
}

#[test]
fn a_lone_equals_ampersand_or_bar_is_an_error_that_names_the_fix() {
    for (src, hint) in [
        ("{$ a = b }", "=="),
        ("{$ a & b }", "&&"),
        ("{$ a | b }", "||"),
    ] {
        let err = parse_file(&format!("{src}\n")).unwrap_err();
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
    assert!(parse_file("{$ a => }\n").is_err());
    assert_eq!(described("a >= b"), "(Identifier(a) >= Identifier(b))");
}

#[test]
fn operators_work_inside_the_braceless_call_form() {
    let doc = parse_file("$f(a == b, c && d)\n").unwrap();
    assert_eq!(
        describe(interp_expr(&doc.body[0])),
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
    let doc = parse_file("@table\n|[ a ][ {$ x || y } ]\n|[ b ][ {$ p && q } ]\n").unwrap();
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
    assert!(parse_file("{$ self #intro }\n").is_err());
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
    let doc = parse_file("$include(addr(tmt:docs/a.tmt))\n").unwrap();
    assert_eq!(
        describe(interp_expr(&doc.body[0])),
        "Call(Identifier(include), [Addr(\"tmt:docs/a.tmt\", raw=true)])"
    );
    let doc = parse_file("$addr(dir:docs)\n").unwrap();
    assert_eq!(
        describe(interp_expr(&doc.body[0])),
        "Addr(\"dir:docs\", raw=true)"
    );
}

#[test]
fn addr_that_is_unterminated_or_empty_is_an_error() {
    assert!(parse_file("{$ addr(tmt:docs/a.tmt }\n").is_err());
    assert!(parse_file("$addr()\n").is_err());
    assert!(parse_file("$addr(\"\")\n").is_err());
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

#[test]
fn an_expression_may_run_over_several_lines() {
    let doc = parse_file("{$ xs\n    .filter(d => d.n > 1)\n    .map(d => d.n) }\n").unwrap();
    assert_eq!(
        describe(interp_expr(&doc.body[0])),
        "Call(Member(Call(Member(Identifier(xs), filter), [Lambda([d], (Member(Identifier(d), n) > Int(1)))]), map), [Lambda([d], Member(Identifier(d), n))])"
    );
}

#[test]
fn a_call_may_put_its_arguments_on_separate_lines() {
    let doc = parse_file("$f(\n  1,\n  2\n)\n").unwrap();
    assert_eq!(
        describe(interp_expr(&doc.body[0])),
        "Call(Identifier(f), [Int(1), Int(2)])"
    );
}

#[test]
fn a_multi_line_expression_in_a_list_item_is_still_one_item() {
    let doc = parse_file("- {$ xs\n    .len() }\n- next\n").unwrap();
    let Block::List(list) = &doc.body[0] else {
        panic!("a list, got {:?}", doc.body[0]);
    };
    assert_eq!(list.items.len(), 2);
}

#[test]
fn an_unterminated_multi_line_expression_is_still_an_error() {
    assert!(parse_file("{$ xs\n    .len()\n\nmore text\n").is_err());
}

#[test]
fn a_raw_address_still_ends_at_the_line() {
    assert!(parse_file("$addr(tmt:a\n.tmt)\n").is_err());
}

/// The CST ends `{$ ... }` at its own closing `}`: braces inside it are
/// counted, and a string literal is one token, so a `}` in a string does not
/// end the expression early.
#[test]
fn the_cst_keeps_a_brace_inside_an_interpolation_within_it() {
    use tomet_cst::SyntaxKind;
    for (src, expected) in [
        ("{$ \"a}b\" } tail #(x)\n", "{$ \"a}b\" }"),
        (
            "{$ addr(tmt:a.tmt#x) } tail #(x)\n",
            "{$ addr(tmt:a.tmt#x) }",
        ),
        ("{$ f(\"{\") } tail\n", "{$ f(\"{\") }"),
        ("$(a.b) tail #(x)\n", "$(a.b)"),
        ("$(\"}\") tail\n", "$(\"}\")"),
    ] {
        let root = tomet_cst_parser::parse_cst(src);
        let interps: Vec<String> = root
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::INTERP_EXPR)
            .map(|n| n.text().to_string())
            .collect();
        assert_eq!(interps, [expected], "{src:?}");
    }
}

#[test]
fn sequence_literals_in_expressions() {
    for (src, expected_desc, expected_print) in [
        ("{$ () }\n", "Seq([])", "{$ () }"),
        ("{$ (a,) }\n", "Seq([Identifier(a)])", "{$ (a,) }"),
        ("{$ (a, b) }\n", "Seq([Identifier(a), Identifier(b)])", "{$ (a, b) }"),
        ("{$ (a, b,) }\n", "Seq([Identifier(a), Identifier(b)])", "{$ (a, b) }"),
        ("{$ (a) }\n", "Group(Identifier(a))", "{$ (a) }"),
        (
            "{$ (\"a\", \"b\") }\n",
            r#"Seq([String("a"), String("b")])"#,
            r#"{$ ("a", "b") }"#,
        ),
        (
            "{$ ((1, 2), (3, 4)) }\n",
            "Seq([Seq([Int(1), Int(2)]), Seq([Int(3), Int(4)])])",
            "{$ ((1, 2), (3, 4)) }",
        ),
    ] {
        let doc = parse_file(src).unwrap();
        let expr = interp_expr(&doc.body[0]);
        assert_eq!(describe(expr), expected_desc, "{src}");
        let Block::Interp(interp) = &doc.body[0] else {
            panic!("expected Block::Interp");
        };
        assert_eq!(interp.to_string(), expected_print, "{src}");
    }
}
