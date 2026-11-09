//! Reading what the standard declarations say, from a [`Declaration`] or from a whole document.
//!
//! One reader per thing a declaration states. The `document_*` functions are the same readers
//! applied to the first (or every) declaration of that name at the top of a [`File`].

use std::collections::BTreeMap;

use tomet_ast::{File, Value};
use tomet_tree::ValueExt;

use crate::declaration::{Declaration, declarations};
use crate::kind::DeclKind;
use crate::profile::Profile;
use crate::vars::{VarDecl, read_var_decls};

/// The name of the kind a `::kind(...)` declares: `::kind(j.daily)` or `::kind(kind: j.daily)`.
///
/// The second positional slot, the major version, is not part of the name.
pub fn kind_name(declaration: &Declaration) -> Option<String> {
    let args = declaration.bound_args()?;
    args.get("kind").and_then(text).or_else(|| text(&args))
}

/// The identity a `::id(...)` declares: `::id(std.config)` or `::id(id: std.config)`.
///
/// `None` when it is not written as text, such as `::id(${uuid()})` before it is evaluated.
pub fn id_name(declaration: &Declaration) -> Option<String> {
    let args = declaration.bound_args()?;
    args.get("id").and_then(text).or_else(|| text(&args))
}

/// The kind of document a `::blueprint(X)` or `::blueprint(target: X)` is for.
pub fn blueprint_target(declaration: &Declaration) -> Option<String> {
    let args = declaration.bound_args()?;
    text(&args).or_else(|| args.get("target").and_then(text))
}

/// The name a `::dictionary(tomet)` or `::dictionary(name: tomet)` gives its dictionary; a term is mentioned
/// through it (`@term(tomet#vault-config)`).
pub fn dictionary_name(declaration: &Declaration) -> Option<String> {
    let args = declaration.bound_args()?;
    text(&args).or_else(|| args.get("name").and_then(text))
}

/// The names a `::profile(...)` is written with, as written and in order: `::profile(oneline, standalone)` is
/// `oneline` and `standalone`. A name that is not a profile is returned too (the checker reports it); a
/// value that is not text comes back as the text it prints as.
///
/// There is no slot to bind the arguments to, so they arrive as the reader left them: a map of unnamed
/// entries (or the lone value, for one name). An entry written `key: value` is not a bare name; its key
/// is returned, so that it is reported like any other name that is not a profile.
pub fn profile_names(declaration: &Declaration) -> Vec<String> {
    match declaration.bound_args() {
        None => Vec::new(),
        Some(Value::Map(entries)) => entries
            .iter()
            .map(|(key, value)| {
                if key == tomet_semantics_core::POSITIONAL_ENTRY_KEY {
                    scalar_text_or_debug(value)
                } else {
                    key.clone()
                }
            })
            .collect(),
        Some(Value::Seq(items)) => items.iter().map(scalar_text_or_debug).collect(),
        Some(one) => vec![scalar_text_or_debug(&one)],
    }
}

/// The version a `::version(...)` declares, as written: `::version(1.10)` is `"1.10"`, not `"1.1"`.
pub fn version_string(declaration: &Declaration) -> Option<String> {
    let args = declaration.bound_args()?;
    Some(match args.get("version") {
        Some(version) => scalar_text_or_debug(version),
        None => scalar_text_or_debug(&args),
    })
}

/// What a `::meta(...)` states: the map of its fields. `::meta()` states none, and is an empty map
/// rather than nothing. `None` when the declaration is not written as entries
/// ([`check_declarations`](crate::check_declarations) says so).
pub fn meta(declaration: &Declaration) -> Option<Value> {
    declaration.entries().ok().map(Value::Map)
}

/// The macros a `::macro` defines, in the order written: each entry is a name and its template.
///
/// A template is a [`Value`]: a string is a text macro, and `[ ... ]` is a block macro (see
/// [`crate::Arity`]). An entry of any other kind is returned too, because refusing it is
/// `tomet check`'s business and not a reader's; whoever calls the macro finds out it cannot.
/// `::macro.gh("...")` defines one and `::macro(gh: "...", n: "...")` defines several, with the same
/// result (see [`Declaration::entries`]).
pub fn macros(declaration: &Declaration) -> Vec<(String, Value)> {
    entries(declaration)
}

