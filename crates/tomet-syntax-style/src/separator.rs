//! How one paragraph is set off from the next.

use tomet_ast::Block;

/// How one paragraph is set off from the one after it. Both spellings read back as the same
/// document: a blank line, or a `;` at the start of the next line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParagraphSeparator {
    /// A blank line: `a`, an empty line, `b`.
    Blank,
    /// A `;` opening the next paragraph's line: `a`, `;b`.
    Semicolon,
}

/// Whether `block` stands like a paragraph, so that a blank line and a `;` set it off alike: a
/// paragraph, and the element, hole or interpolation that stands as one. A section and a list are
/// set off by their own text.
#[deny(clippy::wildcard_enum_match_arm)]
pub fn stands_like_a_paragraph(block: &Block) -> bool {
    match block {
        Block::Paragraph(_) | Block::Element(_) | Block::Hole(_) | Block::Interp(_) => true,
        Block::Section(_) | Block::List(_) => false,
    }
}
