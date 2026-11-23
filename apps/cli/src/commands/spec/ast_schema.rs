//! JSON Schema of the AST wire format, generated from the Rust types.
//!
//! The AST crates derive `schemars::JsonSchema` (behind their `schema` feature) next to the serde
//! derives, so the schema is the serde shape of the types and cannot be described by hand. Two
//! documents come out, one per contract a consumer can hold:
//!
//! - `ast.schema.json`: the AST wire, rooted at `File`. Lossless; what `parseFile` hands out and
//!   `toHtml(file)` and friends take back.
//! - `plain.schema.json`: plain data, rooted at `PlainValue`. Lossy by design; what `parseValue`, `loads`
//!   and a processed file's `meta` hand out.
//!
//! They are two files because they are two contracts with nothing in common: a consumer that wants
//! one should not have to carry, or to resolve a `$ref` into, the other.
//!
//! Descriptions are the Rust doc comments, rewritten for a reader who does not know Rust: rustdoc
//! links lose their link (`` [`Entry`](crate::Entry) `` is `` `Entry` ``), Rust paths lose their crate
//! prefix, the `=[ Heading ]` of Tomet markup is a bold line, and a section `=[ In Rust ]` is left out.

use schemars::JsonSchema;
use schemars::generate::{Contract, SchemaSettings};
use serde_json::{Map, Value};

/// The schema documents `tomet spec ast-schema` writes: file name, and the schema.
///
/// `tomet spec ast-types` reads these same documents, so the types and the schemas have one source.
pub fn documents() -> Vec<(&'static str, Value)> {
    vec![
        (
            "ast.schema.json",
            document::<tomet_ast::File>("ast.schema.json", "Tomet AST (wire format)"),
        ),
        (
            "plain.schema.json",
            document::<tomet_ast::PlainValue>("plain.schema.json", "Tomet plain data"),
        ),
    ]
}

/// The schema documents, rendered: file name, and the text of the file.
pub fn render() -> Vec<(&'static str, String)> {
    documents()
        .into_iter()
        .map(|(file, schema)| {
            let mut text = serde_json::to_string_pretty(&schema).expect("a schema serializes");
            text.push('\n');
            (file, text)
        })
        .collect()
}

/// The version every identifier carries: the workspace version, which is the version of `tomet-ast`
/// (every crate of the workspace takes `version.workspace`). A change of the wire is a release.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Where the schemas are published. An identifier, not a locator: nothing is fetched from it.
const ID_BASE: &str = "https://github.com/tomet-lang/tomet/schemas";

fn document<T: JsonSchema>(file: &str, title: &str) -> Value {
    let settings = SchemaSettings::draft2020_12().with(|s| {
        // What this crate writes: every field is present (an absent one is `null`), so the schema
        // says so, which is what a reader of the type needs. The reading side accepts the same.
        s.contract = Contract::Serialize;
    });
    let schema = settings.into_generator().into_root_schema_for::<T>();
    let mut root = match schema.to_value() {
        Value::Object(map) => map,
        other => panic!("a root schema is an object, got {other}"),
    };
    clean_descriptions(&mut root);

    // The identity of the document goes first, then what the root is, then its definitions.
    let mut out = Map::new();
    out.insert(
        "$schema".into(),
        root.remove("$schema")
            .expect("a draft 2020-12 root names its dialect"),
    );
    out.insert(
        "$id".into(),
        Value::String(format!("{ID_BASE}/{VERSION}/{file}")),
    );
    out.insert("title".into(), Value::String(title.into()));
    // The root is described by the doc comment of its type, like every other definition.
    root.remove("title");
    if let Some(description) = root.remove("description") {
        out.insert("description".into(), description);
    }
    out.extend(root);

    Value::Object(out)
}

/// Rewrites every `description` under `value` for a reader who does not know Rust.
fn clean_descriptions(map: &mut Map<String, Value>) {
    for (key, value) in map.iter_mut() {
        match value {
            Value::String(text) if key == "description" => *text = for_everyone(text),
            Value::Object(inner) => clean_descriptions(inner),
            Value::Array(items) => {
                for item in items {
                    if let Value::Object(inner) = item {
                        clean_descriptions(inner);
                    }
                }
            }
            _ => {}
        }
    }
}

/// A doc comment as a description for someone who reads JSON, not Rust.
///
/// - `` [`X`] `` and `` [`X`](path::to::X) `` and `[text](path)` are the text, in code style if it was;
/// - a Rust path loses its crate prefix: `crate::Entry` and `tomet_scalar::Scalar` are `Entry` and `Scalar`;
/// - a Tomet heading line, `=[ Wire ]`, is a bold line;
/// - a section headed `=[ In Rust ]` is left out, up to the next heading: it is about how the type is
///   built and compared in Rust, which the JSON does not say.
fn for_everyone(doc: &str) -> String {
    let mut lines = Vec::new();
    let mut in_rust = false;
    for line in doc.lines() {
        match heading(line) {
            Some(text) => {
                in_rust = text == "In Rust";
                if !in_rust {
                    lines.push(format!("**{text}**"));
                }
            }
            None if !in_rust => lines.push(line.to_string()),
            None => {}
        }
    }
    strip_rust_paths(&strip_links(lines.join("\n").trim_end()))
}

