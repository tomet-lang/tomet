//! `parse_cst` against the grammar: the CST must not leave structure flat
//! that `parse_document` turns into a form or a list.

use tomet_cst::SyntaxKind as K;

/// The text of every node of `kind` in `src`, in document order. Also checks
/// the tree is lossless, which every case here depends on.
fn texts(src: &str, kind: K) -> Vec<String> {
    let root = crate::parse_cst(src);
    assert_eq!(root.text().to_string(), src, "lossless: {src:?}");
    root.descendants()
        .filter(|n| n.kind() == kind)
        .map(|n| n.text().to_string())
        .collect()
}

fn kinds(src: &str, kind: K) -> usize {
    texts(src, kind).len()
}

/// How many tokens of `kind` the tree has.
fn tokens_of(src: &str, kind: K) -> usize {
    crate::parse_cst(src)
        .descendants_with_tokens()
        .filter(|e| e.kind() == kind)
        .count()
}

// --- a `|` run's margin ----------------------------------------------------

#[test]
fn a_runs_opener_is_a_pipe_and_the_bar_of_each_later_line_is_a_margin() {
    let src = "@callout\n| a\n| b\n| c\n";
    assert_eq!(tokens_of(src, K::PIPE), 1, "the opener");
    assert_eq!(tokens_of(src, K::MARGIN), 2, "the later lines");
    // Nested runs: every bar of the source is one or the other, none is lost or counted twice.
    let nested = "@a\n| x\n| @b\n| | y\n| | z\n";
    assert_eq!(
        tokens_of(nested, K::PIPE) + tokens_of(nested, K::MARGIN),
        nested.matches('|').count()
    );
    assert!(
        tokens_of(nested, K::MARGIN) >= 3,
        "the margins of the later lines"
    );
    // A bar in running text is a bar.
    assert_eq!(tokens_of("a | b\n", K::MARGIN), 0);
}

// --- mistakes the tree keeps ----------------------------------------------

#[test]
fn a_group_written_twice_against_itself_is_an_error_node_read_as_the_group_it_looks_like() {
    let src = "@T(a)(b)\n";
    assert_eq!(texts(src, K::ERROR), ["(b)"]);
    assert_eq!(
        kinds(src, K::ARGS),
        2,
        "the second is read as arguments too"
    );
    // Apart from the first, it is no mistake.
    assert_eq!(kinds("@T(a) (b)\n", K::ERROR), 0);
}

#[test]
fn the_text_after_a_declaration_on_its_line_is_an_error_node() {
    let src = "::meta{a: 1} text\n";
    assert_eq!(texts(src, K::BLOCK_FORM), ["::meta{a: 1} "]);
    assert_eq!(texts(src, K::ERROR), ["text\n"]);
}

// --- list markers ---------------------------------------------------------

#[test]
fn every_marker_the_ast_parser_reads_as_a_list_is_a_list() {
    for src in [
        "- plain\n",
        "-| pipe\n",
        "-[ bracket ]\n",
        "-( a ) text\n",
        "-{ k: 1 }\n",
        "-@link(ref: a)[x]\n",
        "-.| ordered pipe\n",
        "-. ordered\n",
        "-#(id) text\n",
    ] {
        assert_eq!(kinds(src, K::LIST_ITEM), 1, "{src:?}");
        // The list is the whole block: a paragraph inside a `[ ]` is the item's own.
        let root = crate::parse_cst(src);
        let top: Vec<K> = root.children().map(|n| n.kind()).collect();
        assert_eq!(top, [K::LIST], "{src:?}");
    }
}

#[test]
fn a_minus_that_opens_nothing_is_prose() {
    for src in ["-5 apples\n", "-x\n", "--x\n", "-\n", "-.x\n"] {
        assert_eq!(kinds(src, K::LIST), 0, "{src:?}");
    }
}

#[test]
fn a_list_marker_ends_the_paragraph_before_it() {
    let src = "text\n-| item\n";
    let root = crate::parse_cst(src);
    let top: Vec<K> = root.children().map(|n| n.kind()).collect();
    assert_eq!(top, [K::PARAGRAPH, K::LIST]);
}

// --- emphasis -------------------------------------------------------------

