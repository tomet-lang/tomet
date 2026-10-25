//! `include(target)`: another document's content, put where the call is.
//!
//! The target is a document (its body) or an element, section or list found by id
//! (that node, a section with everything under it). The result is a
//! fragment, built in four steps:
//!
//! 1. the selection is read as written, minus the declarations a document
//!    makes about itself (`::meta`, `::config`, `::kind`, ...);
//! 2. it is prepared **in its own file's context** -- its expressions, its
//!    macros, its `self` -- by the resolver, so it means here what it means
//!    there;
//! 3. its headings are moved to the level they have where they land;
//! 4. its relative links (`./`, `../`) are rewritten to still reach the same
//!    file from the including one -- by the host, through
//!    [`Resolver::rebase_links`](super::Resolver::rebase_links): which elements are
//!    links is not the evaluator's to know.
//!
//! A cycle, or a chain deeper than [`MAX_INCLUDE_DEPTH`], is an error.

use tomet_ast::{Block, Element, Inline, Placement, Sigil};

use super::args::Args;
use super::context::{EvalContext, Note};
use super::error::EvalError;
use super::nodes::load;
use super::value::{DocRef, EvalValue, Node};

/// How many files may be included inside one another.
pub const MAX_INCLUDE_DEPTH: usize = 16;

pub(crate) fn include(args: Args, cx: &EvalContext) -> Result<EvalValue, EvalError> {
    let Options {
        target,
        level,
        heading,
    } = Options::read(args)?;
    let (doc, selection) = select(&target, heading, cx)?;

    if cx.includes.contains(&doc) {
        let chain: Vec<&str> = cx
            .includes
            .iter()
            .chain(std::iter::once(&doc))
            .map(|d| d.0.as_str())
            .collect();
        return Err(EvalError::message(format!(
            "include cycle: {}",
            chain.join(" -> ")
        )));
    }
    if cx.includes.len() >= MAX_INCLUDE_DEPTH {
        return Err(EvalError::message(format!(
            "includes are nested more than {MAX_INCLUDE_DEPTH} deep, which is probably a cycle"
        )));
    }

    let mut chain = cx.includes.to_vec();
    chain.push(doc.clone());
    let prepared = cx
        .resolver
        .prepare_blocks(&doc, selection, &chain)
        .map_err(|why| EvalError::Unresolved(format!("content of `{}`: {why}", doc.0)))?;
    // What stayed unresolved inside is reported where it is written, when
    // that file is checked. Here it is only said that it came along.
    if prepared.unresolved > 0 {
        cx.notes.push(Note::Included {
            from: doc.clone(),
            unresolved: prepared.unresolved,
        });
    }
    let mut blocks = prepared.blocks;

    let wanted = level.unwrap_or(cx.heading_level + 1);
    if let Some(top) = top_section_level(&blocks) {
        shift_levels(&mut blocks, wanted as isize - top as isize);
    }
    let blocks = cx.resolver.rebase_links(blocks, &doc, &cx.self_doc);
    Ok(EvalValue::Fragment(blocks))
}

struct Options {
    target: EvalValue,
    /// `level: n`: the level the included top headings get.
    level: Option<usize>,
    /// `heading: false`: a section without its own heading.
    heading: Option<bool>,
}

impl Options {
    fn read(args: Args) -> Result<Self, EvalError> {
        let mut level = None;
        let mut heading = None;
        for (name, value) in &args.named {
            match (name.as_str(), value) {
                ("level", EvalValue::Int(n)) if *n >= 1 => level = Some(*n as usize),
                ("level", other) => {
                    return Err(EvalError::mismatch(
                        "`include`'s `level:`",
                        "an integer of 1 or more",
                        other.type_name(),
                    ));
                }
                ("heading", EvalValue::Bool(b)) => heading = Some(*b),
                ("heading", other) => {
                    return Err(EvalError::mismatch(
                        "`include`'s `heading:`",
                        "true or false",
                        other.type_name(),
                    ));
                }
                (other, _) => {
                    return Err(EvalError::message(format!(
                        "`include` takes `level:` and `heading:`, not `{other}:`"
                    )));
                }
            }
        }
        let mut positional = args.positional;
        if positional.len() != 1 {
            return Err(EvalError::WrongArgCount {
                function: "include".to_string(),
                expected: "1".to_string(),
                got: positional.len(),
            });
        }
        Ok(Self {
            target: positional.remove(0),
            level,
            heading,
        })
    }
}

