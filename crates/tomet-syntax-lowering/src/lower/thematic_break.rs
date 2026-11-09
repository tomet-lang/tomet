//! Thematic breaks: `---`, and `---[ title ]---` with its title as the content of an `hr`.

use super::error_node::lower_error;
use super::groups::lower_content;
use super::paragraph::lower_inlines;
use super::{range_of, unsupported};
use crate::error::Result;
use tomet_ast::{Block, Name, Paragraph, Placement};
use tomet_cst::{SyntaxKind as K, SyntaxNode};
use tomet_span::Span;
use tomet_tree::{ElementExt, element_new};

/// `---`: a run of dashes on a line of its own; or `---[ title ]---`, whose
/// title is the content of the `hr` element.
pub(super) fn lower_thematic_break(node: &SyntaxNode) -> Result<Block> {
    if node.children_with_tokens().any(|e| e.kind() == K::COMMENT) {
        return Err(unsupported("a comment after a thematic break"));
    }
    let (start, end) = range_of(node);
    if let Some(content) = node.children().find(|n| n.kind() == K::CONTENT) {
        // Read from just after the `[`, so the white space before the text is in
        // the span of its `Text`, as the AST parser has it.
        let (open, close) = range_of(&content);
        if let Some(err_node) = content.children().find(|c| c.kind() == K::ERROR) {
            return Err(lower_error(&err_node));
        }
        if let Some(err_node) = node.children().find(|c| c.kind() == K::ERROR) {
            return Err(lower_error(&err_node));
        }
        // The span of a titled break runs through its line end.
        let span = Span::new(start, end);
        let piped = content.first_token().is_some_and(|t| t.kind() == K::PIPE);
        let title = if piped {
            // `|` lines are the content of the break the way `[ ... ]` is, so they are read as
            // any other content is; the break still takes one paragraph for a title.
            let mut blocks = lower_content(&content)?.into_iter();
            match (blocks.next(), blocks.next()) {
                (None, _) => Vec::new(),
                (Some(Block::Paragraph(paragraph)), None) => paragraph.content,
                (Some(_), _) => return Err(unsupported("a title of several blocks")),
            }
        } else {
            match content.children().find(|n| n.kind() == K::PARAGRAPH) {
                Some(paragraph) if content.children().count() == 1 => {
                    lower_inlines(&paragraph, open + 1, close - 1)?
                }
                None => Vec::new(),
                Some(_) => return Err(unsupported("a title of several blocks")),
            }
        };
        let mut hr = element_new(Name::bare("hr"))
            .with_placement(Placement::Block)
            .with_span(span);
        hr.content = Some(vec![Block::Paragraph(Paragraph::new(title, span))]);
        return Ok(Block::Element(hr));
    }
    if node.children().any(|c| c.kind() != K::FENCE) {
        return Err(unsupported("a thematic break with something in it"));
    }
    let end = match node.last_token() {
        Some(token) if token.kind() == K::NEWLINE => usize::from(token.text_range().start()),
        _ => end,
    };
    Ok(Block::Element(
        element_new(Name::bare("hr"))
            .with_placement(Placement::Block)
            .with_span(Span::new(start, end)),
    ))
}
