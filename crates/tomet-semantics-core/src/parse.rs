//! Reading type specifications from AST [`Value`]s.

use tomet_ast::Value;
use tomet_type::{Arg, Datum, Type, TypeError};

/// Reads a `type:` specification as it is written in an AST [`Value`].
///
/// Accepts a name (`string`, `list`, `datetime`) or the text form of a type
/// (`"list(string)"`) as a string, and a call in AST form (`list(string)`, a
/// [`Value::Call`]). A sequence `(string)` is a value, not a type.
///
/// A [`Value::Call`] has no named arguments, so what is written here is a
/// type's name and the arguments it takes by position: the item type of a
/// `list`. The arguments a type takes by name (`length`, `prefix`, `default`)
/// are written where a type is written in full, in its text form. A value keeps
/// no difference between a bare word and a quoted one, so a string argument is
/// read as a type, which is all a position takes.
///
/// What a type may be given is [`Type::build`]'s to say.
pub fn parse_type_spec(spec: &Value) -> Result<Type, TypeError> {
    match spec {
        Value::String(text) => text.parse(),
        Value::Call(name, args) => {
            let positional = args.iter().map(arg).collect::<Result<Vec<_>, _>>()?;
            Type::build(name, &positional, &[])
        }
        _ => Err(TypeError::NotAType),
    }
}

/// One argument of a call: a type where a string or a call is written, and a
/// plain value for a scalar (which `build` refuses where it is not wanted).
fn arg(value: &Value) -> Result<Arg, TypeError> {
    let datum = match value {
        Value::String(_) | Value::Call(..) => return parse_type_spec(value).map(Arg::Type),
        Value::Null => Datum::Null,
        Value::Bool(b) => Datum::Bool(*b),
        Value::Int(n) => Datum::Int(*n),
        Value::Float(n) => Datum::Float(*n),
        Value::Number(text) => match text.parse::<i64>() {
            Ok(n) => Datum::Int(n),
            Err(_) => Datum::Float(value.as_number().ok_or(TypeError::NotAType)?),
        },
        _ => return Err(TypeError::NotAType),
    };
    Ok(Arg::Datum(datum))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_type::Kind;

    fn string(s: &str) -> Value {
        Value::String(s.to_string())
    }

    fn call(name: &str, args: Vec<Value>) -> Value {
        Value::Call(name.to_string(), args)
    }

    #[test]
    fn parses_known_type_names() {
        for kind in Kind::ALL {
            assert_eq!(parse_type_spec(&string(kind.name())), Ok(Type::bare(kind)));
        }
    }

    #[test]
    fn parses_list_type_specifications() {
        let list = |item: Type| Type::build("list", &[Arg::Type(item)], &[]).unwrap();
        let int = Type::bare(Kind::Int);

        assert_eq!(
            parse_type_spec(&string("list(string)")),
            Ok(list(Type::bare(Kind::String)))
        );
        assert_eq!(
            parse_type_spec(&call("list", vec![string("int")])),
            Ok(list(int.clone()))
        );
        assert_eq!(
            parse_type_spec(&call("list", vec![])),
            Ok(Type::bare(Kind::List))
        );
        assert_eq!(
            parse_type_spec(&call("list", vec![call("list", vec![string("bool")])])),
            Ok(list(list(Type::bare(Kind::Bool))))
        );

        // A sequence is a value, not a type.
        let seq = Value::Seq(vec![string("int")]);
        assert_eq!(parse_type_spec(&seq), Err(TypeError::NotAType));
    }

    #[test]
    fn a_string_is_the_text_form_with_its_arguments() {
        let ty = parse_type_spec(&string("nanoid(length: 8, prefix: \"doc-\")")).unwrap();
        assert_eq!(ty.length(), Some(8));
        assert_eq!(ty.prefix(), Some("doc-"));
    }

    #[test]
    fn a_call_is_held_to_the_rules_of_a_type() {
        // Only a list takes anything by position, and only a type.
        assert_eq!(
            parse_type_spec(&call("int", vec![string("string")])),
            Err(TypeError::TooManyArguments {
                ty: Kind::Int,
                takes: 0,
                given: 1
            })
        );
        assert_eq!(
            parse_type_spec(&call("list", vec![string("int"), string("int")])),
            Err(TypeError::TooManyArguments {
                ty: Kind::List,
                takes: 1,
                given: 2
            })
        );
        assert_eq!(
            parse_type_spec(&call("list", vec![Value::Int(3)]))
                .unwrap_err()
                .to_string(),
            "`list`: `item` takes a type, found the integer `3`"
        );
        assert_eq!(
            parse_type_spec(&call("person", vec![])),
            Err(TypeError::Unknown("person".to_string()))
        );
    }

    #[test]
    fn unknown_or_non_name_values_return_error() {
        let unknown = string("person");
        assert_eq!(
            parse_type_spec(&unknown),
            Err(TypeError::Unknown("person".to_string()))
        );

        assert_eq!(parse_type_spec(&Value::Int(42)), Err(TypeError::NotAType));
        assert_eq!(parse_type_spec(&Value::Null), Err(TypeError::NotAType));
    }
}
