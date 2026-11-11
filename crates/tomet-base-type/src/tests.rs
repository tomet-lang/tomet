use super::*;

fn text(s: &str) -> Arg {
    Arg::Datum(Datum::Text(s.to_string()))
}

fn int(n: i64) -> Arg {
    Arg::Datum(Datum::Int(n))
}

fn bare(kind: Kind) -> Type {
    Type::bare(kind)
}

fn list_of(item: Type) -> Type {
    Type::build("list", &[Arg::Type(item)], &[]).unwrap()
}

fn read(source: &str) -> Type {
    source
        .parse()
        .unwrap_or_else(|e| panic!("`{source}` should read: {e}"))
}

fn refused(source: &str) -> TypeError {
    source
        .parse::<Type>()
        .expect_err(&format!("`{source}` should be refused"))
}

// --- building ---------------------------------------------------------------

#[test]
fn a_bare_name_is_the_type_of_that_kind() {
    for kind in Kind::ALL {
        let built = Type::build(kind.name(), &[], &[]).unwrap();
        assert_eq!(built, Type::bare(kind));
        assert_eq!(built.kind(), kind);
        assert_eq!(built.default(), None);
        assert_eq!(built.to_string(), kind.name());
    }
}

#[test]
fn an_unknown_name_is_not_a_type() {
    for name in ["person", "List", "", "list(string)", "auto"] {
        assert_eq!(
            Type::build(name, &[], &[]),
            Err(TypeError::Unknown(name.to_string()))
        );
    }
    let message = TypeError::Unknown("person".to_string()).to_string();
    assert!(message.starts_with("`person` is not a type; the types are `string`"));
    assert!(message.contains("`date`"));
}

