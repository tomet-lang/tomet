use std::path::Path;
use tomet_extract::{extract_rust_comments, collect_comment_elements, CommentKind};

#[test]
fn test_indented_and_multiline_elements() {
    let source = r#"
pub struct Service {
    /// Invariant for service configuration:
    ///
    /// @rule(service-config){
    ///   guard: { twrit: door }
    /// }
    ///
    /// Refer to @file(docs/spec/service.tmt) for details.
    pub config: String,
}
"#;

    let elements = collect_comment_elements(source, Path::new("src/service.rs"));
    assert_eq!(elements.len(), 2);

    let (el0, span0, kind0) = &elements[0];
    assert_eq!(el0.sigil.name().map(|n| n.name.as_str()), Some("rule"));
    assert_eq!(kind0, &CommentKind::OuterDoc);
    // Line 5 is where `@rule` appears in source
    assert_eq!(span0.start.line, 5);
    // Indentation: 4 spaces + '/// ' (4 chars) + 1 = col 9
    assert_eq!(span0.start.column, 9);

    let (el1, span1, kind1) = &elements[1];
    assert_eq!(el1.sigil.name().map(|n| n.name.as_str()), Some("file"));
    assert_eq!(kind1, &CommentKind::OuterDoc);
    // Line 9 is where `@file` appears
    assert_eq!(span1.start.line, 9);
}

#[test]
fn test_empty_comment_lines_preserve_line_mapping() {
    let source = r#"
/// First line
///
/// Third line after empty line
fn bar() {}
"#;

    let blocks = extract_rust_comments(source, Path::new("test.rs"));
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].source_map.lines.len(), 3);
    assert_eq!(blocks[0].source_map.lines[0], (2, 5));
    assert_eq!(blocks[0].source_map.lines[1], (3, 4)); // '///' with no space
    assert_eq!(blocks[0].source_map.lines[2], (4, 5));
}
