//! The nexus rule, held against the shared corpus.
//!
//! A nexus -- `@name`, a `::name` declaration or connect -- needs a primary group: `(args)`,
//! `[content]`, or a `|` run. `{value}`, `#(id)` and a `::connect` only add to
//! one and cannot stand alone. The grammar lives in `tomet-cst-parser` and
//! `tomet-ast-parser`; this target pins it from the outside, with the corpus:
//!
//! - `syntax/nexus-invalid-*.tmt` are the forms that are refused, one to a
//!   file because the parser stops at the first error. `KNOWN_UNPARSEABLE`
//!   lists them, and [`REJECTED`] says where each is refused and why.
//! - `syntax/nexus.tmt` and `syntax/nexus-declarations*.tmt` are the forms
//!   that stay valid, among them the twelve declarations that used to take
//!   `{...}` alone and now take `(...)`.
//!
//! What a bare `@name` is depends on where it stands, and that is the part
//! most easily lost: in running text it is prose (`me@example.com`), as a
//! block it is an error (`@name()` is the empty element).

use std::path::Path;

use tomet_ast_parser::parse_file;
use tomet_tests::{KNOWN_UNPARSEABLE, corpus, read_fixture};

/// The prefix of every fixture that is a form the parser refuses.
const INVALID: &str = "syntax/nexus-invalid-";

/// The declarations that took `{...}` alone, and take `(...)` now.
const PAREN_DECLARATIONS: [&str; 12] = [
    "meta",
    "macro",
    "format",
    "field",
    "workspace",
    "export",
    "blueprint",
    "vocabulary",
    "dictionary",
    "migration",
    "var",
    "element",
];

/// `(fixture, line:col of the refusal, what the message says)`.
const REJECTED: &[(&str, &str, &str)] = &[
    (
        "syntax/nexus-invalid-bare-block.tmt",
        "3:1",
        "write `@memo()` for an empty one",
    ),
    (
        "syntax/nexus-invalid-bare-hr.tmt",
        "3:1",
        "write `@hr()` for an empty one",
    ),
    (
        "syntax/nexus-invalid-combine-data.tmt",
        "3:2",
        "`{...}` cannot stand without one",
    ),
    (
        "syntax/nexus-invalid-connect-args.tmt",
        "3:1",
        "`::connect` cannot stand without one",
    ),
    (
        "syntax/nexus-invalid-connect-data.tmt",
        "3:1",
        "`::connect` cannot stand without one",
    ),
    (
        "syntax/nexus-invalid-connect-name-data.tmt",
        "3:8",
        "`::rule` needs a primary group",
    ),
    (
        "syntax/nexus-invalid-connect-named.tmt",
        "3:1",
        "`::connect` cannot stand without one",
    ),
    (
        "syntax/nexus-invalid-data-and-id.tmt",
        "3:1",
        "`{...}`, `#(id)` cannot stand without one",
    ),
    (
        "syntax/nexus-invalid-data-empty.tmt",
        "3:1",
        "`{...}` cannot stand without one",
    ),
    (
        "syntax/nexus-invalid-data-only.tmt",
        "3:1",
        "`{...}` cannot stand without one",
    ),
    (
        "syntax/nexus-invalid-declaration-bare.tmt",
        "4:1",
        "write `::meta()` for an empty one",
    ),
    (
        "syntax/nexus-invalid-declaration-data.tmt",
        "4:1",
        "`::meta` needs a primary group",
    ),
    (
        "syntax/nexus-invalid-declaration-id.tmt",
        "4:1",
        "`#(id)` cannot stand without one",
    ),
    (
        "syntax/nexus-invalid-heading-connect.tmt",
        "3:",
        "`::rule` needs a primary group",
    ),
    (
        "syntax/nexus-invalid-id-only.tmt",
        "3:1",
        "`#(id)` cannot stand without one",
    ),
    (
        "syntax/nexus-invalid-inline-data.tmt",
        "3:",
        "`{...}` cannot stand without one",
    ),
    (
        "syntax/nexus-invalid-inline-id.tmt",
        "3:",
        "`#(id)` cannot stand without one",
    ),
    (
        "syntax/nexus-invalid-list-item-connect.tmt",
        "3:",
        "`::rule` needs a primary group",
    ),
];

