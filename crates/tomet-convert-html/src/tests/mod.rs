//! The renderer's unit tests, by topic.

mod document;
mod elements;
mod headings;
mod links;
mod lists;
mod markup;
mod raw;
mod spans;

use super::*;
use tomet_ast_parser::parse_file;

/// What a document read with no vault has in scope: the vocabularies tomet carries.
fn carried(doc: &File) -> Bindings {
    tomet_semantics::LoadedVocabularies::default().bindings_for(doc)
}

/// A vault vocabulary declaring one element with a positional argument, for the alias tests.
const NODE_GRAPH: &str = "::kind(vocabulary)
::vocabulary(node-graph, version: \"1.0.0\")
::element(
  node: ( args: ( label: (type: string, positional: true) ) )
)
";

/// The names `doc` has in scope in a vault holding [`NODE_GRAPH`].
fn with_node_graph(doc: &File) -> Bindings {
    let mut loaded = tomet_semantics::LoadedVocabularies::default();
    loaded.add("node-graph", &parse_file(NODE_GRAPH).unwrap());
    assert!(loaded.errors.is_empty(), "{:?}", loaded.errors);
    loaded.bindings_for(doc)
}
