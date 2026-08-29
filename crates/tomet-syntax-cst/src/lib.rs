//! Concrete Syntax Tree (CST) foundation for Tomet, powered by `rowan`.
//!
//! Provides a lossless, resilient, green/red syntax tree representation where
//! 100% of characters (including whitespace, newlines, and comments) are preserved.

pub mod syntax_kind;

pub use syntax_kind::SyntaxKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TometLanguage {}

impl rowan::Language for TometLanguage {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        raw.into()
    }

    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        kind.into()
    }
}

pub type SyntaxNode = rowan::SyntaxNode<TometLanguage>;
pub type SyntaxToken = rowan::SyntaxToken<TometLanguage>;
pub type SyntaxElement = rowan::SyntaxElement<TometLanguage>;
pub type SyntaxNodeChildren = rowan::SyntaxNodeChildren<TometLanguage>;
pub type SyntaxElementChildren = rowan::SyntaxElementChildren<TometLanguage>;

pub use rowan::{
    GreenNode, GreenNodeBuilder, GreenToken, NodeOrToken, TextRange, TextSize, WalkEvent,
};

/// Replaces the byte slice defined by `range` in `source` with `replacement`.
pub fn replace_range(source: &str, range: TextRange, replacement: &str) -> String {
    let start = usize::from(range.start());
    let end = usize::from(range.end());
    let mut out = String::with_capacity(source.len() + replacement.len() - (end - start));
    out.push_str(&source[..start]);
    out.push_str(replacement);
    out.push_str(&source[end..]);
    out
}

/// Replaces the exact source range occupied by `target` in `source` with `replacement`.
pub fn splice_node(source: &str, target: &SyntaxNode, replacement: &str) -> String {
    replace_range(source, target.text_range(), replacement)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replace_range() {
        let src = "Hello world!";
        let range = TextRange::new(TextSize::from(6), TextSize::from(11));
        let replaced = replace_range(src, range, "Tomet");
        assert_eq!(replaced, "Hello Tomet!");
    }
}
