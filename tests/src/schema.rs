//! The JSON Schemas of the AST (`bindings/schema/`) say what the serde shape of the AST is, and a
//! JSON Schema validator holds them to it: every document of the corpus, serialized, is valid
//! against `ast.schema.json`, and every plain value against `plain.schema.json`.
//!
//! The schemas are generated from the Rust types (`tomet spec schema`), and a test of the CLI holds
//! the committed files to what the types generate. This one holds what is generated to what serde
//! writes, which the derive cannot promise where a type is serialized by hand (`PlainValue`) or
//! through an attribute (`Tuple`). That a document serialized and read back is the same document is
//! `wire.rs`'s (`every_document_of_the_corpus_reads_back_as_itself`), over the same fixtures.
//!
//! A schema that accepts everything would pass every positive test, so the negative ones carry the
//! weight: shapes the AST wire used to have, and shapes that are malformed, must be refused.

use jsonschema::Validator;
use serde_json::{Value, json};
use std::path::PathBuf;
use tomet_ast::{File, PlainValue, Value as AstValue};
use tomet_ast_parser::{parse_data_file, parse_file, parse_value};
use tomet_tests::{fixtures_dir, parseable_corpus};

fn schema(file: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../bindings/schema")
        .join(file);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// A validator of `schema`, as a draft 2020-12 one. The schema is first held to the meta-schema, so
/// a schema that is not one cannot pass for a permissive one.
fn validator_of(schema: &Value) -> Validator {
    jsonschema::draft202012::meta::validate(schema)
        .unwrap_or_else(|e| panic!("not a draft 2020-12 schema: {e}"));
    jsonschema::draft202012::new(schema).unwrap_or_else(|e| panic!("cannot compile: {e}"))
}

fn validator(file: &str) -> Validator {
    validator_of(&schema(file))
}

/// What `instance` is invalid for, one line per error, or nothing if it is valid.
fn errors(validator: &Validator, instance: &Value) -> Vec<String> {
    validator
        .iter_errors(instance)
        .map(|e| format!("{} (at {})", e, e.instance_path()))
        .collect()
}

fn assert_valid(validator: &Validator, what: &str, instance: &Value) {
    let errors = errors(validator, instance);
    assert!(
        errors.is_empty(),
        "{what} is not valid:\n  {}",
        errors.join("\n  ")
    );
}

/// The cases `validator` accepts although it should refuse them.
fn accepted<'a>(validator: &Validator, cases: &'a [(&'a str, Value)]) -> Vec<&'a str> {
    cases
        .iter()
        .filter(|(_, instance)| validator.is_valid(instance))
        .map(|(name, _)| *name)
        .collect()
}

// ---------------------------------------------------------------------------------------------
// The schemas are schemas
// ---------------------------------------------------------------------------------------------

#[test]
fn the_schemas_are_draft_2020_12_schemas() {
    for file in ["ast.schema.json", "plain.schema.json"] {
        let schema = schema(file);
        assert_eq!(
            schema["$schema"], "https://json-schema.org/draft/2020-12/schema",
            "{file}"
        );
        validator_of(&schema);
    }
}

// ---------------------------------------------------------------------------------------------
// What serde writes is valid
// ---------------------------------------------------------------------------------------------

#[test]
fn every_parseable_fixture_serializes_to_a_valid_document() {
    let validator = validator("ast.schema.json");
    let corpus = parseable_corpus();
    assert!(corpus.len() > 50, "the corpus is the corpus");
    for (rel, src) in &corpus {
        let doc: File = parse_file(src).unwrap_or_else(|e| panic!("{}: {e}", rel.display()));
        assert_valid(
            &validator,
            &rel.display().to_string(),
            &serde_json::to_value(&doc).unwrap(),
        );
    }
}

/// The fixtures that exist for the wire hold what they exist for, so they cannot be thinned out
/// to something the schema has no trouble with.
#[test]
fn the_fixtures_for_the_wire_hold_the_forms_it_must_tell_apart() {
    let src = std::fs::read_to_string(fixtures_dir().join("syntax/value-interp.tmt")).unwrap();
    let doc = parse_file(&src).unwrap();
    // Every tag `Interp` is an interpolation where the AST has one: in the `::meta`, in `(args)`,
    // in a tuple and in a `{value}` group (a pair of the group has the value in a 2-array).
    let interps = serde_json::to_string(&doc)
        .unwrap()
        .matches("{\"Interp\":")
        .count();
    assert!(interps >= 10, "interpolations in value position: {interps}");

    let src = std::fs::read_to_string(fixtures_dir().join("syntax/value-lookalike.tmt")).unwrap();
    let doc = parse_file(&src).unwrap();
    // A type seat as a key and the text that spells one are two keys, each on the wire as what it is.
    let text = serde_json::to_string(&doc).unwrap();
    assert_eq!(text.matches("\"key\":{\"TypeSeat\":").count(), 2);
    assert_eq!(text.matches("\"key\":{\"Name\":\"<$int>\"}").count(), 1);
    let meta = doc.header.iter().find(|d| d.name.name == "meta").unwrap();
    let args = meta.data.args.as_ref().unwrap();
    assert!(matches!(args.get("call"), Some(AstValue::Call(..))));
    assert!(matches!(args.get("spelled"), Some(AstValue::Tuple(_))));
}

#[test]
fn the_plain_projection_of_every_meta_is_valid_plain_data() {
    let validator = validator("plain.schema.json");
    let mut checked = 0;
    for (rel, src) in parseable_corpus() {
        let doc = parse_file(&src).unwrap();
        for decl in doc
            .header
            .iter()
            .filter(|d| d.name.namespace.is_none() && d.name.name == "meta")
        {
            let Some(args) = &decl.data.args else {
                continue;
            };
            // What `processFile` hands out as `meta`: the entries that are data.
            let meta = PlainValue::Object(args.plain_entries().collect());
            assert_valid(
                &validator,
                &format!("the meta of {}", rel.display()),
                &serde_json::to_value(&meta).unwrap(),
            );
            checked += 1;
            // And the whole tuple, where all of it is data.
            if let Ok(whole) = args.to_plain() {
                assert_valid(
                    &validator,
                    &format!("the whole meta of {}", rel.display()),
                    &serde_json::to_value(&whole).unwrap(),
                );
            }
        }
    }
    assert!(checked > 5, "few fixtures have a `::meta`: {checked}");
}

#[test]
fn the_plain_projection_of_every_data_fixture_is_valid_plain_data() {
    let validator = validator("plain.schema.json");
    let dir = fixtures_dir().join("cotomet");
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).unwrap().flatten() {
        let path = entry.path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if !name.starts_with("ok-") || path.extension().is_none_or(|e| e != "ctmt") {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap();
        // A data file read as a data file, and its text read as a value (`parseValue`).
        let mut values = Vec::new();
        if let Ok(file) = parse_data_file(&src) {
            values.push(file.body);
        }
        if let Ok(value) = parse_value(&src) {
            values.push(value);
        }
        for value in values {
            // A value that holds what is not data has no plain form, which is not the schema's business.
            if let Ok(plain) = value.to_plain() {
                assert_valid(&validator, &name, &serde_json::to_value(&plain).unwrap());
                checked += 1;
            }
        }
    }
    assert!(
        checked > 5,
        "few data fixtures have a plain form: {checked}"
    );

    // Each shape of plain data, once.
    let value =
        parse_value("a: 1\nb: (x, y)\nc: (1, k: 2)\nd: ()\ne: none\nf: 1.10\ng: true\n").unwrap();
    assert_valid(
        &validator,
        "a value of every plain shape",
        &serde_json::to_value(value.to_plain().unwrap()).unwrap(),
    );
}

/// Plain data is any JSON, so the plain schema has nothing to refuse; what it must do is accept
/// every kind of JSON value.
#[test]
fn the_plain_schema_accepts_every_kind_of_json() {
    let validator = validator("plain.schema.json");
    for json in [
        json!(null),
        json!(true),
        json!(1),
        json!(-1.5),
        json!("text"),
        json!([]),
        json!({}),
        json!({"a": [1, 2.5, {"b": null}], "c": {"d": "e"}}),
    ] {
        assert_valid(&validator, &json.to_string(), &json);
    }
}

// ---------------------------------------------------------------------------------------------
// What is not the wire is refused
// ---------------------------------------------------------------------------------------------

/// A file whose one element holds `value` as the value of an unnamed entry of its arguments.
fn file_with(value: Value) -> Value {
    let span = json!({"start": 0, "end": 0});
    json!({
        "header": [],
        "body": [{"Element": {
            "name": {"namespace": null, "name": "x"},
            "placement": "Block",
            "data": {
                "args": {"entries": [{"key": null, "value": value}], "span": null},
                "value": null
            },
            "content": null,
            "id": null,
            "connects": [],
            "span": span
        }}],
        "span": span
    })
}

fn int(n: i64) -> Value {
    json!({"Scalar": {"Int": n}})
}

#[test]
fn the_wire_form_of_a_value_is_valid_in_the_file_it_is_put_in() {
    let validator = validator("ast.schema.json");
    for (name, value) in [
        ("an integer", int(1)),
        ("none", json!({"Scalar": "None"})),
        ("auto", json!({"Scalar": "Auto"})),
        ("a number as written", json!({"Scalar": {"Number": "1.10"}})),
        (
            "a call",
            json!({"Call": ["enum", {"entries": [{"key": null, "value": int(1)}], "span": null}]}),
        ),
        (
            "a tuple",
            json!({"Tuple": {"entries": [
                {"key": null, "value": int(1)},
                {"key": {"Name": "k"}, "value": int(2)}
            ], "span": {"start": 1, "end": 5}}}),
        ),
        ("block content", json!({"Blocks": []})),
    ] {
        assert_valid(&validator, name, &file_with(value));
    }
}

/// What the schema must refuse: the shapes `Value` and `Tuple` had before the wire was made
/// lossless (each was a shape another variant shared), and shapes that are malformed.
fn not_the_wire() -> Vec<(&'static str, Value)> {
    let span = json!({"start": 0, "end": 0});
    let expr = json!({"kind": {"Identifier": "x"}, "span": span});
    let tuple = json!({"entries": [], "span": null});
    vec![
        // The old, natural shapes of a value.
        ("an integer, bare", file_with(json!(1))),
        ("a text, bare", file_with(json!("text"))),
        ("null, bare", file_with(json!(null))),
        ("a boolean, bare", file_with(json!(true))),
        ("a tuple as an array", file_with(json!([1, 2]))),
        ("a tuple as an object", file_with(json!({"a": 1}))),
        (
            "a call as `{call, args}`",
            file_with(json!({"call": "enum", "args": [1, 2]})),
        ),
        (
            "an interpolation untagged",
            file_with(json!({"expr": expr, "span": span})),
        ),
        (
            "block content as an array of blocks",
            file_with(json!([{"Paragraph": {"content": [], "span": span}}])),
        ),
        // The old shape of a tuple, inside the new tag.
        (
            "a tuple of the wire as an array",
            file_with(json!({"Tuple": [int(1)]})),
        ),
        (
            "a tuple of the wire as an object",
            file_with(json!({"Tuple": {"a": int(1)}})),
        ),
        // Malformed.
        ("an unknown variant", file_with(json!({"Foo": 1}))),
        (
            "two variants at once",
            file_with(json!({"Scalar": {"Int": 1}, "Tuple": tuple})),
        ),
        (
            "an integer that is a text",
            file_with(json!({"Scalar": {"Int": "1"}})),
        ),
        (
            "a number as a JSON number",
            file_with(json!({"Scalar": {"Number": 1.5}})),
        ),
        ("auto spelled as text", file_with(json!({"Scalar": "auto"}))),
        (
            "a call without arguments",
            file_with(json!({"Call": ["enum"]})),
        ),
        (
            "a call with three parts",
            file_with(json!({"Call": ["enum", tuple, tuple]})),
        ),
        (
            "a call whose head is not a name",
            file_with(json!({"Call": [1, tuple]})),
        ),
        (
            "a tuple without a span",
            file_with(json!({"Tuple": {"entries": []}})),
        ),
        (
            "a tuple without entries",
            file_with(json!({"Tuple": {"span": null}})),
        ),
        (
            "a tuple entry keyed by a bare string",
            file_with(json!({"Tuple": {"entries": [{"key": "a", "value": int(1)}], "span": null}})),
        ),
        (
            "a tuple entry without a key",
            file_with(json!({"Tuple": {"entries": [{"value": int(1)}], "span": null}})),
        ),
        (
            "a tuple entry keyed by an unknown kind",
            file_with(
                json!({"Tuple": {"entries": [{"key": {"Number": 1}, "value": int(1)}], "span": null}}),
            ),
        ),
        (
            "a span that is not a range",
            file_with(json!({"Tuple": {"entries": [], "span": {"start": 0}}})),
        ),
        (
            "a negative span",
            file_with(json!({"Tuple": {"entries": [], "span": {"start": -1, "end": 0}}})),
        ),
        // A malformed file.
        ("a file without a span", {
            let mut file = file_with(int(1));
            file.as_object_mut().unwrap().remove("span");
            file
        }),
        (
            "a file whose body is not blocks",
            json!({"header": [], "body": [1], "span": span}),
        ),
        ("a file that is a string", json!("a file")),
        ("an element with an unknown placement", {
            let mut file = file_with(int(1));
            file["body"][0]["Element"]["placement"] = json!("Sideways");
            file
        }),
    ]
}

#[test]
fn the_ast_schema_refuses_what_is_not_the_wire() {
    let validator = validator("ast.schema.json");
    let cases = not_the_wire();
    let accepted = accepted(&validator, &cases);
    assert!(
        accepted.is_empty(),
        "the schema accepts what is not the wire: {accepted:?}"
    );
}

/// A mutation of a real document, not a hand-made one: every document of the corpus loses its
/// `span`, or gets a header that is not declarations, and is refused.
#[test]
fn a_corpus_document_with_a_part_wrong_is_refused() {
    let validator = validator("ast.schema.json");
    for (rel, src) in parseable_corpus() {
        let doc = serde_json::to_value(parse_file(&src).unwrap()).unwrap();
        let mut broken = doc.clone();
        broken.as_object_mut().unwrap().remove("span");
        assert!(
            !validator.is_valid(&broken),
            "{} without a span",
            rel.display()
        );
        let mut broken = doc;
        broken["header"] = json!([1]);
        assert!(
            !validator.is_valid(&broken),
            "{} with a bad header",
            rel.display()
        );
    }
}

/// The negatives are a test of the schema only if they can fail: a schema that accepts everything
/// is caught by them.
#[test]
fn a_schema_that_accepts_everything_fails_the_negative_cases() {
    let cases = not_the_wire();
    for permissive in [json!(true), json!({}), json!({"type": "object"})] {
        let validator = validator_of(&permissive);
        let accepted = accepted(&validator, &cases);
        // `{"type": "object"}` still refuses the non-objects (a string, bare scalars are inside a file, so they are objects).
        assert!(
            accepted.len() + 1 >= cases.len(),
            "{permissive} should accept (almost) everything, accepted {} of {}",
            accepted.len(),
            cases.len()
        );
    }
    // And the real one accepts none of them.
    let real = validator("ast.schema.json");
    assert!(accepted(&real, &cases).is_empty());
}
