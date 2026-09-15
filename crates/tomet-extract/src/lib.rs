//! Extraction and mapping of Tomet markup embedded inside source code comments.
//!
//! Code comments often refer to specifications, architectural rules, or invariants:
//!
//! ```rust,ignore
//! /// Resolves document relative paths.
//! /// See @file(docs/spec/resolution.tmt) and @rule(crate-layering).
//! pub fn resolve(...) { ... }
//! ```
//!
//! This crate extracts contiguous comment blocks from source code across multiple
//! programming languages (Rust, Python, JavaScript/TypeScript, Java, C/C++, Go,
//! Shell, etc.), strips comment prefixes while preserving line and column mapping,
//! and parses the extracted text into Tomet AST [`tomet_ast::Document`]s.
//!
//! By preserving original source locations, links and rule references inside comments
//! can be checked by tools like `tomet check-links` or `twrit` with exact line and
//! column diagnostics pointing back to the original source file.

pub mod comment;
pub mod lang;

use std::path::{Path, PathBuf};

pub use comment::{ExtractOptions, extract_comments};
pub use lang::Language;
use tomet_ast::{Document, Element, Position, Span};
use tomet_parser::Error as ParseError;

/// The kind of comment in source code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentKind {
    /// Inner doc comment (e.g. `//!` in Rust), applying to the enclosing item/module.
    InnerDoc,
    /// Outer doc comment (e.g. `///` in Rust, `/**` in JS/Java, `"""` in Python), applying to the subsequent item.
    OuterDoc,
    /// Regular line comment (e.g. `//`, `#`, `--`).
    Line,
    /// Regular block comment (e.g. `/* ... */`, `<!-- ... -->`).
    Block,
}

/// A contiguous block of comment lines extracted from a source file.
#[derive(Debug, Clone)]
pub struct ExtractedCommentBlock {
    /// The concatenated comment text after stripping comment markers.
    pub text: String,
    /// The syntactic kind of this comment block.
    pub kind: CommentKind,
    /// Mapping from line/column in `text` back to the source file.
    pub source_map: CommentSourceMap,
}

/// Records the source location of each line in an extracted comment block.
#[derive(Debug, Clone)]
pub struct CommentSourceMap {
    /// The path to the source file.
    pub file_path: PathBuf,
    /// Per line mapping: for each 0-indexed line in stripped text:
    /// `(original_1_indexed_line, original_1_indexed_column_start)`
    pub lines: Vec<(usize, usize)>,
}

impl CommentSourceMap {
    /// Maps a 1-indexed line and column from the extracted comment text back to
    /// the original source file's [`Position`].
    pub fn map_position(&self, pos: Position) -> Position {
        if pos.line == 0 || pos.line > self.lines.len() {
            return pos;
        }
        let (orig_line, orig_col_start) = self.lines[pos.line - 1];
        let orig_col = orig_col_start + pos.column.saturating_sub(1);
        Position::new(orig_line, orig_col, pos.offset)
    }

    /// Maps a [`Span`] from the extracted comment text back to original source file coordinates.
    pub fn map_span(&self, span: Span) -> Span {
        Span::new(self.map_position(span.start), self.map_position(span.end))
    }
}

/// A parsed Tomet comment document along with its source map.
#[derive(Debug, Clone)]
pub struct ParsedComment {
    /// The parsed Tomet AST document.
    pub doc: Document,
    /// The kind of comment block.
    pub kind: CommentKind,
    /// Location mapping back to the original source file.
    pub source_map: CommentSourceMap,
}

/// Extracts comments from a source file, automatically detecting the language
/// from the file path extension.
pub fn extract_comments_from_file(
    source: &str,
    file: &Path,
    options: ExtractOptions,
) -> Vec<ExtractedCommentBlock> {
    let lang = Language::from_path(file).unwrap_or(Language::Rust);
    extract_comments(source, file, lang, options)
}

/// Extracts contiguous Rust doc comments (`///` and `//!`) from source text.
pub fn extract_rust_comments(source: &str, file: &Path) -> Vec<ExtractedCommentBlock> {
    extract_comments(
        source,
        file,
        Language::Rust,
        ExtractOptions { doc_only: true },
    )
}