/// The text of a Tomet heading line (`=[ Wire ]`, `==[ Wire ]`), if the line is one.
fn heading(line: &str) -> Option<&str> {
    let line = line.trim();
    let inner = line.trim_start_matches('=');
    if inner.len() == line.len() {
        return None;
    }
    inner.strip_prefix('[')?.strip_suffix(']').map(str::trim)
}

/// Removes the link from every rustdoc link, keeping what is said about it.
fn strip_links(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        // `[label]`, with a label that has no bracket, then an optional `(target)`.
        let Some(close) = after
            .find([']', '[', '\n'])
            .filter(|&at| after[at..].starts_with(']'))
        else {
            out.push('[');
            rest = after;
            continue;
        };
        let label = &after[..close];
        let tail = &after[close + 1..];
        let (target, tail) = match tail.strip_prefix('(').and_then(|t| t.split_once(')')) {
            Some((target, tail)) if !target.contains(char::is_whitespace) => (Some(target), tail),
            _ => (None, tail),
        };
        match target {
            // A real link (`https://...`) is kept as the Markdown it is.
            Some(target) if target.contains("://") => {
                out.push_str(&format!("[{label}]({target})"));
            }
            // A rustdoc link: the label.
            Some(_) => out.push_str(label),
            // `[`Code`]` is a rustdoc reference; anything else in brackets is just text.
            None if label.starts_with('`') && label.ends_with('`') && label.len() > 1 => {
                out.push_str(label);
            }
            None => out.push_str(&format!("[{label}]")),
        }
        rest = tail;
    }
    out.push_str(rest);
    out
}

/// Removes the crate prefix of a Rust path: `crate::`, and `tomet_<name>::`.
fn strip_rust_paths(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("::") {
        let before = &rest[..at];
        // The path segment before `::`, back to the last character that cannot be part of a path.
        let start = before
            .rfind(|c: char| !(c.is_alphanumeric() || c == '_'))
            .map_or(0, |i| i + 1);
        let segment = &before[start..];
        if segment == "crate" || segment.starts_with("tomet_") {
            out.push_str(&before[..start]);
        } else {
            out.push_str(before);
            out.push_str("::");
        }
        rest = &rest[at + 2..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn rustdoc_links_lose_their_link() {
        assert_eq!(
            for_everyone("A [`Value`], see [`Tuple::keyed`] and [`Span`](tomet_span::Span)."),
            "A `Value`, see `Tuple::keyed` and `Span`."
        );
        assert_eq!(
            for_everyone("A [module](self) and [`crate::Entry::Element`]."),
            "A module and `Entry::Element`."
        );
        assert_eq!(
            for_everyone("[a link](https://example.com) and [not a link] and `a[0]`"),
            "[a link](https://example.com) and [not a link] and `a[0]`"
        );
    }

    #[test]
    fn a_tomet_heading_is_a_bold_line() {
        assert_eq!(
            for_everyone("Text\n\n=[ Wire ]\n\nMore"),
            "Text\n\n**Wire**\n\nMore"
        );
    }

    #[test]
    fn a_section_about_rust_is_left_out() {
        assert_eq!(
            for_everyone("Text\n\n=[ In Rust ]\n\nNot for JSON.\n\n=[ Wire ]\n\nMore"),
            "Text\n\n**Wire**\n\nMore"
        );
    }

    /// The schemas are generated, so what is committed is what the types say: a change of the wire
    /// is a change of `bindings/schema/`, and a review sees both.
    #[test]
    fn the_committed_schemas_are_what_the_types_generate() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../bindings/schema");
        for (file, rendered) in render() {
            let path = dir.join(file);
            let committed = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
            assert!(
                committed == rendered,
                "{} is out of date; regenerate it with `tomet spec ast-schema --out bindings/schema`",
                path.display()
            );
        }
    }

    /// No Rust leaks into a description: no link syntax, no crate path.
    #[test]
    fn no_description_reads_as_rust() {
        fn descriptions(value: &Value, found: &mut Vec<String>) {
            match value {
                Value::Object(map) => {
                    for (key, inner) in map {
                        match inner {
                            Value::String(text) if key == "description" => found.push(text.clone()),
                            _ => descriptions(inner, found),
                        }
                    }
                }
                Value::Array(items) => items.iter().for_each(|i| descriptions(i, found)),
                _ => {}
            }
        }
        for (file, rendered) in render() {
            let value: Value = serde_json::from_str(&rendered).unwrap();
            let mut found = Vec::new();
            descriptions(&value, &mut found);
            assert!(!found.is_empty(), "{file} has no description at all");
            for text in found {
                for leak in ["crate::", "tomet_", "](", "[`", "Vec<", "Option<", "Box<"] {
                    assert!(
                        !text.contains(leak),
                        "{file}: `{leak}` in a description: {text}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_identity_of_a_schema_carries_the_version() {
        for (file, rendered) in render() {
            let value: Value = serde_json::from_str(&rendered).unwrap();
            assert_eq!(
                value["$schema"],
                "https://json-schema.org/draft/2020-12/schema"
            );
            assert_eq!(
                value["$id"],
                format!("{ID_BASE}/{VERSION}/{file}"),
                "{file}"
            );
        }
    }
}
