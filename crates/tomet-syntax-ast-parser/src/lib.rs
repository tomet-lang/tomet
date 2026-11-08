//! =[ tomet-ast-parser ]
//!
//! Reference AST parser for Tomet, combining CST parsing ([`tomet_cst_parser`])
//! and AST lowering ([`tomet_lowering`]).
//!
//! =[ Architecture & Responsibilities ]
//!
//! - (Standard Parsing Facade)
//!   -| Provides text-to-AST entry points: [`parse_file`], [`parse_value`], [`parse_data_file`].
//!   -| Bridges lossless CST parsing and typed AST representations.
//! - (Nexus Errors)
//!   -| Every entry point refuses a nexus (`@name`, `::name`, `:name`) that has no primary group,
//!    | `(args)` or `[content]`, with an error at the nexus: `@name`, `@name{}`, `@name#(id)`,
//!    | `@name:rule(a)`, `@name:{a: 1}`, `::name{}`, `:name{}` on an owner. A `::` declaration
//!    | takes no connect. This holds for blocks, inline text, values, `-@name` combine items and
//!    | the `::` header of a data file ([`parse_data_file`]). A bare `@name` inside running text
//!    | is text, and `:{...}` / `:(...)` on an owner that has a primary group stay valid. The
//!    | grammar is in the `tomet-cst-parser` crate docs; the refusal is `tomet-lowering`'s.

pub use tomet_lowering::{Error, Result};

/// Parse an entire source string as a markup [`tomet_ast::File`]: the CST of
/// the text ([`tomet_cst_parser::parse_cst`]), read as the AST it stands for.
pub fn parse_file(src: &str) -> Result<tomet_ast::File> {
    let cst = tomet_cst_parser::parse_cst(src);
    tomet_lowering::lower_file(&cst).map_err(|e| e.located_in(src))
}

/// Parse an entire source string as one `Value` (a data-only `.tmt`
/// document -- no headings, prose, or top-level elements).
pub fn parse_value(src: &str) -> Result<tomet_ast::Value> {
    let root = tomet_cst_parser::parse_cst_value(src);
    tomet_lowering::lower_value(&root).map_err(|e| e.located_in(src))
}

/// Parse an entire source string as a standalone cotomet data file (`.ctmt`):
/// optional `::declaration` header at the top, followed by `key: value` map entries.
pub fn parse_data_file(src: &str) -> Result<tomet_ast::File<tomet_ast::Value>> {
    let trimmed = src.trim_start();
    if trimmed.is_empty()
        || (trimmed.starts_with("//")
            && src
                .lines()
                .all(|l| l.trim().is_empty() || l.trim().starts_with("//")))
    {
        return Ok(tomet_ast::File::new(
            Vec::new(),
            tomet_ast::Value::Map(Vec::new()),
            tomet_span::Span::new(0, src.len()),
        ));
    }

    let cst = tomet_cst_parser::parse_cst(src);
    let mut header = Vec::new();
    let mut last_decl_end = 0;
    let mut seen_body = false;

    for child in cst.children() {
        if tomet_lowering::is_declaration(&child) {
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
            let d = tomet_lowering::lower_declaration(&child).map_err(|e| e.located_in(src))?;
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

    let body_src = &src[last_decl_end..];
    if body_src.trim().is_empty() {
        return Ok(tomet_ast::File::new(
            header,
            tomet_ast::Value::Map(Vec::new()),
            tomet_span::Span::new(0, src.len()),
        ));
    }

    let root = tomet_cst_parser::parse_cst_value(body_src);
    let mut file = match tomet_lowering::lower_data_file(&root, body_src) {
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
    file.span = tomet_span::Span::new(0, src.len());

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