#[test]
fn a_nanoid_takes_a_length_and_a_prefix() {
    let ty = Type::build(
        "nanoid",
        &[],
        &[("length", int(12)), ("prefix", text("doc-"))],
    )
    .unwrap();
    assert_eq!(ty.kind(), Kind::Nanoid);
    assert_eq!(ty.length(), Some(12));
    assert_eq!(ty.prefix(), Some("doc-"));
    assert_eq!(ty.item(), None);
    assert_eq!(ty.to_string(), r#"nanoid(length: 12, prefix: "doc-")"#);
}

#[test]
fn a_uuid_takes_a_prefix_and_no_length() {
    let ty = Type::build("uuid", &[], &[("prefix", text("u:"))]).unwrap();
    assert_eq!(ty.prefix(), Some("u:"));
    assert_eq!(ty.length(), None);
    assert_eq!(
        Type::build("uuid", &[], &[("length", int(8))]),
        Err(TypeError::UnknownArgument {
            ty: Kind::Uuid,
            argument: "length".to_string()
        })
    );
}

#[test]
fn a_list_takes_an_item_by_position_and_a_length_by_name() {
    let ty = Type::build(
        "list",
        &[Arg::Type(bare(Kind::String))],
        &[("length", int(6))],
    )
    .unwrap();
    assert_eq!(ty.item(), Some(&bare(Kind::String)));
    assert_eq!(ty.length(), Some(6));
    assert_eq!(ty.to_string(), "list(string, length: 6)");
    // The item is not named: it is what a list is a list of.
    assert_eq!(
        Type::build("list", &[], &[("item", Arg::Type(bare(Kind::String)))]),
        Err(TypeError::UnknownArgument {
            ty: Kind::List,
            argument: "item".to_string()
        })
    );
}

#[test]
fn a_kind_takes_only_its_own_arguments() {
    for kind in [
        Kind::String,
        Kind::Int,
        Kind::Uint,
        Kind::Float,
        Kind::Bool,
        Kind::Datetime,
        Kind::Date,
        Kind::Version,
    ] {
        for argument in ["length", "prefix", "item"] {
            let arg = if argument == "prefix" {
                text("x")
            } else {
                int(1)
            };
            assert_eq!(
                Type::build(kind.name(), &[], &[(argument, arg)]),
                Err(TypeError::UnknownArgument {
                    ty: kind,
                    argument: argument.to_string()
                }),
                "{kind} {argument}"
            );
        }
    }
    assert_eq!(
        Type::build("list", &[], &[("prefix", text("x"))]),
        Err(TypeError::UnknownArgument {
            ty: Kind::List,
            argument: "prefix".to_string()
        })
    );
}

#[test]
fn only_a_list_takes_an_argument_by_position_and_only_one() {
    assert_eq!(
        Type::build("int", &[Arg::Type(bare(Kind::String))], &[]),
        Err(TypeError::TooManyArguments {
            ty: Kind::Int,
            takes: 0,
            given: 1
        })
    );
    assert_eq!(
        Type::build(
            "list",
            &[Arg::Type(bare(Kind::String)), Arg::Type(bare(Kind::Int))],
            &[]
        ),
        Err(TypeError::TooManyArguments {
            ty: Kind::List,
            takes: 1,
            given: 2
        })
    );
}

#[test]
fn an_argument_written_twice_is_refused() {
    assert_eq!(
        Type::build("nanoid", &[], &[("length", int(8)), ("length", int(9))]),
        Err(TypeError::DuplicateArgument {
            ty: Kind::Nanoid,
            argument: "length".to_string()
        })
    );
}

#[test]
fn an_argument_must_be_of_the_sort_the_parameter_takes() {
    let wrong = |ty, argument, expected: &str, found: &str| {
        Err(TypeError::WrongArgument {
            ty,
            argument,
            expected: expected.to_string(),
            found: found.to_string(),
        })
    };
    assert_eq!(
        Type::build("nanoid", &[], &[("length", text("8"))]),
        wrong(
            Kind::Nanoid,
            "length",
            "a positive integer",
            "the text \"8\""
        )
    );
    assert_eq!(
        Type::build("nanoid", &[], &[("length", int(0))]),
        wrong(
            Kind::Nanoid,
            "length",
            "a positive integer",
            "the integer `0`"
        )
    );
    assert_eq!(
        Type::build("list", &[], &[("length", int(-1))]),
        wrong(
            Kind::List,
            "length",
            "a non-negative integer",
            "the integer `-1`"
        )
    );
    assert_eq!(
        Type::build("list", &[], &[("length", Arg::Datum(Datum::Float(2.5)))]),
        wrong(
            Kind::List,
            "length",
            "a non-negative integer",
            "the number `2.5`"
        )
    );
    assert_eq!(
        Type::build("uuid", &[], &[("prefix", int(1))]),
        wrong(Kind::Uuid, "prefix", "text", "the integer `1`")
    );
    assert_eq!(
        Type::build("list", &[int(3)], &[]),
        wrong(Kind::List, "item", "a type", "the integer `3`")
    );
    assert_eq!(
        Type::build("string", &[], &[("default", Arg::Type(bare(Kind::Int)))]),
        wrong(
            Kind::String,
            "default",
            "a value or `auto`",
            "the type `int`"
        )
    );
    assert_eq!(
        Type::build(
            "float",
            &[],
            &[("default", Arg::Datum(Datum::Float(f64::NAN)))]
        ),
        wrong(
            Kind::Float,
            "default",
            "a value whose numbers are finite",
            "the number `NaN`"
        )
    );
    assert_eq!(
        Type::build("nanoid", &[], &[("overwrite", int(1))]),
        wrong(Kind::Nanoid, "overwrite", "true or false", "the integer `1`")
    );
    assert_eq!(
        Type::build("nanoid", &[], &[("overwrite", text("true"))]),
        wrong(Kind::Nanoid, "overwrite", "true or false", "the text \"true\"")
    );
    assert_eq!(
        Type::build(
            "nanoid",
            &[],
            &[("overwrite", Arg::Type(bare(Kind::Bool)))]
        ),
        wrong(Kind::Nanoid, "overwrite", "true or false", "the type `bool`")
    );
}

#[test]
fn an_empty_prefix_is_no_prefix() {
    let ty = Type::build("nanoid", &[], &[("prefix", text(""))]).unwrap();
    assert_eq!(ty, bare(Kind::Nanoid));
    assert_eq!(ty.prefix(), None);
}

#[test]
fn an_item_has_no_default_of_its_own() {
    let item = Type::build("int", &[], &[("default", int(3))]).unwrap();
    let err = Type::build("list", &[Arg::Type(item)], &[]).unwrap_err();
    assert!(matches!(
        err,
        TypeError::WrongArgument {
            ty: Kind::List,
            argument: "item",
            ..
        }
    ));
    // The list's own default is where it goes.
    let ty = Type::build(
        "list",
        &[Arg::Type(bare(Kind::Int))],
        &[("default", Arg::Datum(Datum::Seq(vec![Datum::Int(1)])))],
    )
    .unwrap();
    assert_eq!(
        ty.default(),
        Some(&DefaultValue::Given(Datum::Seq(vec![Datum::Int(1)])))
    );
}

// --- default and auto ---------------------------------------------------------

#[test]
fn every_kind_takes_a_given_default() {
    for kind in Kind::ALL {
        let ty = Type::build(kind.name(), &[], &[("default", Arg::Datum(Datum::Null))]).unwrap();
        assert_eq!(
            ty.default(),
            Some(&DefaultValue::Given(Datum::Null)),
            "{kind}"
        );
        assert_eq!(ty.auto(), None, "{kind}");
    }
}

#[test]
fn auto_is_for_the_kinds_that_make_a_value_up_and_no_other() {
    for kind in Kind::ALL {
        let built = Type::build(kind.name(), &[], &[("default", Arg::Auto)]);
        match kind.auto() {
            Some(fill) => {
                let ty = built.unwrap_or_else(|e| panic!("{kind}: {e}"));
                assert_eq!(ty.default(), Some(&DefaultValue::Auto));
                assert_eq!(ty.auto(), Some(fill));
            }
            None => assert_eq!(built, Err(TypeError::AutoNotAllowed { ty: kind }), "{kind}"),
        }
    }
}

#[test]
fn what_auto_makes_up_is_an_id_or_the_moment() {
    let auto = |name: &str| {
        Type::build(name, &[], &[("default", Arg::Auto)])
            .unwrap()
            .auto()
    };
    assert_eq!(auto("uuid"), Some(AutoFill::Id));
    assert_eq!(auto("nanoid"), Some(AutoFill::Id));
    assert_eq!(auto("datetime"), Some(AutoFill::Now));
    assert_eq!(auto("date"), Some(AutoFill::Now));
    // A type that does not default to `auto` makes nothing up.
    assert_eq!(bare(Kind::Uuid).auto(), None);
}

#[test]
fn auto_is_the_value_of_default_and_of_nothing_else() {
    let elsewhere = |ty, argument: &str| {
        Err(TypeError::AutoElsewhere {
            ty,
            argument: argument.to_string(),
        })
    };
    assert_eq!(
        Type::build("nanoid", &[], &[("length", Arg::Auto)]),
        elsewhere(Kind::Nanoid, "length")
    );
    assert_eq!(
        Type::build("uuid", &[], &[("prefix", Arg::Auto)]),
        elsewhere(Kind::Uuid, "prefix")
    );
    assert_eq!(
        Type::build("list", &[Arg::Auto], &[]),
        elsewhere(Kind::List, "item")
    );
    assert_eq!(
        Type::build("nanoid", &[], &[("overwrite", Arg::Auto)]),
        elsewhere(Kind::Nanoid, "overwrite")
    );
    // Not even where a kind has no `default: auto` to give it meaning.
    assert_eq!(
        Type::build("string", &[], &[("default", Arg::Auto)]),
        Err(TypeError::AutoNotAllowed { ty: Kind::String })
    );
}

#[test]
fn the_messages_name_the_type_and_the_argument() {
    let message = |err: TypeError| err.to_string();
    assert_eq!(
        message(Type::build("string", &[], &[("default", Arg::Auto)]).unwrap_err()),
        "`string` has no `default: auto`; only `datetime`, `date`, `uuid` and `nanoid` make a value up"
    );
    assert_eq!(
        message(Type::build("nanoid", &[], &[("length", int(0))]).unwrap_err()),
        "`nanoid`: `length` takes a positive integer, found the integer `0`"
    );
    assert_eq!(
        message(Type::build("uuid", &[], &[("length", int(8))]).unwrap_err()),
        "`uuid` has no argument `length`; it takes `prefix`, `default` and `overwrite`"
    );
    assert_eq!(
        message(Type::build("list", &[], &[("prefix", text("x"))]).unwrap_err()),
        "`list` has no argument `prefix`; it takes an item type, `length`, `default` and `overwrite`"
    );
    assert_eq!(
        message(Type::build("int", &[int(1)], &[]).unwrap_err()),
        "`int` takes no argument written by position, only named ones: `default` and `overwrite`"
    );
    assert_eq!(
        message(
            Type::build(
                "list",
                &[Arg::Type(bare(Kind::Int)), Arg::Type(bare(Kind::Int))],
                &[]
            )
            .unwrap_err()
        ),
        "`list` takes 1 argument written by position (its item type), and 2 are written"
    );
    assert_eq!(
        message(Type::build("nanoid", &[], &[("prefix", Arg::Auto)]).unwrap_err()),
        "`nanoid`: `auto` is the value of `default` and of nothing else, not of `prefix`"
    );
    assert_eq!(
        message(Type::build("nanoid", &[], &[("length", int(1)), ("length", int(2))]).unwrap_err()),
        "`nanoid`: the argument `length` is written twice"
    );
}

// --- the text form ----------------------------------------------------------

#[test]
fn the_text_form_is_the_canonical_spelling() {
    for (spelling, source) in [
        ("string", "string"),
        ("string", "  string  "),
        ("list", "list"),
        ("list", "list()"),
        ("list", "list( )"),
        ("list(string)", "list(string)"),
        ("list(string)", "list( string )"),
        ("list(list(int))", "list(list(int))"),
        ("list(string, length: 6)", "list(string,length:6)"),
        ("list(length: 2)", "list( length : 2 )"),
        (
            r#"nanoid(length: 8, prefix: "doc-")"#,
            r#"nanoid(prefix: "doc-", length: 8)"#,
        ),
        ("uuid(default: auto)", "uuid(default: auto)"),
        ("date(default: auto)", "date( default:auto )"),
        (r#"string(default: "a b")"#, r#"string(default: "a b")"#),
        (
            r#"list(string, default: ("a", "b"))"#,
            r#"list(string, default: ("a","b"))"#,
        ),
        ("int(default: -3)", "int(default: -3)"),
        ("float(default: 1.5)", "float(default: 1.5)"),
        ("float(default: 100000.0)", "float(default: 1e5)"),
        ("bool(default: true)", "bool(default: true)"),
        ("string(default: null)", "string(default: null)"),
        ("nanoid(overwrite: true)", "nanoid(overwrite: true)"),
        ("nanoid", "nanoid(overwrite: false)"),
        (
            r#"nanoid(length: 8, prefix: "doc-", overwrite: true)"#,
            r#"nanoid(prefix: "doc-", length: 8, overwrite: true)"#,
        ),
        (
            r#"nanoid(length: 8, prefix: "doc-", default: auto, overwrite: true)"#,
            r#"nanoid(length: 8, prefix: "doc-", overwrite: true, default: auto)"#,
        ),
    ] {
        let ty = read(source);
        assert_eq!(ty.to_string(), spelling, "{source}");
    }
}

#[test]
fn the_text_form_is_read_by_the_rules_of_build() {
    assert_eq!(read("date(default: auto)").auto(), Some(AutoFill::Now));
    assert_eq!(
        refused("string(default: auto)"),
        TypeError::AutoNotAllowed { ty: Kind::String }
    );
    assert_eq!(
        refused("uuid(length: 3)"),
        TypeError::UnknownArgument {
            ty: Kind::Uuid,
            argument: "length".to_string()
        }
    );
    assert_eq!(
        refused("list(person)"),
        TypeError::Unknown("person".to_string())
    );
    assert_eq!(refused("person"), TypeError::Unknown("person".to_string()));
    assert_eq!(refused("日本語"), TypeError::Unknown("日本語".to_string()));
    assert_eq!(refused("List"), TypeError::Unknown("List".to_string()));
    assert_eq!(
        refused("list(auto)"),
        TypeError::AutoElsewhere {
            ty: Kind::List,
            argument: "item".to_string()
        }
    );
    assert_eq!(
        refused("nanoid(length: 0)").to_string(),
        "`nanoid`: `length` takes a positive integer, found the integer `0`"
    );
    // A type is not a number, and a number is not a type.
    assert!(matches!(
        refused("nanoid(length: string)"),
        TypeError::WrongArgument {
            argument: "length",
            ..
        }
    ));
    assert!(matches!(
        refused("list(3)"),
        TypeError::WrongArgument {
            argument: "item",
            ..
        }
    ));
}

#[test]
fn a_text_that_is_not_well_formed_says_where() {
    let at = |source: &str| match refused(source) {
        TypeError::Malformed { at, expected, .. } => (at, expected),
        other => panic!("`{source}`: {other}"),
    };
    assert_eq!(at(""), (0, "a type name"));
    assert_eq!(at("   "), (3, "a type name"));
    assert_eq!(at("(string)"), (0, "a type name"));
    assert_eq!(at("list(string"), (11, "`,` or `)`"));
    assert_eq!(at("list(string))"), (12, "the end of the type"));
    assert_eq!(at("list(string) x"), (13, "the end of the type"));
    assert_eq!(at("list(,)"), (5, "a value"));
    assert_eq!(at("list(string,)"), (12, "a value"));
    assert_eq!(at("list(length: )"), (13, "a value"));
    assert_eq!(at("list(length: 1x)"), (13, "a number"));
    assert_eq!(at("list(length: -inf)"), (13, "a number"));
    assert_eq!(
        at("list(length: 2, string)").1,
        "a named argument, `name: value`: one written by position cannot follow a named one"
    );
    assert_eq!(at(r#"nanoid(prefix: "doc)"#).1, "a closing `\"`");
    assert_eq!(
        at(r#"nanoid(prefix: "\q")"#).1,
        "an escape: `\\\"`, `\\\\`, `\\n`, `\\r` or `\\t`"
    );
    assert_eq!(
        at("string(default: (a))").1,
        "text in quotes, a number, `true`, `false`, `null` or a list"
    );
    assert_eq!(
        refused("list(string").to_string(),
        "`list(string` is not a type: expected `,` or `)` at byte 11"
    );
}

#[test]
fn every_type_reads_back_as_itself() {
    let types = sample();
    assert!(types.len() > 100, "{} samples", types.len());
    for ty in types {
        let written = ty.to_string();
        let back: Type = written
            .parse()
            .unwrap_or_else(|e| panic!("`{written}` is written and not read: {e}"));
        assert_eq!(back, ty, "{written}");
        // One spelling: what is read writes the same text.
        assert_eq!(back.to_string(), written);
    }
}

#[test]
fn strings_with_what_needs_escaping_read_back() {
    for prefix in [
        "a b",
        "a,b",
        "a:b",
        "(a)",
        "say \"hi\"",
        "back\\slash",
        "line\nbreak",
        "tab\t",
        "日本語-",
        "auto",
        "null",
        "42",
    ] {
        let ty = Type::build("nanoid", &[], &[("prefix", text(prefix))]).unwrap();
        let back: Type = ty.to_string().parse().unwrap();
        assert_eq!(back.prefix(), Some(prefix), "{}", ty);
        let ty = Type::build("string", &[], &[("default", text(prefix))]).unwrap();
        assert_eq!(ty.to_string().parse::<Type>().unwrap(), ty);
    }
}

/// A spread of types: every kind bare, with each argument it takes, with every
/// sort of default, and lists of those, nested.
fn sample() -> Vec<Type> {
    let defaults: Vec<Option<Arg>> = vec![
        None,
        Some(Arg::Auto),
        Some(Arg::Datum(Datum::Null)),
        Some(Arg::Datum(Datum::Bool(false))),
        Some(Arg::Datum(Datum::Int(-7))),
        Some(Arg::Datum(Datum::Float(0.1))),
        Some(Arg::Datum(Datum::Float(1e21))),
        Some(Arg::Datum(Datum::Float(3.0))),
        Some(Arg::Datum(Datum::Text("x y\n\"z\"".to_string()))),
        Some(Arg::Datum(Datum::Seq(vec![]))),
        Some(Arg::Datum(Datum::Seq(vec![
            Datum::Int(1),
            Datum::Text("a".to_string()),
            Datum::Seq(vec![Datum::Null]),
        ]))),
    ];
    let mut out = Vec::new();
    let mut add = |name: &str, positional: &[Arg], extra: &[(&str, Arg)]| {
        for default in &defaults {
            let mut named: Vec<(&str, Arg)> = extra.to_vec();
            if let Some(default) = default {
                named.push(("default", default.clone()));
            }
            // What `build` refuses is not a type; the rest is.
            if let Ok(ty) = Type::build(name, positional, &named) {
                out.push(ty);
            }
        }
    };
    for kind in Kind::ALL {
        add(kind.name(), &[], &[]);
    }
    for prefix in ["", "doc-", "a \"b\""] {
        add("uuid", &[], &[("prefix", text(prefix))]);
        add(
            "uuid",
            &[],
            &[
                ("prefix", text(prefix)),
                ("overwrite", Arg::Datum(Datum::Bool(true))),
            ],
        );
        for length in [1, 8, 40] {
            add(
                "nanoid",
                &[],
                &[("length", int(length)), ("prefix", text(prefix))],
            );
            add(
                "nanoid",
                &[],
                &[
                    ("length", int(length)),
                    ("prefix", text(prefix)),
                    ("overwrite", Arg::Datum(Datum::Bool(true))),
                ],
            );
        }
    }
    add("nanoid", &[], &[("length", int(3))]);
    for length in [0, 1, 6] {
        add("list", &[], &[("length", int(length))]);
    }
    let items: Vec<Type> = Kind::ALL.into_iter().map(Type::bare).collect();
    for item in &items {
        add("list", &[Arg::Type(item.clone())], &[]);
        add("list", &[Arg::Type(item.clone())], &[("length", int(6))]);
        add(
            "list",
            &[Arg::Type(list_of(item.clone()))],
            &[("length", int(2))],
        );
    }
    out
}

// --- text forms -------------------------------------------------------------

#[test]
fn a_date_is_a_calendar_day_and_a_datetime_is_not_held_to_it() {
    let date = bare(Kind::Date);
    assert!(date.has_text_form());
    assert_eq!(date.holds_text("2026-10-25"), Some(true));
    assert_eq!(date.holds_text("2024-02-29"), Some(true));
    assert_eq!(date.holds_text("2026-02-29"), Some(false));
    assert_eq!(date.holds_text("2026-13-01"), Some(false));
    assert_eq!(date.holds_text("2026-10-25T09:00"), Some(false));
    assert_eq!(date.holds_text("not a date"), Some(false));
    assert_eq!(
        bare(Kind::Datetime).holds_text("2026-10-25T09:00"),
        Some(true)
    );
    assert_eq!(bare(Kind::Datetime).holds_text("2026-10-25"), Some(true));
    assert_eq!(date.describe(), "a date such as `2026-10-25`");
}

#[test]
fn holds_text_matches_intended_forms() {
    let ty = |kind| bare(kind);
    assert_eq!(ty(Kind::Datetime).holds_text("2026-10-25"), Some(true));
    assert_eq!(ty(Kind::Datetime).holds_text("not a date"), Some(false));
    assert_eq!(ty(Kind::Version).holds_text("1.10.0"), Some(true));
    assert_eq!(ty(Kind::Version).holds_text("bad"), Some(false));
    assert_eq!(
        ty(Kind::Uuid).holds_text("123e4567-e89b-12d3-a456-426614174000"),
        Some(true)
    );
    assert_eq!(ty(Kind::Uuid).holds_text("not-a-uuid"), Some(false));
    assert_eq!(ty(Kind::String).holds_text("anything"), None);
    assert_eq!(ty(Kind::Nanoid).holds_text("anything"), None);
    assert_eq!(ty(Kind::List).holds_text("anything"), None);
}

#[test]
fn a_prefixed_uuid_is_the_prefix_and_then_a_uuid() {
    let ty = read(r#"uuid(prefix: "u:")"#);
    assert_eq!(
        ty.holds_text("u:123e4567-e89b-12d3-a456-426614174000"),
        Some(true)
    );
    assert_eq!(
        ty.holds_text("123e4567-e89b-12d3-a456-426614174000"),
        Some(false)
    );
    assert_eq!(
        ty.holds_text("u:u:123e4567-e89b-12d3-a456-426614174000"),
        Some(false)
    );
}

#[test]
fn only_the_kinds_with_a_form_of_their_own_have_one() {
    for ty in sample() {
        let has = matches!(
            ty.kind(),
            Kind::Datetime | Kind::Date | Kind::Version | Kind::Uuid
        );
        assert_eq!(ty.has_text_form(), has, "{ty}");
        assert_eq!(ty.holds_text("x").is_some(), has, "{ty}");
    }
}

#[test]
fn a_type_is_described_for_a_sentence() {
    assert_eq!(bare(Kind::Int).describe(), "an integer");
    assert_eq!(read("list").describe(), "a list");
    assert_eq!(read("list(string)").describe(), "a list of a string");
    assert_eq!(
        read("list(string, length: 1)").describe(),
        "a list of 1 item, each a string"
    );
    assert_eq!(read("list(length: 3)").describe(), "a list of 3 items");
    assert_eq!(
        read("list(int, length: 3)").describe(),
        "a list of 3 items, each an integer"
    );
    assert_eq!(read("uuid").describe(), "a UUID");
    assert_eq!(
        read(r#"uuid(prefix: "u:")"#).describe(),
        "a UUID after the prefix `u:`"
    );
    assert_eq!(read(r#"nanoid(length: 4)"#).describe(), "an identifier");
}

#[test]
fn nested_lists_keep_their_items() {
    let ty = read("list(list(int))");
    assert_eq!(ty.kind(), Kind::List);
    let inner = ty.item().unwrap();
    assert_eq!(inner.kind(), Kind::List);
    assert_eq!(inner.item(), Some(&bare(Kind::Int)));
    assert_eq!(ty, list_of(list_of(bare(Kind::Int))));
}

#[test]
fn overwrite_is_accepted_for_every_kind_and_defaults_to_false() {
    for kind in Kind::ALL {
        let bare = Type::bare(kind);
        assert!(!bare.overwrite(), "{kind}: default is false");
        let positional: Vec<Arg> = if kind == Kind::List {
            vec![Arg::Type(Type::bare(Kind::String))]
        } else {
            vec![]
        };
        let ty_default = Type::build(kind.name(), &positional, &[]).unwrap();
        assert!(!ty_default.overwrite(), "{kind}: build default is false");

        // overwrite: false explicitly
        let ty_false = Type::build(
            kind.name(),
            &positional,
            &[("overwrite", Arg::Datum(Datum::Bool(false)))],
        )
        .unwrap();
        assert!(!ty_false.overwrite(), "{kind}: explicit false");

        // overwrite: true
        let ty_true = Type::build(
            kind.name(),
            &positional,
            &[("overwrite", Arg::Datum(Datum::Bool(true)))],
        )
        .unwrap();
        assert!(ty_true.overwrite(), "{kind}: explicit true");
    }
}

#[test]
fn overwrite_display_and_from_str_roundtrip() {
    for (src, canonical, expected_overwrite) in [
        ("nanoid(overwrite: true)", "nanoid(overwrite: true)", true),
        ("nanoid(overwrite: false)", "nanoid", false),
        (
            r#"nanoid(length: 8, prefix: "note-", default: auto, overwrite: true)"#,
            r#"nanoid(length: 8, prefix: "note-", default: auto, overwrite: true)"#,
            true,
        ),
        (
            r#"nanoid(length: 8, prefix: "note-", default: auto, overwrite: false)"#,
            r#"nanoid(length: 8, prefix: "note-", default: auto)"#,
            false,
        ),
        (
            "uuid(default: auto, overwrite: true)",
            "uuid(default: auto, overwrite: true)",
            true,
        ),
        ("string(overwrite: true)", "string(overwrite: true)", true),
        (
            "list(string, length: 5, overwrite: true)",
            "list(string, length: 5, overwrite: true)",
            true,
        ),
        ("bool(overwrite: true)", "bool(overwrite: true)", true),
    ] {
        let ty: Type = src.parse().unwrap();
        assert_eq!(ty.overwrite(), expected_overwrite, "{src}");
        assert_eq!(ty.to_string(), canonical, "{src}");
        let back: Type = ty.to_string().parse().unwrap();
        assert_eq!(back, ty, "{src}");
        assert_eq!(back.overwrite(), expected_overwrite, "{src}");
    }
}
