//! `parse_cst` against the grammar `parse_document` reads: the CST must not
//! leave structure flat that the AST parser turns into an element or a list.

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
        // The AST parser agrees.
        let doc = crate::parse_document(src).unwrap();
        assert!(
            matches!(doc.blocks.as_slice(), [tomet_ast::Block::List(_)]),
            "{src:?}: {doc:?}"
        );
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
    // The tokenizer reads `_em_s` as one identifier; the AST parser closes at
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
fn emphasis_works_inside_an_element_group() {
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
    assert_eq!(kinds("`@x(a)[b]`\n", K::INLINE_ELEMENT), 0);
}

#[test]
fn a_code_span_closes_on_a_fence_of_the_same_length() {
    assert_eq!(texts("``a`b`` c\n", K::CODE_SPAN), ["``a`b``"]);
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

// --- parity ---------------------------------------------------------------

/// Whatever the AST parser turns into these elements, the CST has a node for.
#[test]
fn the_cst_and_the_ast_agree_on_inline_structure() {
    use tomet_ast::{Block, Inline};
    fn names(inlines: &[Inline], out: &mut Vec<String>) {
        for i in inlines {
            if let Inline::Element(e) = i {
                if let Some(n) = e.sigil.name() {
                    out.push(n.name.clone());
                }
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
        let doc = crate::parse_document(src).unwrap();
        let mut ast = Vec::new();
        for b in &doc.blocks {
            if let Block::Paragraph(p) = b {
                names(&p.content, &mut ast);
            }
        }
        ast.sort();
        let mut cst: Vec<String> = Vec::new();
        for (kind, name) in [
            (K::EMPHASIS, "em"),
            (K::STRONG, "strong"),
            (K::STRIKEOUT, "strikeout"),
            (K::CODE_SPAN, "raw"),
            (K::AUTOLINK, "link"),
        ] {
            cst.extend(std::iter::repeat_n(name.to_string(), kinds(src, kind)));
        }
        cst.sort();
        assert_eq!(cst, ast, "{src:?}");
    }
}

// --- `|` runs ---------------------------------------------------------------

#[test]
fn a_pipe_opens_the_content_of_an_element() {
    assert_eq!(texts("@quote| alpha\n", K::CONTENT), ["| alpha"]);
    assert_eq!(
        texts("@quote(A)\n| alpha\n| beta\n", K::CONTENT),
        ["| alpha\n| beta"]
    );
    // It is the element's: the element owns it.
    assert_eq!(
        texts("@quote\n| alpha\n", K::BLOCK_ELEMENT),
        ["@quote\n| alpha"]
    );
}

#[test]
fn the_markers_stay_in_the_tree_as_pipe_tokens() {
    let root = crate::parse_cst("@quote\n| alpha\n| beta\n");
    let pipes = root
        .descendants_with_tokens()
        .filter(|e| e.kind() == K::PIPE)
        .count();
    assert_eq!(pipes, 2, "the opener and one marker");
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
    assert_eq!(kinds(src, K::BLOCK_ELEMENT), 2);
    assert_eq!(kinds(src, K::PARAGRAPH), 2);
}

#[test]
fn a_marker_in_the_wrong_column_ends_the_run_and_loses_nothing() {
    // The AST parser rejects this; the CST never fails.
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
