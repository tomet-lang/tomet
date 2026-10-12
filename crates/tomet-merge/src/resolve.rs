//! Strips `@conflict` markers back out of a merged document, once the
//! user (or an automatic policy) has picked a side for each one. The
//! counterpart to [`crate::merge`]: it walks the tree in the exact same
//! order merging produced markers in, so resolutions line up with the
//! right conflict.

use tomet_ast::{
    Block, Document, Element, ElementValue, Entry, Inline, List, ListItem, Paragraph, Text, Value,
};

use crate::markers::{
    coalesce_text, conflict_block_sides, conflict_inline_sides, value_conflict_sides,
};

/// Which side of one `@conflict` a caller picked, or that they edited a
/// fresh replacement by hand, or kept both versions. `A`/`B` match
/// `@conflict`'s own structural, non-perspectival names -- this crate
/// never decides which one is "mine"; a caller that wants to offer a
/// mine/theirs choice derives which of `A`/`B` that means itself (such as
/// in a UI resolution module) and picks accordingly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictResolution {
    A,
    B,
    Both,
    /// The user wrote something new that is neither side verbatim.
    Rewritten(String),
}

/// Removes every `@conflict` marker from `document`, keeping the side
/// the caller picked for each.
///
/// `resolutions` are consumed in document order, one per `@conflict`
/// found. A conflict with no corresponding entry (`resolutions` ran out)
/// is left in the document untouched.
pub fn resolve(document: &Document, resolutions: &[ConflictResolution]) -> Document {
    apply_resolutions(document, &mut resolutions.iter().map(Some))
}

/// Removes every `@conflict` marker from `document`, resolving all
/// conflicts to the specified side (`ConflictResolution::A` or
/// `ConflictResolution::B`).
pub fn resolve_all(document: &Document, resolution: ConflictResolution) -> Document {
    apply_resolutions(document, &mut std::iter::repeat(Some(&resolution)))
}

/// Resolves a single conflict at `target_index` to the given `resolution`,
/// leaving all other conflicts untouched.
pub fn resolve_single(
    document: &Document,
    target_index: usize,
    resolution: ConflictResolution,
) -> Document {
    let mut idx = 0;
    apply_resolutions(
        document,
        &mut std::iter::from_fn(|| {
            let res = if idx == target_index {
                Some(&resolution)
            } else {
                None
            };
            idx += 1;
            Some(res)
        }),
    )
}

/// Shared implementation for [`resolve`], [`resolve_all`] and [`resolve_single`]:
/// walks the document applying each resolution from `resolutions` in order.
fn apply_resolutions<'a>(
    document: &Document,
    resolutions: &mut impl Iterator<Item = Option<&'a ConflictResolution>>,
) -> Document {
    let blocks = resolve_block_seq(&document.blocks, resolutions);
    Document {
        blocks,
        span: document.span,
    }
}

/// The block-sequence counterpart of `merge_block_seq`: replaces each
/// `@conflict` with the chosen side's blocks, and recurses into every
/// other block via [`resolve_block`]. Used for `Document.blocks`, an
/// `Element`'s `children`, and its `content` -- resolving must walk the
/// tree in the same order merging did, or a resolution would apply to
/// the wrong conflict.
fn resolve_block_seq<'a>(
    blocks: &[Block],
    resolutions: &mut impl Iterator<Item = Option<&'a ConflictResolution>>,
) -> Vec<Block> {
    let mut out = Vec::new();
    for block in blocks {
        let Block::Element(el) = block else {
            out.push(resolve_block(block, resolutions));
            continue;
        };
        let Some((a, b)) = conflict_block_sides(el) else {
            out.push(resolve_block(block, resolutions));
            continue;
        };

        match resolutions.next() {
            Some(Some(ConflictResolution::A)) => out.extend(a),
            Some(Some(ConflictResolution::B)) => out.extend(b),
            Some(Some(ConflictResolution::Both)) => {
                out.extend(a);
                out.extend(b);
            }
            Some(Some(ConflictResolution::Rewritten(text))) => {
                out.push(Block::Paragraph(Paragraph::new(
                    vec![Inline::Text(Text::from(text.as_str()))],
                    Default::default(),
                )))
            }
            Some(None) | None => out.push(block.clone()),
        }
    }
    out
}

