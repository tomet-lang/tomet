//! What the tree says of a nexus -- `@name`, a `::name` declaration or connect -- and its groups. The tree never
//! fails: a nexus without a primary group is a form with no `ARGS`/`CONTENT` in it, for lowering
//! to refuse; what is running text is not a form at all.

use tomet_cst::{SyntaxKind as K, SyntaxNode};

fn parse(src: &str) -> SyntaxNode {
    let root = crate::parse_cst(src);
    assert_eq!(root.text().to_string(), src, "lossless: {src:?}");
    root
}

/// The forms in `src`, in document order, as the kinds of their children.
fn forms(src: &str) -> Vec<(K, Vec<K>)> {
    parse(src)
        .descendants()
        .filter(|n| matches!(n.kind(), K::BLOCK_FORM | K::INLINE_FORM))
        .map(|n| (n.kind(), n.children().map(|c| c.kind()).collect()))
        .collect()
}

fn count(src: &str, kind: K) -> usize {
    parse(src)
        .descendants()
        .filter(|n| n.kind() == kind)
        .count()
}

#[test]
fn a_bare_name_alone_on_its_line_is_a_block_form_with_no_group() {
    for src in [
        "@name\n",
        "@name",
        "@name // note\n",
        "@name\n\ntext\n",
        "@name\n;text\n",
    ] {
        assert_eq!(forms(src), [(K::BLOCK_FORM, vec![K::SIGIL])], "{src:?}");
    }
}

#[test]
fn a_bare_name_the_next_line_folds_into_is_text() {
    for src in [
        "@name\ntext\n",
        "@name\n@other\n",
        "text\n@name\ntext\n",
        "@name and more\n",
        "ping @name now\n",
        "foo@bar.com\n",
        "@name: hello\n",
        // `::name` with no group is not a connect.
        "@name:other\n",
        "@name :other\n",
        "user@host:22\n",
    ] {
        assert!(forms(src).is_empty(), "{src:?}: {:?}", forms(src));
    }
}

#[test]
fn a_group_on_the_next_line_still_belongs_to_a_bare_name() {
    assert_eq!(
        forms("@name\n(a: 1)\n"),
        [(K::BLOCK_FORM, vec![K::SIGIL, K::ARGS])]
    );
    assert_eq!(
        forms("@name\n[ x ]\n"),
        [(K::BLOCK_FORM, vec![K::SIGIL, K::CONTENT])]
    );
    assert_eq!(
        forms("@name\n{a: 1}\n"),
        [(K::BLOCK_FORM, vec![K::SIGIL, K::VALUE_DATA])]
    );
    assert_eq!(
        forms("@name\n#(id)\n"),
        [(K::BLOCK_FORM, vec![K::SIGIL, K::ID_GROUP])]
    );
    assert_eq!(
        forms("@name\n:rule(a)\n"),
        [(K::BLOCK_FORM, vec![K::SIGIL, K::CONNECT])]
    );
}

#[test]
fn a_pipe_run_on_the_next_line_is_the_content_of_a_bare_name() {
    for src in [
        "@table\n| a | b\n",
        "@table\n  | a | b\n",
        "@table| a | b\n",
        // A comment between the name and the run does not detach the run.
        "@table // note\n| a | b\n",
        "@table /* note */\n| a | b\n",
    ] {
        assert_eq!(
            forms(src),
            [(K::BLOCK_FORM, vec![K::SIGIL, K::CONTENT])],
            "{src:?}"
        );
    }
    // A blank line ends the name's reach: the run is a paragraph's.
    assert_eq!(
        forms("@table\n\n| a | b\n"),
        [(K::BLOCK_FORM, vec![K::SIGIL])]
    );
}

#[test]
fn a_secondary_group_alone_is_kept_in_the_tree_as_the_only_one() {
    assert_eq!(
        forms("@name{}\n"),
        [(K::BLOCK_FORM, vec![K::SIGIL, K::VALUE_DATA])]
    );
    assert_eq!(
        forms("@name#(id)\n"),
        [(K::BLOCK_FORM, vec![K::SIGIL, K::ID_GROUP])]
    );
    assert_eq!(
        forms("@name{}#(id)\n"),
        [(K::BLOCK_FORM, vec![K::SIGIL, K::VALUE_DATA, K::ID_GROUP])]
    );
    assert_eq!(forms("::name\n"), [(K::BLOCK_FORM, vec![K::SIGIL])]);
    assert_eq!(
        forms("::name{}\n"),
        [(K::BLOCK_FORM, vec![K::SIGIL, K::VALUE_DATA])]
    );
}

