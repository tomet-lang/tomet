//! Validation and compilation errors for collation specifications.

use std::error::Error;
use std::fmt;

/// An error encountered when validating or compiling a collation specification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollationError {
    /// A collation group contained zero tokens.
    EmptyGroup {
        /// The 0-based index of the step containing the empty group.
        step: usize,
    },
    /// An empty string was provided as a token in a group or ignore list.
    EmptyToken,
    /// A token appeared more than once across groups or ignore lists.
    DuplicateToken(String),
    /// More than one [`Step::Rest`](crate::Step::Rest) was declared.
    DuplicateRest,
    /// The number of collation steps or group entries exceeds system limits.
    TooManyEntries,
}

impl fmt::Display for CollationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyGroup { step } => {
                write!(f, "collation step {step} has an empty group")
            }
            Self::EmptyToken => {
                write!(f, "collation token cannot be empty")
            }
            Self::DuplicateToken(token) => {
                write!(f, "duplicate collation token '{token}'")
            }
            Self::DuplicateRest => {
                write!(f, "duplicate Rest step in collation specification")
            }
            Self::TooManyEntries => {
                write!(f, "collation specification contains too many entries")
            }
        }
    }
}

impl Error for CollationError {}
