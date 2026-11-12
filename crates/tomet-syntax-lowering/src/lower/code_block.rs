//! Code blocks: a fenced block and a line raw, as a `raw` element.
//!
//! Inside a `|` run the markers at the start of each line are not part of the code.

use super::{range_of, unsupported};
use crate::error::Result;
use tomet_ast::{Block, Inline, Name, Paragraph, Placement, RawText, Value};
use tomet_cst::{AstNode, CstCodeBlock, SyntaxNode};
use tomet_span::Span;
use tomet_tree::{ElementExt, element_new};

/// A fenced code block: a `raw` element whose content is the body (with marked lines
/// stripped of their ` `` ` line marker and one separator space, and unmarked lines taken
/// verbatim), and whose `lang` is the first word of the line the fence opens on.
pub(super) fn lower_code_block(node: &SyntaxNode) -> Result<Block> {
    let cst = CstCodeBlock::cast(node.clone()).expect("cast CstCodeBlock");
    let is_line_raw = cst.is_line_raw();
    let (start, end) = range_of(node);

    let lang = if is_line_raw {
        String::new()
    } else {
        cst.opening_fence()
            .ok_or_else(|| unsupported("a code block without an opening fence"))?;
        cst.info_string()
            .and_then(|info| info.lang())
            .unwrap_or_default()
    };

    let depth = cst.pipe_depth();
    let mut code = String::new();
    let mut content_start = None;
    let mut content_end = start;

    for (n, line) in cst.lines().enumerate() {
        let req_markers = if is_line_raw && n == 0 { 0 } else { depth };
        let parts = line.parts(req_markers).ok_or_else(|| {
            unsupported(if is_line_raw {
                "a line raw without the run's marker"
            } else {
                "a fence line without the run's marker"
            })
        })?;

        if let Some(bs) = parts.body_start {
            content_start.get_or_insert(bs);
        }
        content_end = parts.body_end;
        if n > 0 {
            code.push('\n');
        }
        code.push_str(&parts.body);
    }

    let content_span = Span::new(content_start.unwrap_or(start), content_end);
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
