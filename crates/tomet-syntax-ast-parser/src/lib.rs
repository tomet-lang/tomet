//! =[ tomet-ast-parser ]
//!
//! Reference AST parser for Tomet, combining CST parsing ([`tomet_cst_parser`])
//! and AST lowering ([`tomet_lowering`]).
//!
//! =[ Architecture & Responsibilities ]
//!
//! - (Standard Parsing Facade)
//!   -| Provides text-to-AST entry points: [`parse_document`], [`parse_value`], [`parse_file`].
//!   -| Bridges lossless CST parsing and typed AST representations.

pub use tomet_lowering::{Error, Result};

/// Parse an entire source string as a markup [`tomet_ast::Document`]: the CST of
/// the text ([`tomet_cst_parser::parse_cst`]), read as the AST it stands for.
pub fn parse_document(src: &str) -> Result<tomet_ast::Document> {
    let cst = tomet_cst_parser::parse_cst(src);
    tomet_lowering::lower_document(&cst).map_err(|e| e.located_in(src))
}

/// Parse an entire source string as one `Value` (a data-only `.tmt`
/// document -- no headings, prose, or top-level elements).
pub fn parse_value(src: &str) -> Result<tomet_ast::Value> {
    let root = tomet_cst_parser::parse_cst_value(src);
    tomet_lowering::lower_value(&root).map_err(|e| e.located_in(src))
}

/// Parse an entire source string as a standalone cotomet data file (`.ctmt`):
/// optional `::declaration` header at the top, followed by `key: value` map entries.
pub fn parse_file(src: &str) -> Result<tomet_ast::File> {
    let trimmed = src.trim_start();
    if trimmed.is_empty()
        || (trimmed.starts_with("//")
            && src
                .lines()
                .all(|l| l.trim().is_empty() || l.trim().starts_with("//")))
    {
        return Ok(tomet_ast::File {
            header: Vec::new(),
            body: tomet_ast::Value::Map(Vec::new()),
        });
    }

    let cst = tomet_cst_parser::parse_cst(src);
    let mut header = Vec::new();
    let mut last_decl_end = 0;
    let mut seen_body = false;

    for child in cst.children() {
        let is_decl = child.kind() == tomet_cst::SyntaxKind::BLOCK_ELEMENT
            && child
                .children()
                .find(|n| n.kind() == tomet_cst::SyntaxKind::SIGIL)
                .and_then(|n| n.first_token())
                .is_some_and(|t| t.kind() == tomet_cst::SyntaxKind::COLON);

        if is_decl {
            if seen_body {
                let start = usize::from(child.text_range().start());
                let lines = tomet_position::LineIndex::new(src);
                let at = lines.line_col(start);
                return Err(tomet_lowering::Error {
                    message: "a declaration belongs at the top of the file".to_string(),
                    line: at.line,
                    column: at.column,
                    offset: start,
                });
            }
            last_decl_end = usize::from(child.text_range().end());
        } else if !matches!(
            child.kind(),
            tomet_cst::SyntaxKind::WHITESPACE
                | tomet_cst::SyntaxKind::NEWLINE
                | tomet_cst::SyntaxKind::COMMENT
        ) {
            seen_body = true;
        }
    }

    if last_decl_end > 0 {
        let header_src = &src[..last_decl_end];
        let doc = parse_document(header_src)?;
        for block in doc.blocks {
            if let tomet_ast::Block::Declaration(d) = block {
                if let Some(pos) = d
                    .data
                    .args
                    .as_ref()
                    .and_then(tomet_lowering::value::find_interp_in_value)
                    .or_else(|| {
                        d.data
                            .value
                            .as_ref()
                            .and_then(tomet_lowering::value::find_interp_in_group)
                    })
                {
                    let at = tomet_position::LineIndex::new(src).line_col(pos);
                    return Err(tomet_lowering::Error {
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
        return Ok(tomet_ast::File {
            header,
            body: tomet_ast::Value::Map(Vec::new()),
        });
    }

    let root = tomet_cst_parser::parse_cst_value(body_src);
    let mut file = match tomet_lowering::lower_file(&root, body_src) {
        Ok(f) => f,
        Err(mut e) => {
            e.offset += last_decl_end;
            let at = tomet_position::LineIndex::new(src).line_col(e.offset);
            e.line = at.line;
            e.column = at.column;
            return Err(e);
        }
    };

    if last_decl_end > 0 {
        tomet_lowering::value::shift_spans(&mut file.body, last_decl_end);
    }
    file.header = header;

    Ok(file)
}

/// Parse an interpolation starting at `cur`.
pub fn parse_interpolation(cur: &mut tomet_lexer::Cursor) -> Result<tomet_ast::Interpolation> {
    tomet_lowering::interp::parse_interpolation(cur)
}

/// Parse a `$func(args)` or `${expr}` interpolation directly from a string slice.
pub fn parse_interpolation_str(source: &str) -> Result<tomet_ast::Interpolation> {
    let mut cur = tomet_lexer::Cursor::new(source);
    parse_interpolation(&mut cur).map_err(|e| e.located_in(source))
}

#[cfg(test)]
mod tests;