#[test]
fn an_inline_name_with_a_group_is_a_form_and_without_one_is_not() {
    assert_eq!(
        forms("a @name{} b\n"),
        [(K::INLINE_FORM, vec![K::SIGIL, K::VALUE_DATA])]
    );
    assert_eq!(
        forms("a @name#(id) b\n"),
        [(K::INLINE_FORM, vec![K::SIGIL, K::ID_GROUP])]
    );
    assert_eq!(
        forms("a @name::rule(x) b\n"),
        [(K::INLINE_FORM, vec![K::SIGIL, K::CONNECT])]
    );
    assert_eq!(
        forms("a @name::{x: 1} b\n"),
        [(K::INLINE_FORM, vec![K::SIGIL, K::CONNECT])]
    );
    assert!(forms("a @name b\n").is_empty());
}

#[test]
fn a_connect_where_none_is_taken_does_not_make_the_name_a_form() {
    // In the text of a full-form item no connect is taken, so `@name::rule(a)` is a bare name
    // and text; claiming it as a form would only leave a form with nothing in it.
    assert!(forms("- {a: 1} see @name::rule(a)\n").is_empty());
}

#[test]
fn a_combine_item_keeps_a_bare_name_as_a_form() {
    // `-@name`, the `-` against the `@`, is the item's own element; `- @name` is an item
    // that mentions one.
    assert_eq!(forms("-@name\n"), [(K::INLINE_FORM, vec![K::SIGIL])]);
    assert_eq!(forms("-.@name\n"), [(K::INLINE_FORM, vec![K::SIGIL])]);
    assert_eq!(forms("-@name text\n"), [(K::INLINE_FORM, vec![K::SIGIL])]);
    assert_eq!(
        forms("-@name(a)\n"),
        [(K::INLINE_FORM, vec![K::SIGIL, K::ARGS])]
    );
    assert!(forms("- @name\n").is_empty());
}

#[test]
fn a_declaration_keeps_a_connect_in_the_tree() {
    // For lowering to refuse; the tree does not decide it.
    assert_eq!(
        forms("::name:(a)\n"),
        [(K::BLOCK_FORM, vec![K::SIGIL, K::CONNECT])]
    );
    assert_eq!(
        forms("::name(a)::{b: 1}\n"),
        [(K::BLOCK_FORM, vec![K::SIGIL, K::ARGS, K::CONNECT])]
    );
}

#[test]
fn a_bracket_less_line_claims_a_trailing_connect_that_is_structurally_one() {
    // Whether the connect has a primary group is lowering's to judge, as for a full-form head.
    assert_eq!(count("- item ::rule(a)\n", K::CONNECT), 1);
    assert_eq!(count("- item ::rule(a)::other(b)\n", K::CONNECT), 2);
    assert_eq!(count("- item ::rule{a: 1}\n", K::CONNECT), 1);
    assert_eq!(count("- item ::rule{}\n", K::CONNECT), 1);
    assert_eq!(count("= title ::rule{a: 1}\n", K::CONNECT), 1);
    assert_eq!(count("- item {a: 1} ::rule{}\n", K::CONNECT), 1);
    // A nameless connect holds no name to need a group for.
    assert_eq!(count("- item ::{a: 1}\n", K::CONNECT), 1);
    assert_eq!(count("- item ::(a)\n", K::CONNECT), 1);
    assert_eq!(count("= title ::{a: 1}\n", K::CONNECT), 1);
}

#[test]
fn text_that_is_not_structurally_a_connect_is_not_claimed() {
    for src in [
        // White space after the `:`: a label, not a connect.
        "- Note: {x}\n",
        "- Note: {a: 1}\n",
        "= Note: {a: 1}\n",
        // No group at all, or a group that does not read.
        "- note: a:b\n",
        "- foo:bar{not a map}\n",
        "- a ::b{c}\n",
        "- item ::rule[x]\n",
    ] {
        assert_eq!(count(src, K::CONNECT), 0, "{src:?}");
    }
}

#[test]
fn a_full_form_item_or_heading_claims_its_named_connect_whatever_it_holds() {
    // No doubt about what these are, so the tree keeps the connect for lowering to judge.
    assert_eq!(count("-#(id) ::rule{}\n", K::CONNECT), 1);
    assert_eq!(count("=#(id) ::rule{}\n", K::CONNECT), 1);
    assert_eq!(count("=[ T ]::rule{x: 1}\n", K::CONNECT), 1);
}
