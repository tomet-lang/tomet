use tomet_deps::{
    Change, Dependency, DependencyGraph, DependencyKind, DocumentNode, compute_affected_paths,
    extract_document_node, fnv1a_hash,
};
use tomet_parser::parse_document;

fn make_node(
    path: &str,
    content: &str,
    exports_val: &str,
    outgoing_links: Vec<(&str, DependencyKind)>,
) -> DocumentNode {
    DocumentNode {
        path: path.to_string(),
        content_hash: fnv1a_hash(content.as_bytes()),
        exports_hash: fnv1a_hash(exports_val.as_bytes()),
        outgoing: outgoing_links
            .into_iter()
            .map(|(target, kind)| Dependency {
                target: target.to_string(),
                kind,
                raw: target.to_string(),
            })
            .collect(),
    }
}

#[test]
fn test_text_only_modification_invalidates_only_modified_document() {
    let mut graph = DependencyGraph::new();

    let node_a = make_node(
        "doc_a.tmt",
        "Hello World",
        "Doc A",
        vec![("doc_b.tmt", DependencyKind::Link)],
    );
    let node_b = make_node("doc_b.tmt", "Target Doc", "Doc B", vec![]);

    graph.insert(node_a);
    graph.insert(node_b);

    // Modify only text in doc_a
    let changes = vec![Change::ContentOnly("doc_a.tmt".to_string())];
    let report = graph.affected_by_changes(&changes);

    assert!(report.modified.contains("doc_a.tmt"));
    assert!(report.rebuild_required.contains("doc_a.tmt"));
    assert_eq!(report.rebuild_required.len(), 1);
    assert!(report.backlinks_changed.is_empty());
    assert!(report.exports_changed.is_empty());
}

#[test]
fn test_link_addition_invalidates_source_and_target_backlinks() {
    let mut graph = DependencyGraph::new();

    let node_a = make_node("doc_a.tmt", "Hello", "Doc A", vec![]);
    let node_b = make_node("doc_b.tmt", "Target", "Doc B", vec![]);

    graph.insert(node_a);
    graph.insert(node_b);

    // doc_a adds link to doc_b
    let node_a_v2 = make_node(
        "doc_a.tmt",
        "Hello [[doc_b]]",
        "Doc A",
        vec![("doc_b.tmt", DependencyKind::Link)],
    );
    let changes = vec![Change::Modified(node_a_v2.clone())];
    let report = graph.affected_by_changes(&changes);

    assert!(report.modified.contains("doc_a.tmt"));
    assert!(report.rebuild_required.contains("doc_a.tmt"));
    assert!(report.rebuild_required.contains("doc_b.tmt"));
    assert!(report.backlinks_changed.contains("doc_b.tmt"));

    // Apply update to graph and verify backlinks query
    graph.insert(node_a_v2);
    let backlinks_b = graph.backlinks_to("doc_b.tmt");
    assert_eq!(backlinks_b.len(), 1);
    assert_eq!(backlinks_b[0].source, "doc_a.tmt");
}

#[test]
fn test_link_removal_invalidates_source_and_target_backlinks() {
    let mut graph = DependencyGraph::new();

    let node_a = make_node(
        "doc_a.tmt",
        "Hello [[doc_b]]",
        "Doc A",
        vec![("doc_b.tmt", DependencyKind::Link)],
    );
    let node_b = make_node("doc_b.tmt", "Target", "Doc B", vec![]);

    graph.insert(node_a);
    graph.insert(node_b);

    // doc_a removes link to doc_b
    let node_a_v2 = make_node("doc_a.tmt", "Hello without link", "Doc A", vec![]);
    let changes = vec![Change::Modified(node_a_v2.clone())];
    let report = graph.affected_by_changes(&changes);

    assert!(report.rebuild_required.contains("doc_a.tmt"));
    assert!(report.rebuild_required.contains("doc_b.tmt"));
    assert!(report.backlinks_changed.contains("doc_b.tmt"));

    graph.insert(node_a_v2);
    assert!(graph.backlinks_to("doc_b.tmt").is_empty());
}

#[test]
fn test_exports_change_propagates_to_all_referring_documents() {
    let mut graph = DependencyGraph::new();

    let node_a = make_node(
        "doc_a.tmt",
        "Links to B",
        "Doc A",
        vec![("doc_b.tmt", DependencyKind::Link)],
    );
    let node_b = make_node("doc_b.tmt", "Doc B content", "Doc B Old Title", vec![]);

    graph.insert(node_a);
    graph.insert(node_b);

    // doc_b changes its exported title / metadata
    let node_b_v2 = make_node("doc_b.tmt", "Doc B content", "Doc B New Title", vec![]);
    let changes = vec![Change::Modified(node_b_v2)];
    let report = graph.affected_by_changes(&changes);

    assert!(report.exports_changed.contains("doc_b.tmt"));
    assert!(report.rebuild_required.contains("doc_b.tmt"));
    assert!(
        report.rebuild_required.contains("doc_a.tmt"),
        "doc_a must rebuild because its link target updated exports"
    );
}

