//! Guard: `|content` is `[content]` spelled without brackets, and nothing
//! more.
//!
//! `|` was added so a body could be written with a visible left edge
//! instead of a pair of brackets far apart. What keeps it from becoming a
//! second construct is that the parser gives it no rules of its own: a
//! run's markers are folded away with the newlines they follow, so the
//! text handed to the inline parser is the same contiguous slice it would
//! be between brackets.
//!
//! That is worth pinning rather than trusting, because the properties
//! inherited this way are not all decided yet. How lines join, and what a
//! table does with the whitespace between two rows, are open questions
//! about `[content]` -- see `docs/design/ideas/syntax-continuation.tmt`.
//! While the two spellings agree, answering them answers them for `|` too,
//! and no one has to remember that `|` exists. The moment they disagree,
//! `|` has started deciding something, and this test says so.

use tomet_ast::Block;
use tomet_parser::parse_document;

/// A construct written both ways. The two sources must parse to the same
/// document -- `Span`'s `PartialEq` is unconditionally true, so this
/// compares shape and content rather than positions.
const SAME: &[(&str, &str, &str)] = &[
    (
        "a body opened on the line after the element",
        "@blockquote(A)\n| alpha\n| beta\n",
        "@blockquote(A)[ alpha\n  beta ]\n",
    ),
    (
        "a body opened on the element's own line",
        "@blockquote| alpha\n",
        "@blockquote[ alpha ]\n",
    ),
    (
        "wide characters fold with no space either way",
        "@blockquote\n| 一行目\n| 二行目\n",
        "@blockquote[ 一行目\n  二行目 ]\n",
    ),
    (
        "a table's rows",
        "@table\n|[ a ][ b ]\n|[ c ][ d ]\n",
        "@table[\n[ a ][ b ]\n[ c ][ d ]\n]\n",
    ),
    (
        "a list item carrying a marker",
        "- (x)| alpha\n     | beta\n",
        "- (x)[ alpha\n  beta ]\n",
    ),
    (
        "a list item without one",
        "- | alpha\n  | beta\n",
        "- [ alpha\n  beta ]\n",
    ),
    (
        "a heading",
        "#| alpha\n | beta\n",
        "#[ alpha\n  beta ]\n",
    ),
    (
        "a group opening directly on the list marker, with no space",
        "-| alpha\n | beta\n",
        "-[ alpha\n  beta ]\n",
    ),
    (
        "an element inside the body still stands as a block",
        "@references\n| @link(target:\"a\")[x]\n| @link(target:\"b\")[y]\n",
        "@references[\n@link(target:\"a\")[x]\n@link(target:\"b\")[y]\n]\n",
    ),
];

#[test]
fn a_marked_run_parses_the_same_as_the_bracketed_form() {
    for (label, marked, bracketed) in SAME {
        let from_marked = parse_document(marked)
            .unwrap_or_else(|e| panic!("{label}: the `|` form did not parse: {e}"));
        let from_brackets = parse_document(bracketed)
            .unwrap_or_else(|e| panic!("{label}: the bracketed form did not parse: {e}"));
        assert_eq!(
            from_marked, from_brackets,
            "{label}: `|` and `[ ]` produced different documents"
        );
    }
}

/// `explicit-form-first` in the workspace writ: a shorthand must have a
/// formatter expansion, and the expansion must round-trip.
///
/// The expansion is free rather than written. The printer only knows how
/// to emit `[content]`, so printing a document parsed from a `|` run *is*
/// the expansion -- and because the two spellings parse to one tree, the
/// expansion is byte-identical to printing the explicit form. It can only
/// go wrong by the two forms ceasing to agree, which the test above holds.
///
/// The round-trip is asserted only where the explicit spelling itself
/// round-trips. `@references[` puts its first block child on the opening
/// line, and that child comes back inline: a printer bug that predates `|`
/// and is not this test's to carry.
#[test]
fn the_printer_expands_a_marked_run_into_brackets() {
    for (label, marked, bracketed) in SAME {
        let from_marked = parse_document(marked).unwrap();
        let from_brackets = parse_document(bracketed).unwrap();
        let expanded = tomet_printer::document_to_tm(&from_marked);

        assert!(
            !expanded.contains('|'),
            "{label}: the expansion still carries a marker:\n{expanded}"
        );
        assert_eq!(
            expanded,
            tomet_printer::document_to_tm(&from_brackets),
            "{label}: the expansion is not what the explicit form prints"
        );

        let explicit_round_trips = parse_document(&tomet_printer::document_to_tm(&from_brackets))
            .is_ok_and(|back| back == from_brackets);
        if explicit_round_trips {
            assert_eq!(
                parse_document(&expanded).unwrap(),
                from_marked,
                "{label}: the expansion does not round-trip:\n{expanded}"
            );
        }
    }
}

/// The column is what says which content a marker belongs to. Once lists
/// nest there is nothing else to go on, and ending the run quietly would
/// drop the line into prose -- the failure that used to break a wrapped
/// list item in every export.
#[test]
fn a_marker_standing_in_the_wrong_column_is_rejected() {
    let err = parse_document("- (x)| a\n  - (y)| b\n| c\n")
        .expect_err("a marker lining up with nothing should not parse");
    assert!(
        err.message.contains("lines up with no content"),
        "unexpected message: {}",
        err.message
    );
}

/// A `|` with no content to continue is a character, the way `@` is one in
/// `me@example.com`. There is no escape in running text, so a line that
/// happens to begin with `|` has to stay writable.
#[test]
fn a_marker_with_nothing_to_continue_is_prose() {
    let doc = parse_document("| a | b |\n").unwrap();
    assert_eq!(doc.blocks.len(), 1);
    assert!(
        matches!(doc.blocks[0], Block::Paragraph(_)),
        "expected a paragraph, got {:?}",
        doc.blocks[0]
    );
}

/// A blank line closes every block, and a run is no exception.
#[test]
fn a_blank_line_closes_a_run() {
    let doc = parse_document("@blockquote\n| a\n| b\n\n| c\n").unwrap();
    assert_eq!(doc.blocks.len(), 2);
    assert!(matches!(doc.blocks[0], Block::Element(_)));
    assert!(matches!(doc.blocks[1], Block::Paragraph(_)));
}
