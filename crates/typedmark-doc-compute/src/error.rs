use std::fmt;
use typedmark_ast::Value;
use typedmark_resolver::ResolveError;

#[derive(Debug)]
pub enum ComputeError {
    /// Resolving an `Identifier`/`Member` sub-expression failed.
    Resolve(ResolveError),
    /// A `Call`'s `callee` wasn't a bare `Identifier` -- v1 only supports
    /// calling a plain function name (`${add(a, b)}`), not the result of
    /// a `Member`/another `Call` (`${a.b(x)}`'s `b` couldn't be looked up
    /// as a function even though the grammar parses it).
    UnsupportedCallee,
    UnknownFunction(String),
    WrongArgCount {
        function: String,
        expected: usize,
        got: usize,
    },
    NotNumeric {
        function: String,
        value: Value,
    },
    DivisionByZero,
}

impl fmt::Display for ComputeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ComputeError::Resolve(e) => write!(f, "{e}"),
            ComputeError::UnsupportedCallee => {
                write!(f, "a call's callee must be a plain function name")
            }
            ComputeError::UnknownFunction(name) => write!(f, "unknown function `{name}`"),
            ComputeError::WrongArgCount {
                function,
                expected,
                got,
            } => write!(f, "`{function}` expects {expected} argument(s), got {got}"),
            ComputeError::NotNumeric { function, value } => {
                write!(f, "`{function}` expects a number, got {value:?}")
            }
            ComputeError::DivisionByZero => write!(f, "division by zero"),
        }
    }
}

impl std::error::Error for ComputeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ComputeError::Resolve(e) => Some(e),
            _ => None,
        }
    }
}
