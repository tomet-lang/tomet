//! Finding the `@conflict` markers of a document.
//!
//! =[ Where a marker is found ]
//!
//! In the CST, [`tomet_parser::parse_cst`], and nowhere else: a marker is an
//! `@conflict` element wherever the tree puts one -- a paragraph, a list item,
//! a heading, an element's content, a `{ }` value -- so a marker the merge
//! wrote cannot be written in a place this does not look. Text that is code
//! (a `` `code span` ``, a fenced block) or a comment holds no element and so
//! no marker. An `@conflict` named in running text, with no `( )` after it, is
//! a word, not a marker.
//!
//! =[ What is reported ]
//!
//! Each marker gives its byte `span` in the source, its [`ConflictKind`], and
//! its two sides as they are written: neither side is read into a tree and
//! printed again, so `*y*` stays `*y*`.
//!
//! =[ A marker that cannot be read is an error ]
//!
//! A merge that leaves a conflict must never look like a merge that does
//! not. A marker that is unclosed, lacks a side, mixes the two forms of a
//! side, or takes anything but `a:` and `b:` is a [`ConflictError::Malformed`],
//! not a marker to step over. [`has_conflicts`] says `true` for it.

use std::fmt;
use std::ops::Range;

use tomet_cst::{NodeOrToken, SyntaxElement, SyntaxKind as K, SyntaxNode, WalkEvent};

use crate::markers::{
    extract_conflict_block_side, extract_conflict_inline_side, unquote_scalar_display,
};

/// Where a marker stands, which says how its sides are laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictKind {
    /// A marker standing as a block, its sides `[ ... ]` blocks of their own.
    Block,
    /// A marker inside running text, its sides `[ ... ]` inline text.
    Inline,
    /// A marker where a value goes (`key: @conflict(a: 1, b: 2)`), its sides
    /// plain values.
    Value,
}

/// One unresolved `@conflict`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictItem {
    /// Its place among the markers of the document, from 0, in the order the
    /// text has them.
    pub index: usize,
    /// The bytes of the `@conflict(...)` element in the source.
    pub span: Range<usize>,
    pub kind: ConflictKind,
    /// The text of side `a`, as written: a block side with its indentation
    /// taken off, a value side with its quotes taken off. `a` and `b` are the
    /// structural names `@conflict` itself uses; neither is "mine".
    pub a: String,
    /// The text of side `b`, as `a`.
    pub b: String,
}

/// Why a document's conflicts could not be read or resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictError {
    /// An `@conflict` that cannot be read. It is not skipped: a document with
    /// a marker that is not understood is a document with a conflict.
    Malformed {
        /// Where the marker's `@conflict` starts.
        offset: usize,
        message: String,
    },
    /// There is no conflict number `index`; the document has `count`.
    IndexOutOfBounds { index: usize, count: usize },
}

impl fmt::Display for ConflictError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConflictError::Malformed { offset, message } => {
                write!(f, "unreadable `@conflict` at byte {offset}: {message}")
            }
            ConflictError::IndexOutOfBounds { index, count } => {
                write!(f, "no conflict number {index}: the document has {count}")
            }
        }
    }
}

impl std::error::Error for ConflictError {}

/// One side of a marker, as written.
#[derive(Debug, Clone)]
pub(crate) enum Side {
    /// `[ ... ]`: what is between the brackets, untouched.
    Blocks(String),
    /// A plain value (`2`, `"text"`), trimmed.
    Value(String),
}

/// A marker with the sides it will be resolved from.
#[derive(Debug, Clone)]
pub(crate) struct Found {
    pub item: ConflictItem,
    pub a: Side,
    pub b: Side,
}

/// Finds the `@conflict` markers of `src`, in the order the text has them.
///
/// Fails on a marker that cannot be read (see [`ConflictError::Malformed`]).
pub fn find_conflicts(src: &str) -> Result<Vec<ConflictItem>, ConflictError> {
    Ok(scan(src)?.into_iter().map(|f| f.item).collect())
}

/// Does `src` hold an unresolved `@conflict`? A marker that cannot be read
/// counts: there is something to resolve.
pub fn has_conflicts(src: &str) -> bool {
    !matches!(find_conflicts(src), Ok(items) if items.is_empty())
}

