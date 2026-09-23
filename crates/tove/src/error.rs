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

impl serde::ser::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Error::msg(msg.to_string())
    }
}

impl serde::de::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Error::msg(msg.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
