//! Regression guard: asserts that converters (HTML, Markdown, Pandoc, Typst)
//! do not silently flatten `Value::Blocks` args into lossy placeholder strings
//! like `<N block(s)>`.
//!
//! When `@conflict(a: [...], b: [...])` was introduced, generic fallbacks in each
//! converter flattened arguments into scalar data attributes, replacing the block
//! contents with `<N block(s)>` while all tests stayed green.
//! This test ensures any builtin element with block-content arguments renders
//! its contents faithfully across all converters without lossy placeholders.

use tomet_ast::Document;

/// Builtin kinds whose `(args)` intentionally carry `Value::Blocks`.
/// Currently `@conflict(a: [...], b: [...])`.
const BLOCK_BEARING_KINDS: &[&str] = &["conflict"];

const LOSSY_PLACEHOLDER_SUBSTRING: &str = "block(s)>";

fn convert_all(doc: &Document) -> [(&'static str, String); 4] {
    let html = tomet_html::render_body(doc);
    let md = tomet_markdown::to_markdown(doc);
    let typ = tomet_typst::to_typst(doc);
    let pandoc = serde_json::to_string(&tomet_pandoc::to_pandoc(doc))
        .expect("pandoc serialization must succeed");

    [
        ("HTML", html),
        ("Markdown", md),
        ("Typst", typ),
        ("Pandoc", pandoc),
    ]
}

#[test]
fn conflict_named_block_arguments_preserve_content_across_all_converters() {
    let src = r#"
@conflict(
  a: [
    Alpha unique local line.

    Alpha second paragraph with @em[emphasis].
  ],
  b: [
    Beta unique remote line.
  ]
)
"#;
    let doc = tomet_parser::parse_document(src).expect("document should parse cleanly");
    let outputs = convert_all(&doc);

    for (format, text) in outputs {
        assert!(
            !text.contains(LOSSY_PLACEHOLDER_SUBSTRING),
            "Converter for {format} emitted lossy placeholder '{LOSSY_PLACEHOLDER_SUBSTRING}'!\nOutput:\n{text}"
        );
        assert!(
            text.contains("Alpha") && text.contains("local"),
            "Converter for {format} dropped side 'a' content!\nOutput:\n{text}"
        );
        assert!(
            text.contains("Beta") && text.contains("remote"),
            "Converter for {format} dropped side 'b' content!\nOutput:\n{text}"
        );
    }
}

#[test]
fn conflict_positional_block_arguments_preserve_content_across_all_converters() {
    let src = r#"
@conflict(
  [PositionalAlphaContent],
  [PositionalBetaContent]
)
"#;
    let doc = tomet_parser::parse_document(src).expect("document should parse cleanly");
    let outputs = convert_all(&doc);

    for (format, text) in outputs {
        assert!(
            !text.contains(LOSSY_PLACEHOLDER_SUBSTRING),
            "Converter for {format} emitted lossy placeholder in positional shorthand!\nOutput:\n{text}"
        );
        assert!(
            text.contains("PositionalAlphaContent"),
            "Converter for {format} dropped positional side 'a' content!\nOutput:\n{text}"
        );
        assert!(
            text.contains("PositionalBetaContent"),
            "Converter for {format} dropped positional side 'b' content!\nOutput:\n{text}"
        );
    }
}

#[test]
fn all_block_bearing_kinds_are_in_builtin_kinds() {
    // Sanity check that every element tracked here is actually recognized as a builtin kind.
    for kind in BLOCK_BEARING_KINDS {
        assert!(
            tomet_semantics::BUILTIN_KINDS
                .iter()
                .any(|(name, _)| name == kind),
            "'{kind}' should be in BUILTIN_KINDS"
        );
    }
}
