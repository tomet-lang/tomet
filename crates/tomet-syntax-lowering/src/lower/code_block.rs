//! Code blocks: a fenced block and a line raw, as a `raw` element.
//!
//! Inside a `|` run the markers at the start of each line are not part of the code.

use super::{range_of, unsupported};
use crate::error::Result;
use tomet_ast::{Block, Inline, Name, Paragraph, Placement, RawText, Value};
use tomet_cst::{AstNode, CstCodeBlock, NodeOrToken, SyntaxKind as K, SyntaxNode};
use tomet_span::Span;
use tomet_tree::{ElementExt, element_new};

/// A fenced code block: a `raw` element whose content is the body, verbatim,
/// and whose `lang` is the first word of the line the fence opens on.
pub(super) fn lower_code_block(node: &SyntaxNode) -> Result<Block> {
    let cst = CstCodeBlock::cast(node.clone()).expect("cast CstCodeBlock");
    if cst.is_line_raw() {
        return lower_line_raw(node);
    }
    let (start, end) = range_of(node);
    cst.opening_fence()
        .ok_or_else(|| unsupported("a code block without an opening fence"))?;
    let lang = cst
        .info_string()
        .and_then(|info| info.lang())
        .unwrap_or_default();

    let lines: Vec<SyntaxNode> = node
        .children()
        .filter(|c| c.kind() == K::CODE_LINE)
        .collect();
    let closing_fence = cst.closing_fence();

    let body_start = match lines.first() {
        Some(line) => usize::from(line.text_range().start()),
        None => match &closing_fence {
            Some(fence) => usize::from(fence.syntax().text_range().start()),
            None => end,
        },
    };

    if pipe_depth(node) > 0 {
        return lower_code_block_in_run(node, start, end, &lines, &lang, body_start);
    }

    let mut code = String::new();
    for line in &lines {
        use std::fmt::Write;
        write!(code, "{}", line.text()).unwrap();
    }
    if code.ends_with('\n') {
        code.pop();
    }
    let body_end = match lines.last() {
        Some(line) => usize::from(line.text_range().end()),
        None => body_start,
    };
    let content_span = Span::new(body_start, body_end);

    let mut element = element_new(Name::bare("raw"))
        .with_placement(Placement::Block)
        .with_span(Span::new(start, end))
        .with_content(vec![Block::Paragraph(Paragraph::new(
            vec![Inline::Raw(RawText::new(code, content_span))],
            content_span,
        ))]);
    if !lang.is_empty() {
        element.data.args = Some(Value::Map(vec![("lang".to_string(), Value::String(lang))]));
    }
    Ok(Block::Element(element))
}