fn element_names(doc: &tomet_ast::File) -> Vec<String> {
    let mut names = Vec::new();
    tomet_tree::for_each_element(doc, |el| names.push(el.name.name.clone()));
    names
}

#[test]
fn every_invalid_form_is_refused_where_and_why_it_says() {
    for (rel, at, reason) in REJECTED {
        let src = read_fixture(Path::new(rel));
        let err = match parse_file(&src) {
            Ok(_) => panic!("{rel} should be refused, and parses"),
            Err(e) => e.to_string(),
        };
        assert!(
            err.starts_with(at),
            "{rel}: refused at the wrong place -- wanted {at}, got `{err}`"
        );
        assert!(
            err.contains(reason),
            "{rel}: refused for the wrong reason -- wanted `{reason}`, got `{err}`"
        );
    }
}

/// `KNOWN_UNPARSEABLE` and [`REJECTED`] describe the same files: a fixture
/// listed in one and missing from the other would either be refused for no
/// stated reason, or have a stated reason that nothing checks.
#[test]
fn the_invalid_fixtures_are_listed_and_explained_once() {
    let mut listed: Vec<&str> = KNOWN_UNPARSEABLE
        .iter()
        .copied()
        .filter(|name| name.starts_with(INVALID))
        .collect();
    let mut explained: Vec<&str> = REJECTED.iter().map(|(name, _, _)| *name).collect();
    listed.sort_unstable();
    explained.sort_unstable();
    assert_eq!(
        listed, explained,
        "KNOWN_UNPARSEABLE (src/lib.rs) and REJECTED (src/nexus.rs) disagree"
    );

    let on_disk: Vec<String> = corpus()
        .iter()
        .map(|rel| rel.to_string_lossy().replace('\\', "/"))
        .filter(|rel| rel.starts_with(INVALID))
        .collect();
    let mut on_disk: Vec<&str> = on_disk.iter().map(String::as_str).collect();
    on_disk.sort_unstable();
    assert_eq!(
        on_disk, explained,
        "a `{INVALID}*` fixture on disk is not in REJECTED, or the reverse"
    );
}

/// In running text a bare `@name` is just text. The line of prose that
/// mentions a person or holds an address must never become an error.
#[test]
fn an_at_name_with_no_group_is_prose_inline() {
    for src in [
        "連絡は me@example.com まで\n",
        "ping @someone about it\n",
        "a @hr b\n",
        "同じ行に @mention と @other\n",
    ] {
        let doc = parse_file(src).unwrap_or_else(|e| panic!("`{src}` should be prose: {e}"));
        assert!(
            element_names(&doc).is_empty(),
            "`{src}` read as an element: {:?}",
            element_names(&doc)
        );
    }
}

/// ...but the moment a group that is not primary follows it, it is a nexus,
/// and an error rather than a fall back to prose.
#[test]
fn an_inline_at_name_with_a_secondary_group_is_an_error() {
    for (src, wanted) in [
        ("text @name{} more\n", "`{...}` cannot stand without one"),
        ("text @name#(id) more\n", "`#(id)` cannot stand without one"),
        (
            "text @name::rule(a) more\n",
            "`::connect` cannot stand without one",
        ),
        (
            "text @name::{ a: 1 } more\n",
            "`::connect` cannot stand without one",
        ),
        (
            "text @name::(a) more\n",
            "`::connect` cannot stand without one",
        ),
    ] {
        let err = parse_file(src)
            .err()
            .unwrap_or_else(|| panic!("`{src}` should be refused"))
            .to_string();
        assert!(err.contains(wanted), "`{src}`: {err}");
    }
}

