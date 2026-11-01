//! =[ About ]
//!
//! Source span representations ([`Span`]) for the Tomet toolchain.
//!
//! =[ Design & Invariants ]
//!
//! - (Byte Offsets Only)
//!   | A [`Span`] records byte offsets (`start`, `end`) into source text. Line and column
//!   | numbers are not stored; resolve them on demand using a line index.
//! - (Structural Equality Contract)
//!   | [`Span`] implements [`PartialEq`] to always return `true` so that AST structural
//!   | equality comparisons ignore source positions. Use [`Span::exact_eq()`] when exact
//!   | byte offsets need to be compared.

use serde::{Deserialize, Serialize};

/// A source span: a range of byte offsets into the text it was parsed from.
///
/// A span deliberately holds no line or column. Those are a view of the source
/// text, not a property of the node, and computing them while parsing made
/// every node cost a rescan of everything before it. Resolve them on demand
/// with `tomet_position::LineIndex`.
///
/// Note: [`PartialEq`] is implemented to always return `true` so that AST
/// structural equality checks (e.g. `assert_eq!(doc1, doc2)`) compare node
/// content and semantics without failing on source location differences.
/// Use [`exact_eq`](Self::exact_eq) or direct field comparison when exact
/// byte offsets need to be validated.
///
/// `Hash` is deliberately *not* derived: a derived impl would hash the real
/// `start`/`end` offsets while `PartialEq` above always returns `true`,
/// so two "equal" `Span`s could hash differently -- a Hash/Eq contract
/// violation that breaks `HashMap`/`HashSet` lookups. Don't re-add it
/// without also reconciling it with the custom equality above.
#[derive(Debug, Clone, Copy, Eq, Default, Serialize, Deserialize)]
pub struct Span {
    /// Byte offset of the first byte.
    pub start: usize,
    /// Byte offset one past the last byte.
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn dummy() -> Self {
        Self::default()
    }

    /// Returns `true` if this span represents a default/dummy span with zero offsets.
    pub fn is_dummy(&self) -> bool {
        self.start == 0 && self.end == 0
    }

    pub fn exact_eq(&self, other: &Self) -> bool {
        self.start == other.start && self.end == other.end
    }

    /// Combines two spans into a single span spanning from `self.start` to `other.end`.
    pub fn union(&self, other: &Self) -> Self {
        Self {
            start: self.start,
            end: other.end,
        }
    }

    /// Returns `true` if the given byte offset falls within this span (`start..=end`).
    pub fn contains_offset(&self, offset: usize) -> bool {
        self.start <= offset && offset <= self.end
    }
}

impl PartialEq for Span {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span() {
        let span = Span::new(0, 9);

        assert!(!span.is_dummy());
        assert!(span.contains_offset(0));
        assert!(span.contains_offset(5));
        assert!(span.contains_offset(9));
        assert!(!span.contains_offset(10));

        let dummy = Span::dummy();
        assert!(dummy.is_dummy());

        assert_eq!(span, dummy);
        assert!(!span.exact_eq(&dummy));
    }
}
