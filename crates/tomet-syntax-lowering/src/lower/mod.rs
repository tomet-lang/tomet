//! Lowering: the CST read as an AST.
//!
//! The CST parser is the one reader of the grammar's shape (where a block, an
//! element, a group starts and ends); the AST is the owned, typed tree the rest
//! of the system works on. Lowering is the fold from the first to the second, so
//! that a construct is written once, in the CST parser: `parse_document` is
//! `parse_cst` and then [`lower_file`].
//!
//! Lowering is also where a document is refused. The CST never fails, so what
//! the grammar does not allow -- a second `(args)` group, a `[` that never
//! closes, a `|` in a column that continues nothing -- is found here, with a
//! message and the offset it is at.
//!
//! What it reads from text rather than from nodes is not shape: how a run of
//! source text becomes `Text` and `SoftBreak` items, and the grammars of the leaves --
//! `${...}` expressions and a group's values, which hand back to the CST
//! whatever they embed.
//!
//! The fold is split by construct, one file each; the entry points and the block
//! dispatch are here.

use self::code_block::lower_code_block;
use self::error_node::lower_error;
use self::form::{lower_block_form, require_primary_group};
use self::groups::lower_groups;
use self::list::lower_list;
use self::paragraph::lower_paragraph;
use self::section::lower_section;
use self::thematic_break::lower_thematic_break;
use crate::error::{Error, Result};
use crate::value::err;
use tomet_ast::{Block, Declaration, File, Name};
use tomet_cst::{AstNode, CstName, SyntaxElement, SyntaxKind as K, SyntaxNode};
use tomet_span::Span;

mod code_block;
mod connect;
pub(crate) mod error_node;
pub(crate) mod form;
pub(crate) mod groups;
mod list;
mod paragraph;
mod section;
mod thematic_break;

/// The error for a tree lowering cannot read: a node the CST should not have put
/// there. It is a bug in the CST parser or in lowering, not in the document.
fn unsupported(what: impl std::fmt::Display) -> Error {
    Error {
        message: format!("the syntax tree holds something lowering does not read: {what}"),
        line: 0,
        column: 0,
        offset: 0,
    }
}

/// Where `node` starts and ends, white space and all.
fn range_of(node: &SyntaxNode) -> (usize, usize) {
    let range = node.text_range();
    (usize::from(range.start()), usize::from(range.end()))
}

/// The name of an element, declaration, or connect, from its `NAME` child.
fn name_of(node: &SyntaxNode) -> Result<Name> {
    let name_node = node
        .children()
        .find(|n| n.kind() == K::NAME)
        .ok_or_else(|| unsupported("a name that does not read"))?;
    let cst_name = CstName::cast(name_node).expect("cast to CstName");
    let (namespace, name) = cst_name.parts();
    Ok(Name { namespace, name })
}

fn is_bracket(kind: K) -> bool {
    matches!(kind, K::L_BRACKET | K::R_BRACKET)
}

/// Reads the CST root as an AST [`File`].
pub fn lower_file(root: &SyntaxNode) -> Result<File> {
    reject_too_deep(root)?;
    let mut header = Vec::new();
    let mut blocks = Vec::new();
    let mut seen_content = false;

    for child in root.children_with_tokens() {
        match child {
            // Trivia, and the `;` that only breaks a block, leave nothing behind.
            SyntaxElement::Token(t) if t.kind().is_trivia() || t.kind() == K::SEMI => {}
            SyntaxElement::Token(t) => return Err(unsupported(format!("token {:?}", t.kind()))),
            SyntaxElement::Node(n) if n.kind() == K::ERROR => return Err(lower_error(&n)),
            SyntaxElement::Node(n) => {
                if is_declaration(&n) {
                    if seen_content {
                        return Err(err(
                            range_of(&n).0,
                            "a declaration belongs at the top of the file",
                        ));
                    }
                    header.push(lower_declaration(&n)?);
                } else if let Some(block) = lower_block(&n)? {
                    seen_content = true;
                    blocks.push(block);
                }
            }
        }
    }

    Ok(File::new(
        header,
        blocks,
        Span::new(0, usize::from(root.text_range().len())),
    ))
}