/// The two places a group stands without a nexus of its own, and the table
/// that has no `()` or `[]` but a `|` run.
#[test]
fn the_exceptions_still_parse() {
    for src in [
        // A nameless connect puts data into the host it follows.
        "- 項目 ::{ k: v }\n",
        "- 項目 ::(k: v)\n",
        "= 見出し ::{ k: v }\n",
        "= 見出し ::(k: v)\n",
        "@el(a)::{ b: 1 }\n",
        "@el(a)::(b: 1)\n",
        // A `|` run is the primary group of the line before it.
        "@table\n|[ a ][ b ]\n|[ 1 ][ 2 ]\n",
        "@quote(A)\n| 一行目\n| 二行目\n",
        "@quote| 本文\n",
        // List items, headings and `-@name(...)` are not bare nexus either.
        "- #(id)\n",
        "- {a: 1}\n",
        "= H {a: 1}\n",
        "==#(id)\n",
        "-@name(a)\n",
        "-@name[ x ]\n",
        // The empty element is written with its group.
        "@hr()\n",
    ] {
        parse_file(src).unwrap_or_else(|e| panic!("`{src}` should parse: {e}"));
    }
    // The pipe run belongs to the line directly before it; a blank line in
    // between leaves `@table` without a primary group.
    assert!(parse_file("@table\n\n|[ a ][ b ]\n").is_err());
}

#[test]
fn the_twelve_declarations_read_their_parenthesised_arguments() {
    let src = read_fixture(Path::new("syntax/nexus-declarations.tmt"));
    let doc = parse_file(&src).expect("the declarations fixture parses");
    for name in PAREN_DECLARATIONS {
        let decl = doc
            .header
            .iter()
            .find(|d| d.name.name == name)
            .unwrap_or_else(|| panic!("no `::{name}` in the fixture"));
        assert!(
            decl.data.args.is_some(),
            "`::{name}(...)` should carry its entries as `(args)`"
        );
        assert!(
            decl.data.value.is_none(),
            "`::{name}(...)` has no `{{...}}` body"
        );
    }
    // The header of `::blueprint(kind){...}` keeps its body.
    let decl = doc
        .header
        .iter()
        .find(|d| d.name.name == "blueprint" && d.data.value.is_some())
        .expect("no `::blueprint(...){...}` in the fixture");
    assert!(decl.data.args.is_some(), "`::blueprint(...)` names itself");
}

#[test]
fn a_declaration_head_is_a_path_that_is_kept_whole() {
    let src = read_fixture(Path::new("syntax/declaration-paths.tmt"));
    let doc = parse_file(&src).expect("the declaration-paths fixture parses");
    let heads: Vec<String> = doc.header.iter().map(|d| d.name.to_string()).collect();
    for want in [
        "macro.copyright",
        "field.url.wiki",
        "field.author",
        "element.card",
    ] {
        assert!(heads.iter().any(|head| head == want), "{want} in {heads:?}");
    }
    let src = read_fixture(Path::new("syntax/declaration-paths-positional.tmt"));
    let doc = parse_file(&src).expect("the positional fixture parses");
    let heads: Vec<String> = doc.header.iter().map(|d| d.name.to_string()).collect();
    for want in ["workspace.ignore", "blueprint.daily", "dictionary.tomet"] {
        assert!(heads.iter().any(|head| head == want), "{want} in {heads:?}");
    }
    let src = read_fixture(Path::new("syntax/declaration-paths.tmt"));
    let doc = parse_file(&src).expect("the declaration-paths fixture parses");
    // The prose after the head is kept with the declaration.
    let author = doc
        .header
        .iter()
        .find(|d| d.name.to_string() == "field.author")
        .expect("::field.author");
    assert!(author.content.is_some());
}

#[test]
fn an_empty_declaration_keeps_its_parentheses() {
    let src = read_fixture(Path::new("syntax/nexus-declarations-empty.tmt"));
    parse_file(&src).expect("`::meta()`, `::macro()` and `::element[ ... ]` parse");
}

#[test]
fn the_old_spelling_of_each_declaration_is_refused() {
    for name in PAREN_DECLARATIONS {
        for src in [
            format!("::{name}{{ a: 1 }}\n"),
            format!("::{name}{{}}\n"),
            format!("::{name}\n"),
        ] {
            let err = parse_file(&src)
                .err()
                .unwrap_or_else(|| panic!("{src:?} should be refused"))
                .to_string();
            assert!(
                err.contains(&format!("`::{name}` needs a primary group")),
                "{src:?}: {err}"
            );
        }
    }
}
