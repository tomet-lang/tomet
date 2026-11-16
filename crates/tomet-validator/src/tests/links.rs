//! Links to an anchor, an id or an embedded document.

use super::*;

#[test]
fn dangling_anchor_link_is_reported() {
    let doc = parse("@link(#missing)\n");
    let errors = validate_document(&doc);
    assert_eq!(errors.len(), 1);
    assert!(matches!(
        &errors[0],
        Diagnostic::DanglingAnchor { anchor, .. } if anchor == "missing"
    ));
    assert_eq!(errors[0].severity(), Severity::Error);
    assert_eq!(errors[0].to_string(), "dangling anchor `#missing`");
}

#[test]
fn dangling_id_link_is_reported() {
    let doc = parse("@link(id:missing)\n");
    let errors = validate_document(&doc);
    assert_eq!(errors.len(), 1);
    assert!(matches!(
        &errors[0],
        Diagnostic::DanglingAnchor { anchor, .. } if anchor == "missing"
    ));
}

#[test]
fn dangling_embed_is_reported() {
    let doc = parse("@embed(#missing)\n");
    let errors = validate_document(&doc);
    assert_eq!(errors.len(), 1);
    assert!(matches!(
        &errors[0],
        Diagnostic::DanglingAnchor { anchor, .. } if anchor == "missing"
    ));
}

#[test]
fn valid_path_traversal_anchor_is_fine() {
    let doc = parse("@card()#(intro)\n\n@link(#intro.prop.tags)\n");
    assert_eq!(validate_document(&doc), vec![]);
}

#[test]
fn dangling_path_traversal_anchor_is_reported() {
    let doc = parse("@link(#missing.prop.tags)\n");
    let errors = validate_document(&doc);
    assert_eq!(errors.len(), 1);
    assert!(matches!(
        &errors[0],
        Diagnostic::DanglingAnchor { anchor, .. } if anchor == "missing.prop.tags"
    ));
}

#[test]
fn positional_item_link_is_allowed() {
    let doc = parse("@link(#item(0))\n");
    assert_eq!(validate_document(&doc), vec![]);
}

#[test]
fn external_and_doc_links_are_not_flagged_as_dangling_internal_anchors() {
    let doc = parse(
        "@link(https://example.com)\n@link(file:readme.md)\n@link(tmt:other.tmt#some)\n@link(ref:Note#some)\n",
    );
    assert_eq!(validate_document(&doc), vec![]);
}

#[test]
fn anchor_on_list_item_is_found() {
    let doc = parse("- item #(task1)\n\n@link(#task1)\n");
    assert_eq!(validate_document(&doc), vec![]);
}

#[test]
fn empty_anchor_is_reported() {
    let doc = parse("@link(#)\n");
    let errors = validate_document(&doc);
    assert_eq!(errors.len(), 1);
    assert!(matches!(
        &errors[0],
        Diagnostic::DanglingAnchor { anchor, .. } if anchor.is_empty()
    ));
}
