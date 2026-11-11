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

impl ParagraphSeparator {
    /// What comes after the line end of a paragraph's last line, up to the start of the next
    /// paragraph, at `indent`: `Blank` an empty line and then the indentation, `Semicolon` the
    /// indentation and then the `;` that opens the next line.
    pub fn after_line_end(self, indent: &str) -> String {
        match self {
            ParagraphSeparator::Blank => format!("\n{indent}"),
            ParagraphSeparator::Semicolon => format!("{indent};"),
        }
    }
}

/// Whether `block` stands like a paragraph, so that a blank line and a `;` set it off alike: a
/// paragraph, and the element, hole, type seat or interpolation that stands as one. A section and a list are
/// set off by their own text.
#[deny(clippy::wildcard_enum_match_arm)]
pub fn stands_like_a_paragraph(block: &Block) -> bool {
    match block {
        Block::Paragraph(_)
        | Block::Element(_)
        | Block::Hole(_)
        | Block::TypeSeat(_)
        | Block::Interp(_) => true,
        Block::Section(_) | Block::List(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_text_after_a_line_end_is_an_empty_line_or_a_semicolon() {
        assert_eq!(ParagraphSeparator::Blank.after_line_end(""), "\n");
        assert_eq!(ParagraphSeparator::Blank.after_line_end("  "), "\n  ");
        assert_eq!(ParagraphSeparator::Semicolon.after_line_end(""), ";");
        assert_eq!(ParagraphSeparator::Semicolon.after_line_end("  "), "  ;");
    }
}