/// Resolves every kind of `@conflict` a single block might carry: an
/// inline marker in a `Paragraph`'s or `Element`'s content, and a
/// `@conflict(a: ..., b: ...)` value sitting in `args` or `{data}`.
/// Visits args/content/value in that order -- the same order
/// `merge_one` recurses in -- so resolutions line up with the conflicts
/// `merge` produced.
fn resolve_block<'a>(
    block: &Block,
    resolutions: &mut impl Iterator<Item = Option<&'a ConflictResolution>>,
) -> Block {
    match block {
        Block::Paragraph(p) => Block::Paragraph(Paragraph::new(
            resolve_inline_seq(&p.content, resolutions),
            p.span,
        )),
        Block::Element(e) => Block::Element(resolve_element(e, resolutions)),
        Block::List(l) => Block::List(resolve_list(l, resolutions)),
        Block::Section(sec) => {
            let mut s = sec.clone();
            s.title = resolve_inline_seq(&s.title, resolutions);
            if let Some(args) = &s.args {
                s.args = Some(resolve_value(args, resolutions));
            }
            s.blocks = resolve_block_seq(&s.blocks, resolutions);
            if let Some(value) = &s.value {
                s.value = Some(resolve_element_value(value, resolutions));
            }
            Block::Section(s)
        }
    }
}

fn resolve_element<'a>(
    e: &Element,
    resolutions: &mut impl Iterator<Item = Option<&'a ConflictResolution>>,
) -> Element {
    let mut e = e.clone();
    if let Some(args) = &e.args {
        e.args = Some(resolve_value(args, resolutions));
    }
    if let Some(content) = &e.content {
        e.content = Some(resolve_block_seq(content, resolutions));
    }
    if let Some(value) = &e.value {
        e.value = Some(resolve_element_value(value, resolutions));
    }
    e
}

fn resolve_list<'a>(
    l: &List,
    resolutions: &mut impl Iterator<Item = Option<&'a ConflictResolution>>,
) -> List {
    let mut l = l.clone();
    l.items = l
        .items
        .iter()
        .map(|item| ListItem {
            element: resolve_element(&item.element, resolutions),
            sublist: item
                .sublist
                .as_ref()
                .map(|sub| resolve_list(sub, resolutions)),
        })
        .collect();
    l
}

/// Resolves a `@conflict(a, b)` sitting where a value goes, recursing
/// into `Value::Map`/`Value::Seq` to find one nested arbitrarily deep.
/// The order this walks a `Map`'s entries matches the order
/// `merge_value_map` built them in, since it visits the *merged*
/// document's own entries in the order they already sit in.
fn resolve_value<'a>(
    value: &Value,
    resolutions: &mut impl Iterator<Item = Option<&'a ConflictResolution>>,
) -> Value {
    if let Some((a, b)) = value_conflict_sides(value) {
        return match resolutions.next() {
            Some(Some(ConflictResolution::A)) => a.clone(),
            Some(Some(ConflictResolution::B)) => b.clone(),
            Some(Some(ConflictResolution::Both)) => match (a, b) {
                (Value::String(m), Value::String(t)) => Value::String(format!("{m} / {t}")),
                _ => a.clone(),
            },
            Some(Some(ConflictResolution::Rewritten(text))) => Value::String(text.clone()),
            Some(None) | None => value.clone(),
        };
    }
    match value {
        Value::Map(entries) => Value::Map(
            entries
                .iter()
                .map(|(k, v)| (k.clone(), resolve_value(v, resolutions)))
                .collect(),
        ),
        Value::Seq(items) => Value::Seq(
            items
                .iter()
                .map(|v| resolve_value(v, resolutions))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn resolve_element_value<'a>(
    value: &ElementValue,
    resolutions: &mut impl Iterator<Item = Option<&'a ConflictResolution>>,
) -> ElementValue {
    let ElementValue::Group(entries) = value else {
        return value.clone();
    };
    ElementValue::Group(
        entries
            .iter()
            .map(|e| match e {
                Entry::Pair(k, v) => Entry::Pair(k.clone(), resolve_value(v, resolutions)),
                // Groups mixing pairs with nested elements were never
                // narrowed by merge_element_value, so there is nothing
                // here for a resolution to apply to.
                Entry::Element(el) => Entry::Element(el.clone()),
            })
            .collect(),
    )
}

/// The inline-level counterpart of `resolve`'s main loop.
fn resolve_inline_seq<'a>(
    content: &[Inline],
    resolutions: &mut impl Iterator<Item = Option<&'a ConflictResolution>>,
) -> Vec<Inline> {
    let mut out = Vec::new();
    for inline in content {
        let Inline::Element(el) = inline else {
            out.push(inline.clone());
            continue;
        };
        let Some((a, b)) = conflict_inline_sides(el) else {
            out.push(inline.clone());
            continue;
        };

        match resolutions.next() {
            Some(Some(ConflictResolution::A)) => out.extend(a),
            Some(Some(ConflictResolution::B)) => out.extend(b),
            Some(Some(ConflictResolution::Both)) => {
                out.extend(a);
                out.extend(b);
            }
            Some(Some(ConflictResolution::Rewritten(text))) => {
                out.push(Inline::Text(Text::from(text.as_str())))
            }
            Some(None) | None => out.push(inline.clone()),
        }
    }
    coalesce_text(out)
}
