//! Parsing of type specifications from AST [`Value`]s.

use tomet_ast::Value;
use tomet_type::{Type, TypeError};

/// Reads a `type:` specification as it is written in an AST [`Value`].
///
/// Only a name is read. Any other spelling -- a call such as `list(string)`,
/// a list, a map -- is a [`TypeError::NotAName`], because a type with a
/// parameter has no spelling yet.
pub fn parse_type_spec(spec: &Value) -> Result<Type, TypeError> {
    match spec {
        Value::String(name) => Type::parse_str(name),
        _ => Err(TypeError::NotAName),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_type_names() {
        for ty in Type::ALL {
            let val = Value::String(ty.name().to_string());
            assert_eq!(parse_type_spec(&val), Ok(ty));
        }
    }

    #[test]
    fn unknown_or_non_name_values_return_error() {
        let unknown = Value::String("date".to_string());
        assert_eq!(
            parse_type_spec(&unknown),
            Err(TypeError::Unknown("date".to_string()))
        );

        let non_name = Value::Int(42);
        assert_eq!(parse_type_spec(&non_name), Err(TypeError::NotAName));
    }
}