pub(crate) fn scan(src: &str) -> Result<Vec<Found>, ConflictError> {
    if !src.contains(crate::markers::CONFLICT_MARKER) {
        return Ok(Vec::new());
    }
    let root = tomet_parser::parse_cst(src);
    let mut found = Vec::new();
    let mut walk = root.preorder();
    while let Some(event) = walk.next() {
        let WalkEvent::Enter(node) = event else {
            continue;
        };
        if !matches!(node.kind(), K::BLOCK_ELEMENT | K::INLINE_ELEMENT) {
            continue;
        }
        let Some(sigil) = node.children().find(|c| c.kind() == K::SIGIL) else {
            continue;
        };
        if sigil.text() != crate::markers::CONFLICT_MARKER {
            continue;
        }
        found.push(read(&node, &sigil, found.len(), src)?);
        // What is inside is a side of this marker, not a marker of its own.
        walk.skip_subtree();
    }
    Ok(found)
}

fn read(
    node: &SyntaxNode,
    sigil: &SyntaxNode,
    index: usize,
    src: &str,
) -> Result<Found, ConflictError> {
    let start = usize::from(sigil.text_range().start());
    let bad = |message: &str| ConflictError::Malformed {
        offset: start,
        message: message.to_string(),
    };

    let mut args = None;
    for child in node.children() {
        match child.kind() {
            K::SIGIL => {}
            K::ARGS if args.is_none() => args = Some(child),
            _ => return Err(bad("it takes `(a: ..., b: ...)` and nothing else")),
        }
    }
    let args = args.ok_or_else(|| bad("it has no `(a: ..., b: ...)`"))?;
    if args.last_token().map(|t| t.kind()) != Some(K::R_PAREN) {
        return Err(bad("its `(` is never closed"));
    }

    // A marker standing where a value goes (`key: @conflict(...)`) replaces a
    // value, so what it resolves to is a value too -- `[ ... ]` sides included,
    // brackets and all.
    let value_position = node
        .parent()
        .is_some_and(|p| matches!(p.kind(), K::MAP_ENTRY | K::SEQ_ITEM));

    let entries: Vec<SyntaxNode> = args.children().collect();
    let (a, b) = if entries.iter().all(|e| e.kind() == K::SEQ_ITEM) {
        // `@conflict(x, y)`: the language assigns by written position, `a`
        // first and `b` second.
        match entries.as_slice() {
            [first, second] => (
                read_side(first, value_position, &bad)?,
                read_side(second, value_position, &bad)?,
            ),
            _ => return Err(bad("written by position it takes exactly two sides")),
        }
    } else {
        let (mut a, mut b) = (None, None);
        for entry in &entries {
            if entry.kind() != K::MAP_ENTRY {
                return Err(bad(
                    "it takes `a:` and `b:`, or two sides by position, not both",
                ));
            }
            let key = entry
                .first_token()
                .map(|t| unquote_scalar_display(t.text()))
                .unwrap_or_default();
            let slot = match key.as_str() {
                "a" => &mut a,
                "b" => &mut b,
                other => return Err(bad(&format!("it takes `a:` and `b:`, not `{other}:`"))),
            };
            if slot.is_some() {
                return Err(bad(&format!("`{key}:` is given twice")));
            }
            *slot = Some(read_side(entry, value_position, &bad)?);
        }
        (
            a.ok_or_else(|| bad("it has no `a:`"))?,
            b.ok_or_else(|| bad("it has no `b:`"))?,
        )
    };

    let kind = match (&a, &b) {
        (Side::Blocks(_), Side::Blocks(_)) if node.kind() == K::BLOCK_ELEMENT => {
            ConflictKind::Block
        }
        (Side::Blocks(_), Side::Blocks(_)) => ConflictKind::Inline,
        (Side::Value(_), Side::Value(_)) => ConflictKind::Value,
        _ => return Err(bad("one side is `[ ... ]` and the other a plain value")),
    };
    let shown = |side: &Side| match (side, kind) {
        (Side::Blocks(inner), ConflictKind::Block) => {
            extract_conflict_block_side(inner).trim().to_string()
        }
        (Side::Blocks(inner), _) => extract_conflict_inline_side(inner),
        (Side::Value(raw), _) => match bracketed_inner(raw) {
            Some(inner) if inner.contains('\n') => {
                extract_conflict_block_side(inner).trim().to_string()
            }
            Some(inner) => extract_conflict_inline_side(inner),
            None => unquote_scalar_display(raw),
        },
    };
    let end = usize::from(args.text_range().end());
    debug_assert!(start < end && end <= src.len());
    Ok(Found {
        item: ConflictItem {
            index,
            span: start..end,
            kind,
            a: shown(&a),
            b: shown(&b),
        },
        a,
        b,
    })
}

