//! TypeScript declarations of the AST wire format, generated from its JSON Schemas.
//!
//! The types and the schemas have one source: `tomet spec ast-schema` writes the schemas from the Rust
//! types, and this reads those same documents ([`ast_schema::documents`]) and writes `ast.d.ts`, so
//! a type that is in the schema is in the declarations, with the same description.
//!
//! =[ The subset ]
//!
//! This is not a general JSON Schema to TypeScript converter. It reads exactly what `schemars` writes
//! for the types of the AST, and refuses everything else with the path of the construct, so a type
//! that starts to use a keyword the generator does not know is an error here and not an `unknown`
//! in the declarations:
//!
//! - `$ref` to `#/$defs/<Name>` (and `#`, the root of the document), alone or with a `description`;
//! - `oneOf` and `anyOf`, as a union; `type: [..]`, `const` and `enum`, as literal types;
//! - `type`: `null`, `boolean`, `integer` and `number` (`number`), `string`;
//! - `array` with `items` (`T[]`), or with `prefixItems` of exactly `minItems`/`maxItems` (a tuple);
//! - `object` with `properties` and `required` (an interface or a literal type, optional keys with
//!   `?`), or with `additionalProperties` a schema and no property (`{ [key: string]: T }`);
//! - the annotations `description` (a JSDoc comment), `title`, `format`, `minimum`, and
//!   `additionalProperties: false`, which say nothing a type can.

use super::ast_schema;
use serde_json::{Map, Value};
use std::collections::BTreeSet;
use std::fmt::Write as _;

/// The file `tomet spec ast-types` writes.
pub const FILE: &str = "ast.d.ts";

/// The declarations of the AST wire (`File` and what it holds) and of plain data (`PlainValue`).
pub fn render() -> Result<String, String> {
    let mut declarations: Vec<(String, String)> = Vec::new();
    for (file, schema) in ast_schema::documents() {
        let root_name = match file {
            "ast.schema.json" => "File",
            "plain.schema.json" => "PlainValue",
            other => return Err(format!("{other}: no root type is named for this schema")),
        };
        let mut found = declare_document(file, &schema, root_name)?;
        declarations.append(&mut found);
    }

    // `File` first and `PlainValue` last, the definitions between them in alphabetical order.
    let mut names = BTreeSet::new();
    for (name, _) in &declarations {
        if !names.insert(name.clone()) {
            return Err(format!("two schemas declare `{name}`"));
        }
    }
    declarations.sort_by_key(|(name, _)| match name.as_str() {
        "File" => (0, String::new()),
        "PlainValue" => (2, String::new()),
        other => (1, other.to_string()),
    });

    let mut out = String::from(HEADER);
    for (_, text) in declarations {
        out.push('\n');
        out.push_str(&text);
    }
    Ok(out)
}

/// The file `tomet spec ast-types` writes for what the bindings hand out.
pub const BINDING_FILE: &str = "binding.d.ts";

/// The definitions of the AST schemas, by name: every definition of each document, and the root of
/// each (`File`, `PlainValue`) under its own name.
fn ast_definitions() -> Result<Map<String, Value>, String> {
    let mut found = Map::new();
    for (file, schema) in ast_schema::documents() {
        let root_name = match file {
            "ast.schema.json" => "File",
            "plain.schema.json" => "PlainValue",
            other => return Err(format!("{other}: no root type is named for this schema")),
        };
        let Value::Object(mut root) = schema else {
            return Err(format!("{file}: a schema document is an object"));
        };
        let defs = match root.remove("$defs") {
            Some(Value::Object(defs)) => defs,
            _ => Map::new(),
        };
        for annotation in ["$schema", "$id", "title"] {
            root.remove(annotation);
        }
        // The root refers to itself as `#`; as a definition, it is `#/$defs/<name>`.
        let mut root = Value::Object(root);
        name_the_root(&mut root, root_name);
        found.insert(root_name.to_string(), root);
        found.extend(defs);
    }
    Ok(found)
}

