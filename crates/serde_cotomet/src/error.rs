use std::fmt;

/// What went wrong reading or writing a Rust type as cotomet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The text is not cotomet; it carries the line and column.
    Syntax(cotomet_syntax::Error),
    /// The text parsed, but the data does not fit the Rust type (or the other
    /// way round when writing).
    Data {
        message: String,
        /// From the document root to the value that did not fit; empty when
        /// the error is about the document itself.
        path: Vec<Segment>,
    },
}

/// One step from a map or sequence down to a value inside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    Key(String),
    Index(usize),
}

impl Error {
    pub fn msg(s: impl Into<String>) -> Self {
        Error::Data {
            message: s.into(),
            path: Vec::new(),
        }
    }

    /// Records that the failing value sits under `segment` of the value being
    /// read. Deserializing calls this on the way out, innermost first, so the
    /// path ends up root first. A syntax error has no path and is unchanged.
    pub(crate) fn within(mut self, segment: Segment) -> Self {
        if let Error::Data { path, .. } = &mut self {
            path.insert(0, segment);
        }
        self
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Syntax(e) => e.fmt(f),
            Error::Data { message, path } if path.is_empty() => f.write_str(message),
            Error::Data { message, path } => {
                for (i, segment) in path.iter().enumerate() {
                    match segment {
                        Segment::Key(key) if i == 0 => f.write_str(&segment_text(key))?,
                        Segment::Key(key) => write!(f, ".{}", segment_text(key))?,
                        Segment::Index(index) => write!(f, "[{index}]")?,
                    }
                }
                write!(f, ": {message}")
            }
        }
    }
}

/// A key as one step of a path. A `.` separates the steps, and a bare key may hold
/// one (`a.tmt`), so a key with a `.` is quoted here though it need not be
/// anywhere else: `"a.tmt".source`, not `a.tmt.source`.
fn segment_text(key: &str) -> String {
    if key.contains('.') {
        cotomet_syntax::quoted_text(key)
    } else {
        cotomet_syntax::key_text(key)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Syntax(e) => Some(e),
            Error::Data { .. } => None,
        }
    }
}

impl From<cotomet_syntax::Error> for Error {
    fn from(e: cotomet_syntax::Error) -> Self {
        Error::Syntax(e)
    }
}

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
