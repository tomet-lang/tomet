//! Every exporter keys an element on what its name resolves to in the document's vault, never on
//! its spelling: `@ng.node` under `::use(node-graph, as: ng)`, `@node-graph.node` under
//! `::use(node-graph)`, and a bare `@node` under `::kind(node-graph)` are one element, and each
//! format renders the three identically.
//!
//! The bare case resolves only with the vocabulary in hand (an alias is read from the document
//! alone), so the three agree only when the vault's bindings reach the exporter.

use tomet_config::PrinterConfig;
use tomet_vault::Vault;

const NODE_GRAPH: &str = "::kind(vocabulary)
::vocabulary(node-graph, version: \"1.0.0\")
::element(
  node: ( args: ( label: (type: string, positional: true) ) )
)
";

/// One document, written three ways.
const WRITTEN: [(&str, &str); 3] = [
    ("::use(node-graph, as: ng)", "ng."),
    ("::use(node-graph)", "node-graph."),
    ("::kind(node-graph)", ""),
];

fn vault() -> Vault {
    let vault = Vault::from_sources(
        PrinterConfig::default(),
        ".",
        &[("node-graph.vocabulary.tmt", NODE_GRAPH)],
    );
    assert!(
        vault.vocabulary_errors.is_empty(),
        "{:?}",
        vault.vocabulary_errors
    );
    vault
}

/// What `export` makes of the document, for each way it is written.
fn exported(
    export: impl Fn(&tomet_ast::File, &tomet_semantics::Bindings) -> String,
) -> Vec<String> {
    let vault = vault();
    WRITTEN
        .iter()
        .map(|(header, prefix)| {
            let src = format!(
                "{header}\n\n@{prefix}node(a)[A block node.]\n\nText with @{prefix}node(b)[an inline one].\n"
            );
            let (doc, bindings) = vault.parse(&src).expect("parses");
            export(&doc, &bindings)
        })
        .collect()
}

/// The three spellings exported alike, and `marker` -- how the format names the element -- in it.
fn assert_one_element(format: &str, outputs: &[String], marker: &str) {
    assert!(
        outputs.iter().all(|out| *out == outputs[0]),
        "{format} differs by spelling:\n{}",
        outputs.join("\n----\n")
    );
    assert!(
        outputs[0].contains(marker),
        "{format} lacks {marker:?}:\n{}",
        outputs[0]
    );
}

#[test]
fn html_renders_an_aliased_name_as_its_real_namespace() {
    let outputs = exported(tomet_html::render_body);
    assert_one_element("html", &outputs, "class=\"tm-element tm-node-graph.node\"");
}

#[test]
fn markdown_renders_an_aliased_name_as_its_real_namespace() {
    let outputs = exported(tomet_markdown::to_markdown);
    assert_one_element("markdown", &outputs, "data-tm-kind=\"node-graph.node\"");
}

#[test]
fn typst_renders_an_aliased_name_as_its_real_namespace() {
    let outputs = exported(tomet_typst::to_typst);
    assert_one_element("typst", &outputs, "// tomet:node-graph.node");
}

#[test]
fn pandoc_renders_an_aliased_name_as_its_real_namespace() {
    let outputs = exported(|doc, bindings| {
        serde_json::to_string(&tomet_pandoc::to_pandoc(doc, bindings)).expect("serializes")
    });
    assert_one_element("pandoc", &outputs, "\"tomet-node-graph.node\"");
}
