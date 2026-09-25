use super::*;

#[test]
fn parses_flat_map() {
    let v = parse_value("title: value\ntags: list(a, b)").unwrap();
    assert_eq!(
        v,
        Value::Map(vec![
            ("title".into(), Value::String("value".into())),
            (
                "tags".into(),
                Value::Call(
                    "list".into(),
                    vec![Value::String("a".into()), Value::String("b".into())]
                )
            ),
        ])
    );
}

/// The `[a, b]` list literal was retired -- `list(...)` is the sole
/// spelling now, since `[`/`]` already mean `[content]` at the
/// element level.
#[test]
fn a_bracket_list_literal_in_a_value_position_is_a_parse_error() {
    let err = parse_value("tags: [a, b]").unwrap_err();
    assert!(
        err.message.contains("list(...)"),
        "expected the error to point at 'list(...)', got: {}",
        err.message
    );
}

/// `@name(...)` in a value position parses to `Value::Element`, not
/// the inert `Value::Call` `list(...)`/`enum(...)` use -- the `@`
/// is what says "a real, classifiable element", and `try_parse_call`
/// never matches it anyway (`@` isn't a name-start character).
#[test]
fn at_sigiled_call_in_a_value_position_is_an_element() {
    let v = parse_value(r#"icon: @doc.icon("triangle", pkg:"lucide")"#).unwrap();
    let Value::Map(entries) = v else {
        panic!("expected a map, got {v:?}");
    };
    let Value::Element(el) = &entries[0].1 else {
        panic!("expected an embedded element, got {:?}", entries[0].1);
    };
    assert_eq!(
        el.sigil,
        Sigil::Named(tomet_ast::Name::namespaced("doc", "icon"))
    );
    assert_eq!(
        el.args,
        Some(Value::Map(vec![
            ("".to_string(), Value::String("triangle".to_string())),
            ("pkg".to_string(), Value::String("lucide".to_string())),
        ]))
    );
}

/// An embedded element can take `[content]` and `(args)`.
#[test]
fn an_embedded_element_with_content_is_accepted() {
    let v = parse_value(r#"link: @link(ref:"doc")[Guide]"#).unwrap();
    let Value::Map(entries) = v else {
        panic!("expected map");
    };
    let Value::Element(el) = &entries[0].1 else {
        panic!("expected element");
    };
    assert_eq!(el.sigil, Sigil::named("link"));
    assert!(el.args.is_some());
    assert!(el.content.is_some());
}

/// A quoted positional value used to parse correctly only when it was
/// the group's sole value (`parse_value_at`'s old fast path for a
/// leading `"`). Anywhere else -- first among several entries, or
/// after a `key:` entry -- it either failed outright (leading quote,
/// `expected ')'` on the following `,`) or came back with its quote
/// marks baked into the string (`eat_scalar_raw` does not know about
/// quoting). `@doc.icon("star", pkg:"lucide")` is exactly this shape.
#[test]
fn quoted_positional_value_parses_the_same_regardless_of_position() {
    assert_eq!(
        parse_value(r#""star", pkg:"lucide""#).unwrap(),
        Value::Map(vec![
            ("".into(), Value::String("star".into())),
            ("pkg".into(), Value::String("lucide".into())),
        ])
    );
    assert_eq!(
        parse_value(r#"pkg:"lucide", "star""#).unwrap(),
        Value::Map(vec![
            ("pkg".into(), Value::String("lucide".into())),
            ("".into(), Value::String("star".into())),
        ])
    );
}

#[test]
fn parses_scalars() {
    assert_eq!(parse_value("42").unwrap(), Value::Int(42));
    assert_eq!(parse_value("-3.5").unwrap(), Value::Float(-3.5));
    assert_eq!(parse_value("true").unwrap(), Value::Bool(true));
    assert_eq!(parse_value("null").unwrap(), Value::Null);
    assert_eq!(parse_value("hello").unwrap(), Value::String("hello".into()));
}

#[test]
fn parses_call_syntax() {
    assert_eq!(
        parse_value("list(card)").unwrap(),
        Value::Call("list".into(), vec![Value::String("card".into())])
    );
    assert_eq!(
        parse_value("list(card, ns.mycard)").unwrap(),
        Value::Call(
            "list".into(),
            vec![
                Value::String("card".into()),
                Value::String("ns.mycard".into())
            ]
        )
    );
    assert_eq!(
        parse_value("list(a, list(b))").unwrap(),
        Value::Call(
            "list".into(),
            vec![
                Value::String("a".into()),
                Value::Call("list".into(), vec![Value::String("b".into())])
            ]
        )
    );
    assert_eq!(
        parse_value("allow: list(card)").unwrap(),
        Value::Map(vec![(
            "allow".into(),
            Value::Call("list".into(), vec![Value::String("card".into())])
        )])
    );
}

#[test]
fn a_name_without_a_trailing_paren_is_still_a_plain_scalar() {
    assert_eq!(
        parse_value("card-name").unwrap(),
        Value::String("card-name".into())
    );
}

#[test]
fn preserves_colon_in_url_values() {
    let v = parse_value("url: https://example.com/path").unwrap();
    assert_eq!(
        v,
        Value::Map(vec![(
            "url".into(),
            Value::String("https://example.com/path".into())
        )])
    );
}
