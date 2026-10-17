//! A call's evaluated arguments.

use super::error::EvalError;
use super::value::EvalValue;

/// Positional arguments in order, then `name: value` arguments.
pub(crate) struct Args {
    pub positional: Vec<EvalValue>,
    pub named: Vec<(String, EvalValue)>,
}

impl Args {
    /// Exactly `n` positional arguments and no named ones.
    pub fn exactly(self, function: &str, n: usize) -> Result<Vec<EvalValue>, EvalError> {
        self.no_named(function)?;
        if self.positional.len() == n {
            Ok(self.positional)
        } else {
            Err(EvalError::WrongArgCount {
                function: function.to_string(),
                expected: n.to_string(),
                got: self.positional.len(),
            })
        }
    }

    /// Between `min` and `max` positional arguments and no named ones.
    pub fn between(
        self,
        function: &str,
        min: usize,
        max: usize,
    ) -> Result<Vec<EvalValue>, EvalError> {
        self.no_named(function)?;
        if (min..=max).contains(&self.positional.len()) {
            Ok(self.positional)
        } else {
            Err(EvalError::WrongArgCount {
                function: function.to_string(),
                expected: format!("{min} to {max}"),
                got: self.positional.len(),
            })
        }
    }

    fn no_named(&self, function: &str) -> Result<(), EvalError> {
        match self.named.first() {
            None => Ok(()),
            Some((name, _)) => Err(EvalError::message(format!(
                "`{function}` takes no named argument, got `{name}`"
            ))),
        }
    }
}
