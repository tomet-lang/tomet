//! Reading what the standard declarations say, from a [`Declaration`] or from a whole document.
//!
//! One reader per thing a declaration states. The `document_*` functions are the same readers
//! applied to the first (or every) declaration of that name at the top of a [`Document`].

use std::collections::BTreeMap;

use tomet_ast::{Document, Value};
use tomet_tree::ValueExt;

use crate::declaration::{Declaration, declarations};
use crate::kind::DeclKind;
use crate::vars::{VarDecl, read_var_decls};

/// The name of the kind a `::kind(...)` declares: `::kind(j.daily)` or `::kind(kind: j.daily)`.
///
/// The second positional slot, the major version, is not part of the name.
pub fn kind_name(declaration: &Declaration) -> Option<String> {
    if let Some(args) = declaration.bound_args() {
        if let Some(name) = args.get("kind").and_then(text) {
            return Some(name);
        }
        if let Some(name) = text(&args) {
            return Some(name);
        }
    }
    let data = declaration.data.as_ref()?;
    data.get("kind").and_then(text).or_else(|| text(data))
}

/// The identity a `::id(...)` declares: `::id(std.config)` or `::id(id: std.config)`.
///
/// `None` when it is not written as text, such as `::id(${uuid()})` before it is evaluated.
pub fn id_name(declaration: &Declaration) -> Option<String> {
    if let Some(args) = declaration.bound_args() {
        if let Some(name) = args.get("id").and_then(text) {
            return Some(name);
        }
        if let Some(name) = text(&args) {
            return Some(name);
        }
    }
    let data = declaration.data.as_ref()?;
    data.get("id").and_then(text).or_else(|| text(data))
}

/// The kind of document a `::blueprint(X)` or `::blueprint(target: X)` is for.
pub fn blueprint_target(declaration: &Declaration) -> Option<String> {
    let args = declaration.bound_args()?;
    text(&args).or_else(|| args.get("target").and_then(text))
}

/// The version a `::version(...)` declares, as written: `::version(1.10)` is `"1.10"`, not `"1.1"`.
pub fn version_string(declaration: &Declaration) -> Option<String> {
    let from = |value: &Value| match value.get("version") {
        Some(version) => scalar_text_or_debug(version),
        None => scalar_text_or_debug(value),
    };
    if let Some(args) = declaration.bound_args() {
        return Some(from(&args));
    }
    declaration.data.as_ref().map(from)
}

/// What a `::meta{ ... }` states: its data, the map of fields. `None` for a `::meta` with no `{...}`.
pub fn meta(declaration: &Declaration) -> Option<Value> {
    declaration.data.clone()
}

/// The macros a `::macros` defines, in the order written: each entry is a name and its template.
///
/// An entry whose value is not a string is not a macro and is left out; `tomet check` reports a
/// definition it cannot use. The `(args)` form (`::macros(gh: "...")`) and the `{data}` form are both
/// read.
pub fn macros(declaration: &Declaration) -> Vec<(String, String)> {
    entries(declaration)
        .into_iter()
        .filter_map(|(name, template)| match template {
            Value::String(template) => Some((name, template)),
            _ => None,
        })
        .collect()
}

/// The `(name, value)` entries of a declaration whose body is a map, `(args)` first, then `{data}`.
pub(crate) fn entries(declaration: &Declaration) -> Vec<(String, Value)> {
    let args = match declaration.bound_args() {
        Some(Value::Map(entries)) => entries,
        _ => Vec::new(),
    };
    let data = match &declaration.data {
        Some(Value::Map(entries)) => entries.clone(),
        _ => Vec::new(),
    };
    args.into_iter().chain(data).collect()
}

fn first(doc: &Document, kind: DeclKind) -> Option<Declaration> {
    declarations(doc)
        .into_iter()
        .find(|found| found.kind == kind)
        .map(|found| found.declaration)
}

/// The kind this document declares with its first `::kind(...)`.
pub fn document_kind(doc: &Document) -> Option<String> {
    kind_name(&first(doc, DeclKind::Kind)?)
}

/// The identity this document declares with its first `::id(...)`.
pub fn document_id(doc: &Document) -> Option<String> {
    id_name(&first(doc, DeclKind::Id)?)
}

/// The kind a blueprint document is for; `None` for a document that is not a blueprint.
pub fn document_blueprint(doc: &Document) -> Option<String> {
    blueprint_target(&first(doc, DeclKind::Blueprint)?)
}

/// The language version this document declares, as written.
pub fn document_version(doc: &Document) -> Option<String> {
    version_string(&first(doc, DeclKind::Version)?)
}

/// The data of the document's first `::meta`. Only the first is read: `::meta` is written once.
pub fn document_meta(doc: &Document) -> Option<Value> {
    meta(&first(doc, DeclKind::Meta)?)
}

/// The `(name, value)` entries of the document's first declaration of `kind` whose body is a map
/// (`::format{ ... }`, `::fields{ ... }`, `::blueprints{ ... }`): `(args)` first, then `{data}`.
/// Empty when the document does not write it.
pub fn document_entries(doc: &Document, kind: DeclKind) -> Vec<(String, Value)> {
    first(doc, kind)
        .map(|found| entries(&found))
        .unwrap_or_default()
}