/// The error for a tree that holds a `TOO_DEEP` node: the parser stops reading structure there, so
/// nothing below it is meaningful. One walk of the tree, once.
pub(crate) fn reject_too_deep(root: &SyntaxNode) -> Result<()> {
    match root.descendants().find(|n| n.kind() == K::TOO_DEEP) {
        Some(node) => Err(err(
            range_of(&node).0,
            format!(
                "this is nested more than {} levels deep",
                tomet_cst::MAX_DEPTH
            ),
        )),
        None => Ok(()),
    }
}

/// The blocks that are children of `node`: the document, and (later) a section
/// or a `[content]` group.
fn lower_blocks(node: &SyntaxNode) -> Result<Vec<Block>> {
    let mut blocks = Vec::new();
    for child in node.children_with_tokens() {
        match child {
            // Trivia, and the `;` that only breaks a block, leave nothing behind.
            SyntaxElement::Token(t) if t.kind().is_trivia() || t.kind() == K::SEMI => {}
            // The brackets of a `[content]` group are not content.
            SyntaxElement::Token(t) if node.kind() == K::CONTENT && is_bracket(t.kind()) => {}
            // The `|` that opens a run.
            SyntaxElement::Token(t) if node.kind() == K::CONTENT && t.kind() == K::PIPE => {}
            // The `|` that begins each later line of a run.
            SyntaxElement::Token(t) if t.kind() == K::MARGIN => {}
            SyntaxElement::Token(t) => return Err(unsupported(format!("token {:?}", t.kind()))),
            SyntaxElement::Node(n) if n.kind() == K::ERROR => return Err(lower_error(&n)),
            SyntaxElement::Node(n) => blocks.extend(lower_block(&n)?),
        }
    }
    Ok(blocks)
}

/// One block node. `None` is a block that leaves nothing in the AST (an empty paragraph).
fn lower_block(node: &SyntaxNode) -> Result<Option<Block>> {
    match node.kind() {
        K::PARAGRAPH => lower_paragraph(node),
        K::BLOCK_FORM => lower_block_form(node).map(Some),
        K::SECTION => lower_section(node).map(Some),
        K::LIST => lower_list(node).map(|l| Some(Block::List(l))),
        K::THEMATIC_BREAK => lower_thematic_break(node).map(Some),
        K::CODE_BLOCK => lower_code_block(node).map(Some),
        // The text after a declaration on its line, or an error node.
        K::ERROR => Err(lower_error(node)),
        other => Err(unsupported(format!("node {other:?}"))),
    }
}

/// Whether `node` is a `::name(...)` block declaration.
pub fn is_declaration(node: &SyntaxNode) -> bool {
    node.kind() == K::BLOCK_FORM
        && node
            .children()
            .find(|n| n.kind() == K::SIGIL)
            .and_then(|n| n.first_token())
            .is_some_and(|t| t.kind() == K::COLON)
}

/// `::name(args){value}[content]#(id)`.
pub fn lower_declaration(node: &SyntaxNode) -> Result<Declaration> {
    let sigil = node
        .children()
        .find(|n| n.kind() == K::SIGIL)
        .ok_or_else(|| unsupported("a declaration without a sigil"))?;
    let (start, _) = range_of(&sigil);
    let name = name_of(&sigil)?;
    // Before any group is read: a nameless `::(...)` would otherwise merge into the args.
    if node.children().any(|n| n.kind() == K::CONNECT) {
        return Err(err(
            start,
            "a `::` declaration takes no `::connect`: a connect belongs to an element",
        ));
    }
    require_primary_group(node, &sigil.text().to_string(), start)?;
    let read = lower_groups(node)?;
    let end = node
        .children()
        .map(|n| range_of(&n).1)
        .max()
        .unwrap_or(start);
    Ok(Declaration {
        name,
        data: read.data,
        content: read.content,
        id: read.id,
        span: Span::new(start, end),
    })
}
