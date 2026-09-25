mod caret;
mod codeblock;
pub mod cst;
mod document;
mod element;
mod error;
mod fence;
mod inline;
mod interp;
mod list;
mod section;
mod value;

pub use cst::parse_cst;
pub use document::parse_document;
pub use error::{Error, Result};
pub use value::parse_value;

/// Parse a `$func(args)` or `${expr}` dollar element directly from a string slice.
pub fn parse_dollar_element_str(source: &str) -> Result<tomet_ast::Element> {
    let mut cur = tomet_lexer::Cursor::new(source);
    interp::parse_dollar_element(&mut cur)
}

/// Parse a `^(id)` or `^name(id)` caret reference element directly from a string slice.
pub fn parse_caret_element_str(source: &str) -> Result<tomet_ast::Element> {
    let mut cur = tomet_lexer::Cursor::new(source);
    caret::parse_caret_element(&mut cur, true)
}

#[cfg(test)]
mod tests;