/// Every macro the document defines, in the order written; a name defined twice appears twice,
/// and the later one is the one that stands.
pub fn document_macros(doc: &Document) -> Vec<(String, String)> {
    declarations(doc)
        .iter()
        .filter(|found| found.kind == DeclKind::Macros)
        .flat_map(|found| macros(&found.declaration))
        .collect()
}

/// Every variable the document declares that can be read, by name. A name declared twice is the later
/// declaration. An entry that is not a declaration is left out; `tomet check` says why.
pub fn document_vars(doc: &Document) -> Vec<(String, VarDecl)> {
    let mut vars: Vec<(String, VarDecl)> = Vec::new();
    let mut index: BTreeMap<String, usize> = BTreeMap::new();
    for found in declarations(doc)
        .iter()
        .filter(|found| found.kind == DeclKind::Vars)
    {
        for (name, decl) in read_var_decls(&found.declaration) {
            let Ok(decl) = decl else { continue };
            match index.get(&name) {
                Some(&i) => vars[i].1 = decl,
                None => {
                    index.insert(name.clone(), vars.len());
                    vars.push((name, decl));
                }
            }
        }
    }
    vars
}

/// The text of a string value.
fn text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        _ => None,
    }
}

/// A version as written. A number the reader kept as written is its text; one it did not is the text
/// it prints as.
fn scalar_text_or_debug(value: &Value) -> String {
    match value.scalar_text() {
        Some(text) => text.into_owned(),
        None => format!("{value:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Document {
        tomet_parser::parse_document(src).expect("parses")
    }

    #[test]
    fn an_id_is_what_the_id_declaration_names_dots_and_all() {
        assert_eq!(
            document_id(&parse("::id(std.config)\n\n=[ T ]\n")).as_deref(),
            Some("std.config")
        );
        assert_eq!(
            document_id(&parse("::id(doc-kWBcIUUz)\n")).as_deref(),
            Some("doc-kWBcIUUz")
        );
        assert_eq!(document_id(&parse("::kind(x)\n")), None);
        // `::meta{ id: ... }` is not an identity any more.
        assert_eq!(document_id(&parse("::meta{ id: a }\n")), None);
    }

    #[test]
    fn a_kind_is_what_the_kind_declaration_names() {
        assert_eq!(
            document_kind(&parse("::kind(j.daily)\n\n=[ Daily ]\n")).as_deref(),
            Some("j.daily")
        );
        assert_eq!(
            document_kind(&parse("::kind(config, 2)\n")).as_deref(),
            Some("config")
        );
        assert_eq!(
            document_kind(&parse("::kind(kind: note)\n")).as_deref(),
            Some("note")
        );
        assert_eq!(document_kind(&parse("::meta{ id: x }\n")), None);
        // written as an element it is not a declaration
        assert_eq!(document_kind(&parse("@kind(config)\n")), None);
    }

    #[test]
    fn a_blueprint_names_the_kind_it_is_for() {
        for src in [
            "::blueprint(daily-note)\n",
            "::kind(blueprint)\n::blueprint(daily-note)\n\n=[ Tasks ]\n",
            "::blueprint(target: daily-note)\n",
        ] {
            assert_eq!(
                document_blueprint(&parse(src)).as_deref(),
                Some("daily-note"),
                "{src:?}"
            );
        }
        assert_eq!(document_blueprint(&parse("::kind(note)\n")), None);
    }

    #[test]
    fn a_version_is_the_text_it_was_written_as() {
        for (src, want) in [
            ("::version(1.0)\n", "1.0"),
            ("::version(1.10)\n", "1.10"),
            ("::version(1.10.0)\n", "1.10.0"),
            ("::version(2)\n", "2"),
            ("::version(\"1.0\")\n", "1.0"),
            ("::version(version: 1.10)\n", "1.10"),
        ] {
            assert_eq!(
                document_version(&parse(src)).as_deref(),
                Some(want),
                "{src}"
            );
        }
        assert_eq!(document_version(&parse("::kind(a)\n")), None);
    }

    #[test]
    fn meta_is_the_data_of_the_first_meta_declaration() {
        let value = document_meta(&parse("::meta{ title: Hello }\n")).expect("meta");
        assert_eq!(
            value,
            Value::Map(vec![("title".into(), Value::String("Hello".into()))])
        );
        assert_eq!(document_meta(&parse("#[ Hello ]\n")), None);
        // no `{...}`: nothing to read
        assert_eq!(document_meta(&parse("::meta\n")), None);
    }

    #[test]
    fn macros_are_the_string_entries_of_every_macros_declaration() {
        let doc = parse("::macros{ gh: \"https://x/${1}\", n: 3, wiki: \"w/${1}\" }\n");
        assert_eq!(
            document_macros(&doc),
            [
                ("gh".to_string(), "https://x/${1}".to_string()),
                ("wiki".to_string(), "w/${1}".to_string())
            ]
        );
        // the args form is read too
        let doc = parse("::macros(gh: \"g\")\n");
        assert_eq!(document_macros(&doc), [("gh".to_string(), "g".to_string())]);
    }

    #[test]
    fn a_variable_declared_twice_is_the_later_declaration() {
        let doc = parse("::vars{ a: { default: 1 } }\n::vars{ a: { default: 2 }, b: {} }\n");
        let vars = document_vars(&doc);
        assert_eq!(vars.len(), 2);
        assert_eq!(vars[0].0, "a");
        assert_eq!(vars[0].1.default, Some(Value::Int(2)));
        assert_eq!(vars[1].0, "b");
    }
}