/// Parses extracted comments from source into Tomet documents for a given language.
pub fn parse_comments(
    source: &str,
    file: &Path,
    lang: Language,
    options: ExtractOptions,
) -> (Vec<ParsedComment>, Vec<(ParseError, Position)>) {
    let blocks = extract_comments(source, file, lang, options);
    let mut parsed = Vec::new();
    let mut errors = Vec::new();

    for block in blocks {
        match tomet_parser::parse_document(&block.text) {
            Ok(doc) => {
                parsed.push(ParsedComment {
                    doc,
                    kind: block.kind,
                    source_map: block.source_map,
                });
            }
            Err(err) => {
                let mapped_pos = block
                    .source_map
                    .map_position(Position::new(err.line, err.column, err.offset));
                errors.push((err, mapped_pos));
            }
        }
    }

    (parsed, errors)
}

/// Parses all extracted doc comments in a Rust source file into Tomet documents.
pub fn parse_rust_comments(
    source: &str,
    file: &Path,
) -> (Vec<ParsedComment>, Vec<(ParseError, Position)>) {
    parse_comments(
        source,
        file,
        Language::Rust,
        ExtractOptions { doc_only: true },
    )
}

/// Collects all Tomet elements found in comments for a specific language.
pub fn collect_comment_elements_for_lang(
    source: &str,
    file: &Path,
    lang: Language,
    options: ExtractOptions,
) -> Vec<(Element, Span, CommentKind)> {
    let (comments, _) = parse_comments(source, file, lang, options);
    let mut results = Vec::new();

    for comment in comments {
        tomet_tree::for_each_element(&comment.doc, |el| {
            let mapped_span = comment.source_map.map_span(el.span);
            results.push((el.clone(), mapped_span, comment.kind));
        });
    }

    results
}

/// Collects all Tomet elements found in comments, auto-detecting language from `file`.
pub fn collect_comment_elements(source: &str, file: &Path) -> Vec<(Element, Span, CommentKind)> {
    let lang = Language::from_path(file).unwrap_or(Language::Rust);
    // For Rust, default to doc comments only; for others include regular comments if no dedicated doc prefix exists
    let doc_only = !lang.syntax().doc_line_prefixes.is_empty();
    collect_comment_elements_for_lang(source, file, lang, ExtractOptions { doc_only })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_rust_outer_doc_comments() {
        let src = r#"
// normal comment
/// Outer doc line 1
/// Outer doc line 2
fn foo() {}
"#;
        let blocks = extract_rust_comments(src, Path::new("test.rs"));
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].kind, CommentKind::OuterDoc);
        assert_eq!(blocks[0].text, "Outer doc line 1\nOuter doc line 2\n");
        assert_eq!(blocks[0].source_map.lines, vec![(3, 5), (4, 5)]);
    }

    #[test]
    fn test_extract_rust_inner_doc_comments() {
        let src = r#"//! Module header line 1
//! Module header line 2

fn bar() {}
"#;
        let blocks = extract_rust_comments(src, Path::new("lib.rs"));
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].kind, CommentKind::InnerDoc);
        assert_eq!(
            blocks[0].text,
            "Module header line 1\nModule header line 2\n"
        );
        assert_eq!(blocks[0].source_map.lines, vec![(1, 5), (2, 5)]);
    }

    #[test]
    fn test_collect_elements_with_spans() {
        let src = r#"
/// Before link.
/// See @file(docs/spec/syntax.tmt) for details.
/// And @rule(crate-layering).
pub fn parse() {}
"#;
        let elements = collect_comment_elements(src, Path::new("src/parser.rs"));
        assert_eq!(elements.len(), 2);

        let (el0, span0, kind0) = &elements[0];
        assert_eq!(el0.sigil.name().map(|n| n.name.as_str()), Some("file"));
        assert_eq!(kind0, &CommentKind::OuterDoc);
        assert_eq!(span0.start.line, 3);

        let (el1, span1, kind1) = &elements[1];
        assert_eq!(el1.sigil.name().map(|n| n.name.as_str()), Some("rule"));
        assert_eq!(kind1, &CommentKind::OuterDoc);
        assert_eq!(span1.start.line, 4);
    }
}