/// The `(name, value)` entries of a declaration, as [`Declaration::entries`] reads them. Empty for
/// `::name()`, and for a declaration that is not written as entries.
pub(crate) fn entries(declaration: &Declaration) -> Vec<(String, Value)> {
    declaration.entries().unwrap_or_default()
}

fn first<B>(doc: &File<B>, kind: DeclKind) -> Option<Declaration> {
    declarations(doc)
        .into_iter()
        .find(|found| found.kind == kind)
        .map(|found| found.declaration)
}

/// The profiles this document promises with its first `::profile(...)`, each once, in the order written. A name
/// that is not a profile is left out here and reported by [`check_declarations`](crate::check_declarations).
pub fn document_profiles<B>(doc: &File<B>) -> Vec<Profile> {
    let Some(declaration) = first(doc, DeclKind::Profile) else {
        return Vec::new();
    };
    let mut profiles = Vec::new();
    for name in profile_names(&declaration) {
        if let Some(profile) = Profile::from_name(&name)
            && !profiles.contains(&profile)
        {
            profiles.push(profile);
        }
    }
    profiles
}

/// The kind this document declares with its first `::kind(...)`.
pub fn document_kind<B>(doc: &File<B>) -> Option<String> {
    kind_name(&first(doc, DeclKind::Kind)?)
}

/// The identity this document declares with its first `::id(...)`.
pub fn document_id<B>(doc: &File<B>) -> Option<String> {
    id_name(&first(doc, DeclKind::Id)?)
}

/// The kind a blueprint document is for; `None` for a document that is not a blueprint.
pub fn document_blueprint<B>(doc: &File<B>) -> Option<String> {
    blueprint_target(&first(doc, DeclKind::Blueprint)?)
}

/// The name of the dictionary a document is; `None` for a document that is not one.
pub fn document_dictionary<B>(doc: &File<B>) -> Option<String> {
    dictionary_name(&first(doc, DeclKind::Dictionary)?)
}

/// The language version this document declares, as written.
pub fn document_version<B>(doc: &File<B>) -> Option<String> {
    version_string(&first(doc, DeclKind::Version)?)
}

/// The fields of the document's first `::meta(...)`: a map, empty for `::meta()`. Only the first is
/// read: `::meta` is written once.
pub fn document_meta<B>(doc: &File<B>) -> Option<Value> {
    meta(&first(doc, DeclKind::Meta)?)
}

/// The `(name, value)` entries of every declaration of `kind` in the document, in the order first
/// written (`::format.table(...)`, `::field.created(...)`, `::blueprint.writ(...)`, or the same
/// grouped under `::format(...)`). An entry written twice is the later one. Empty when the document
/// does not write it.
pub fn document_entries<B>(doc: &File<B>, kind: DeclKind) -> Vec<(String, Value)> {
    let mut merged = Vec::new();
    for found in declarations(doc).iter().filter(|found| found.kind == kind) {
        merge_entries(&mut merged, entries(&found.declaration));
    }
    merged
}

/// Adds `more` to `into`; a name already there takes the new value where it stands.
fn merge_entries(into: &mut Vec<(String, Value)>, more: Vec<(String, Value)>) {
    for (name, value) in more {
        match into.iter().position(|(have, _)| *have == name) {
            Some(at) => into[at].1 = value,
            None => into.push((name, value)),
        }
    }
}

/// Every macro the document defines, in the order written; a name defined twice appears twice,
/// and the later one is the one that stands.
pub fn document_macros<B>(doc: &File<B>) -> Vec<(String, Value)> {
    declarations(doc)
        .iter()
        .filter(|found| found.kind == DeclKind::Macro)
        .flat_map(|found| macros(&found.declaration))
        .collect()
}

