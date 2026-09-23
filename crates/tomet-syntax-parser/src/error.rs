use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub message: String,
    pub line: usize,
    pub column: usize,
    pub offset: usize,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.column, self.message)
    }
}

impl std::error::Error for Error {}

impl From<tove::Error> for Error {
    fn from(e: tove::Error) -> Self {
        Error {
            message: e.message,
            line: e.line,
            column: e.column,
            offset: e.offset,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