/// What a value written `[ ... ]` holds between its brackets.
pub(crate) fn bracketed_inner(raw: &str) -> Option<&str> {
    raw.strip_prefix('[')?.strip_suffix(']')
}

/// What follows the `:` of an entry (or all of a positional one): a
/// `[ ... ]` group, or a plain value. In a value position the group is kept
/// whole, as the value it is.
fn read_side(
    entry: &SyntaxNode,
    value_position: bool,
    bad: &impl Fn(&str) -> ConflictError,
) -> Result<Side, ConflictError> {
    let keyed = entry.kind() == K::MAP_ENTRY;
    let mut parts: Vec<SyntaxElement> = entry
        .children_with_tokens()
        .skip_while(|e| keyed && e.kind() != K::COLON)
        .skip(usize::from(keyed))
        .collect();
    let is_space = |e: &SyntaxElement| matches!(e.kind(), K::WHITESPACE | K::NEWLINE);
    while parts.last().is_some_and(is_space) {
        parts.pop();
    }
    let lead = parts.iter().take_while(|e| is_space(e)).count();
    parts.drain(..lead);

    let text = |e: &SyntaxElement| match e {
        NodeOrToken::Node(n) => n.text().to_string(),
        NodeOrToken::Token(t) => t.text().to_string(),
    };
    match parts.as_slice() {
        [] => Err(bad("a side is empty")),
        [NodeOrToken::Node(group)] if group.kind() == K::CONTENT => {
            let whole = group.text().to_string();
            let closed = group.last_token().map(|t| t.kind()) == Some(K::R_BRACKET);
            match (bracketed_inner(&whole), closed) {
                (Some(_), true) if value_position => Ok(Side::Value(whole)),
                (Some(inner), true) => Ok(Side::Blocks(inner.to_string())),
                _ => Err(bad("a side's `[` is never closed")),
            }
        }
        [first, ..] if first.kind() == K::CONTENT => Err(bad("text follows a side's `]`")),
        parts => Ok(Side::Value(parts.iter().map(text).collect::<String>())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merge;

    fn sides(src: &str) -> Vec<(ConflictKind, String, String)> {
        find_conflicts(src)
            .unwrap_or_else(|e| panic!("{src:?}: {e}"))
            .into_iter()
            .map(|c| (c.kind, c.a, c.b))
            .collect()
    }

    fn one(src: &str) -> ConflictItem {
        let mut all = find_conflicts(src).unwrap_or_else(|e| panic!("{src:?}: {e}"));
        assert_eq!(all.len(), 1, "{src:?}");
        all.remove(0)
    }

    fn merged(base: &str, local: &str, remote: &str) -> String {
        let m = merge(base, local, remote);
        assert!(!m.clean, "these edits should conflict");
        m.text
    }

    // --- where a marker is found -----------------------------------------

    #[test]
    fn a_value_conflict() {
        let src = merged("@x{ k: 1 }\n", "@x{ k: 2 }\n", "@x{ k: 3 }\n");
        let c = one(&src);
        assert_eq!(
            (c.kind, c.a.as_str(), c.b.as_str()),
            (ConflictKind::Value, "2", "3")
        );
        assert_eq!(&src[c.span], "@conflict(a: 2, b: 3)");
    }

    #[test]
    fn an_inline_conflict() {
        let src = merged(
            "alpha beta gamma\n",
            "alpha BETA gamma\n",
            "alpha Beta gamma\n",
        );
        let c = one(&src);
        assert_eq!(
            (c.kind, c.a.as_str(), c.b.as_str()),
            (ConflictKind::Inline, "BETA", "Beta")
        );
        assert_eq!(&src[c.span], "@conflict(a: [ BETA ], b: [ Beta ])");
    }

    #[test]
    fn a_block_conflict_keeps_the_sides_as_written() {
        // The sides are not read into a tree and printed again: `= A heading`
        // stays `= A heading`.
        let src = merged("text\n", "- a list\n", "= A heading\n");
        assert_eq!(
            sides(&src),
            [(ConflictKind::Block, "- a list".into(), "= A heading".into())]
        );
    }

    #[test]
    fn a_conflict_in_a_list_item() {
        let src = merged("- one\n- two\n", "- uno\n- two\n", "- ein\n- two\n");
        assert_eq!(
            sides(&src),
            [(ConflictKind::Inline, "uno".into(), "ein".into())]
        );
    }

    #[test]
    fn a_conflict_in_a_heading() {
        let src = merged("=[ One ]\n", "=[ Uno ]\n", "=[ Ein ]\n");
        assert_eq!(
            sides(&src),
            [(ConflictKind::Inline, "Uno".into(), "Ein".into())]
        );
    }

    #[test]
    fn a_conflict_in_an_element_whose_content_has_trivia() {
        let src = merged(
            "@note[ a ]  // keep\n",
            "@note[ b ]  // keep\n",
            "@note[   c   ]  // keep\n",
        );
        assert_eq!(find_conflicts(&src).unwrap().len(), 1, "{src:?}");
    }

    #[test]
    fn a_paragraph_and_a_list_both_conflict() {
        let src = merged("one\n\n- a\n", "uno\n\n- b\n", "ein\n\n- c\n");
        let found = sides(&src);
        assert_eq!(found.len(), 2, "{src:?}");
        assert_eq!((found[1].1.as_str(), found[1].2.as_str()), ("b", "c"));
    }

    #[test]
    fn two_conflicts_in_japanese_text() {
        let src = merged(
            "吾輩は猫である。名前はまだ無い。\n\n- 一\n- 二\n",
            "吾輩は犬である。名前はまだ無い。\n\n- 一\n- 二改\n",
            "吾輩は鳥である。名前はまだ無い。\n\n- 一\n- 二訂\n",
        );
        let found = find_conflicts(&src).unwrap();
        assert_eq!(found.len(), 2);
        assert_eq!((found[0].index, found[1].index), (0, 1));
        assert!(found[0].span.end <= found[1].span.start);
        assert_eq!((found[1].a.as_str(), found[1].b.as_str()), ("二改", "二訂"));
    }

    #[test]
    fn a_side_with_a_bracket_inside_code() {
        let src = "see @conflict(a: [ use `]` here ], b: [ x ]) end\n";
        assert_eq!(one(src).a, "use `]` here");
    }

    #[test]
    fn a_side_that_is_a_list() {
        let src = "@conflict(\n  a: [\n    - one\n    - two\n  ]\n  b: [ x ]\n)\n";
        let c = one(src);
        assert_eq!(
            (c.kind, c.a.as_str(), c.b.as_str()),
            (ConflictKind::Block, "- one\n- two", "x")
        );
    }

    #[test]
    fn a_marker_inside_a_pipe_run() {
        let src = "@quote\n| text @conflict(a: [ x ], b: [ y ])\n| more\n";
        assert_eq!(sides(src), [(ConflictKind::Inline, "x".into(), "y".into())]);
    }

    #[test]
    fn a_string_value_side_is_shown_without_quotes() {
        let c = one("@x{ k: @conflict(a: \"one\", b: \"two\") }\n");
        assert_eq!((c.a.as_str(), c.b.as_str()), ("one", "two"));
    }

    // --- what is not a marker --------------------------------------------

    #[test]
    fn code_and_comments_hold_no_marker() {
        for src in [
            "// @conflict(a: 1, b: 2)\ntext\n",
            "see `@conflict(a: 1, b: 2)` here\n",
            "```\n@conflict(a: 1, b: 2)\n```\n",
            "/* @conflict(a: 1, b: 2) */ text\n",
        ] {
            assert!(find_conflicts(src).unwrap().is_empty(), "{src:?}");
            assert!(!has_conflicts(src), "{src:?}");
        }
    }

    #[test]
    fn the_word_in_a_sentence_is_a_word() {
        let src = "The @conflict marker is written in a note.\n";
        assert!(find_conflicts(src).unwrap().is_empty());
        assert!(!has_conflicts(src));
    }

    #[test]
    fn other_elements_are_not_markers() {
        assert!(
            find_conflicts("@note(a: 1)[ x ]\n::conflict(a: 1, b: 2)\n")
                .unwrap()
                .is_empty()
        );
    }

    // --- a marker that cannot be read is an error -----------------------------

    #[test]
    fn a_broken_marker_is_an_error_and_a_conflict() {
        for (src, why) in [
            ("@conflict(a: 1\n", "never closed"),
            ("@conflict(a: [ x ], b: [ y ]\n", "never closed"),
            ("@conflict(a: 1)\n", "no b"),
            ("@conflict(b: 1)\n", "no a"),
            ("@conflict(a: [ x, b: [ y ])\n", "bracket never closed"),
            ("@conflict(a: [ x ], b: 2)\n", "mixed forms"),
            ("@conflict(a: 1, b: 2, c: 3)\n", "a third key"),
            ("@conflict(a: 1, a: 2, b: 3)\n", "a twice"),
            ("@conflict(a: 1, b: 2)[ x ]\n", "another group"),
            ("@conflict\n", "no arguments at all"),
            (
                "@conflict(a: [ x ] y, b: [ z ])\n",
                "text after the bracket",
            ),
            ("@conflict(a: , b: 1)\n", "empty side"),
        ] {
            assert!(
                matches!(find_conflicts(src), Err(ConflictError::Malformed { .. })),
                "{why}: {src:?} -> {:?}",
                find_conflicts(src)
            );
            assert!(has_conflicts(src), "{why}: {src:?}");
        }
    }

    #[test]
    fn a_broken_marker_among_good_ones_is_still_an_error() {
        let src = "@conflict(a: [ x ], b: [ y ])\n\n@conflict(a: [ z ]\n";
        assert!(find_conflicts(src).is_err());
    }

    #[test]
    fn the_error_says_where() {
        let src = "line\n\ntext @conflict(a: 1\n";
        match find_conflicts(src) {
            Err(ConflictError::Malformed { offset, .. }) => {
                assert_eq!(&src[offset..offset + 9], "@conflict")
            }
            other => panic!("{other:?}"),
        }
    }

    // --- has_conflicts ------------------------------------------------------

    #[test]
    fn has_conflicts_agrees_with_find() {
        assert!(has_conflicts("a @conflict(a: [ x ], b: [ y ]) b\n"));
        assert!(!has_conflicts("nothing here\n"));
        assert!(!has_conflicts(""));
    }
}

#[cfg(test)]
mod positional_tests {
    use super::*;

    // `@conflict(x, y)` assigns by written position, `a` first and `b` second
    // (the language's `builtin_positional_arg_key`).
    #[test]
    fn sides_by_position() {
        let c = &find_conflicts("@conflict([ ローカル ], [ リモート ])\n").unwrap()[0];
        assert_eq!(
            (c.kind, c.a.as_str(), c.b.as_str()),
            (ConflictKind::Block, "ローカル", "リモート")
        );
        let c = &find_conflicts("::meta{ x: @conflict(2, 3) }\n").unwrap()[0];
        assert_eq!(
            (c.kind, c.a.as_str(), c.b.as_str()),
            (ConflictKind::Value, "2", "3")
        );
    }

    #[test]
    fn a_positional_marker_resolves_like_a_keyed_one() {
        let src = "a @conflict([ foo ], [ bar ]) b\n";
        assert_eq!(
            crate::resolve_all(src, crate::ConflictResolution::B).unwrap(),
            "a bar b\n"
        );
    }

    #[test]
    fn not_two_sides_is_an_error() {
        for src in [
            "@conflict(1)\n",
            "@conflict(1, 2, 3)\n",
            "@conflict(a: 1, 2)\n",
            "@conflict(1, b: 2)\n",
        ] {
            assert!(
                matches!(find_conflicts(src), Err(ConflictError::Malformed { .. })),
                "{src:?} -> {:?}",
                find_conflicts(src)
            );
            assert!(has_conflicts(src), "{src:?}");
        }
    }
}
