use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub message: String,
    pub line: usize,
    pub column: usize,
    pub offset: usize,
}

impl Error {
    /// Resolves `line` and `column` from `offset`, which must point into `src`.
    /// Does nothing if they are already set.
    ///
    /// Parsing builds errors with only an offset: many are caught and thrown
    /// away by speculative parses, and finding a line and column means
    /// scanning the text. The entry point that owns the source calls this
    /// once, on the error that actually escapes.
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
        write!(f, "{}:{}: {}", self.line, self.column, self.message)
    }
}

impl std::error::Error for Error {}

impl From<cotomet_syntax::Error> for Error {
    fn from(e: cotomet_syntax::Error) -> Self {
        Error {
            message: e.message,
            line: e.line,
            column: e.column,
            offset: e.offset,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
