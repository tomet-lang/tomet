//! `include(target)`: another document's content, put where the call is.
//!
//! The target is a document (its body) or an element or section found by id
//! (that node, a section with everything under it). The result is a
//! fragment, built in four steps:
//!
//! 1. the selection is read as written, minus the declarations a document
//!    makes about itself (`@meta`, `@config`, `@kind`, ...);
//! 2. it is prepared **in its own file's context** -- its expressions, its
//!    macros, its `self` -- by the resolver, so it means here what it means
//!    there;
//! 3. its headings are moved to the level they have where they land;
//! 4. its relative links (`./`, `../`) are rewritten to still reach the same
//!    file from the including one.
//!
//! A cycle, or a chain deeper than [`MAX_INCLUDE_DEPTH`], is an error.

use tomet_ast::{Block, Document, Element, Inline, Placement, Span, Value};
use tomet_semantics::{ElementKind, classify_std_lenient, link_target_of};
use tomet_tree::for_each_element_mut;

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
    rebase_links(&mut blocks, &doc, &cx.self_doc);
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
/// not go along when its body is included.
fn is_declaration(element: &Element) -> bool {
    matches!(
        classify_std_lenient(element),
        ElementKind::Kind
            | ElementKind::Version
            | ElementKind::Meta
            | ElementKind::Config
            | ElementKind::Settings
            | ElementKind::Use
            | ElementKind::Blueprint
    )
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

// ---- relative links ---------------------------------------------------------

/// The directory part of a vault-relative path, as components.
fn directory(doc: &DocRef) -> Vec<&str> {
    let mut parts: Vec<&str> = doc.0.split('/').filter(|p| !p.is_empty()).collect();
    parts.pop();
    parts
}

/// Joins `relative` onto `base` and resolves `.` and `..`. `None` when it
/// climbs out of the vault.
fn resolve(base: &[&str], relative: &str) -> Option<Vec<String>> {
    let mut out: Vec<String> = base.iter().map(|p| p.to_string()).collect();
    for part in relative.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                out.pop()?;
            }
            other => out.push(other.to_string()),
        }
    }
    Some(out)
}

/// `target` spelled relative to `from`, both as components from the vault root.
fn relative_to(from: &[&str], target: &[String]) -> String {
    let common = from
        .iter()
        .zip(target)
        .take_while(|(a, b)| **a == b.as_str())
        .count();
    let ups = from.len() - common;
    let rest = target[common..].join("/");
    if ups == 0 {
        format!("./{rest}")
    } else {
        format!("{}{rest}", "../".repeat(ups))
    }
}

/// Rewrites the `./` and `../` targets of links and embeds in `blocks`, which
/// were written in `from`, so that they reach the same files from `to`.
/// `tmt:`, `ref:`, `https:` and the rest name a file the same way anywhere
/// and are left alone.
fn rebase_links(blocks: &mut Vec<Block>, from: &DocRef, to: &DocRef) {
    let (from_dir, to_dir) = (directory(from), directory(to));
    if from_dir == to_dir {
        return;
    }
    let mut holder = Document::new(std::mem::take(blocks), Span::default());
    for_each_element_mut(&mut holder, |element| {
        let Some((_, target)) = link_target_of(element) else {
            return;
        };
        if !(target.starts_with("./") || target.starts_with("../")) {
            return;
        }
        let (path, anchor) = match target.find('#') {
            Some(at) => target.split_at(at),
            None => (target.as_str(), ""),
        };
        let Some(resolved) = resolve(&from_dir, path) else {
            return;
        };
        set_target(
            element,
            format!("{}{anchor}", relative_to(&to_dir, &resolved)),
        );
    });
    *blocks = holder.blocks;
}

/// Writes a link's target back, whether it was given as the one argument or
/// as `target:`.
fn set_target(element: &mut Element, target: String) {
    match &mut element.args {
        Some(Value::String(existing)) => *existing = target,
        Some(Value::Map(entries)) => {
            if let Some((_, value)) = entries.iter_mut().find(|(key, _)| key == "target") {
                *value = Value::String(target);
            }
        }
        _ => {}
    }
}
