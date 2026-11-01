//! =[ tomet-position ]
//!
//! Mapping between byte offsets and 1-based line/column positions ([`LineIndex`], [`LineCol`]).
//!
//! =[ Overview & Design ]
//!
//! - (On-Demand Position Resolution)
//!   | Spans and syntax trees carry only byte offsets; lines and columns are a view
//!   | of the source text, built on demand when needed for diagnostics or editor positions.
//! - (Binary Search Lookup)
//!   | Building [`LineIndex`] requires a single pass over source newlines; querying
//!   | line and column performs a fast binary search over line starts.

/// A 1-based line and a 1-based column, the column counted in `char`s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct LineCol {
    pub line: usize,
    pub column: usize,
}

/// Resolves byte offsets of one source text to lines and columns, and back.
///
/// Spans and errors carry byte offsets only; a line and column is a view of
/// the text, built here when something has to show one (a diagnostic, an
/// editor position). Building the index is one pass over the text, a lookup is
/// a binary search plus a count of the characters in that one line, and
/// nothing about it is paid while parsing.
#[derive(Debug, Clone)]
pub struct LineIndex<'a> {
    src: &'a str,
    /// Byte offset at which each line starts; `starts[0]` is always `0`.
    starts: Vec<usize>,
}

impl<'a> LineIndex<'a> {
    pub fn new(src: &'a str) -> Self {
        let mut starts = vec![0];
        starts.extend(src.match_indices('\n').map(|(i, _)| i + 1));
        LineIndex { src, starts }
    }

    pub fn src(&self) -> &'a str {
        self.src
    }

    /// Number of lines. A text ending in `\n` has an empty last line.
    pub fn line_count(&self) -> usize {
        self.starts.len()
    }

    /// Byte offset at which the 1-based `line` starts, or `None` if there is no
    /// such line.
    pub fn line_start(&self, line: usize) -> Option<usize> {
        self.starts.get(line.checked_sub(1)?).copied()
    }

    /// `offset` brought into the text: clamped to its length, and moved back to
    /// the start of the character it falls inside.
    pub fn clamp(&self, offset: usize) -> usize {
        let mut offset = offset.min(self.src.len());
        while !self.src.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    }

    /// The line and column of byte `offset`. An offset past the end resolves
    /// to the end; one inside a multi-byte character resolves to that
    /// character's start.
    pub fn line_col(&self, offset: usize) -> LineCol {
        let offset = self.clamp(offset);
        let line = self.starts.partition_point(|&start| start <= offset) - 1;
        let column = self.src[self.starts[line]..offset].chars().count() + 1;
        LineCol {
            line: line + 1,
            column,
        }
    }

    /// The byte offset of a 1-based line and column, or `None` if the line
    /// does not exist or the column is past the end of that line (the column
    /// just after the last character is allowed). The inverse of
    /// [`line_col`](Self::line_col).
    pub fn offset(&self, position: LineCol) -> Option<usize> {
        let line = position.line.checked_sub(1)?;
        let column = position.column.checked_sub(1)?;
        let start = *self.starts.get(line)?;
        let end = self.starts.get(line + 1).copied().unwrap_or(self.src.len());
        let text = &self.src[start..end];
        let text = text.strip_suffix('\n').unwrap_or(text);
        let mut chars = text.char_indices();
        match chars.nth(column) {
            Some((i, _)) => Some(start + i),
            None if text.chars().count() == column => Some(start + text.len()),
            None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lc(line: usize, column: usize) -> LineCol {
        LineCol { line, column }
    }

    #[test]
    fn test_line_index_resolves_lines_and_columns() {
        let src = "line1\nline2";
        let index = LineIndex::new(src);
        assert_eq!(index.line_count(), 2);
        assert_eq!(index.line_col(0), lc(1, 1));
        assert_eq!(index.line_col(5), lc(1, 6));
        assert_eq!(index.line_col(6), lc(2, 1));
        assert_eq!(index.line_col(src.len()), lc(2, 6));
        // Past the end clamps to the end.
        assert_eq!(index.line_col(999), lc(2, 6));
    }

    #[test]
    fn test_line_index_counts_chars_not_bytes() {
        // Each of these is 3 bytes.
        let src = "あいう\nえお";
        let index = LineIndex::new(src);
        assert_eq!(index.line_col(3), lc(1, 2));
        assert_eq!(index.line_col(9), lc(1, 4));
        assert_eq!(index.line_col(10), lc(2, 1));
        // Inside a character: resolves to that character's start.
        assert_eq!(index.line_col(4), lc(1, 2));
    }

    #[test]
    fn test_line_index_offset_is_the_inverse() {
        let src = "あいう\nabc\n\nz";
        let index = LineIndex::new(src);
        for offset in (0..=src.len()).filter(|&o| src.is_char_boundary(o)) {
            assert_eq!(
                index.offset(index.line_col(offset)),
                Some(offset),
                "offset {offset}"
            );
        }
        assert_eq!(index.offset(lc(0, 1)), None);
        assert_eq!(index.offset(lc(1, 5)), None);
        assert_eq!(index.offset(lc(9, 1)), None);
    }
}