/// Every `$ref` to the root of its document (`#`) in `schema`, as the `$ref` to the definition `name`.
fn name_the_root(schema: &mut Value, name: &str) {
    match schema {
        Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                match value {
                    Value::String(target) if key == "$ref" && target == "#" => {
                        *target = format!("#/$defs/{name}");
                    }
                    other => name_the_root(other, name),
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|item| name_the_root(item, name)),
        _ => {}
    }
}

/// The names `schema` refers to with `$ref`.
fn references(schema: &Value, found: &mut BTreeSet<String>) {
    match schema {
        Value::Object(map) => {
            for (key, value) in map {
                match value {
                    Value::String(target) if key == "$ref" => {
                        if let Some(name) = target.strip_prefix("#/$defs/") {
                            found.insert(name.to_string());
                        }
                    }
                    other => references(other, found),
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|item| references(item, found)),
        _ => {}
    }
}

/// The declarations of what the bindings hand out: the vault to read in, the processed file, the
/// diagnostics. What of the AST they hold is imported from `ast.d.ts`, not declared a second time.
pub fn render_binding() -> Result<String, String> {
    let schema = ast_schema::binding_document();
    let Value::Object(map) = &schema else {
        return Err(format!("{BINDING_FILE}: a schema document is an object"));
    };
    let defs = match map.get("$defs") {
        Some(Value::Object(defs)) => defs,
        _ => return Err(format!("{BINDING_FILE}: `$defs` is not an object")),
    };
    let ast = ast_definitions()?;

    let generator = Generator {
        file: ast_schema::BINDING_FILE,
        root: "",
        defs: defs.keys().cloned().collect(),
    };
    let mut imports = BTreeSet::new();
    let mut declarations: Vec<(String, String)> = Vec::new();
    for (name, def) in defs {
        if let Some(of_the_ast) = ast.get(name) {
            // The same name must be the same type: the two documents are generated from one source,
            // and a name that means two things is a clash, not an import.
            if of_the_ast != def {
                return Err(format!(
                    "{}: `{name}` is not the type the AST schemas define under that name",
                    ast_schema::BINDING_FILE
                ));
            }
            continue;
        }
        let mut referred = BTreeSet::new();
        references(def, &mut referred);
        imports.extend(referred.into_iter().filter(|r| ast.contains_key(r)));
        declarations.push((
            name.clone(),
            generator.declaration(name, def, &format!("#/$defs/{name}"))?,
        ));
    }
    declarations.sort();

    let mut out = String::from(BINDING_HEADER);
    if !imports.is_empty() {
        let names: Vec<&str> = imports.iter().map(String::as_str).collect();
        let _ = writeln!(
            out,
            "import type {{ {} }} from \"./ast.js\";",
            names.join(", ")
        );
    }
    for (_, text) in declarations {
        out.push('\n');
        out.push_str(&text);
    }
    Ok(out)
}

const BINDING_HEADER: &str = "\
// Generated by `tomet spec ast-types` from the JSON Schema of the bindings (`bindings/schema/binding.schema.json`).
// Do not edit it: change the doc comments of the Rust types, then run `tomet spec ast-schema` and
// `tomet spec ast-types` again (`just docs`).
//
// What the functions of `@tomet/wasm` take and return besides a document. The AST it refers to is `ast.d.ts`.
";

const HEADER: &str = "\
// Generated by `tomet spec ast-types` from the JSON Schemas of the AST (`bindings/schema/`).
// Do not edit it: change the doc comments of the Rust types, then run `tomet spec ast-schema` and
// `tomet spec ast-types` again (`just docs`).
//
// `File` is the wire format of a parsed document, lossless; `PlainValue` is plain data, lossy by design.
";

/// The declarations of one schema document: its root as `root_name`, and every definition.
fn declare_document(
    file: &str,
    schema: &Value,
    root_name: &str,
) -> Result<Vec<(String, String)>, String> {
    let Value::Object(map) = schema else {
        return Err(format!("{file}: a schema document is an object"));
    };
    let mut root = map.clone();
    for annotation in ["$schema", "$id", "title"] {
        root.remove(annotation);
    }
    let defs = match root.remove("$defs") {
        Some(Value::Object(defs)) => defs,
        Some(_) => return Err(format!("{file}: `$defs` is not an object")),
        None => Map::new(),
    };
    let generator = Generator {
        file,
        root: root_name,
        defs: defs.keys().cloned().collect(),
    };

    let mut out = Vec::new();
    out.push((
        root_name.to_string(),
        generator.declaration(root_name, &Value::Object(root), "#")?,
    ));
    for (name, def) in &defs {
        out.push((
            name.clone(),
            generator.declaration(name, def, &format!("#/$defs/{name}"))?,
        ));
    }
    Ok(out)
}

struct Generator<'a> {
    file: &'a str,
    /// The name of the type the document's root is.
    root: &'a str,
    defs: BTreeSet<String>,
}

/// One alternative of a union, with the description of the alternative if it has one.
struct Member {
    doc: Option<String>,
    text: String,
}

impl Generator<'_> {
    fn unsupported(&self, path: &str, what: impl std::fmt::Display) -> String {
        format!("{}: {path}: {what}", self.file)
    }

    /// `export interface Name { ... }` for an object with properties, `export type Name = ...` for the rest.
    fn declaration(&self, name: &str, schema: &Value, path: &str) -> Result<String, String> {
        let map = self.object(schema, path)?;
        let doc = description(map);
        let mut out = String::new();
        if let Some(doc) = &doc {
            out.push_str(&jsdoc(doc, ""));
        }
        let is_record = map.get("type").and_then(Value::as_str) == Some("object")
            && map.contains_key("properties");
        if is_record {
            self.only(
                map,
                &[
                    "type",
                    "properties",
                    "required",
                    "additionalProperties",
                    "description",
                ],
                path,
            )?;
            let props = self.properties(map, path)?;
            let _ = writeln!(out, "export interface {name} {{");
            out.push_str(&props);
            out.push_str("}\n");
        } else {
            let members = self.members(schema, path)?;
            let _ = writeln!(
                out,
                "export type {name} ={};",
                union(&members, 0, UnionAt::Declaration)
            );
        }
        Ok(out)
    }

    fn object<'v>(&self, schema: &'v Value, path: &str) -> Result<&'v Map<String, Value>, String> {
        schema.as_object().ok_or_else(|| {
            self.unsupported(path, format!("expected a schema object, found {schema}"))
        })
    }

    /// Fails on a keyword outside `allowed`.
    fn only(&self, map: &Map<String, Value>, allowed: &[&str], path: &str) -> Result<(), String> {
        for key in map.keys() {
            if !allowed.contains(&key.as_str()) && !ANNOTATIONS.contains(&key.as_str()) {
                return Err(self.unsupported(path, format!("the keyword `{key}` is not supported")));
            }
        }
        Ok(())
    }

    /// The TypeScript type of `schema`, as the alternatives of a union (one, for most schemas).
    fn members(&self, schema: &Value, path: &str) -> Result<Vec<Member>, String> {
        let map = self.object(schema, path)?;

        if let Some(target) = map.get("$ref") {
            self.only(map, &["$ref"], path)?;
            let name = self.reference(target, path)?;
            return Ok(vec![Member {
                doc: None,
                text: name,
            }]);
        }

        for keyword in ["oneOf", "anyOf"] {
            if let Some(branches) = map.get(keyword) {
                self.only(map, &[keyword], path)?;
                let Value::Array(branches) = branches else {
                    return Err(self.unsupported(path, format!("`{keyword}` is not an array")));
                };
                let mut members = Vec::new();
                for (i, branch) in branches.iter().enumerate() {
                    let at = format!("{path}/{keyword}/{i}");
                    let doc = description(self.object(branch, &at)?);
                    let mut found = self.members(branch, &at)?;
                    if let (Some(doc), [only]) = (doc, found.as_mut_slice()) {
                        only.doc.get_or_insert(doc);
                    }
                    members.append(&mut found);
                }
                return Ok(dedupe(members));
            }
        }

        if let Some(constant) = map.get("const") {
            self.only(map, &["const", "type"], path)?;
            return Ok(vec![Member {
                doc: None,
                text: self.literal(constant, path)?,
            }]);
        }

        if let Some(Value::Array(values)) = map.get("enum") {
            self.only(map, &["enum", "type"], path)?;
            let mut members = Vec::new();
            for value in values {
                members.push(Member {
                    doc: None,
                    text: self.literal(value, path)?,
                });
            }
            return Ok(members);
        }

        // `type` is one name, or a list of them (`["array", "null"]`, an optional list): each name
        // is a type of the same schema, which holds the keywords of all of them.
        let names: Vec<&str> = match map.get("type") {
            Some(Value::String(name)) => vec![name],
            Some(Value::Array(names)) => {
                let mut found = Vec::new();
                for name in names {
                    let Some(name) = name.as_str() else {
                        return Err(self.unsupported(path, "a type name is not a string"));
                    };
                    found.push(name);
                }
                found
            }
            _ => {
                // A schema made of something else (`allOf`, `if`, ...) names the keyword it is made of.
                self.only(map, &[], path)?;
                return Err(self.unsupported(
                    path,
                    "no `type`, `$ref`, `oneOf`, `anyOf`, `const` or `enum`",
                ));
            }
        };
        let mut allowed = vec!["type"];
        for name in &names {
            allowed.extend(match *name {
                "array" => &["items", "prefixItems", "minItems", "maxItems"][..],
                "object" => &["properties", "required", "additionalProperties"][..],
                _ => &[][..],
            });
        }
        self.only(map, &allowed, path)?;
        let mut members = Vec::new();
        for name in names {
            let text = match name {
                "null" | "boolean" | "string" => name.to_string(),
                "integer" | "number" => "number".to_string(),
                "array" => self.array(map, path)?,
                "object" => self.record(map, path)?,
                other => return Err(self.unsupported(path, format!("the type `{other}`"))),
            };
            members.push(Member { doc: None, text });
        }
        Ok(dedupe(members))
    }

    fn array(&self, map: &Map<String, Value>, path: &str) -> Result<String, String> {
        if let Some(prefix) = map.get("prefixItems") {
            let Value::Array(items) = prefix else {
                return Err(self.unsupported(path, "`prefixItems` is not an array"));
            };
            for bound in ["minItems", "maxItems"] {
                if map.get(bound).and_then(Value::as_u64) != Some(items.len() as u64) {
                    return Err(self.unsupported(
                        path,
                        format!("a tuple needs `{bound}` equal to its {} items", items.len()),
                    ));
                }
            }
            let mut texts = Vec::new();
            for (i, item) in items.iter().enumerate() {
                let members = self.members(item, &format!("{path}/prefixItems/{i}"))?;
                texts.push(union(&members, 0, UnionAt::Inline));
            }
            return Ok(format!("[{}]", texts.join(", ")));
        }
        let items = map
            .get("items")
            .ok_or_else(|| self.unsupported(path, "an array with no `items` or `prefixItems`"))?;
        let members = self.members(items, &format!("{path}/items"))?;
        let element = union(&members, 0, UnionAt::Inline);
        Ok(if members.len() > 1 {
            format!("({element})[]")
        } else {
            format!("{element}[]")
        })
    }

    /// An object type: literal `{ ... }` for properties, an index signature for `additionalProperties`.
    fn record(&self, map: &Map<String, Value>, path: &str) -> Result<String, String> {
        let additional = map.get("additionalProperties");
        if map.contains_key("properties") {
            if matches!(additional, Some(Value::Object(_) | Value::Bool(true))) {
                return Err(self.unsupported(
                    path,
                    "an object with properties and open `additionalProperties`",
                ));
            }
            let body = self.properties(map, path)?;
            let props = map["properties"].as_object().map_or(0, Map::len);
            let documented = map["properties"]
                .as_object()
                .is_some_and(|p| p.values().any(|v| v.get("description").is_some()));
            // One property without a comment stays on the line: `{ Scalar: Scalar }`.
            if props == 1 && !documented && !body.trim().contains('\n') {
                return Ok(format!("{{ {} }}", body.trim().trim_end_matches(';')));
            }
            return Ok(format!("{{\n{body}}}"));
        }
        match additional {
            Some(schema @ Value::Object(_)) => {
                let members = self.members(schema, &format!("{path}/additionalProperties"))?;
                Ok(format!(
                    "{{ [key: string]: {} }}",
                    union(&members, 0, UnionAt::Inline)
                ))
            }
            _ => Err(self.unsupported(
                path,
                "an object with neither `properties` nor a schema for `additionalProperties`",
            )),
        }
    }

    /// The lines of the properties of an object, each indented by two spaces and ended by `;`.
    fn properties(&self, map: &Map<String, Value>, path: &str) -> Result<String, String> {
        let Some(Value::Object(props)) = map.get("properties") else {
            return Err(self.unsupported(path, "`properties` is not an object"));
        };
        let required: BTreeSet<&str> = match map.get("required") {
            Some(Value::Array(names)) => names.iter().filter_map(Value::as_str).collect(),
            None => BTreeSet::new(),
            Some(_) => return Err(self.unsupported(path, "`required` is not an array")),
        };
        for name in &required {
            if !props.contains_key(*name) {
                return Err(
                    self.unsupported(path, format!("`{name}` is required but not declared"))
                );
            }
        }
        let mut out = String::new();
        for (key, schema) in props {
            let at = format!("{path}/properties/{key}");
            let prop = self.object(schema, &at)?;
            if let Some(doc) = description(prop) {
                out.push_str(&jsdoc(&doc, "  "));
            }
            let members = self.members(schema, &at)?;
            let optional = if required.contains(key.as_str()) {
                ""
            } else {
                "?"
            };
            let _ = writeln!(
                out,
                "  {}{optional}:{};",
                property_name(key),
                indent_after_first(&union(&members, 2, UnionAt::Property), 0)
            );
        }
        Ok(out)
    }

    fn reference(&self, target: &Value, path: &str) -> Result<String, String> {
        let Some(target) = target.as_str() else {
            return Err(self.unsupported(path, "`$ref` is not a string"));
        };
        if target == "#" {
            return Ok(self.root.to_string());
        }
        match target.strip_prefix("#/$defs/") {
            Some(name) if self.defs.contains(name) => Ok(name.to_string()),
            _ => Err(self.unsupported(
                path,
                format!("`$ref` to `{target}`, which is not in this document"),
            )),
        }
    }

    fn literal(&self, value: &Value, path: &str) -> Result<String, String> {
        match value {
            Value::String(_) | Value::Bool(_) | Value::Number(_) | Value::Null => {
                Ok(value.to_string())
            }
            other => Err(self.unsupported(path, format!("the literal {other}"))),
        }
    }
}

