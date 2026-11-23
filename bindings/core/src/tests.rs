use super::*;
use tomet::ast::PlainValue;

fn process(vault: &Vault, src: &str, options: &ProcessOptions) -> ProcessedFile {
    vault.process(Input::Source(src), options).unwrap()
}

fn processed(src: &str) -> ProcessedFile {
    process(&Vault::empty(), src, &ProcessOptions::default())
}

fn object(entries: Vec<(&str, PlainValue)>) -> PlainValue {
    PlainValue::Object(entries.into_iter().map(|(k, v)| (k.into(), v)).collect())
}

fn vault_of(files: &[&str]) -> Vault {
    Vault::new(&VaultSources {
        files: files.iter().map(|f| f.to_string()).collect(),
        ..Default::default()
    })
    .unwrap()
}

fn codes(file: &ProcessedFile) -> Vec<&str> {
    file.diagnostics.iter().map(|d| d.code.as_str()).collect()
}

#[test]
fn the_title_is_the_meta_title() {
    let src = "::meta(\n  title: \"My Meta Title\"\n)\n\n=[ Document Title ]\n\n==[ Section One ]\n\nContent.\n";
    let options = ProcessOptions {
        advanced: Some(true),
        ..Default::default()
    };
    let res = process(&Vault::empty(), src, &options);
    assert_eq!(res.title.as_deref(), Some("My Meta Title"));
    assert!(!res.is_data_only);
    assert_eq!(res.toc.len(), 1);
    assert_eq!(res.toc[0].level, 2);
    assert_eq!(res.toc[0].text, "Section One");
    assert_eq!(res.toc[0].id, "section-one");
}

#[test]
fn meta_is_the_entries_that_are_data_and_header_is_everything() {
    let res = processed(
        "::meta(title: \"T\", n: 1, tags: (a, b), icon: @doc.icon(\"star\"), mode: auto, t: enum(1))\n\nBody.\n",
    );
    assert_eq!(
        res.meta,
        Some(object(vec![
            ("title", PlainValue::Text("T".into())),
            ("n", PlainValue::Int(1)),
            (
                "tags",
                PlainValue::Array(vec![
                    PlainValue::Text("a".into()),
                    PlainValue::Text("b".into())
                ])
            ),
        ]))
    );
    // What `meta` leaves out is in the header, as the AST.
    let args = res.header[0].data.args.as_ref().expect("::meta args");
    let Some(tomet::ast::Value::Element(icon)) = args.get("icon") else {
        panic!("the icon is an element in the header")
    };
    assert_eq!(icon.name.to_string(), "doc.icon");
}

#[test]
fn meta_is_null_without_a_meta_and_empty_without_data() {
    let none = processed("Body.\n");
    assert_eq!(none.meta, None);
    assert!(none.header.is_empty());
    let only_an_element = processed("::meta(icon: @doc.icon(\"star\"))\n\nBody.\n");
    assert_eq!(only_an_element.meta, Some(object(vec![])));
    assert_eq!(processed("::meta()\n").meta, Some(object(vec![])));
}

#[test]
fn the_title_falls_back_to_the_first_heading() {
    let src = "=[ First Heading Title ]\n\n==[ Section A ]\n\n===[ Sub Section ]\n";
    let options = ProcessOptions {
        advanced: Some(true),
        ..Default::default()
    };
    let res = process(&Vault::empty(), src, &options);
    assert_eq!(res.title.as_deref(), Some("First Heading Title"));
    assert!(!res.is_data_only);
    assert_eq!(res.toc.len(), 2);
    assert_eq!(res.toc[0].text, "Section A");
    assert_eq!(res.toc[0].level, 2);
    assert_eq!(res.toc[1].text, "Sub Section");
    assert_eq!(res.toc[1].level, 3);
}

#[test]
fn a_document_of_data_only_has_nothing_to_show() {
    let res = processed("::version(1.0)\n::meta(\n  title: \"Just Config\"\n)\n");
    assert_eq!(res.title.as_deref(), Some("Just Config"));
    assert!(res.is_data_only);
    assert!(res.toc.is_empty());
}

#[test]
fn a_note_is_resolved_in_the_vault_it_is_read_in() {
    let vault = vault_of(&["30-39 Knowledge/Linux.tmt", "10-19 Journal/daily/memo.tmt"]);
    let options = ProcessOptions {
        current_path: Some("10-19 Journal/daily/memo.tmt".into()),
        ..Default::default()
    };
    let res = process(
        &vault,
        "- @link(\"ref:Linux\")[Go to Linux]\n- @link(\"ref:NonExistentNote\")[Missing]\n",
        &options,
    );
    assert!(
        res.html
            .contains("href=\"/docs/30-39 Knowledge/Linux\">Go to Linux</a>")
    );
    assert!(res.html.contains("class=\"tm-ref tm-ref-unresolved\" aria-disabled=\"true\" data-ref=\"NonExistentNote\">Missing</a>"));
    assert_eq!(codes(&res), ["unresolved-link"]);
    let d = &res.diagnostics[0];
    assert_eq!(d.severity, Severity::Error);
    assert_eq!(d.start, Some(Position { line: 2, column: 3 }));
    assert!(d.message.contains("ref:NonExistentNote"), "{}", d.message);
}

