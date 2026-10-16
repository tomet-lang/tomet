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
//! Shell, etc.), strips comment prefixes while mapping every byte offset back to the original file,
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
use tomet_ast::{Document, Element, Span};
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
    /// Mapping from byte offsets in `text` back to the source file.
    pub source_map: CommentSourceMap,
}

/// Where one line of an extracted comment block came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineOrigin {
    /// Byte offset of the line's first byte in the extracted `text`.
    pub text_start: usize,
    /// Byte offset of that same byte in the original source file.
    pub source_start: usize,
}

/// Records where each line of an extracted comment block sits in the source file.
///
/// Everything is a byte offset: a [`Span`] from parsing `text` points into
/// `text`, [`map_span`](Self::map_span) turns it into a span of the original
/// file, and the caller resolves a line and column from the file with a
/// `tomet_lexer::LineIndex` when it has to show one.
#[derive(Debug, Clone)]
pub struct CommentSourceMap {
    /// The path to the source file.
    pub file_path: PathBuf,
    /// One entry per line of the extracted text, in order.
    pub lines: Vec<LineOrigin>,
}

impl CommentSourceMap {
    /// Builds the map for `text`, whose lines came from the original source at
    /// `source_starts` (one byte offset per line of `text`).
    pub(crate) fn new(file_path: &Path, text: &str, source_starts: &[usize]) -> Self {
        let mut text_start = 0;
        let mut lines = Vec::with_capacity(source_starts.len());
        for (line, &source_start) in text.split_inclusive('\n').zip(source_starts) {
            lines.push(LineOrigin {
                text_start,
                source_start,
            });
            text_start += line.len();
        }
        CommentSourceMap {
            file_path: file_path.to_path_buf(),
            lines,
        }
    }

    /// Maps a byte offset in the extracted comment text to the byte offset of
    /// the same byte in the original source file. An offset before the first
    /// line, or in a map with no lines, is returned unchanged.
    pub fn map_offset(&self, offset: usize) -> usize {
        let line = self.lines.partition_point(|l| l.text_start <= offset);
        match line.checked_sub(1).map(|i| self.lines[i]) {
            Some(origin) => origin.source_start + (offset - origin.text_start),
            None => offset,
        }
    }

    /// Maps a [`Span`] from the extracted comment text back to the original source file.
    pub fn map_span(&self, span: Span) -> Span {
        Span::new(self.map_offset(span.start), self.map_offset(span.end))
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
) -> (Vec<ParsedComment>, Vec<(ParseError, usize)>) {
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
                // The error's offset points into the comment text; report where
                // that is in the original file.
                let at = block.source_map.map_offset(err.offset);
                errors.push((err, at));
            }
        }
    }

    (parsed, errors)
}

/// Parses all extracted doc comments in a Rust source file into Tomet documents.
pub fn parse_rust_comments(
    source: &str,
    file: &Path,
) -> (Vec<ParsedComment>, Vec<(ParseError, usize)>) {
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

    fn line_of(src: &str, offset: usize) -> usize {
        src[..offset].matches('\n').count() + 1
    }

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
        // Offsets of the same bytes in the extracted text and in `src`.
        assert_eq!(
            blocks[0].source_map.lines,
            vec![
                LineOrigin {
                    text_start: 0,
                    source_start: 23
                },
                LineOrigin {
                    text_start: 17,
                    source_start: 44
                },
            ]
        );
        let at = blocks[0].source_map.lines[1].source_start;
        assert_eq!(&src[at..at + 16], "Outer doc line 2");
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
        assert_eq!(
            blocks[0].source_map.lines,
            vec![
                LineOrigin {
                    text_start: 0,
                    source_start: 4
                },
                LineOrigin {
                    text_start: 21,
                    source_start: 29
                },
            ]
        );
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
        assert!(src[span0.start..].starts_with("@file(docs/spec/syntax.tmt)"));
        assert_eq!(line_of(src, span0.start), 3);

        let (el1, span1, kind1) = &elements[1];
        assert_eq!(el1.sigil.name().map(|n| n.name.as_str()), Some("rule"));
        assert_eq!(kind1, &CommentKind::OuterDoc);
        assert!(src[span1.start..].starts_with("@rule(crate-layering)"));
        assert_eq!(line_of(src, span1.start), 4);
    }
}
