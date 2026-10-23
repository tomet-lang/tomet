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
                Value::Seq(vec![Value::String("a".into()), Value::String("b".into())])
            ),
        ])
    );
}

/// The `[a, b]` list literal was retired -- `list(...)` is the sole
/// spelling for a list. `[`/`]` in a value position isn't a parse error
/// either, though: it means block content (`Value::Blocks`, the same
/// grammar as an element's own `[content]`), so `[a, b]` parses, but as
/// one paragraph of plain text `"a, b"`, not a two-item list.
#[test]
fn a_bracket_value_parses_as_block_content_not_a_list() {
    let v = parse_value("tags: [a, b]").unwrap();
    let Value::Map(entries) = v else {
        panic!("expected a map, got {v:?}");
    };
    let Value::Blocks(blocks) = &entries[0].1 else {
        panic!("expected block content, got {:?}", entries[0].1);
    };
    match first_para(blocks) {
        [Inline::Text(t)] => assert_eq!(t.value, "a, b"),
        other => panic!("expected a single text run, got {other:?}"),
    }
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
fn a_list_call_is_a_sequence() {
    assert_eq!(
        parse_value("list(card)").unwrap(),
        Value::Seq(vec![Value::String("card".into())])
    );
    assert_eq!(
        parse_value("list(card, ns.mycard)").unwrap(),
        Value::Seq(vec![
            Value::String("card".into()),
            Value::String("ns.mycard".into())
        ])
    );
    assert_eq!(
        parse_value("list(a, list(b))").unwrap(),
        Value::Seq(vec![
            Value::String("a".into()),
            Value::Seq(vec![Value::String("b".into())])
        ])
    );
    assert_eq!(
        parse_value("allow: list(card)").unwrap(),
        Value::Map(vec![(
            "allow".into(),
            Value::Seq(vec![Value::String("card".into())])
        )])
    );
}

/// The parser resolves exactly the literal constructors (`list`, `string`,
/// `int`, `float`); any other callee is left as an inert `Value::Call` for
/// `tomet-semantics` to judge.
#[test]
fn only_the_literal_constructors_are_resolved() {
    assert_eq!(
        parse_value("enum(card)").unwrap(),
        Value::Call("enum".into(), vec![Value::String("card".into())])
    );
    assert_eq!(
        parse_value("k: int(\"5\")").unwrap(),
        Value::Map(vec![("k".into(), Value::Int(5))])
    );
    assert_eq!(
        parse_value("k: string(1.50)").unwrap(),
        Value::Map(vec![("k".into(), Value::String("1.50".into()))])
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

/// A quoted key is a map key (`motem` reads it, the tomet value grammar
/// delegates to it), so a path can key an entry in a document's `{...}` data
/// and in its `(args)` as well as in a standalone motem file.
#[test]
fn a_quoted_key_is_a_map_key() {
    let v = parse_value("\"tmtroot/a.tmt\": ../x\nplain: 1").unwrap();
    assert_eq!(
        v,
        Value::Map(vec![
            ("tmtroot/a.tmt".into(), Value::String("../x".into())),
            ("plain".into(), Value::Int(1)),
        ])
    );
}

/// A quoted string that is not followed by `:` is still a value.
#[test]
fn a_quoted_string_without_a_colon_is_still_a_value() {
    assert_eq!(parse_value("\"a/b\"").unwrap(), Value::String("a/b".into()));
}

/// A map holds a named key once, however it is spelled: in `(args)`, which
/// shares `motem`'s map body, and in an element's `{...}` group, which has a
/// loop of its own.
#[test]
fn a_repeated_key_is_an_error_in_args_and_in_a_group() {
    for src in ["::meta(a: 1, a: 2)\n", "::meta{\n  a: 1\n  a: 2\n}\n"] {
        let err = parse_document(src).unwrap_err().to_string();
        assert!(err.contains("duplicate key `a`"), "{src:?}: {err}");
    }
    for src in ["::meta(a: 1, b: 2)\n", "::meta{\n  a: 1\n  b: 2\n}\n"] {
        assert!(parse_document(src).is_ok(), "{src:?}");
    }
}
