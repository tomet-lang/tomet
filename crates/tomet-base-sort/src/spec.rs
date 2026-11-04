//! Declarative specification of collation steps, equivalence groups, and ignored tokens.

use crate::collator::Collator;
use crate::error::CollationError;

/// A single step in a collation sequence.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Step {
    /// One or more non-empty tokens sharing the same primary weight,
    /// distinguished at the secondary level by their index (`0, 1, ...`).
    Group(Vec<String>),
    /// Placeholder for all characters not matched by any mapped token or
    /// ignored token, ordered among themselves by Unicode scalar value (`char`).
    Rest,
}

/// A declarative specification for custom character and string collation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct CollationSpec {
    /// Ordered steps defining the collation sequence.
    pub steps: Vec<Step>,
    /// Tokens to ignore entirely during collation.
    pub ignore: Vec<String>,
}

impl CollationSpec {
    /// Creates an empty collation specification.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a single token as an explicit step.
    pub fn token(mut self, token: impl Into<String>) -> Self {
        self.steps.push(Step::Group(vec![token.into()]));
        self
    }

    /// Appends an equivalence group of tokens sharing the same primary weight.
    pub fn group<I, S>(mut self, tokens: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.steps
            .push(Step::Group(tokens.into_iter().map(Into::into).collect()));
        self
    }

    /// Appends a `Rest` placeholder step.
    pub fn rest(mut self) -> Self {
        self.steps.push(Step::Rest);
        self
    }

    /// Adds tokens to be ignored entirely during collation.
    pub fn ignore<I, S>(mut self, tokens: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.ignore.extend(tokens.into_iter().map(Into::into));
        self
    }

    /// Compiles this specification into a [`Collator`].
    pub fn compile(&self) -> Result<Collator, CollationError> {
        Collator::new(self)
    }
}
