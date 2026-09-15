use std::path::Path;
use tomet_extract::{CommentKind, collect_comment_elements};

#[test]
fn test_python_hash_comments_and_docstrings() {
    let py_src = r#"
# Service implementation.
# See @file(docs/spec/python.tmt) for specification.

def run():
    """
    Main entry point.
    Follows @rule(service-config) invariants.
    """
    pass
"#;

    let elements = collect_comment_elements(py_src, Path::new("service.py"));
    assert_eq!(elements.len(), 2);

    let (el0, span0, kind0) = &elements[0];
    assert_eq!(el0.sigil.name().map(|n| n.name.as_str()), Some("file"));
    assert_eq!(kind0, &CommentKind::Line);
    assert_eq!(span0.start.line, 3);

    let (el1, span1, kind1) = &elements[1];
    assert_eq!(el1.sigil.name().map(|n| n.name.as_str()), Some("rule"));
    assert_eq!(kind1, &CommentKind::OuterDoc);
    assert_eq!(span1.start.line, 8);
}

#[test]
fn test_javascript_and_typescript_jsdoc() {
    let ts_src = r#"
// Utility functions.
// See @link(url: "https://example.com") for origin.

/**
 * Parses user input.
 * Relies on @rule(crate-layering).
 */
export function parseInput() {}
"#;

    let elements = collect_comment_elements(ts_src, Path::new("src/parser.ts"));
    assert_eq!(elements.len(), 2);

    let (el0, span0, kind0) = &elements[0];
    assert_eq!(el0.sigil.name().map(|n| n.name.as_str()), Some("link"));
    assert_eq!(kind0, &CommentKind::Line);
    assert_eq!(span0.start.line, 3);

    let (el1, span1, kind1) = &elements[1];
    assert_eq!(el1.sigil.name().map(|n| n.name.as_str()), Some("rule"));
    assert_eq!(kind1, &CommentKind::OuterDoc);
    assert_eq!(span1.start.line, 7);
}

#[test]
fn test_java_javadoc() {
    let java_src = r#"
package com.example;

/**
 * Controller service.
 * Follows @file(docs/spec/controller.tmt).
 */
public class Controller {
}
"#;

    let elements = collect_comment_elements(java_src, Path::new("Controller.java"));
    assert_eq!(elements.len(), 1);

    let (el0, span0, kind0) = &elements[0];
    assert_eq!(el0.sigil.name().map(|n| n.name.as_str()), Some("file"));
    assert_eq!(kind0, &CommentKind::OuterDoc);
    assert_eq!(span0.start.line, 6);
}

#[test]
fn test_shell_and_config_comments() {
    let sh_src = r#"
#!/usr/bin/env bash
# Entrypoint script.
# Governed by @rule(deployment-rules).
echo "starting"
"#;

    let elements = collect_comment_elements(sh_src, Path::new("deploy.sh"));
    assert_eq!(elements.len(), 1);

    let (el0, span0, kind0) = &elements[0];
    assert_eq!(el0.sigil.name().map(|n| n.name.as_str()), Some("rule"));
    assert_eq!(kind0, &CommentKind::Line);
    assert_eq!(span0.start.line, 4);
}
