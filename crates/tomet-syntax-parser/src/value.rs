//! Parsing for `Value` and `File`: uses the CST parser and folds it into an AST [`Value`] or [`File`].

use crate::error::{Error, Result};
use tomet_ast::{Block, File, Value};
use tomet_cst::SyntaxKind;
use tomet_position::LineIndex;

/// Parse an entire source string as one `Value` (a data-only `.tmt`
/// document -- no headings, prose, or top-level elements).
pub fn parse_value(src: &str) -> Result<Value> {
    let root = crate::cst::parse_cst_value(src);
    crate::lower::reject_too_deep(&root).map_err(|e| e.located_in(src))?;
    crate::element::lower_value(&root).map_err(|e| e.located_in(src))
}

/// Parse an entire source string as a standalone cotomet data file (`.ctmt`):
/// optional `::declaration` header at the top, followed by `key: value` map entries.
pub fn parse_file(src: &str) -> Result<File> {
    let trimmed = src.trim_start();
    if trimmed.is_empty()
        || (trimmed.starts_with("//")
            && src
                .lines()
                .all(|l| l.trim().is_empty() || l.trim().starts_with("//")))
    {
        return Ok(File {
            header: Vec::new(),
            body: Value::Map(Vec::new()),
        });
    }

    let cst = crate::cst::parse_cst(src);
    let mut header = Vec::new();
    let mut last_decl_end = 0;
    let mut seen_body = false;

    for child in cst.children() {
        let is_decl = child.kind() == SyntaxKind::BLOCK_ELEMENT
            && child
                .children()
                .find(|n| n.kind() == SyntaxKind::SIGIL)
                .and_then(|n| n.first_token())
                .is_some_and(|t| t.kind() == SyntaxKind::COLON);

        if is_decl {
            if seen_body {
                let start = usize::from(child.text_range().start());
                let lines = LineIndex::new(src);
                let at = lines.line_col(start);
                return Err(Error {
                    message: "a declaration belongs at the top of the file".to_string(),
                    line: at.line,
                    column: at.column,
                    offset: start,
                });
            }
            last_decl_end = usize::from(child.text_range().end());
        } else if !matches!(
            child.kind(),
            SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE | SyntaxKind::COMMENT
        ) {
            seen_body = true;
        }
    }

    if last_decl_end > 0 {
        let header_src = &src[..last_decl_end];
        let doc = crate::parse_document(header_src)?;
        for block in doc.blocks {
            if let Block::Declaration(d) = block {
                if let Some(pos) = d
                    .data
                    .args
                    .as_ref()
                    .and_then(find_interp_in_value)
                    .or_else(|| d.data.value.as_ref().and_then(find_interp_in_group))
                {
                    let at = LineIndex::new(src).line_col(pos);
                    return Err(Error {
                        message: "a declaration takes no expression".to_string(),
                        line: at.line,
                        column: at.column,
                        offset: pos,
                    });
                }
                header.push(d);
            }
        }
    }

    let body_src = &src[last_decl_end..];
    if body_src.trim().is_empty() {
        return Ok(File {
            header,
            body: Value::Map(Vec::new()),
        });
    }

    let root = crate::cst::parse_cst_value(body_src);
    crate::lower::reject_too_deep(&root).map_err(|e| e.located_in(src))?;
    let mut body = match crate::element::lower_file_body(&root) {
        Ok(v) => v,
        Err(mut e) => {
            e.offset += last_decl_end;
            let at = LineIndex::new(src).line_col(e.offset);
            e.line = at.line;
            e.column = at.column;
            return Err(e);
        }
    };

    if last_decl_end > 0 {
        shift_spans(&mut body, last_decl_end);
    }

    Ok(File {
        header,
        body,
    })
}

fn shift_spans(value: &mut Value, delta: usize) {
    match value {
        Value::Interp(interp) => {
            interp.span.start += delta;
            interp.span.end += delta;
        }
        Value::Map(entries) => {
            for (_, v) in entries {
                shift_spans(v, delta);
            }
        }
        Value::Seq(items) => {
            for v in items {
                shift_spans(v, delta);
            }
        }
        Value::Call(_, args) => {
            for v in args {
                shift_spans(v, delta);
            }
        }
        _ => {}
    }
}

fn find_interp_in_value(v: &Value) -> Option<usize> {
    match v {
        Value::Interp(it) => Some(it.span.start),
        Value::Map(entries) => entries.iter().find_map(|(_, val)| find_interp_in_value(val)),
        Value::Seq(items) => items.iter().find_map(find_interp_in_value),
        Value::Call(_, args) => args.iter().find_map(find_interp_in_value),
        _ => None,
    }
}

fn find_interp_in_group(g: &tomet_ast::Group) -> Option<usize> {
    g.0.iter().find_map(|e| match e {
        tomet_ast::Entry::Pair(_, v) => find_interp_in_value(v),
        _ => None,
    })
}

pub(crate) fn err(pos: usize, message: impl Into<String>) -> Error {
    Error {
        message: message.into(),
        line: 0,
        column: 0,
        offset: pos,
    }
}
