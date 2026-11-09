//! Lowering's refusals: what an `ERROR` node of the tree says went wrong.
//!
//! The CST never fails, so the node that marks a mistake is read here for its message and offset.

use super::range_of;
use crate::error::Error;
use crate::value::err;
use tomet_cst::{SyntaxKind as K, SyntaxNode, SyntaxToken};

pub(crate) fn lower_error(node: &SyntaxNode) -> Error {
    let start = usize::from(node.text_range().start());

    let first = node.first_token().map(|t| t.kind());
    if matches!(
        first,
        Some(K::HASH | K::L_PAREN | K::L_BRACKET | K::L_BRACE | K::PIPE)
    ) && node.parent().is_some_and(|p| {
        matches!(
            p.kind(),
            K::BLOCK_FORM | K::INLINE_FORM | K::SECTION_HEADING | K::SLOT | K::CONNECT
        )
    }) {
        return second_group(node);
    }

    if first == Some(K::COMMENT) {
        return err(start, "unterminated block comment '/*', expected '*/'");
    }

    if node.parent().is_some_and(|p| p.kind() == K::THEMATIC_BREAK) {
        if node.children_with_tokens().count() == 0 {
            return err(
                start,
                "expected 3 or more '-' to close the titled thematic break",
            );
        } else {
            return err(
                start,
                "unexpected trailing content after titled thematic break",
            );
        }
    }

    if let Some(parent) = node.parent().filter(|p| p.kind() == K::CONTENT) {
        if parent
            .first_token()
            .is_some_and(|t| t.kind() == K::L_BRACKET)
        {
            let eof = usize::from(node.ancestors().last().unwrap().text_range().end());
            return err(eof, "unterminated, expected ']'");
        }
        if parent.first_token().is_some_and(|t| t.kind() == K::PIPE) {
            let opener = |n: &SyntaxNode| {
                n.kind() == K::CONTENT && n.first_token().is_some_and(|t| t.kind() == K::PIPE)
            };
            let columns: Vec<usize> = parent
                .ancestors()
                .filter(opener)
                .filter_map(|n| n.first_token())
                .map(|t| token_column(&t))
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();

            let root = node.ancestors().last().unwrap();
            let end = usize::from(node.text_range().end());
            let tok = root
                .descendants_with_tokens()
                .filter_map(|e| e.into_token())
                .filter(|t| usize::from(t.text_range().start()) >= end)
                .find(|t| !matches!(t.kind(), K::WHITESPACE | K::NEWLINE));

            if let Some(mut look) = tok {
                for expected in columns {
                    if look.kind() != K::PIPE {
                        break;
                    }
                    let found = token_column(&look);
                    let pos = usize::from(look.text_range().start());
                    if found != expected {
                        return err(
                            pos,
                            format!(
                                "this `|` stands in column {found}, and the content it would \
                                 continue opens in column {expected}: it lines up with no content"
                            ),
                        );
                    }
                    let look_end = usize::from(look.text_range().end());
                    look = match root
                        .descendants_with_tokens()
                        .filter_map(|e| e.into_token())
                        .filter(|t| usize::from(t.text_range().start()) >= look_end)
                        .find(|t| t.kind() != K::WHITESPACE)
                    {
                        Some(n) => n,
                        None => break,
                    };
                }
            }
        }
    }

    err(start, "a `::` declaration stands alone on its line")
}

/// The error for a group of a kind the owner already has, written against the last one (the
/// CST keeps it in an `ERROR` node). `:` in front of it merges into the one that is there.
fn second_group(node: &SyntaxNode) -> Error {
    let (start, _) = range_of(node);
    let first = node.first_token().map(|t| t.kind());
    if first == Some(K::HASH) {
        return err(start, "a second `#(...)`: an element takes at most one id");
    }
    let group = match first {
        Some(K::L_PAREN) => "(args)",
        Some(K::L_BRACKET) | Some(K::PIPE) => "[content]",
        _ => "{value}",
    };
    err(
        start,
        format!(
            "a second `{group}` group: an element takes one of each. \
             Write `:` in front of it to merge into the one already there"
        ),
    )
}

fn token_column(tok: &SyntaxToken) -> usize {
    let mut col = 1;
    let mut prev = tok.prev_token();
    while let Some(t) = prev {
        if t.kind() == K::NEWLINE {
            break;
        }
        col += t.text().chars().count();
        prev = t.prev_token();
    }
    col
}
