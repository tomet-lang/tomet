//! Construction and detection of `@conflict` marker elements, and the
//! small text-building helpers shared by [`crate::merge`], [`crate::resolve`]
//! and [`crate::find_conflicts`]. Nothing here walks a whole document --
//! that's each of those modules' own job -- this only knows what a single
//! marker or value looks like.
//!
//! `@conflict` is `tomet`'s own std element now (bare, no `@use`), with
//! `a`/`b` holding real block content directly (`Value::Blocks`) rather
//! than the three positional range markers (`mine`/`theirs`/`end`) this
//! crate used to scatter through the block/inline sequence under the
//! retired `@mobile.conflict` name -- see the crate's module doc for the
//! shape this replaced and why.

use tomet_ast::{
    Block, Document, Element, Inline, Name, Paragraph, Placement, Sigil, Span, Text, Value,
};

/// The raw-text substring other crates can look for without parsing a
/// document -- a cheap "does this note have a conflict in it at all"
/// check over file content read straight off disk.
pub const CONFLICT_MARKER: &str = "@conflict";

/// Returns `Name` for `@conflict`, used in every place this crate creates
/// or inspects a conflict marker element. Bare: `std`'s own name, not a
/// namespace this crate owns.
fn conflict_name() -> Name {
    Name::bare("conflict")
}

/// Whether `el` is a `@conflict` element -- `std`'s own, not a
/// namespaced look-alike (`@ns.conflict` is someone else's name, not
/// this crate's).
pub(crate) fn is_conflict_element(el: &Element) -> bool {
    matches!(&el.sigil, Sigil::Named(name) if name.namespace.is_none() && name.name == "conflict")
}

pub(crate) fn is_conflict_block(block: &Block) -> bool {
    matches!(block, Block::Element(el) if is_conflict_element(el))
}

pub(crate) fn inline_is_conflict(inline: &Inline) -> bool {
    matches!(inline, Inline::Element(el) if is_conflict_element(el))
}

fn conflict_args(a: Value, b: Value) -> Value {
    Value::Map(vec![("a".to_string(), a), ("b".to_string(), b)])
}

/// A `@conflict(a: ..., b: ...)` standing as its own block, `a`/`b`
/// holding real block content (`merge`'s block-level fallback, and an
/// element's `[content]`/`children` merged the same way).
pub(crate) fn conflict_block(a: Vec<Block>, b: Vec<Block>) -> Block {
    Block::Element(Element {
        sigil: Sigil::Named(conflict_name()),
        placement: Placement::Block,
        args: Some(conflict_args(Value::Blocks(a), Value::Blocks(b))),
        ..Default::default()
    })
}

/// The inline-level counterpart of [`conflict_block`] -- `a`/`b` still
/// hold `Value::Blocks` (the one shape `@conflict`'s `(args)` takes
/// regardless of where the element itself sits), each wrapped in a
/// single `Paragraph` so a run of `Inline`s fits the same grammar.
pub(crate) fn conflict_inline(a: Vec<Inline>, b: Vec<Inline>) -> Inline {
    Inline::Element(Element {
        sigil: Sigil::Named(conflict_name()),
        placement: Placement::Inline,
        args: Some(conflict_args(
            Value::Blocks(wrap_inline(a)),
            Value::Blocks(wrap_inline(b)),
        )),
        ..Default::default()
    })
}

fn wrap_inline(inlines: Vec<Inline>) -> Vec<Block> {
    vec![Block::Paragraph(Paragraph::new(inlines, Span::default()))]
}

fn unwrap_inline(blocks: Vec<Block>) -> Vec<Inline> {
    match blocks.into_iter().next() {
        Some(Block::Paragraph(p)) => p.content,
        _ => Vec::new(),
    }
}

/// `el`'s `a`/`b`, as block content -- for a `@conflict` sitting at the
/// block or inline level (`el.placement` says which; both shapes carry
/// `Value::Blocks` in `(args)` the same way).
pub(crate) fn conflict_block_sides(el: &Element) -> Option<(Vec<Block>, Vec<Block>)> {
    if !is_conflict_element(el) {
        return None;
    }
    let Value::Map(entries) = el.args.as_ref()? else {
        return None;
    };
    let get = |key: &str| {
        entries
            .iter()
            .find(|(k, _)| k == key)
            .and_then(|(_, v)| match v {
                Value::Blocks(blocks) => Some(blocks.clone()),
                _ => None,
            })
    };
    Some((get("a")?, get("b")?))
}

