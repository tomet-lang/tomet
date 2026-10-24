use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub message: String,
    pub line: usize,
    pub column: usize,
    pub offset: usize,
}

impl Error {
    pub fn msg(s: impl Into<String>) -> Self {
        Self {
            message: s.into(),
            line: 0,
            column: 0,
            offset: 0,
        }
    }

    pub fn at(message: impl Into<String>, line: usize, column: usize, offset: usize) -> Self {
        Self {
            message: message.into(),
            line,
            column,
            offset,
        }
    }

    /// An error at byte `offset` whose line and column are not resolved yet.
    ///
    /// Parsing creates errors this way, and many are caught and discarded
    /// (speculative parses), so creating one must not scan the source. The
    /// entry point that owns the text resolves the one error that actually
    /// escapes with [`located_in`](Self::located_in).
    pub fn at_offset(message: impl Into<String>, offset: usize) -> Self {
        Self {
            message: message.into(),
            line: 0,
            column: 0,
            offset,
        }
    }

    /// Resolves `line` and `column` from `offset`, which must point into
    /// `src`. Does nothing if they are already set.
    pub fn located_in(mut self, src: &str) -> Self {
        if self.line == 0 {
            let at = tomet_lexer::LineIndex::new(src).line_col(self.offset);
            self.line = at.line;
            self.column = at.column;
        }
        self
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line > 0 || self.column > 0 {
            write!(f, "{}:{}: {}", self.line, self.column, self.message)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