/// Every variable the document declares that can be read, by name. A name declared twice is the later
/// declaration. An entry that is not a declaration is left out; `tomet check` says why.
pub fn document_vars<B>(doc: &File<B>) -> Vec<(String, VarDecl)> {
    let mut vars: Vec<(String, VarDecl)> = Vec::new();
    let mut index: BTreeMap<String, usize> = BTreeMap::new();
    for found in declarations(doc)
        .iter()
        .filter(|found| found.kind == DeclKind::Var)
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

    fn parse(src: &str) -> File {
        tomet_ast_parser::parse_file(src).expect("parses")
    }

    #[test]
    fn a_profile_declaration_names_the_subsets_the_document_promises() {
        assert_eq!(
            document_profiles(&parse("::profile(oneline, standalone)\n")),
            [Profile::Oneline, Profile::Standalone]
        );
        // One name is not a list, and the order is the order written.
        assert_eq!(
            document_profiles(&parse("::profile(standalone)\n")),
            [Profile::Standalone]
        );
        assert_eq!(
            document_profiles(&parse("::profile(standalone, oneline)\n")),
            [Profile::Standalone, Profile::Oneline]
        );
        // Each once; a name that is not a profile is not one.
        assert_eq!(
            document_profiles(&parse("::profile(oneline, oneline, pure)\n")),
            [Profile::Oneline]
        );
        assert_eq!(document_profiles(&parse("::kind(note)\n")), []);
        // Only the first declaration counts, and written as an element it is not one.
        assert_eq!(
            document_profiles(&parse("::profile(oneline)\n::profile(standalone)\n")),
            [Profile::Oneline]
        );
        assert_eq!(document_profiles(&parse("@profile(oneline)\n")), []);
    }

    #[test]
    fn the_names_of_a_profile_declaration_come_back_as_written() {
        let names = |src: &str| {
            let doc = parse(src);
            let found = declarations(&doc);
            profile_names(&found[0].declaration)
        };
        assert_eq!(names("::profile(oneline, pure)\n"), ["oneline", "pure"]);
        assert_eq!(names("::profile(7)\n"), ["7"]);
        assert!(names("::profile()\n").is_empty());
    }

    #[test]
    fn a_dictionary_is_named_by_its_declaration() {
        assert_eq!(
            document_dictionary(&parse("::dictionary(tomet){ \"a\": @term[x] }\n")).as_deref(),
            Some("tomet")
        );
        assert_eq!(
            document_dictionary(&parse("::dictionary(name: tomet)\n")).as_deref(),
            Some("tomet")
        );
        assert_eq!(document_dictionary(&parse("::kind(note)\n")), None);
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
        // `::meta(id: ...)` is not an identity any more.
        assert_eq!(document_id(&parse("::meta(id: a)\n")), None);
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
        assert_eq!(document_kind(&parse("::meta(id: x)\n")), None);
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
    fn meta_is_the_fields_of_the_first_meta_declaration() {
        let value = document_meta(&parse("::meta(title: Hello)\n")).expect("meta");
        assert_eq!(
            value,
            Value::Map(vec![("title".into(), Value::String("Hello".into()))])
        );
        assert_eq!(document_meta(&parse("#[ Hello ]\n")), None);
        // `::meta()` states no field: an empty map, not nothing
        assert_eq!(
            document_meta(&parse("::meta()\n")),
            Some(Value::Map(Vec::new()))
        );
        // a value without a name is not a field
        assert_eq!(document_meta(&parse("::meta(x)\n")), None);
    }

    #[test]
    fn macros_are_the_entries_of_every_macros_declaration() {
        let doc =
            parse("::macro(gh: \"https://x/${1}\", n: 3, wiki: \"w/${1}\", blk: [ hello ])\n");
        let read = document_macros(&doc);
        let names: Vec<&str> = read.iter().map(|(name, _)| name.as_str()).collect();
        // A template of any kind is read; refusing a kind is `check`'s.
        assert_eq!(names, ["gh", "n", "wiki", "blk"]);
        assert_eq!(read[0].1, Value::String("https://x/${1}".to_string()));
        assert_eq!(read[1].1, Value::Int(3));
        assert!(matches!(read[3].1, Value::Blocks(_)));
        // no entries, no macros
        assert!(document_macros(&parse("::macro()\n")).is_empty());
    }

    #[test]
    fn a_variable_declared_twice_is_the_later_declaration() {
        let doc = parse("::var(a: { default: 1 })\n::var(a: { default: 2 }, b: {})\n");
        let vars = document_vars(&doc);
        assert_eq!(vars.len(), 2);
        assert_eq!(vars[0].0, "a");
        assert_eq!(vars[0].1.default, Some(Value::Int(2)));
        assert_eq!(vars[1].0, "b");
    }
}