/// [`conflict_block_sides`], unwrapped back to a flat `Inline` run each --
/// for a `@conflict` sitting inline, where `a`/`b`'s one `Paragraph` of
/// content ([`conflict_inline`] built it) is what the caller actually
/// wants spliced back into the surrounding content.
pub(crate) fn conflict_inline_sides(el: &Element) -> Option<(Vec<Inline>, Vec<Inline>)> {
    let (a, b) = conflict_block_sides(el)?;
    Some((unwrap_inline(a), unwrap_inline(b)))
}

/// `value`'s `a`/`b`, if it is a `@conflict(a: ..., b: ...)` marker
/// sitting where a value goes -- a leaf-level `{data}`/`(args)` value
/// both sides changed differently. A different shape from the block/
/// inline markers above: there, `a`/`b` are `Value::Blocks`; here they
/// are plain scalars (or whatever structured `Value` the disputed data
/// itself was), since a value conflict has no block content to hold in
/// the first place.
pub(crate) fn value_conflict_sides(value: &Value) -> Option<(&Value, &Value)> {
    let Value::Element(el) = value else {
        return None;
    };
    if !is_conflict_element(el) {
        return None;
    }
    let Value::Map(entries) = el.args.as_ref()? else {
        return None;
    };
    let get = |key: &str| entries.iter().find(|(k, _)| k == key).map(|(_, v)| v);
    Some((get("a")?, get("b")?))
}

pub(crate) fn value_conflict(a: Value, b: Value) -> Value {
    Value::Element(Box::new(Element {
        sigil: Sigil::Named(conflict_name()),
        args: Some(conflict_args(a, b)),
        ..Default::default()
    }))
}

pub(crate) fn push_text(out: &mut Vec<Inline>, chars: Vec<char>) {
    if !chars.is_empty() {
        out.push(Inline::Text(Text::from(
            chars.into_iter().collect::<String>(),
        )));
    }
}

/// Merges adjacent `Inline::Text` runs into one. Diffing and resolving
/// both build their output as separate pieces (a `Same` run here, an
/// `a`/`b` pick there), which otherwise leaves needlessly fragmented
/// text behind even when nothing about the *content* disagrees.
pub(crate) fn coalesce_text(items: Vec<Inline>) -> Vec<Inline> {
    let mut out: Vec<Inline> = Vec::with_capacity(items.len());
    for item in items {
        match item {
            Inline::Text(t) => match out.last_mut() {
                Some(Inline::Text(prev)) => prev.value.push_str(&t.value),
                _ => out.push(Inline::Text(t)),
            },
            other => out.push(other),
        }
    }
    out
}

pub(crate) fn inlines_to_text(inlines: &[Inline]) -> String {
    let mut s = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => s.push_str(&t.value),
            Inline::Raw(r) => s.push_str(&r.value),
            Inline::SoftBreak(_) => s.push(' '),
            Inline::LineBreak(_) => s.push('\n'),
            Inline::Element(el) => {
                let doc = Document {
                    blocks: vec![Block::Element(el.clone())],
                    span: Default::default(),
                };
                s.push_str(tomet_printer::document_to_tm(&doc).trim());
            }
        }
    }
    s
}

/// `blocks` printed back to `.tmt` source, for [`crate::find_conflicts`]'s
/// `ConflictItem::a`/`b` -- a block-level conflict's sides are real
/// `Vec<Block>` now, not a flat inline run, so showing them as text goes
/// through the printer rather than [`inlines_to_text`].
pub(crate) fn blocks_to_text(blocks: &[Block]) -> String {
    let doc = Document {
        blocks: blocks.to_vec(),
        span: Default::default(),
    };
    tomet_printer::document_to_tm(&doc).trim().to_string()
}

pub(crate) fn value_to_text(val: &Value) -> String {
    match val {
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Null => "null".to_string(),
        _ => format!("{val:?}"),
    }
}