#[test]
fn delimited_spans_are_nodes() {
    assert_eq!(texts("a *em* b\n", K::EMPHASIS), ["*em*"]);
    assert_eq!(texts("a _em_ b\n", K::EMPHASIS), ["_em_"]);
    assert_eq!(texts("a _two words_ b\n", K::EMPHASIS), ["_two words_"]);
    assert_eq!(texts("a **strong** b\n", K::STRONG), ["**strong**"]);
    assert_eq!(texts("a __strong__ b\n", K::STRONG), ["__strong__"]);
    assert_eq!(texts("a ~~gone~~ b\n", K::STRIKEOUT), ["~~gone~~"]);
}

#[test]
fn a_delimiter_can_close_in_the_middle_of_a_word() {
    // The tokenizer reads `_em_s` as one identifier; `parse_document` closes at
    // the second `_` all the same.
    assert_eq!(texts("a _em_s b\n", K::EMPHASIS), ["_em_"]);
}

#[test]
fn delimited_spans_nest() {
    assert_eq!(texts("**a *b* c**\n", K::STRONG), ["**a *b* c**"]);
    assert_eq!(texts("**a *b* c**\n", K::EMPHASIS), ["*b*"]);
}

#[test]
fn delimiters_that_do_not_open_stay_prose() {
    for src in [
        "a * b * c\n",
        "snake_case_name\n",
        "x_y_z and w\n",
        "*unclosed\n",
        "_unclosed\n",
    ] {
        assert_eq!(kinds(src, K::EMPHASIS), 0, "{src:?}");
        assert_eq!(kinds(src, K::STRONG), 0, "{src:?}");
    }
}

#[test]
fn emphasis_works_inside_a_form_group() {
    assert_eq!(texts("@note[ *x* and **y** ]\n", K::EMPHASIS), ["*x*"]);
    assert_eq!(texts("@note[ *x* and **y** ]\n", K::STRONG), ["**y**"]);
}

#[test]
fn emphasis_does_not_swallow_the_group_that_closes_around_it() {
    let src = "@note[ *x ]\n";
    assert_eq!(kinds(src, K::CONTENT), 1);
    assert_eq!(texts(src, K::CONTENT), ["[ *x ]"]);
}

// --- code spans -----------------------------------------------------------

#[test]
fn a_code_span_is_a_node_and_its_inside_is_not_parsed() {
    assert_eq!(texts("a `code` b\n", K::CODE_SPAN), ["`code`"]);
    assert_eq!(texts("`*not em*`\n", K::CODE_SPAN), ["`*not em*`"]);
    assert_eq!(kinds("`*not em*`\n", K::EMPHASIS), 0);
    assert_eq!(kinds("`@x(a)[b]`\n", K::INLINE_FORM), 0);
}

#[test]
fn a_code_span_is_single_backtick_only_and_non_empty() {
    assert_eq!(texts("x `` c\n", K::CODE_SPAN), Vec::<String>::new());
    assert_eq!(texts("x ``` c\n", K::CODE_SPAN), Vec::<String>::new());
    assert_eq!(texts("x `a` and \\`b\\` c\n", K::CODE_SPAN), ["`a`"]);
    assert_eq!(texts("x `a` and \\`b\\` c\n", K::ESCAPE), ["\\`", "\\`"]);
}

#[test]
fn two_backticks_at_the_start_of_a_line_are_a_line_raw_not_a_code_span() {
    assert_eq!(texts("``a`b`` c\n", K::CODE_SPAN), Vec::<String>::new());
    assert_eq!(texts("``a`b`` c\n", K::CODE_BLOCK), ["``a`b`` c\n"]);
    // Consecutive lines are one node.
    assert_eq!(texts("`` a\n`` b\nc\n", K::CODE_BLOCK), ["`` a\n`` b\n"]);
}

#[test]
fn a_comment_marker_inside_a_code_span_is_code() {
    // The tokenizer reads `// b` as a comment up to the end of the line,
    // closing backtick included; the code span still ends at that backtick.
    assert_eq!(texts("`a // b` tail\n", K::CODE_SPAN), ["`a // b`"]);
    assert_eq!(kinds("`a // b` tail\n", K::COMMENT), 0);
}

#[test]
fn an_unclosed_code_span_is_prose() {
    assert_eq!(kinds("`oops\n", K::CODE_SPAN), 0);
    assert_eq!(kinds("`oops\nnext` line\n", K::CODE_SPAN), 0);
}

// --- autolinks ------------------------------------------------------------

#[test]
fn a_url_is_an_autolink_without_its_trailing_punctuation() {
    assert_eq!(
        texts("see https://a.b/c?d=1, ok\n", K::AUTOLINK),
        ["https://a.b/c?d=1"]
    );
    assert_eq!(texts("mail mailto:x@y.z.\n", K::AUTOLINK), ["mailto:x@y.z"]);
    assert_eq!(
        texts("@note[ https://a.b/c ]\n", K::AUTOLINK),
        ["https://a.b/c"]
    );
}