/// Keywords that annotate and say nothing a type can: allowed beside any other.
const ANNOTATIONS: &[&str] = &["description", "title", "format", "minimum"];

/// The description of a schema, if it has one.
fn description(map: &Map<String, Value>) -> Option<String> {
    map.get("description")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// The same alternative once: the first one's place, the descriptions of all of them.
fn dedupe(members: Vec<Member>) -> Vec<Member> {
    let mut out: Vec<Member> = Vec::new();
    for member in members {
        match out.iter_mut().find(|m| m.text == member.text) {
            Some(held) => {
                held.doc = match (held.doc.take(), member.doc) {
                    (Some(a), Some(b)) if a != b => Some(format!("{a}\n\n{b}")),
                    (a, b) => a.or(b),
                };
            }
            None => out.push(member),
        }
    }
    out
}

/// Where a union is written, which says how it is laid out.
#[derive(Clone, Copy, PartialEq)]
enum UnionAt {
    /// After `export type Name =`.
    Declaration,
    /// After `key:` in an object.
    Property,
    /// Inside a bigger type: an array's element, a tuple's item.
    Inline,
}

/// The text of a union at `indent`, starting with the space or newline that follows `=` or `:`.
///
/// On one line if no alternative has a comment or spans lines; otherwise one alternative per line
/// after a `|`, each under its comment.
fn union(members: &[Member], indent: usize, at: UnionAt) -> String {
    let flat = members
        .iter()
        .all(|m| m.doc.is_none() && !m.text.contains('\n'));
    if members.len() == 1 && members[0].doc.is_none() {
        let text = indent_after_first(&members[0].text, indent);
        return if at == UnionAt::Inline {
            text
        } else {
            format!(" {text}")
        };
    }
    if flat {
        let text = members
            .iter()
            .map(|m| m.text.as_str())
            .collect::<Vec<_>>()
            .join(" | ");
        return if at == UnionAt::Inline {
            text
        } else {
            format!(" {text}")
        };
    }
    let pad = " ".repeat(indent + 2);
    let mut out = String::new();
    for member in members {
        out.push('\n');
        if let Some(doc) = &member.doc {
            out.push_str(&jsdoc(doc, &pad));
        }
        let _ = write!(
            out,
            "{pad}| {}",
            indent_after_first(&member.text, indent + 4)
        );
    }
    out
}

/// `text` with `by` more spaces before every line but the first.
fn indent_after_first(text: &str, by: usize) -> String {
    let pad = " ".repeat(by);
    let mut lines = text.split('\n');
    let mut out = lines.next().unwrap_or_default().to_string();
    for line in lines {
        out.push('\n');
        if !line.is_empty() {
            out.push_str(&pad);
        }
        out.push_str(line);
    }
    out
}

/// A JSDoc comment of `text`, each line indented by `pad`, ending with a newline.
fn jsdoc(text: &str, pad: &str) -> String {
    let text = text.replace("*/", "*\\/");
    let lines: Vec<&str> = text.lines().collect();
    if let [line] = lines.as_slice() {
        return format!("{pad}/** {line} */\n");
    }
    let mut out = format!("{pad}/**\n");
    for line in lines {
        if line.is_empty() {
            let _ = writeln!(out, "{pad} *");
        } else {
            let _ = writeln!(out, "{pad} * {line}");
        }
    }
    let _ = writeln!(out, "{pad} */");
    out
}

/// A property name, quoted if it is not an identifier.
fn property_name(key: &str) -> String {
    let mut chars = key.chars();
    let identifier = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$');
    if identifier {
        key.to_string()
    } else {
        Value::String(key.into()).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    /// The declarations of a schema document of one root, `Root`, and the given definitions.
    fn declare(schema: Value) -> Result<String, String> {
        declare_document("test.schema.json", &schema, "Root").map(|found| {
            found
                .into_iter()
                .map(|(_, text)| text)
                .collect::<Vec<_>>()
                .join("\n")
        })
    }

    #[test]
    fn the_declarations_are_what_the_schemas_say() {
        let committed = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../bindings/js")
            .join(FILE);
        let text = std::fs::read_to_string(&committed)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", committed.display()));
        assert!(
            text == render().unwrap(),
            "{} is out of date; regenerate it with `tomet spec ast-types --out bindings/js`",
            committed.display()
        );
    }

    #[test]
    fn every_definition_of_the_schemas_is_declared() {
        let text = render().unwrap();
        for (file, schema) in ast_schema::documents() {
            let Some(Value::Object(defs)) = schema.get("$defs") else {
                continue;
            };
            for name in defs.keys() {
                let declared = format!("export interface {name} ");
                let aliased = format!("export type {name} =");
                assert!(
                    text.contains(&declared) || text.contains(&aliased),
                    "{file}: `{name}` is not declared"
                );
            }
        }
        for root in ["File", "PlainValue"] {
            assert!(
                text.contains(&format!("export interface {root} "))
                    || text.contains(&format!("export type {root} =")),
                "{root}"
            );
        }
        // No loose type: outside the comments, nothing is `unknown` or `any`.
        let code: Vec<&str> = text
            .lines()
            .filter(|line| {
                let line = line.trim_start();
                !(line.starts_with("/**") || line.starts_with('*') || line.starts_with("//"))
            })
            .collect();
        for line in code {
            for loose in ["unknown", "any"] {
                assert!(
                    !line
                        .split(|c: char| !c.is_alphanumeric())
                        .any(|w| w == loose),
                    "`{loose}` in `{line}`"
                );
            }
        }
    }

    #[test]
    fn an_object_is_an_interface_with_optional_keys_marked() {
        let out = declare(json!({
            "type": "object",
            "description": "A root.",
            "properties": {
                "a": {"type": "string", "description": "The a."},
                "b": {"anyOf": [{"type": "integer"}, {"type": "null"}]},
                "weird-key": {"type": "boolean"}
            },
            "required": ["a", "b"]
        }))
        .unwrap();
        assert_eq!(
            out,
            "/** A root. */\nexport interface Root {\n  /** The a. */\n  a: string;\n  b: number | null;\n  \"weird-key\"?: boolean;\n}\n"
        );
    }

    #[test]
    fn a_tagged_enum_is_a_union_of_single_key_objects() {
        let out = declare(json!({
            "$defs": {"Value": {"oneOf": [
                {"type": "object", "properties": {"A": {"$ref": "#/$defs/Value"}}, "required": ["A"], "additionalProperties": false},
                {"type": "string", "const": "B", "description": "The B."}
            ]}},
            "$ref": "#/$defs/Value"
        }))
        .unwrap();
        assert!(
            out.contains("export type Value =\n  | { A: Value }\n"),
            "{out}"
        );
        assert!(out.contains("  /** The B. */\n  | \"B\""), "{out}");
    }

    #[test]
    fn arrays_tuples_maps_and_the_root_reference() {
        let out = declare(json!({
            "anyOf": [
                {"type": "null"},
                {"type": "array", "items": {"$ref": "#"}},
                {"type": "object", "additionalProperties": {"$ref": "#"}},
                {"type": "array", "prefixItems": [{"type": "string"}, {"type": "integer"}], "minItems": 2, "maxItems": 2},
                {"type": "array", "items": {"anyOf": [{"type": "string"}, {"type": "null"}]}}
            ]
        }))
        .unwrap();
        assert_eq!(
            out,
            "export type Root = null | Root[] | { [key: string]: Root } | [string, number] | (string | null)[];\n"
        );
    }

    #[test]
    fn what_the_generator_does_not_know_is_an_error_naming_where() {
        for (schema, expected) in [
            (json!({"allOf": [{"type": "string"}]}), "allOf"),
            (json!({"type": "string", "pattern": "^a"}), "pattern"),
            (
                json!({"type": "object", "properties": {"a": {"if": {}}}}),
                "#/properties/a",
            ),
            (json!({"$ref": "#/$defs/Missing"}), "Missing"),
            (json!({"type": "array"}), "items"),
            (json!({"type": "object"}), "additionalProperties"),
            (
                json!({"type": "array", "prefixItems": [{"type": "string"}]}),
                "minItems",
            ),
            (json!({"type": "number", "multipleOf": 2}), "multipleOf"),
            (json!(true), "a schema document is an object"),
            (json!({}), "no `type`"),
        ] {
            let error = declare(schema.clone()).expect_err(&schema.to_string());
            assert!(error.contains(expected), "{schema}: {error}");
        }
    }

    #[test]
    fn a_doc_comment_cannot_end_the_comment_it_is_in() {
        assert_eq!(jsdoc("a */ b", ""), "/** a *\\/ b */\n");
        assert_eq!(
            jsdoc("a\n\nb", "  "),
            "  /**\n   * a\n   *\n   * b\n   */\n"
        );
    }
}
