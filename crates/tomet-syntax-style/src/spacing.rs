//! The spaces inside a pair of brackets and after a colon.

/// The spaces inside the brackets of a heading's title: `=[ title ]` or `=[title]`. Both read back
/// as the same document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BracketSpacing {
    /// `[title]`.
    Tight,
    /// `[ title ]`.
    Padded,
}

impl BracketSpacing {
    /// `inner` between brackets, spaced this way.
    pub fn wrap(self, inner: &str) -> String {
        match self {
            BracketSpacing::Tight => format!("[{inner}]"),
            BracketSpacing::Padded => format!("[ {inner} ]"),
        }
    }
}

/// The space after the colon of a `target` that is the only argument of an element, as in
/// `@link(target: x)`: `target: x` or `target:x`. Both read back as the same document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColonSpacing {
    /// `target: x`.
    Spaced,
    /// `target:x`.
    Tight,
}

impl ColonSpacing {
    /// What stands between the colon and the value.
    pub fn after_colon(self) -> &'static str {
        match self {
            ColonSpacing::Spaced => " ",
            ColonSpacing::Tight => "",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brackets_are_tight_or_padded() {
        assert_eq!(BracketSpacing::Tight.wrap("title"), "[title]");
        assert_eq!(BracketSpacing::Padded.wrap("title"), "[ title ]");
    }

    #[test]
    fn a_colon_is_followed_by_a_space_or_not() {
        assert_eq!(ColonSpacing::Spaced.after_colon(), " ");
        assert_eq!(ColonSpacing::Tight.after_colon(), "");
    }
}