#[test]
fn a_url_does_not_start_inside_a_word() {
    assert_eq!(kinds("xhttps://a.b\n", K::AUTOLINK), 0);
}

// --- `|` runs ---------------------------------------------------------------

#[test]
fn a_pipe_opens_the_content_of_a_form() {
    assert_eq!(texts("@quote| alpha\n", K::CONTENT), ["| alpha"]);
    assert_eq!(
        texts("@quote(A)\n| alpha\n| beta\n", K::CONTENT),
        ["| alpha\n| beta"]
    );
    // It is the form's: the form owns it.
    assert_eq!(
        texts("@quote\n| alpha\n", K::BLOCK_FORM),
        ["@quote\n| alpha"]
    );
}

#[test]
fn the_markers_stay_in_the_tree_as_margin_tokens() {
    let root = crate::parse_cst("@quote\n| alpha\n| beta\n");
    let count = |kind| {
        root.descendants_with_tokens()
            .filter(|e| e.kind() == kind)
            .count()
    };
    assert_eq!(count(K::PIPE), 1, "the opener");
    assert_eq!(count(K::MARGIN), 1, "the marker of the second line");
    assert_eq!(
        texts("@quote\n| alpha\n| beta\n", K::PARAGRAPH),
        ["alpha\n| beta"]
    );
}

#[test]
fn a_blank_line_ends_the_run() {
    let src = "@quote\n| a\n| b\n\nafter\n";
    assert_eq!(texts(src, K::CONTENT), ["| a\n| b"]);
    assert_eq!(kinds(src, K::PARAGRAPH), 2);
}

#[test]
fn a_marker_alone_on_its_line_starts_a_new_paragraph() {
    let src = "@quote\n| alpha\n|\n| beta\n";
    assert_eq!(kinds(src, K::PARAGRAPH), 2);
    assert_eq!(kinds(src, K::CONTENT), 1);
}

#[test]
fn runs_nest_by_column() {
    let src = "@tree\n| @tree\n| | alpha\n| |\n| | beta\n";
    assert_eq!(kinds(src, K::CONTENT), 2);
    assert_eq!(kinds(src, K::BLOCK_FORM), 2);
    assert_eq!(kinds(src, K::PARAGRAPH), 2);
}

#[test]
fn a_marker_in_the_wrong_column_ends_the_run_and_loses_nothing() {
    // `parse_document` rejects this; the CST never fails.
    let src = "- (x)| a\n  - (y)| b\n| c\n";
    assert_eq!(texts(src, K::LIST_ITEM).len(), 2);
}

#[test]
fn a_pipe_that_continues_nothing_is_prose() {
    for src in ["| a | b |\n", "a | b\n", "x\n| y\n"] {
        assert_eq!(kinds(src, K::CONTENT), 0, "{src:?}");
    }
}

#[test]
fn a_pipe_opens_the_content_of_a_list_item_and_a_heading() {
    assert_eq!(
        texts("-| alpha\n | beta\n", K::CONTENT),
        ["| alpha\n | beta"]
    );
    assert_eq!(
        texts("- | alpha\n  | beta\n", K::CONTENT),
        ["| alpha\n  | beta"]
    );
    assert_eq!(
        texts("=| alpha\n | beta\n", K::CONTENT),
        ["| alpha\n | beta"]
    );
    assert_eq!(kinds("=| alpha\n | beta\n", K::SECTION_HEADING), 1);
}

#[test]
fn blocks_inside_a_run_are_blocks() {
    let src = "@container\n| - alpha\n| - beta\n| trailing\n";
    assert_eq!(kinds(src, K::LIST), 1);
    assert_eq!(kinds(src, K::LIST_ITEM), 2);
    let src = "@quote\n| ```rust\n| fn foo() {}\n| ```\n";
    assert_eq!(kinds(src, K::CODE_BLOCK), 1);
    let src = "@card\n| above\n| ---\n| below\n";
    assert_eq!(kinds(src, K::THEMATIC_BREAK), 1);
}

#[test]
fn a_pipe_run_is_a_value() {
    let src = "@x{\n  about: | one, {two}\n         | three\n  k: 1\n}\n";
    // The run is the entry's value, closed by its markers, not by the `,` or `}`.
    assert_eq!(texts(src, K::CONTENT), ["| one, {two}\n         | three"]);
    assert_eq!(kinds(src, K::MAP_ENTRY), 2);
    assert_eq!(kinds(src, K::SEQ_ITEM), 0);
}