/// A line raw: each line of the node is two backticks and the text after them,
/// less the one space that may follow the backticks. The lines are one `raw`
/// element, joined with a newline; the markers of a `|` run at the start of every
/// line but the first are not part of them.
fn lower_line_raw(node: &SyntaxNode) -> Result<Block> {
    let (start, end) = range_of(node);
    let depth = pipe_depth(node);
    let mut code = String::new();
    let mut content_start = None;
    let mut content_end = start;
    let lines = node.children().filter(|c| c.kind() == K::CODE_LINE);
    for (n, line) in lines.enumerate() {
        let (line_start, _) = range_of(&line);
        let req_markers = if n == 0 { 0 } else { depth };
        let mut marker_count = 0;
        let mut past_fence = false;
        let mut skipped_space = false;
        let mut body = String::new();
        let mut body_start = None;
        let mut fence_end = line_start;
        for elem in line.children_with_tokens() {
            match elem {
                NodeOrToken::Node(child) if child.kind() == K::FENCE => {
                    past_fence = true;
                    fence_end = usize::from(child.text_range().end());
                }
                NodeOrToken::Token(token) if !past_fence => {
                    if token.kind() == K::MARGIN {
                        marker_count += 1;
                    }
                }
                NodeOrToken::Token(token) if past_fence => {
                    if !skipped_space {
                        skipped_space = true;
                        if token.kind() == K::WHITESPACE && token.text().starts_with(' ') {
                            let text = token.text();
                            if text.len() > 1 {
                                let tok_start = usize::from(token.text_range().start()) + 1;
                                body_start.get_or_insert(tok_start);
                                body.push_str(&text[1..]);
                            }
                            continue;
                        }
                    }
                    if matches!(token.kind(), K::NEWLINE) {
                        continue;
                    }
                    body_start.get_or_insert(usize::from(token.text_range().start()));
                    body.push_str(token.text());
                }
                _ => {}
            }
        }
        if marker_count < req_markers {
            return Err(unsupported("a line raw without the run's marker"));
        }
        if !past_fence {
            return Err(unsupported("a line raw line without its backticks"));
        }
        let b_start = body_start.unwrap_or(if skipped_space {
            fence_end + 1
        } else {
            fence_end
        });
        content_start.get_or_insert(b_start);
        content_end = b_start + body.len();
        if n > 0 {
            code.push('\n');
        }
        code.push_str(&body);
    }
    let content_span = Span::new(content_start.unwrap_or(start), content_end);
    let element = element_new(Name::bare("raw"))
        .with_placement(Placement::Block)
        .with_span(Span::new(start, end))
        .with_content(vec![Block::Paragraph(Paragraph::new(
            vec![Inline::Raw(RawText::new(code, content_span))],
            content_span,
        ))]);
    Ok(Block::Element(element))
}

/// How many `|` runs `node` stands inside: each one folds the marker at the start
/// of a line away with the line break before it.
fn pipe_depth(node: &SyntaxNode) -> usize {
    node.ancestors()
        .filter(|a| {
            a.kind() == K::CONTENT
                && a.children_with_tokens()
                    .any(|e| e.kind() == K::PIPE && e.as_token().is_some())
        })
        .count()
}

/// A fenced code block in a `|` run: each line of it begins with the markers of the
/// run, which are not part of the code, nor is the one space after the last.
fn lower_code_block_in_run(
    node: &SyntaxNode,
    start: usize,
    end: usize,
    lines: &[SyntaxNode],
    lang: &str,
    body_start: usize,
) -> Result<Block> {
    let depth = pipe_depth(node);
    let mut code = String::new();
    for line in lines {
        let mut marker_count = 0;
        let mut line_content = String::new();
        let mut after_markers = depth == 0;
        let mut skipped_space = false;
        let mut has_newline = false;
        for elem in line.children_with_tokens() {
            if let NodeOrToken::Token(token) = elem {
                if !after_markers {
                    if token.kind() == K::MARGIN {
                        marker_count += 1;
                        if marker_count == depth {
                            after_markers = true;
                        }
                        continue;
                    } else if token.kind() == K::WHITESPACE {
                        continue;
                    }
                }
                if after_markers && !skipped_space {
                    skipped_space = true;
                    if token.kind() == K::WHITESPACE && token.text().starts_with(' ') {
                        let text = token.text();
                        if text.len() > 1 {
                            line_content.push_str(&text[1..]);
                        }
                        continue;
                    }
                }
                if token.kind() == K::NEWLINE {
                    has_newline = true;
                    continue;
                }
                line_content.push_str(token.text());
            }
        }
        if depth > 0 && marker_count < depth {
            return Err(unsupported("a fence line without the run's marker"));
        }
        code.push_str(&line_content);
        if has_newline {
            code.push('\n');
        }
    }
    if code.ends_with('\n') {
        code.pop();
    }
    let content_span = Span::new(body_start, end);
    let mut element = element_new(Name::bare("raw"))
        .with_placement(Placement::Block)
        .with_span(Span::new(start, end))
        .with_content(vec![Block::Paragraph(Paragraph::new(
            vec![Inline::Raw(RawText::new(code, content_span))],
            content_span,
        ))]);
    if !lang.is_empty() {
        element.data.args = Some(Value::Map(vec![(
            "lang".to_string(),
            Value::String(lang.to_string()),
        )]));
    }
    Ok(Block::Element(element))
}
