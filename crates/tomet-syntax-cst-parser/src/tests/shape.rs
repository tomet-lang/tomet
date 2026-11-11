//! The shape of the tree `parse_cst` builds, construct by construct: what is a node of
//! which kind, what nests in what, and that the text always survives.

use tomet_cst::{SyntaxKind, SyntaxKind as K, SyntaxNode};

use crate::parse_cst;

/// Kinds of `node`'s child nodes, in order.
fn child_kinds(node: &SyntaxNode) -> Vec<SyntaxKind> {
    node.children().map(|n| n.kind()).collect()
}

fn find(root: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxNode> {
    root.descendants().find(|n| n.kind() == kind)
}

fn count(root: &SyntaxNode, kind: SyntaxKind) -> usize {
    root.descendants().filter(|n| n.kind() == kind).count()
}

#[test]
fn test_cst_lossless_roundtrip_all_syntaxes() {
    let cases = [
        "#[ Heading ]\n\nParagraph text with *em* and @tag[ content ].\n",
        "::config{ format: json }\n",
        "- item 1\n- item 2\n-. ordered 1\n\n```rust\nfn main() {}\n```\n",
        "// Comment line\n@card(id: 123){ priority: high }[ Note ]\n",
        "=[ A ]=\n== B ==\n=\n---\n---[ x ]---\n\ntext $(a.b) $f(x) @l[t](ref:y)::rule(a)\n",
        // Unterminated everything: still lossless.
        "@a(b: {c: [1, 2\n=[ open\n{$ x\n@f[ never closed",
        "",
        "\n\n  \n",
    ];

    for case in cases {
        let root = parse_cst(case);
        let roundtripped = root.text().to_string();
        assert_eq!(roundtripped, case);
    }
}

#[test]
fn test_cst_node_hierarchy() {
    let src = "=[ Title ]\n\nParagraph\n";
    let root = parse_cst(src);
    assert_eq!(root.kind(), K::ROOT);

    let children: Vec<_> = root.children().map(|n| n.kind()).collect();
    assert!(children.contains(&K::SECTION));
}

#[test]
fn old_hash_heading_is_plain_paragraph() {
    // `#` is free: it is plain text, not an ATX-style heading marker.
    let root = parse_cst("#[ Title ]\n\nParagraph\n");
    assert_eq!(child_kinds(&root), [K::PARAGRAPH, K::PARAGRAPH]);
}

#[test]
fn sections_nest_by_level() {
    let src = "=[ A ]\n\ntext\n\n==[ A1 ]\n\ninner\n\n==[ A2 ]\n\n=[ B ]\n\nlast\n";
    let root = parse_cst(src);
    assert_eq!(child_kinds(&root), [K::SECTION, K::SECTION]);
    let a = root.children().next().unwrap();
    assert_eq!(
        child_kinds(&a),
        [K::SECTION_HEADING, K::PARAGRAPH, K::SECTION, K::SECTION]
    );
    let a1 = a.children().nth(2).unwrap();
    assert_eq!(child_kinds(&a1), [K::SECTION_HEADING, K::PARAGRAPH]);
    // 3 real bodies ("text"/"inner"/"last") + 4 section titles ("A"/
    // "A1"/"A2"/"B"), each title's own `[...]` wrapping its bare
    // inline content in a `PARAGRAPH` the same way any other content
    // group does.
    assert_eq!(count(&root, K::PARAGRAPH), 7);
}

#[test]
fn section_forms() {
    // Bracketed, sugar, decorative `=`, and heading-less.
    for src in [
        "=[ T ]\n",
        "= T\n",
        "=[ T ]=\n",
        "== T ==\n",
        "=\n",
        "== // c\n",
    ] {
        let root = parse_cst(src);
        assert_eq!(child_kinds(&root), [K::SECTION], "{src:?}");
        assert_eq!(root.text().to_string(), src);
    }
    // `==mark==` is inline emphasis, not a section.
    let root = parse_cst("==mark== text\n");
    assert_eq!(child_kinds(&root), [K::PARAGRAPH]);
    // A bracketed title is CONTENT inside the heading.
    let heading = find(&parse_cst("=[ T ]\n"), K::SECTION_HEADING).unwrap();
    assert_eq!(child_kinds(&heading), [K::CONTENT]);
}

#[test]
fn thematic_breaks() {
    let root = parse_cst("---\n---[ Title ]---\n- item\n");
    assert_eq!(
        child_kinds(&root),
        [K::THEMATIC_BREAK, K::THEMATIC_BREAK, K::LIST]
    );
    // `|` lines are the title of the break, then the closing fence: one node, whole.
    let src = "---\n| one\n| two\n---\n- item\n";
    let root = parse_cst(src);
    assert_eq!(child_kinds(&root), [K::THEMATIC_BREAK, K::LIST]);
    let hr = find(&root, K::THEMATIC_BREAK).unwrap();
    assert_eq!(hr.text().to_string(), "---\n| one\n| two\n---\n");
    assert_eq!(
        child_kinds(&hr),
        [K::FENCE, K::CONTENT, K::FENCE],
        "{src:?}"
    );
    // Without the closing fence the break is an error, and the line after it is not its own.
    let root = parse_cst("---\n| one\nafter\n");
    let hr = find(&root, K::THEMATIC_BREAK).unwrap();
    assert_eq!(child_kinds(&hr), [K::FENCE, K::CONTENT, K::ERROR]);
    assert_eq!(hr.text().to_string(), "---\n| one");
}

#[test]
fn inline_forms_are_structured() {
    let src = "See @link[label](ref:x) and @ftn(1) and $(a.b) and $sum(1, 2) and {$ a + b }.\n";
    let root = parse_cst(src);
    let para = find(&root, K::PARAGRAPH).unwrap();
    let forms: Vec<_> = para
        .children()
        .filter(|n| n.kind() == K::INLINE_FORM)
        .collect();
    assert_eq!(forms.len(), 5);

    assert_eq!(child_kinds(&forms[0]), [K::SIGIL, K::CONTENT, K::ARGS]);
    assert_eq!(forms[0].text().to_string(), "@link[label](ref:x)");
    assert_eq!(child_kinds(&forms[1]), [K::SIGIL, K::ARGS]);
    assert_eq!(forms[1].text().to_string(), "@ftn(1)");
    // `$(ref)` and `{$ expr }` are one `INTERP_EXPR`, the whole form.
    assert_eq!(child_kinds(&forms[2]), [K::INTERP_EXPR]);
    assert_eq!(forms[2].text().to_string(), "$(a.b)");
    assert_eq!(child_kinds(&forms[3]), [K::SIGIL, K::ARGS]);
    assert_eq!(child_kinds(&forms[4]), [K::INTERP_EXPR]);
    assert_eq!(forms[4].text().to_string(), "{$ a + b }");

    // `ref:x` is a structured entry.
    let entry = find(&forms[0], K::MAP_ENTRY).unwrap();
    assert_eq!(entry.text().to_string(), "ref:x");
}

#[test]
fn an_expression_closes_at_the_bracket_that_opened_it_and_counts_only_that_pair() {
    // In `{$ ... }` only braces count: an unclosed `(` does not swallow the `}`.
    let root = parse_cst("a {$ (a } b\n");
    let form = find(&root, K::INTERP_EXPR).unwrap();
    assert_eq!(form.text().to_string(), "{$ (a }");
    // A nested brace pair is counted, and a `)` closes nothing.
    let root = parse_cst("a {$ f({x}) } b\n");
    let form = find(&root, K::INTERP_EXPR).unwrap();
    assert_eq!(form.text().to_string(), "{$ f({x}) }");

    // In `$( ... )` only parentheses count: a stray `}` closes nothing.
    let root = parse_cst("a $(x } y) b\n");
    let form = find(&root, K::INTERP_EXPR).unwrap();
    assert_eq!(form.text().to_string(), "$(x } y)");
    // A nested pair is counted.
    let root = parse_cst("a $(f(x)) b\n");
    let form = find(&root, K::INTERP_EXPR).unwrap();
    assert_eq!(form.text().to_string(), "$(f(x))");

    // A string closes nothing, whichever bracket it holds.
    for (src, form_text) in [
        ("a {$ \"}\" } b\n", "{$ \"}\" }"),
        ("a $(\")\") b\n", "$(\")\")"),
        ("a $(\"(\") b\n", "$(\"(\")"),
    ] {
        let root = parse_cst(src);
        let form = find(&root, K::INTERP_EXPR).unwrap();
        assert_eq!(form.text().to_string(), form_text, "{src:?}");
    }

    // Two forms on a line stay two.
    let root = parse_cst("{$ (a } and {$ b }\n");
    let texts: Vec<String> = root
        .descendants()
        .filter(|n| n.kind() == K::INTERP_EXPR)
        .map(|n| n.text().to_string())
        .collect();
    assert_eq!(texts, ["{$ (a }", "{$ b }"]);
}

#[test]
fn plain_at_and_dollar_are_text() {
    for src in [
        "mail me @ home\n",
        "a@b.com\n",
        "x ^ y\n",
        "cost $5\n",
        "braces {x} and { $ x } and $ (x)\n",
        "@name\n\n",
    ] {
        let root = parse_cst(src);
        assert_eq!(count(&root, K::INLINE_FORM), 0, "{src:?}");
    }
}

#[test]
fn inline_forms_nest_in_content() {
    let root = parse_cst("@note[ see @link[x](ref:y) ]{ k: v }\n");
    let el = find(&root, K::BLOCK_FORM).unwrap();
    assert_eq!(child_kinds(&el), [K::SIGIL, K::CONTENT, K::VALUE_DATA]);
    let content = find(&el, K::CONTENT).unwrap();
    assert_eq!(count(&content, K::INLINE_FORM), 1);
}

#[test]
fn block_level_constructs_nest_in_content_with_their_own_id() {
    // `[content]` recurses into the same block grammar ROOT/SECTION
    // use, not just inline text -- a section (or list, code block)
    // nested inside it is a real `SECTION`/`ID_GROUP`, not flattened
    // into `CONTENT`'s plain text. This is what lets
    // `tomet-semantics-validator::collect_ids_cst` (the LSP's only
    // duplicate-id check) see a duplicate id nested this way.
    let root = parse_cst("@outer[\n=[ Heading ]#(x)\n\nbody\n]\n");
    let el = find(&root, K::BLOCK_FORM).unwrap();
    let content = find(&el, K::CONTENT).unwrap();
    let section = find(&content, K::SECTION).unwrap();
    assert_eq!(child_kinds(&section), [K::SECTION_HEADING, K::PARAGRAPH]);
    let heading = find(&section, K::SECTION_HEADING).unwrap();
    let id_group = find(&heading, K::ID_GROUP).unwrap();
    assert_eq!(id_group.text().to_string(), "#(x)");
}

#[test]
fn a_literal_bracket_in_content_prose_does_not_end_it_early() {
    // `array[0]` inside `[content]`: the inner `[`/`]` are plain
    // text, not recognized as a nested group (nothing precedes `[`
    // that would make it one), so the outer group's own `]` -- the
    // *third* one here -- is the one that actually closes it.
    // The paragraph's `]` is found by counting depth.
    let root = parse_cst("@a[ array[0] is nice ]\n");
    let content = find(&root, K::CONTENT).unwrap();
    assert_eq!(content.text().to_string(), "[ array[0] is nice ]");
}

#[test]
fn bare_paragraph_content_spanning_lines_does_not_swallow_the_close() {
    // No blank line, no new block starting the second line -- the
    // paragraph's own bracket-depth tracking has to stop it at `]`,
    // even after crossing a newline.
    let root = parse_cst("@card[ hello\nworld ]\nafter\n");
    let content = find(&root, K::CONTENT).unwrap();
    assert_eq!(content.text().to_string(), "[ hello\nworld ]");
    // One paragraph inside the content group, one for "after".
    assert_eq!(count(&root, K::PARAGRAPH), 2);
}

#[test]
fn block_form_needs_a_bare_line() {
    // Ends its line: a block. Text follows: part of a paragraph.
    assert_eq!(
        child_kinds(&parse_cst("@card(id: a){ p: 1 }\n")),
        [K::BLOCK_FORM]
    );
    assert_eq!(child_kinds(&parse_cst("::meta\n")), [K::BLOCK_FORM]);
    let root = parse_cst("@link[a](ref:b) trailing\n");
    assert_eq!(child_kinds(&root), [K::PARAGRAPH]);
    assert_eq!(count(&root, K::INLINE_FORM), 1);
}

#[test]
fn groups_are_structured() {
    let root = parse_cst("@x(a: 1, (p, q), name(r), \"k\": { n: [1, 2] })\n");
    let args = find(&root, K::ARGS).unwrap();
    assert_eq!(
        child_kinds(&args),
        [K::MAP_ENTRY, K::SEQ_ITEM, K::SEQ_ITEM, K::MAP_ENTRY]
    );
    // `(p, q)` is a sequence, `name(r)` a call: each an ARGS inside its entry.
    let item = args.children().nth(1).unwrap();
    assert_eq!(item.text().to_string(), "(p, q)");
    assert_eq!(count(&item, K::ARGS), 1);
    let call = args.children().nth(2).unwrap();
    assert_eq!(call.text().to_string(), "name(r)");
    assert_eq!(count(&call, K::ARGS), 1);
    assert_eq!(count(&root, K::MAP_LITERAL), 1);
    // `[1, 2]` as a value is blocks: a content, not a list.
    assert_eq!(count(&root, K::CONTENT), 1);
    // `(p, q)` and its `p` and `q`, `name(r)` and its `r`.
    assert_eq!(count(&root, K::SEQ_ITEM), 5);
}

#[test]
fn value_data_holds_entries() {
    let root = parse_cst("::meta{\n  _id: intro\n  tags: [a, b]\n}\n");
    let data = find(&root, K::VALUE_DATA).unwrap();
    let entries: Vec<_> = data.children().map(|n| n.text().to_string()).collect();
    assert_eq!(entries, ["_id: intro", "tags: [a, b]"]);
}

#[test]
fn connect_joins_the_form() {
    let root = parse_cst("@link[x](ref:y)::rule(allow: (a))\n");
    let el = find(&root, K::BLOCK_FORM).unwrap();
    assert_eq!(
        child_kinds(&el),
        [K::SIGIL, K::CONTENT, K::ARGS, K::CONNECT]
    );
    assert_eq!(
        find(&el, K::CONNECT).unwrap().text().to_string(),
        "::rule(allow: (a))"
    );
    // A list item's own trailing group is not claimed by its form.
    let root = parse_cst("- @link(ref:x) ::{ id: b }\n");
    let el = find(&root, K::INLINE_FORM).unwrap();
    assert_eq!(count(&el, K::CONNECT), 0);
}

#[test]
fn full_form_list_item_claims_its_own_trailing_connect() {
    // A full-form item's own groups reach the connect arm directly
    // (nothing else sits between the closing `]` and the `:`), so this
    // mirrors a plain form's own connect -- the list item only
    // disables connects for the rest of the line (the sugar/trailing-
    // text scan), not for the item's own groups.
    let root = parse_cst("- [ x ]::rule(allow: (a))\n");
    let item = find(&root, K::LIST_ITEM).unwrap();
    assert_eq!(count(&item, K::CONNECT), 1);
    assert_eq!(
        find(&item, K::CONNECT).unwrap().text().to_string(),
        "::rule(allow: (a))"
    );
}

#[test]
fn a_connect_opens_with_two_colons() {
    let root = parse_cst("@x(a)::rule(b)::{c: 1}\n");
    let el = find(&root, K::BLOCK_FORM).unwrap();
    assert_eq!(
        child_kinds(&el),
        [K::SIGIL, K::ARGS, K::CONNECT, K::CONNECT]
    );
    let connects: Vec<_> = el
        .children()
        .filter(|n| n.kind() == K::CONNECT)
        .map(|n| n.text().to_string())
        .collect();
    assert_eq!(connects, ["::rule(b)", "::{c: 1}"]);
    // The retired single colon is still a connect in the tree, for lowering to refuse.
    let root = parse_cst("@x(a):rule(b)\n");
    let connect = find(&root, K::CONNECT).unwrap();
    assert_eq!(connect.text().to_string(), ":rule(b)");
}

#[test]
fn a_double_colon_name_on_the_next_line_is_a_declaration_not_a_connect() {
    let root = parse_cst("@x(a)\n::rule(b)\n");
    assert_eq!(count(&root, K::CONNECT), 0);
    assert_eq!(child_kinds(&root), [K::BLOCK_FORM, K::BLOCK_FORM]);
    // Nameless, it cannot be one, so it is the connect of the line above.
    let root = parse_cst("@x(a)\n::{b: 1}\n");
    assert_eq!(count(&root, K::CONNECT), 1);
}

#[test]
fn each_group_kind_at_most_once() {
    // The second `(...)` is prose, not more args.
    let root = parse_cst("@link[a](ref:b) (see below)\n");
    assert_eq!(child_kinds(&root), [K::PARAGRAPH]);
    let el = find(&root, K::INLINE_FORM).unwrap();
    assert_eq!(count(&el, K::ARGS), 1);
}

#[test]
fn paragraph_continuation_and_breaks() {
    // A block start on the next line ends the paragraph without a blank.
    let root = parse_cst("text\n=[ S ]\n- a\n");
    assert_eq!(child_kinds(&root), [K::PARAGRAPH, K::SECTION]);
}

#[test]
fn urls_and_apostrophes_do_not_break_groups() {
    // `//` after `:` is not a comment; `'` inside a word is not a quote.
    let root = parse_cst("@link[it's](url:https://example.com/a)\n=[ S ]\n");
    assert_eq!(child_kinds(&root), [K::BLOCK_FORM, K::SECTION]);
}

#[test]
fn typed_wrappers_walk_the_tree() {
    use tomet_cst::{AstNode, CstRoot};

    let src = "=[ A ]{ id: a }\n==[ B ]\n@x(\"k:1\": v, n: 2)\n";
    let root = CstRoot::cast(parse_cst(src)).unwrap();
    let a = root.sections().next().unwrap();
    assert_eq!(a.level(), 1);
    let b = a.sections().next().unwrap();
    assert_eq!(b.level(), 2);
    let id = a
        .heading()
        .unwrap()
        .value_data()
        .unwrap()
        .entries()
        .next()
        .unwrap();
    assert_eq!(
        (id.key().as_deref(), id.value_text().as_str()),
        (Some("id"), "a")
    );
    let el = b
        .syntax()
        .descendants()
        .find_map(tomet_cst::CstBlockForm::cast)
        .unwrap();
    let entries: Vec<_> = el
        .args()
        .unwrap()
        .entries()
        .map(|e| e.value_text())
        .collect();
    assert_eq!(entries, ["v", "2"]);
}

#[test]
fn id_group_is_structured_and_roundtrips() {
    let src = "@memo(a: 1)[ x ]#(myid)\n";
    let root = parse_cst(src);
    assert_eq!(root.text().to_string(), src);
    let el = find(&root, K::BLOCK_FORM).unwrap();
    assert_eq!(
        child_kinds(&el),
        [K::SIGIL, K::ARGS, K::CONTENT, K::ID_GROUP]
    );
    let id_group = find(&el, K::ID_GROUP).unwrap();
    assert_eq!(id_group.text().to_string(), "#(myid)");
}

#[test]
fn id_group_can_come_before_its_groups() {
    let el = find(&parse_cst("@memo#(myid)(a: 1)\n"), K::BLOCK_FORM).unwrap();
    assert_eq!(child_kinds(&el), [K::SIGIL, K::ID_GROUP, K::ARGS]);
}

#[test]
fn a_connect_can_carry_its_own_id_group() {
    let el = find(
        &parse_cst("@memo::rule(allow: (card))#(myid)\n"),
        K::CONNECT,
    )
    .unwrap();
    assert_eq!(child_kinds(&el), [K::NAME, K::ARGS, K::ID_GROUP]);
}

#[test]
fn bare_hash_not_followed_by_paren_is_not_an_id_group() {
    // Same regression this session's AST-path fix covers: `#` must be
    // immediately followed by `(` to be the id slot, elsewhere it's
    // free text -- confirmed here for the lossless tree too.
    let root = parse_cst("::meta{a: 1}\n#[ Title ]\n");
    assert_eq!(count(&root, K::ID_GROUP), 0);
    assert_eq!(child_kinds(&root), [K::BLOCK_FORM, K::PARAGRAPH]);
}

#[test]
fn a_document_declaration_is_a_block_form_with_a_double_colon_sigil() {
    let src = "::kind(config)\n::meta{ id: x }\n\n=[ T ]\n";
    let root = parse_cst(src);
    assert_eq!(root.text().to_string(), src);
    assert_eq!(
        child_kinds(&root),
        [K::BLOCK_FORM, K::BLOCK_FORM, K::SECTION]
    );
    let kind = find(&root, K::BLOCK_FORM).unwrap();
    assert_eq!(child_kinds(&kind), [K::SIGIL, K::ARGS]);
    let sigil = find(&kind, K::SIGIL).unwrap();
    assert_eq!(sigil.text().to_string(), "::kind");
    let tokens: Vec<_> = sigil
        .children_with_tokens()
        .filter_map(|e| e.into_token())
        .map(|t| t.kind())
        .collect();
    assert_eq!(tokens, [K::COLON, K::COLON]);
    assert_eq!(child_kinds(&sigil), [K::NAME]);
}

#[test]
fn a_declaration_takes_groups_and_connects_like_an_at_form() {
    let src = "::x(a)[ b ]{ c: 1 }#(i)::rule(allow:(k))\n";
    let root = parse_cst(src);
    assert_eq!(root.text().to_string(), src);
    let el = find(&root, K::BLOCK_FORM).unwrap();
    assert_eq!(
        child_kinds(&el),
        [
            K::SIGIL,
            K::ARGS,
            K::CONTENT,
            K::VALUE_DATA,
            K::ID_GROUP,
            K::CONNECT
        ]
    );
}

#[test]
fn the_next_declaration_is_not_a_connect_of_the_one_before() {
    let root = parse_cst("::kind(config)\n::meta{ id: x }\n");
    assert_eq!(count(&root, K::CONNECT), 0);
    assert_eq!(count(&root, K::BLOCK_FORM), 2);
}

#[test]
fn a_double_colon_that_starts_no_declaration_is_text() {
    for src in [
        "std::vec is a type\n",
        "text ::kind(x) in the middle\n",
        "::\n",
        "::(x)\n",
        ":::kind(x)\n",
    ] {
        let root = parse_cst(src);
        assert_eq!(root.text().to_string(), src);
        assert_eq!(count(&root, K::BLOCK_FORM), 0, "{src:?}");
        assert_eq!(count(&root, K::INLINE_FORM), 0, "{src:?}");
    }
}
