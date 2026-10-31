//! Guard: the two parsers see the same blocks.
//!
//! `tomet-parser` has two readers of one grammar: the AST parser
//! (`parse_document`) and the CST parser (`parse_cst`, which the LSP,
//! highlight, merge and edit use). Nothing else says they agree, and before
//! this test they did not: nested lists, a group on the line after its element,
//! a fence closing on a run of backticks in the middle of a line, and a
//! section with only an id all read differently in the CST.
//!
//! This compares the *shape* of every fixture: for each block, its kind and where
//! it starts, in document order and through sections. It does not compare what is
//! inside a paragraph or a group. The point is that a grammar change which one
//! parser takes and the other does not shows up here, in a test that names the
//! fixture, before it shows up in an editor.

use tomet_ast::Block;
use tomet_cst::{SyntaxKind as K, SyntaxNode};
use tomet_tests::{corpus, read_fixture};

/// A block's kind as both parsers can spell it: a section, a paragraph, a list,
/// or an element (a block element, a declaration, a fenced code block, a
/// thematic break).
type Shape = (char, usize);

fn ast_shape(blocks: &[Block], out: &mut Vec<Shape>) {
    for block in blocks {
        match block {
            Block::Section(s) => {
                out.push(('S', s.span.start));
                ast_shape(&s.blocks, out);
            }
            Block::Paragraph(p) => out.push(('P', p.span.start)),
            // The CST has no node of its own for a `${...}` block: it is a paragraph.
            Block::Interp(i) => out.push(('P', i.span.start)),
            Block::List(l) => out.push(('L', l.span.start)),
            Block::Element(e) => out.push(('E', e.span.start)),
            Block::Declaration(d) => out.push(('E', d.span.start)),
            Block::Hole(h) => out.push(('E', h.span.start)),
        }
    }
}

/// The first character of the node's text that is not white space.
fn start_of(node: &SyntaxNode) -> usize {
    let text = node.text().to_string();
    usize::from(node.text_range().start()) + (text.len() - text.trim_start().len())
}

/// A paragraph that holds nothing but comments is trivia to the AST parser.
fn only_comments(node: &SyntaxNode) -> bool {
    let text = node.text().to_string();
    let mut rest = text.as_str();
    let mut without_block_comments = String::new();
    while let Some(i) = rest.find("/*") {
        without_block_comments.push_str(&rest[..i]);
        rest = match rest[i..].find("*/") {
            Some(j) => &rest[i + j + 2..],
            None => "",
        };
    }
    without_block_comments.push_str(rest);
    without_block_comments
        .lines()
        .all(|line| line.trim().is_empty() || line.trim_start().starts_with("//"))
}

fn cst_shape(node: &SyntaxNode, out: &mut Vec<Shape>) {
    for child in node.children() {
        let kind = match child.kind() {
            K::SECTION => 'S',
            K::PARAGRAPH if only_comments(&child) => continue,
            K::PARAGRAPH => 'P',
            K::LIST => 'L',
            K::BLOCK_ELEMENT | K::CODE_BLOCK | K::THEMATIC_BREAK => 'E',
            _ => continue,
        };
        out.push((kind, start_of(&child)));
        if kind == 'S' {
            cst_shape(&child, out);
        }
    }
}

#[test]
fn the_cst_and_the_ast_see_the_same_blocks_in_every_fixture() {
    let mut disagreements = Vec::new();
    for rel in corpus() {
        let src = read_fixture(&rel);
        // A fixture the AST parser rejects has no AST shape to compare.
        let Ok(doc) = tomet_parser::parse_document(&src) else {
            continue;
        };
        let (mut ast, mut cst) = (Vec::new(), Vec::new());
        ast_shape(&doc.blocks, &mut ast);
        cst_shape(&tomet_parser::parse_cst(&src), &mut cst);
        if ast != cst {
            let at = ast
                .iter()
                .zip(&cst)
                .position(|(a, c)| a != c)
                .unwrap_or(ast.len().min(cst.len()));
            let line = |shape: Option<&Shape>| {
                shape.map_or("nothing".to_string(), |&(kind, start)| {
                    let n = src[..start].matches('\n').count() + 1;
                    format!("{kind} at line {n}")
                })
            };
            disagreements.push(format!(
                "{}: the AST has {} where the CST has {}",
                rel.display(),
                line(ast.get(at)),
                line(cst.get(at)),
            ));
        }
    }
    assert!(
        disagreements.is_empty(),
        "the two parsers read different blocks:\n  {}",
        disagreements.join("\n  ")
    );
}