#[test]
fn test_transclusion_embed_propagates_content_change() {
    let mut graph = DependencyGraph::new();

    let node_a = make_node(
        "doc_a.tmt",
        "Embeds B",
        "Doc A",
        vec![("doc_b.tmt", DependencyKind::Embed)],
    );
    let node_b = make_node("doc_b.tmt", "Snippet v1", "Doc B", vec![]);

    graph.insert(node_a);
    graph.insert(node_b);

    // doc_b changes body text
    let changes = vec![Change::ContentOnly("doc_b.tmt".to_string())];
    let report = graph.affected_by_changes(&changes);

    assert!(report.modified.contains("doc_b.tmt"));
    assert!(
        report.rebuild_required.contains("doc_a.tmt"),
        "doc_a must rebuild because it embeds doc_b and doc_b content changed"
    );
}

#[test]
fn test_transclusion_nested_chain_propagation() {
    let mut graph = DependencyGraph::new();

    // C embeds B, B embeds A
    let node_c = make_node(
        "doc_c.tmt",
        "C",
        "C",
        vec![("doc_b.tmt", DependencyKind::Embed)],
    );
    let node_b = make_node(
        "doc_b.tmt",
        "B",
        "B",
        vec![("doc_a.tmt", DependencyKind::Embed)],
    );
    let node_a = make_node("doc_a.tmt", "A", "A", vec![]);

    graph.insert(node_c);
    graph.insert(node_b);
    graph.insert(node_a);

    // doc_a content changed
    let changes = vec![Change::ContentOnly("doc_a.tmt".to_string())];
    let report = graph.affected_by_changes(&changes);

    assert!(report.rebuild_required.contains("doc_a.tmt"));
    assert!(report.rebuild_required.contains("doc_b.tmt"));
    assert!(report.rebuild_required.contains("doc_c.tmt"));
}

#[test]
fn test_document_deletion_invalidates_referrers() {
    let mut graph = DependencyGraph::new();

    let node_a = make_node(
        "doc_a.tmt",
        "Links to B",
        "Doc A",
        vec![("doc_b.tmt", DependencyKind::Link)],
    );
    let node_b = make_node("doc_b.tmt", "Target", "Doc B", vec![]);

    graph.insert(node_a);
    graph.insert(node_b);

    let changes = vec![Change::Removed("doc_b.tmt".to_string())];
    let report = graph.affected_by_changes(&changes);

    assert!(report.removed.contains("doc_b.tmt"));
    assert!(report.rebuild_required.contains("doc_a.tmt"));
}

#[test]
fn test_cycle_dependency_safety() {
    let mut graph = DependencyGraph::new();

    // A embeds B, B embeds A (transclusion cycle)
    let node_a = make_node(
        "doc_a.tmt",
        "A",
        "A",
        vec![("doc_b.tmt", DependencyKind::Embed)],
    );
    let node_b = make_node(
        "doc_b.tmt",
        "B",
        "B",
        vec![("doc_a.tmt", DependencyKind::Embed)],
    );

    graph.insert(node_a);
    graph.insert(node_b);

    let changes = vec![Change::ContentOnly("doc_a.tmt".to_string())];
    let report = graph.affected_by_changes(&changes);

    assert!(report.rebuild_required.contains("doc_a.tmt"));
    assert!(report.rebuild_required.contains("doc_b.tmt"));
}

#[test]
fn test_compute_affected_paths_pure_function() {
    let node_a = make_node(
        "doc_a.tmt",
        "Links to B",
        "Doc A",
        vec![("doc_b.tmt", DependencyKind::Link)],
    );
    let node_b = make_node("doc_b.tmt", "Doc B", "Doc B Title", vec![]);

    let all_nodes = vec![node_a, node_b];
    let changes = vec![Change::ContentOnly("doc_a.tmt".to_string())];

    let report = compute_affected_paths(all_nodes, &changes);
    assert_eq!(report.rebuild_required.len(), 1);
    assert!(report.rebuild_required.contains("doc_a.tmt"));
}

#[test]
fn test_extract_document_node_from_ast() {
    let src = r#"
::title "Getting Started"
::kind note

Welcome to the documentation.
Check out @link("guide/setup") or @embed("snippets/code").
"#;
    let doc = parse_document(src).expect("valid tomet source");

    let resolve = |from: &str, raw: &str| -> Option<String> {
        if raw.starts_with('/') {
            Some(raw.trim_start_matches('/').to_string())
        } else {
            let parent = std::path::Path::new(from)
                .parent()
                .unwrap_or(std::path::Path::new(""));
            let combined = parent.join(raw);
            Some(combined.to_string_lossy().to_string())
        }
    };

    let node = extract_document_node("index.tmt", src, &doc, resolve);

    assert_eq!(node.path, "index.tmt");
    assert_eq!(node.outgoing.len(), 2);

    let link_dep = node
        .outgoing
        .iter()
        .find(|d| d.target == "guide/setup")
        .expect("should have link to guide/setup");
    assert_eq!(link_dep.kind, DependencyKind::Link);

    let embed_dep = node
        .outgoing
        .iter()
        .find(|d| d.target == "snippets/code")
        .expect("should have embed to snippets/code");
    assert_eq!(embed_dep.kind, DependencyKind::Embed);
}
