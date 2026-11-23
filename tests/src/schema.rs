//! The JSON Schemas of the AST (`bindings/schema/`) say what the serde shape of the AST is: every
//! document of the corpus, serialized, is valid against `ast.schema.json`, and every plain value
//! against `plain.schema.json`.
//!
//! The schemas are generated from the Rust types (`tomet spec ast-schema`), and a test of the CLI holds
//! the committed files to what the types generate. This one holds what is generated to what serde
//! writes, which the derive cannot promise where a type is serialized by hand (`PlainValue`) or
//! through an attribute (`Tuple`).
//!
//! The validator below is the part of JSON Schema that schemars writes, nothing more: a keyword it
//! does not know is a failure, not a silent pass, so a schema that starts to say something new
//! cannot be taken for validated.

use serde_json::Value;
use std::path::PathBuf;
use tomet_ast::{File, Value as AstValue};
use tomet_ast_parser::{parse_data_file, parse_file};
use tomet_tests::{fixtures_dir, parseable_corpus};

fn schema(file: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../bindings/schema")
        .join(file);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The first reason `instance` is not valid against `schema` (found in `root`), if any, with the
/// path to it.
fn invalid(root: &Value, schema: &Value, instance: &Value, path: &str) -> Option<String> {
    let fail = |why: String| Some(format!("{path}: {why}"));
    let Value::Object(keywords) = schema else {
        return match schema {
            Value::Bool(true) => None,
            other => panic!("unsupported schema {other}"),
        };
    };
    for (keyword, argument) in keywords {
        match keyword.as_str() {
            "$schema" | "$id" | "$defs" | "title" | "description" | "format" => {}
            "$ref" => {
                let target = match argument.as_str().unwrap() {
                    "#" => root,
                    r => root
                        .pointer(r.strip_prefix('#').unwrap())
                        .unwrap_or_else(|| panic!("dangling $ref {r}")),
                };
                if let Some(why) = invalid(root, target, instance, path) {
                    return Some(why);
                }
            }
            "type" => {
                let names: Vec<&str> = match argument {
                    Value::String(name) => vec![name.as_str()],
                    Value::Array(names) => names.iter().map(|n| n.as_str().unwrap()).collect(),
                    other => panic!("unsupported type {other}"),
                };
                let is = |name: &str| match name {
                    "null" => instance.is_null(),
                    "boolean" => instance.is_boolean(),
                    "string" => instance.is_string(),
                    "array" => instance.is_array(),
                    "object" => instance.is_object(),
                    "number" => instance.is_number(),
                    "integer" => instance.is_i64() || instance.is_u64(),
                    other => panic!("unsupported type name {other}"),
                };
                if !names.iter().any(|n| is(n)) {
                    return fail(format!("expected {names:?}, got {instance}"));
                }
            }
            "const" => {
                if instance != argument {
                    return fail(format!("expected {argument}, got {instance}"));
                }
            }
            "enum" => {
                if !argument.as_array().unwrap().contains(instance) {
                    return fail(format!("{instance} is not one of {argument}"));
                }
            }
            "minimum" => {
                if instance
                    .as_f64()
                    .is_some_and(|n| n < argument.as_f64().unwrap())
                {
                    return fail(format!("{instance} is below {argument}"));
                }
            }
            "required" => {
                if let Value::Object(map) = instance {
                    for key in argument.as_array().unwrap() {
                        if !map.contains_key(key.as_str().unwrap()) {
                            return fail(format!("missing `{}`", key.as_str().unwrap()));
                        }
                    }
                }
            }
            "properties" => {
                if let Value::Object(map) = instance {
                    for (key, sub) in argument.as_object().unwrap() {
                        if let Some(value) = map.get(key)
                            && let Some(why) = invalid(root, sub, value, &format!("{path}.{key}"))
                        {
                            return Some(why);
                        }
                    }
                }
            }
            "additionalProperties" => {
                if let Value::Object(map) = instance {
                    let known = keywords.get("properties").and_then(Value::as_object);
                    for (key, value) in map {
                        if known.is_some_and(|k| k.contains_key(key)) {
                            continue;
                        }
                        match argument {
                            Value::Bool(false) => return fail(format!("unexpected `{key}`")),
                            sub => {
                                if let Some(why) =
                                    invalid(root, sub, value, &format!("{path}.{key}"))
                                {
                                    return Some(why);
                                }
                            }
                        }
                    }
                }
            }
            "items" => {
                if let Value::Array(items) = instance {
                    let skip = keywords
                        .get("prefixItems")
                        .map_or(0, |p| p.as_array().unwrap().len());
                    for (i, item) in items.iter().enumerate().skip(skip) {
                        if let Some(why) = invalid(root, argument, item, &format!("{path}[{i}]")) {
                            return Some(why);
                        }
                    }
                }
            }
            "prefixItems" => {
                if let Value::Array(items) = instance {
                    for (i, (sub, item)) in
                        argument.as_array().unwrap().iter().zip(items).enumerate()
                    {
                        if let Some(why) = invalid(root, sub, item, &format!("{path}[{i}]")) {
                            return Some(why);
                        }
                    }
                }
            }
            "minItems" | "maxItems" => {
                if let Value::Array(items) = instance {
                    let bound = argument.as_u64().unwrap() as usize;
                    let bad = if keyword == "minItems" {
                        items.len() < bound
                    } else {
                        items.len() > bound
                    };
                    if bad {
                        return fail(format!("{} items, {keyword} {bound}", items.len()));
                    }
                }
            }
            "oneOf" | "anyOf" => {
                let branches = argument.as_array().unwrap();
                let valid = branches
                    .iter()
                    .filter(|b| invalid(root, b, instance, path).is_none())
                    .count();
                let ok = if keyword == "oneOf" {
                    valid == 1
                } else {
                    valid >= 1
                };
                if !ok {
                    return fail(format!(
                        "{valid} of {} alternatives of `{keyword}` match {instance}",
                        branches.len()
                    ));
                }
            }
            other => panic!("the schema uses `{other}`, which this validator does not know"),
        }
    }
    None
}

fn assert_valid(file: &str, what: &str, instance: &Value) {
    let root = schema(file);
    if let Some(why) = invalid(&root, &root, instance, "$") {
        panic!("{what} is not valid against {file}: {why}");
    }
}

#[test]
fn every_document_of_the_corpus_is_valid_against_the_ast_schema() {
    for (rel, src) in parseable_corpus() {
        let doc: File = parse_file(&src).unwrap_or_else(|e| panic!("{}: {e}", rel.display()));
        assert_valid(
            "ast.schema.json",
            &rel.display().to_string(),
            &serde_json::to_value(&doc).unwrap(),
        );
    }
}

/// The validator is not a rubber stamp: what the schema forbids, it refuses.
#[test]
fn the_ast_schema_refuses_what_is_not_the_wire() {
    let root = schema("ast.schema.json");
    let doc = parse_file("@x(a: enum(1))\n").unwrap();
    let good = serde_json::to_value(&doc).unwrap();
    assert!(invalid(&root, &root, &good, "$").is_none());

    // The old natural shape of a value: a bare number where `{"Scalar": ...}` is.
    let mut bad = good.clone();
    let args = &mut bad["body"][0]["Element"]["data"]["args"];
    args["entries"][0]["value"] = serde_json::json!({"call": "enum", "args": [1]});
    assert!(invalid(&root, &root, &bad, "$").is_some());

    // A missing field.
    let mut bad = good;
    bad.as_object_mut().unwrap().remove("span");
    assert!(invalid(&root, &root, &bad, "$").is_some());
}

#[test]
fn plain_data_is_valid_against_the_plain_schema() {
    let dir = fixtures_dir().join("cotomet");
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).unwrap().flatten() {
        let path = entry.path();
        if !path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("ok-")
        {
            continue;
        }
        let doc = parse_data_file(&std::fs::read_to_string(&path).unwrap()).unwrap();
        // A data file that holds what is not data has no plain form, which is not the schema's business.
        let AstValue::Tuple(body) = &doc.body else {
            continue;
        };
        let Ok(plain) = body.to_plain() else { continue };
        assert_valid(
            "plain.schema.json",
            &path.display().to_string(),
            &serde_json::to_value(&plain).unwrap(),
        );
        checked += 1;
    }
    assert!(checked > 0, "no data fixture has a plain form");

    // Each shape of plain data, once.
    let value = tomet_ast_parser::parse_value(
        "a: 1\nb: (x, y)\nc: (1, k: 2)\nd: ()\ne: none\nf: 1.10\ng: true\n",
    )
    .unwrap();
    assert_valid(
        "plain.schema.json",
        "a value of every plain shape",
        &serde_json::to_value(value.to_plain().unwrap()).unwrap(),
    );
}

/// Plain data is any JSON: the schema holds that, and the projection's job is to refuse what is not it.
#[test]
fn any_json_is_plain_data() {
    let root = schema("plain.schema.json");
    for json in [
        serde_json::json!(null),
        serde_json::json!({"Scalar": {"Int": 1}}),
        serde_json::json!({"a": [1, 2.5, {"b": null}]}),
    ] {
        assert!(invalid(&root, &root, &json, "$").is_none(), "{json}");
    }
}
