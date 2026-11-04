//! Directional collation modes and ordering transformations.

use std::cmp::Ordering;
use std::fmt;

/// The ordering direction for collation comparisons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Direction {
    /// Ascending order (lowest to highest).
    #[default]
    Asc,
    /// Descending order (highest to lowest).
    Desc,
}

impl Direction {
    /// Parses a direction from its canonical name (`"asc"` or `"desc"`).
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "asc" => Some(Self::Asc),
            "desc" => Some(Self::Desc),
            _ => None,
        }
    }

    /// Returns the string representation (`"asc"` or `"desc"`).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Asc => "asc",
            Self::Desc => "desc",
        }
    }

    /// Reverses the collation direction.
    #[inline]
    pub const fn reverse(self) -> Self {
        match self {
            Self::Asc => Self::Desc,
            Self::Desc => Self::Asc,
        }
    }

    /// Applies this direction to an ordering.
    ///
    /// [`Direction::Asc`] leaves the ordering unchanged, while
    /// [`Direction::Desc`] reverses it.
    #[inline]
    pub const fn apply(self, ordering: Ordering) -> Ordering {
        match self {
            Self::Asc => ordering,
            Self::Desc => match ordering {
                Ordering::Less => Ordering::Greater,
                Ordering::Equal => Ordering::Equal,
                Ordering::Greater => Ordering::Less,
            },
        }
    }
}

impl fmt::Display for Direction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