// --- `[ ... ]` as a value ---------------------------------------------------

#[test]
fn a_bracket_in_a_value_is_content_and_a_code_span_cannot_close_it() {
    // `[ ... ]` in a value is read as blocks (`Value::Blocks`).
    // Read as a list it closed at the `]` inside the code span.
    let src = "a @conflict(a: [ use `]` here ], b: [ x ]) b\n";
    assert_eq!(texts(src, K::CONTENT), ["[ use `]` here ]", "[ x ]"]);
    assert_eq!(texts(src, K::CODE_SPAN), ["`]`"]);
    assert_eq!(kinds(src, K::INLINE_FORM), 1);
}

#[test]
fn a_value_content_may_hold_blocks() {
    let src = "@conflict(\n  a: [\n    - one\n    - two\n  ]\n  b: [ x ]\n)\n";
    assert_eq!(kinds(src, K::LIST_ITEM), 2);
    assert_eq!(kinds(src, K::CONTENT), 2);
}

// --- quotes in running text ---------------------------------------------------

#[test]
fn a_quote_in_a_sentence_swallows_nothing() {
    // `'s` after a code span looked like the start of a string and took the
    // rest of the line, and with it the form at the end.
    let src = "`a`'s own merge, after @conflict(a: [ L ], b: [ R ])\nnext.\n";
    assert_eq!(kinds(src, K::INLINE_FORM), 1);
    assert_eq!(tokens_of(src, K::STRING_LITERAL), 0);
    let src = "He said \"see *this* and `that`\" twice.\n";
    assert_eq!(texts(src, K::EMPHASIS), ["*this*"]);
    assert_eq!(texts(src, K::CODE_SPAN), ["`that`"]);
}

#[test]
fn a_string_is_still_a_string_in_a_value() {
    let src = "@x(k: \"v w\", n: 'a')\n";
    assert_eq!(tokens_of(src, K::STRING_LITERAL), 2);
}

// --- `;` -------------------------------------------------------------------

#[test]
fn a_semicolon_at_the_start_of_a_line_breaks_the_paragraph() {
    assert_eq!(kinds("a\n;b\n", K::PARAGRAPH), 2);
    let semis = crate::parse_cst("a\n;b\n")
        .descendants_with_tokens()
        .filter(|e| e.kind() == K::SEMI)
        .count();
    assert_eq!(semis, 1);
    // In the middle of a line it is part of the text.
    assert_eq!(kinds("a;b\n", K::PARAGRAPH), 1);
}

#[test]
fn a_semicolon_stands_blocks_of_forms_side_by_side() {
    assert_eq!(kinds(";@a()\n;@b()\n", K::BLOCK_FORM), 2);
}

// --- fences ------------------------------------------------------------------

#[test]
fn a_fence_closes_on_a_line_of_its_own_and_not_on_a_run_in_the_middle_of_one() {
    // The body has a line that ends in three backticks. It closes nothing:
    // only a line that is a run of backticks and white space does.
    let src = "```tm\n@codeblock | ```\n@next\n```\nafter\n";
    assert_eq!(kinds(src, K::CODE_BLOCK), 1);
    assert_eq!(
        texts(src, K::CODE_BLOCK),
        ["```tm\n@codeblock | ```\n@next\n```"]
    );
    // A longer opening run is how a body holds a line that looks like a closing one.
    let src = "````\n```\n````\n";
    assert_eq!(texts(src, K::CODE_BLOCK), ["````\n```\n````"]);
}

#[test]
fn a_section_may_have_an_id_and_no_title() {
    let src = "==#(background)\n\ntext\n";
    assert_eq!(kinds(src, K::SECTION), 1);
    assert_eq!(kinds(src, K::ID_GROUP), 1);
    assert_eq!(texts(src, K::SECTION_HEADING), ["==#(background)\n"]);
}

// --- lists -------------------------------------------------------------------

#[test]
fn a_deeper_marker_opens_the_sublist_of_the_item_before_it() {
    let src = "- a\n  - b\n- c\n";
    let root = crate::parse_cst(src);
    let top: Vec<_> = root.children().filter(|n| n.kind() == K::LIST).collect();
    assert_eq!(top.len(), 1, "one list at the top, not one per indentation");
    assert_eq!(
        top[0]
            .children()
            .filter(|n| n.kind() == K::LIST_ITEM)
            .count(),
        2
    );
    assert_eq!(texts(src, K::LIST).len(), 2, "and the sublist inside it");
    assert_eq!(texts(src, K::LIST)[1], "  - b\n");
}

