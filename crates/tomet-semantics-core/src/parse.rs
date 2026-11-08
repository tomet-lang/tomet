//! Parsing of type specifications from AST [`Value`]s.

use tomet_ast::Value;
use tomet_type::{Type, TypeError};

/// Reads a `type:` specification as it is written in an AST [`Value`].
///
/// Accepts a bare name (`string`, `list`, `datetime`) as a string, a parameterized
/// call in AST form (`list(...)`, a [`Value::Call`]), or a string representation of
/// a parameterized type (such as `"list(string)"`). A sequence `(string)` is a
/// value, not a type.
pub fn parse_type_spec(spec: &Value) -> Result<Type, TypeError> {
    match spec {
        Value::String(name) => Type::parse_str(name),
        Value::Call(name, args) if name == "list" => {
            if args.is_empty() {
                Ok(Type::List(None))
            } else if args.len() == 1 {
                let inner = parse_type_spec(&args[0])?;
                Ok(Type::List(Some(Box::new(inner))))
            } else {
                Err(TypeError::NotAName)
            }
        }
        _ => Err(TypeError::NotAName),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_type_names() {
        for ty in &Type::ALL {
            let val = Value::String(ty.name().to_string());
            assert_eq!(parse_type_spec(&val), Ok(ty.clone()));
        }
    }

    #[test]
    fn parses_list_type_specifications() {
        let list_str = Value::String("list(string)".to_string());
        assert_eq!(
            parse_type_spec(&list_str),
            Ok(Type::List(Some(Box::new(Type::String))))
        );

        let list_call = Value::Call("list".to_string(), vec![Value::String("int".to_string())]);
        assert_eq!(
            parse_type_spec(&list_call),
            Ok(Type::List(Some(Box::new(Type::Int))))
        );

        let nested = Value::Call(
            "list".to_string(),
            vec![Value::Call(
                "list".to_string(),
                vec![Value::String("bool".to_string())],
            )],
        );
        assert_eq!(
            parse_type_spec(&nested),
            Ok(Type::List(Some(Box::new(Type::List(Some(Box::new(
                Type::Bool
            )))))))
        );

        // A sequence is a value, not a type.
        let seq = Value::Seq(vec![Value::String("int".to_string())]);
        assert_eq!(parse_type_spec(&seq), Err(TypeError::NotAName));
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