#[test]
fn two_vaults_never_see_each_others_files() {
    let a = vault_of(&["a/Note.tmt"]);
    let b = vault_of(&["b/Other.tmt"]);
    let src = "@link(\"ref:Note\")[n]\n";
    let options = ProcessOptions::default();
    assert!(process(&a, src, &options).diagnostics.is_empty());
    assert_eq!(codes(&process(&b, src, &options)), ["unresolved-link"]);
    assert!(process(&a, src, &options).diagnostics.is_empty());
}

#[test]
fn what_the_validator_finds_comes_with_a_place_and_a_message() {
    let res = processed("Fine.\n\n@frobnicate[ hi ]\n\n@draft(例)\n");
    assert_eq!(codes(&res), ["unknown-element", "draft"]);
    assert_eq!(res.diagnostics[0].severity, Severity::Error);
    assert_eq!(
        res.diagnostics[0].start,
        Some(Position { line: 3, column: 1 })
    );
    assert_eq!(res.diagnostics[1].severity, Severity::Warning);
    assert!(res.diagnostics[1].message.contains("unfinished"));
}

#[test]
fn a_dangling_anchor_and_an_expression_without_a_value_are_diagnostics() {
    let res = processed("@link(id:nowhere)[x]\n\nresult {$ vault::docs } here\n");
    assert!(
        codes(&res).contains(&"dangling-anchor"),
        "{:?}",
        codes(&res)
    );
    assert!(
        codes(&res).contains(&"unresolved-expression"),
        "{:?}",
        codes(&res)
    );
    let expr = res
        .diagnostics
        .iter()
        .find(|d| d.code == "unresolved-expression")
        .unwrap();
    assert_eq!(expr.severity, Severity::Warning);
    assert_eq!(expr.start.map(|p| p.line), Some(3));
}

#[test]
fn a_document_given_as_a_tree_has_no_line_and_column() {
    let doc = crate::vault::parse_plain("@frobnicate[ hi ]\n").unwrap();
    let res = Vault::empty()
        .process(Input::File(doc), &ProcessOptions::default())
        .unwrap();
    assert_eq!(codes(&res), ["unknown-element"]);
    assert_eq!(res.diagnostics[0].start, None);
    assert_eq!(res.diagnostics[0].end, None);
}

#[test]
fn a_part_of_a_vault_that_does_not_read_is_an_error_naming_it() {
    let err = Vault::new(&VaultSources {
        dictionaries: [("d.tmt".to_string(), "@@@(".to_string())].into(),
        ..Default::default()
    })
    .err()
    .expect("a dictionary that does not parse");
    assert!(err.contains("d.tmt"), "{err}");
    let err = Vault::new(&VaultSources {
        blueprints: vec!["@foo(\n=[ x".to_string()],
        ..Default::default()
    })
    .err()
    .expect("a blueprint that does not parse");
    assert!(err.contains("blueprints[0]"), "{err}");
}

#[test]
fn a_document_that_does_not_parse_is_an_error_with_its_place() {
    let err = Vault::empty()
        .process(Input::Source("@foo(\n=[ x"), &ProcessOptions::default())
        .err()
        .unwrap();
    assert!(err.starts_with("2:5: "), "{err}");
}

#[test]
fn the_registry_names_the_files_a_config_registers() {
    let registry = registry(
        "::vocabulary(writ: @file(schema/writ.vocabulary.tmt))\n::dictionary(tomet: @file(dict/tomet.dict.tmt))\n",
    )
    .unwrap();
    assert_eq!(registry.vocabularies, ["schema/writ.vocabulary.tmt"]);
    assert_eq!(registry.dictionaries, ["dict/tomet.dict.tmt"]);
}

#[test]
fn site_links_follow_the_vault() {
    let vault = Vault::new(&VaultSources {
        files: vec![
            "spec/a.tmt".into(),
            "spec/b.tmt".into(),
            "spec/stub.tmt".into(),
        ],
        documents: Some(vec!["spec/a.tmt".into(), "spec/b.tmt".into()]),
        base_path: Some("tmtroot".into()),
        source_url: Some(SourceUrls {
            file: "https://git.example/blob/rev/{path}".into(),
            dir: "https://git.example/tree/rev/{path}".into(),
        }),
        ..Default::default()
    })
    .unwrap();
    let res = process(
        &vault,
        "@link(file:tmtroot/spec/b.tmt)[b]\n\n@link(file:crates/x/src/lib.rs)[x]\n\n@link(ref:stub)[stub]\n\n@link(file:tmtroot/spec/gone.tmt)[gone]\n",
        &ProcessOptions {
            current_path: Some("spec/a.tmt".into()),
            ..Default::default()
        },
    );
    assert!(res.html.contains("href=\"/docs/spec/b\""), "{}", res.html);
    assert!(
        res.html
            .contains("href=\"https://git.example/blob/rev/crates/x/src/lib.rs\""),
        "{}",
        res.html
    );
    assert_eq!(codes(&res), ["unresolved-link", "unresolved-link"]);
    assert!(res.diagnostics[0].message.contains("not published"));
    assert!(res.diagnostics[1].message.contains("not in the vault"));
}