#[test]
fn a_list_ends_where_the_kind_of_marker_changes() {
    let src = "-. a\n-. b\n- c\n";
    let root = crate::parse_cst(src);
    assert_eq!(
        root.children().filter(|n| n.kind() == K::LIST).count(),
        2,
        "`-.` and `-` are two lists"
    );
}

// --- groups on the next line ---------------------------------------------------

#[test]
fn a_group_may_stand_on_the_line_after_its_form() {
    // `[ text ]` here is the form's content.
    let src = "@file(a)\n[ text ]\n";
    assert_eq!(kinds(src, K::BLOCK_FORM), 1);
    assert_eq!(
        kinds(src, K::PARAGRAPH),
        1,
        "the paragraph inside the content"
    );
    assert_eq!(texts(src, K::BLOCK_FORM), ["@file(a)\n[ text ]"]);
    // A blank line in between ends the form.
    let src = "@file(a)\n\n[ text ]\n";
    assert_eq!(texts(src, K::BLOCK_FORM), ["@file(a)"]);
    // Trailing non-group content on the next line cannot join.
    let src = "@file(a)\ntrailing\n";
    assert_eq!(kinds(src, K::BLOCK_FORM), 0);
    assert_eq!(texts(src, K::INLINE_FORM), ["@file(a)"]);
}

#[test]
fn a_group_whose_slot_is_taken_is_not_pulled_in_from_the_next_line() {
    // `(b)` would be a second `args` group, so it is text; and as text on the
    // next line it folds into the form's line.
    let src = "@file(a)\n(b)\n";
    assert_eq!(kinds(src, K::BLOCK_FORM), 0);
    assert_eq!(texts(src, K::INLINE_FORM), ["@file(a)"]);
    assert_eq!(kinds(src, K::PARAGRAPH), 1);
}

// --- backslash escapes -----------------------------------------------------

#[test]
fn backslash_escapes_ascii_punctuation_runs_and_not_words_or_spaces() {
    assert_eq!(
        texts(
            "\\; \\* \\** \\` \\`` \\``` \\// \\/* \\:: \\=== \\--- \\\\\n",
            K::ESCAPE
        ),
        [
            "\\;", "\\*", "\\**", "\\`", "\\``", "\\```", "\\//", "\\/", "\\::", "\\===", "\\---",
            "\\\\"
        ]
    );
    // Unescaped backslash before alphanumeric, whitespace, or non-ASCII is plain text.
    for src in ["a\\b\n", "C:\\Users\\name\n", "a\\ b\n", "\\あ\n", "\\\n"] {
        assert_eq!(kinds(src, K::ESCAPE), 0, "{src:?}");
    }
}

#[test]
fn backslash_escapes_prevent_inline_and_block_triggers() {
    for src in [
        "\\@name(x) \\${x} \\<slot>\n",
        "\\*not em*\n",
        "*not em\\*\n",
        "\\**not strong**\n",
        "**not strong\\**\n",
    ] {
        assert_eq!(kinds(src, K::INLINE_FORM), 0, "{src:?}");
        assert_eq!(kinds(src, K::SLOT), 0, "{src:?}");
        assert_eq!(kinds(src, K::EMPHASIS), 0, "{src:?}");
        assert_eq!(kinds(src, K::STRONG), 0, "{src:?}");
    }

    // Double backslash escapes the backslash, leaving `*em*` active.
    let src = "\\\\*em*\n";
    assert_eq!(texts(src, K::ESCAPE), ["\\\\"]);
    assert_eq!(texts(src, K::EMPHASIS), ["*em*"]);

    // Escaped brackets inside `@note[...]` do not close or open the content group.
    let src = "@note[ a \\[ b \\] c ]\n";
    assert_eq!(texts(src, K::CONTENT), ["[ a \\[ b \\] c ]"]);
    assert_eq!(texts(src, K::ESCAPE), ["\\[", "\\]"]);

    // Escaped trailing groups in headings and list items stay in the body.
    let src = "= Heading \\#(id) \\:connect(x)\n- item \\#(id) \\:connect(x)\n";
    assert_eq!(kinds(src, K::ID_GROUP), 0);
    assert_eq!(kinds(src, K::CONNECT), 0);
    assert_eq!(texts(src, K::ESCAPE), ["\\#", "\\:", "\\#", "\\:"]);
}
