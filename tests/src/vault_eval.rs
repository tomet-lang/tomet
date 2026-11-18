use std::path::Path;

use tomet_ast::{Block, File};
use tomet_config::PrinterConfig;
use tomet_expr::{MemoryResolver, NoResolver, Resolver};
use tomet_transform::Unresolved;
use tomet_vault::Vault;

const ACME: &str = "::kind(vocabulary)\n::vocabulary(acme, version: \"1.0.0\")\n";
const PROMISE: &str = "::profile(standalone)\n";

fn prepare_doc(
    vault: &Vault,
    path: &Path,
    doc: &mut File,
    resolver: &dyn Resolver,
) -> Vec<Unresolved> {
    let scope = vault.scope_for(doc, path, resolver);
    let unresolved = tomet_eval::resolve_expressions(doc, &scope.context());
    vault.fill_terms(doc);
    unresolved
}

fn prepared(vault: &Vault, path: &str, src: &str) -> (File, Vec<Unresolved>) {
    let (mut doc, _) = vault.parse(src).expect("parses");
    let unresolved = prepare_doc(vault, Path::new(path), &mut doc, &NoResolver);
    (doc, unresolved)
}

fn text_of(doc: &File) -> String {
    doc.body
        .iter()
        .filter_map(|block| match block {
            Block::Paragraph(p) => Some(
                p.content
                    .iter()
                    .filter_map(|i| match i {
                        tomet_ast::Inline::Text(t) => Some(t.value.as_str()),
                        _ => None,
                    })
                    .collect::<String>(),
            ),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn vault_with_dictionary() -> Vault {
    let mut dictionaries = tomet_semantics::LoadedDictionaries::default();
    dictionaries.add(
        "tomet.tmt",
        &tomet_ast_parser::parse_file(
            "::dictionary(tomet)\n\n;@term(\"vault.config.tmt\")#(vault-config)\n| The file that marks a vault.\n",
        )
        .expect("parses"),
    );
    Vault::from_sources(PrinterConfig::default(), ".", &[]).with_dictionaries(dictionaries)
}

fn vault_with_a_macro() -> Vault {
    let config = tomet_config::load_config_from_str(
        "::macro(gh: \"https://gh/$(1)\")\n::field(n: (type: uuid))\n",
    )
    .unwrap();
    Vault::from_sources(config, "/v", &[("acme.tmt", ACME)])
}

#[test]
fn a_resolved_term_mention_is_given_the_terms_text() {
    let vault = vault_with_dictionary();
    let term_texts = |src: &str| -> Vec<Option<String>> {
        let (doc, _) = prepared(&vault, "a.tmt", src);
        let mut found = Vec::new();
        tomet_tree::for_each_element(&doc, |el| {
            if tomet_semantics::classify_std_lenient(el.as_view())
                == tomet_semantics::ElementKind::Term
            {
                found.push(el.content.as_ref().and_then(|blocks| match blocks.first() {
                    Some(Block::Paragraph(p)) => match p.content.first() {
                        Some(tomet_ast::Inline::Text(t)) => Some(t.value.clone()),
                        _ => None,
                    },
                    _ => None,
                }));
            }
        });
        found
    };
    assert_eq!(
        term_texts("a @term(tomet#vault-config) b\n"),
        [Some("vault.config.tmt".to_string())]
    );
    assert_eq!(
        term_texts("a @term(tomet#vault-config)[ the config ] b\n"),
        [Some("the config".to_string())]
    );
    assert_eq!(term_texts("a @term(tomet#gone) b\n"), [None]);
}

#[test]
fn an_include_rewrites_the_relative_links_of_what_it_brings() {
    let resolver = MemoryResolver::new()
        .with_source(
            "docs/guide.tmt",
            "See @link(./img/a.png) and @link(../shared/s.tmt#top) and @link(tmt:abs.tmt).\n",
        )
        .unwrap();
    let vault = Vault::from_sources(PrinterConfig::default(), "/v", &[]);
    let targets = |host: &str| {
        let (mut doc, _) = vault
            .parse("$include(tmt(\"docs/guide.tmt\"))\n")
            .expect("parses");
        prepare_doc(&vault, Path::new(host), &mut doc, &resolver);
        let mut found = Vec::new();
        tomet_tree::for_each_element_mut(&mut doc, |el| {
            if let Some((_, target)) = tomet_semantics::link_target_of(el.as_view()) {
                found.push(target);
            }
        });
        found
    };
    assert_eq!(
        targets("/v/here.tmt"),
        ["./docs/img/a.png", "./shared/s.tmt#top", "tmt:abs.tmt"]
    );
    assert_eq!(
        targets("/v/docs/sub/host.tmt"),
        ["../img/a.png", "../../shared/s.tmt#top", "tmt:abs.tmt"]
    );
    assert_eq!(
        targets("/v/docs/other.tmt"),
        ["./img/a.png", "../shared/s.tmt#top", "tmt:abs.tmt"]
    );
}

#[test]
fn prepare_measures_self_path_from_the_root() {
    let vault = Vault::from_sources(PrinterConfig::default(), "/v", &[]);
    let (doc, unresolved) = prepared(
        &vault,
        "/v/notes/a.tmt",
        "at $(self::path) ($(self::filename))\n",
    );
    assert!(unresolved.is_empty(), "{unresolved:?}");
    assert_eq!(text_of(&doc), "at notes/a.tmt (a.tmt)");
}

#[test]
fn a_document_reads_its_own_meta_typed_by_the_settings() {
    let config = tomet_config::load_config_from_str("::field(tags: (type: list))\n").unwrap();
    let vault = Vault::from_sources(config, "/v", &[]);
    let (doc, unresolved) = prepared(
        &vault,
        "/v/a.tmt",
        "::meta(title: A)\n\nrust? {$ self::meta.tags.contains(\"rust\") }\n",
    );
    assert!(unresolved.is_empty(), "{unresolved:?}");
    assert_eq!(text_of(&doc), "rust? false");
}

#[test]
fn macros_from_the_vault_and_the_document_are_both_in_scope() {
    let config =
        tomet_config::load_config_from_str("::macro(gh: \"https://gh/$(1)\", site: \"vault\")\n")
            .unwrap();
    let vault = Vault::from_sources(config, "/v", &[]);
    let (doc, unresolved) = prepared(
        &vault,
        "/v/a.tmt",
        "::macro(site: \"doc\")\n\n$macro.gh(1) $macro.site()\n",
    );
    assert!(unresolved.is_empty(), "{unresolved:?}");
    assert_eq!(text_of(&doc), "https://gh/1 doc");
}

#[test]
fn references_are_answered_from_the_document_as_written() {
    let vault = Vault::from_sources(PrinterConfig::default(), "/v", &[]);
    let (doc, unresolved) = prepared(
        &vault,
        "/v/a.tmt",
        "@card(n: $add(1, 2))#(c1)\n\nn is $(#c1.n)\n",
    );
    assert_eq!(unresolved.len(), 1, "{unresolved:?}");
    assert_eq!(text_of(&doc), "n is $(#c1.n)");
}

#[test]
fn a_standalone_document_does_not_get_the_vaults_macros() {
    let vault = vault_with_a_macro();
    let (_, with) = prepared(&vault, "/v/a.tmt", "$macro.gh(1)\n");
    assert!(with.is_empty());

    let (doc, alone) = prepared(&vault, "/v/a.tmt", &format!("{PROMISE}\n$macro.gh(1)\n"));
    assert_eq!(alone.len(), 1, "{alone:?}");
    assert!(text_of(&doc).contains("$macro.gh(1)"));
    let (doc, own) = prepared(
        &vault,
        "/v/a.tmt",
        &format!("{PROMISE}::macro(gh: \"mine/$(1)\")\n\n$macro.gh(1)\n"),
    );
    assert!(own.is_empty(), "{own:?}");
    assert_eq!(text_of(&doc), "mine/1");
}

#[test]
fn a_standalone_document_does_not_find_other_files() {
    let resolver = MemoryResolver::new()
        .with_source("docs/b.tmt", "Body of B.\n")
        .unwrap();
    let vault = Vault::from_sources(PrinterConfig::default(), "/v", &[]);
    let run = |src: &str| {
        let (mut doc, _) = vault.parse(src).unwrap();
        let unresolved = prepare_doc(&vault, Path::new("/v/a.tmt"), &mut doc, &resolver);
        (text_of(&doc), unresolved.len())
    };
    let (text, unresolved) = run("$include(tmt(\"docs/b.tmt\"))\n");
    assert_eq!((text.as_str(), unresolved), ("Body of B.", 0));
    let (_, unresolved) = run(&format!("{PROMISE}\n$include(tmt(\"docs/b.tmt\"))\n"));
    assert_eq!(unresolved, 1);
}

#[test]
fn a_standalone_document_does_not_get_the_vaults_dictionaries_but_may_be_one() {
    let vault = vault_with_dictionary();
    let mention = "a @term(tomet#vault-config) b\n";
    let (_, with) = prepared(&vault, "/v/a.tmt", mention);
    assert!(with.is_empty());
}
