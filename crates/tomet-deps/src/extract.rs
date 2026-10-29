//! Pure in-memory dependency and export extraction from parsed Tomet ASTs.

use tomet_ast::{Block, Document};
use tomet_semantics::{ElementKind, TargetScheme, link_target_of, path_target_of, target_scheme};
use tomet_tree::for_each_element;

use crate::model::{Dependency, DependencyKind, DocumentNode, fnv1a_hash};

/// Extracts raw outgoing targets alongside their dependency kind from an AST Document.
pub fn extract_raw_dependencies(doc: &Document) -> Vec<(String, DependencyKind)> {
    let mut out = Vec::new();
    for_each_element(doc, |el| {
        if let Some((kind, target)) = link_target_of(el).or_else(|| path_target_of(el)) {
            match kind {
                ElementKind::Embed => {
                    out.push((target, DependencyKind::Embed));
                }
                ElementKind::File | ElementKind::Dir => {
                    out.push((target, DependencyKind::Link));
                }
                ElementKind::Link => {
                    let (scheme, rest) = target_scheme(&target);
                    match scheme {
                        TargetScheme::File
                        | TargetScheme::Dir
                        | TargetScheme::Document
                        | TargetScheme::Ref => {
                            out.push((rest.to_string(), DependencyKind::Link));
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    });
    out
}

/// Computes a hash of document declarations (such as `::meta`, `::title`, `::kind`, `::slug`, etc.).
pub fn extract_exports_hash(doc: &Document) -> u64 {
    let mut hash_bytes = Vec::new();
    for block in &doc.blocks {
        if let Block::Element(el) = block
            && el.sigil.is_document()
        {
            if let Some(name) = el.sigil.name() {
                hash_bytes.extend_from_slice(name.name.as_bytes());
            }
            if let Some(args) = &el.args
                && let Some(text) = args.scalar_text()
            {
                hash_bytes.extend_from_slice(text.as_bytes());
            }
            if let Some(val) = &el.value {
                for (k, v) in val.pairs() {
                    hash_bytes.extend_from_slice(k.as_bytes());
                    if let Some(text) = v.scalar_text() {
                        hash_bytes.extend_from_slice(text.as_bytes());
                    }
                }
            }
        }
    }
    fnv1a_hash(&hash_bytes)
}

/// Builds a [`DocumentNode`] from a parsed document given a target resolution function.
pub fn extract_document_node<F>(
    path: &str,
    source: &str,
    doc: &Document,
    resolve_target: F,
) -> DocumentNode
where
    F: Fn(&str, &str) -> Option<String>,
{
    let content_hash = fnv1a_hash(source.as_bytes());
    let exports_hash = extract_exports_hash(doc);
    let raw_deps = extract_raw_dependencies(doc);

    let mut outgoing = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for (raw, kind) in raw_deps {
        if let Some(resolved) = resolve_target(path, &raw) {
            if resolved == path {
                continue;
            }
            if seen.insert((resolved.clone(), kind)) {
                outgoing.push(Dependency {
                    target: resolved,
                    kind,
                    raw,
                });
            }
        }
    }

    DocumentNode {
        path: path.to_string(),
        content_hash,
        exports_hash,
        outgoing,
    }
}