/// The document the content comes from, and the blocks selected from it.
fn select(
    target: &EvalValue,
    heading: Option<bool>,
    cx: &EvalContext,
) -> Result<(DocRef, Vec<Block>), EvalError> {
    let not_a_section =
        || EvalError::message("`heading:` applies to a section: give `include` a heading's id");
    match target {
        EvalValue::Document(doc) => {
            if heading.is_some() {
                return Err(not_a_section());
            }
            let loaded = load(doc, cx)?;
            Ok((doc.clone(), strip_declarations(loaded.blocks.clone())))
        }
        EvalValue::Node { doc, node } => match &**node {
            Node::Section(section) => {
                let blocks = if heading == Some(false) {
                    section.blocks.clone()
                } else {
                    vec![Block::Section((**section).clone())]
                };
                Ok((doc.clone(), blocks))
            }
            Node::Element(element) => {
                if heading.is_some() {
                    return Err(not_a_section());
                }
                Ok((doc.clone(), vec![element_block(element)]))
            }
            Node::List(list) => {
                if heading.is_some() {
                    return Err(not_a_section());
                }
                Ok((doc.clone(), vec![Block::List((**list).clone())]))
            }
        },
        other => Err(EvalError::mismatch(
            "`include`",
            "a document or an element with an id",
            other.type_name(),
        )),
    }
}

/// An element as a block: an inline element goes into a paragraph of its
/// own, since a bare inline element is not a block.
fn element_block(element: &Element) -> Block {
    if element.placement == Placement::Inline {
        Block::Paragraph(tomet_ast::Paragraph::new(
            vec![Inline::Element(element.clone())],
            element.span,
        ))
    } else {
        Block::Element(element.clone())
    }
}

/// Whether an element is something a document says about itself, which does
/// not go along when its body is included: a declaration, written `::name`.
///
/// The sigil says so; no name is looked up. That also covers a vocabulary's own preamble
/// elements (`::character`), which no list of standard names could.
fn is_declaration(element: &Element) -> bool {
    matches!(element.sigil, Sigil::Document(_))
}

/// The blocks without the document's own declarations. Declarations on
/// adjacent lines are one paragraph, so they are taken out of paragraphs too,
/// and a paragraph left with nothing in it goes.
fn strip_declarations(blocks: Vec<Block>) -> Vec<Block> {
    blocks
        .into_iter()
        .filter_map(|block| match block {
            Block::Element(element) if is_declaration(&element) => None,
            Block::Paragraph(mut paragraph) => {
                let had = paragraph.content.len();
                paragraph.content.retain(|inline| match inline {
                    Inline::Element(element) => !is_declaration(element),
                    _ => true,
                });
                if paragraph.content.len() == had {
                    return Some(Block::Paragraph(paragraph));
                }
                let empty = paragraph
                    .content
                    .iter()
                    .all(|i| matches!(i, Inline::SoftBreak(_) | Inline::LineBreak(_)));
                (!empty).then_some(Block::Paragraph(paragraph))
            }
            other => Some(other),
        })
        .collect()
}

// ---- heading levels ---------------------------------------------------------

/// The shallowest section among `blocks`.
fn top_section_level(blocks: &[Block]) -> Option<usize> {
    blocks
        .iter()
        .filter_map(|block| match block {
            Block::Section(section) => Some(section.level),
            _ => None,
        })
        .min()
}

/// Moves every heading in `blocks` by `delta` levels, never above level 1.
fn shift_levels(blocks: &mut [Block], delta: isize) {
    for block in blocks {
        match block {
            Block::Section(section) => {
                section.level = (section.level as isize + delta).max(1) as usize;
                shift_levels(&mut section.blocks, delta);
            }
            Block::Element(element) => {
                if let Some(content) = &mut element.content {
                    shift_levels(content, delta);
                }
            }
            Block::List(list) => {
                for item in &mut list.items {
                    if let Some(content) = &mut item.element.content {
                        shift_levels(content, delta);
                    }
                }
            }
            Block::Paragraph(_) => {}
        }
    }
}
