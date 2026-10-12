//! The merge half of this crate's algorithm: walks `base`/`local`/`remote`
//! together and narrows a conflict as deep into the tree as it can before
//! giving up and embedding a marker (see the crate's module doc for the
//! full algorithm).

use tomet_ast::{
    Block, Document, Element, ElementValue, Entry, Inline, List, ListItem, Paragraph, Section,
    Value,
};

use crate::diff3;
use crate::markers::{coalesce_text, conflict_block, conflict_inline, push_text, value_conflict};

/// The outcome of merging one note across two divergent versions of a
/// vault.
#[derive(Debug, Clone, PartialEq)]
pub struct MergeResult {
    /// The merged document. Always a structurally valid Tomet document --
    /// conflicts are represented as `@conflict` markers, never as
    /// malformed or partial output.
    pub document: Document,
    /// Where merging succeeded without leaving a conflict marker behind.
    pub clean: bool,
}

/// Merges `local` and `remote`, both descendants of `base`, into one
/// document.
///
/// Never fails: a merge that cannot be resolved automatically produces a
/// document containing `@conflict` markers rather than an error. The
/// caller always has something to commit or save. `@conflict`
/// is `tomet`'s own bare `std` element, so nothing needs adding to the preamble for it.
pub fn merge(base: &Document, local: &Document, remote: &Document) -> MergeResult {
    let (blocks, clean) = merge_block_seq(&base.blocks, &local.blocks, &remote.blocks);

    MergeResult {
        document: Document {
            blocks,
            span: base.span,
        },
        clean,
    }
}

/// Diffs one `Vec<Block>` three ways, narrowing each conflict via
/// [`merge_one`] before falling back to one `@conflict` marker holding
/// both sides' blocks directly. Used for `Document.blocks` (top level),
/// an `Element`'s `children`, and now its `content` too (recursively,
/// from inside `merge_one` itself) -- all three are `Vec<Block>`.
fn merge_block_seq(base: &[Block], local: &[Block], remote: &[Block]) -> (Vec<Block>, bool) {
    let mut blocks = Vec::new();
    let mut clean = true;
    for region in diff3::merge3(base, local, remote) {
        match region {
            diff3::Region::Same(items) => blocks.extend(items),
            diff3::Region::Conflict { base, mine, theirs } => {
                let narrowed = match (base.as_slice(), mine.as_slice(), theirs.as_slice()) {
                    ([b], [m], [t]) => merge_one(b, m, t),
                    _ => None,
                };
                if let Some((block, sub_clean)) = narrowed {
                    blocks.push(block);
                    clean &= sub_clean;
                    continue;
                }
                clean = false;
                blocks.push(conflict_block(mine, theirs));
            }
        }
    }
    (blocks, clean)
}

/// Lifts a three-way merge function over `Option`, returning `None` if the
/// three options don't all agree on presence/absence (a shape mismatch the
/// merge algorithm can't narrow).
///
/// - All three `Some`: calls `f(b, l, r)` and wraps the result in `Some`.
/// - All three `None`: clean no-op, returns `Some((None, true))`.
/// - Any mix: returns `None` so the caller can bail out early.
fn merge_optional<T, F>(
    base: &Option<T>,
    local: &Option<T>,
    remote: &Option<T>,
    f: F,
) -> Option<(Option<T>, bool)>
where
    F: FnOnce(&T, &T, &T) -> Option<(T, bool)>,
{
    match (base, local, remote) {
        (Some(b), Some(l), Some(r)) => {
            let (merged, clean) = f(b, l, r)?;
            Some((Some(merged), clean))
        }
        (None, None, None) => Some((None, true)),
        _ => None,
    }
}

/// Attempts to narrow a conflict between one base block and the single
/// block each side made of it, instead of giving up on the whole block.
///
/// `None` means recursion can't help here -- the caller falls back to
/// wrapping the whole block in marker nodes. `Some((block, clean))`
/// means it produced a merged block; `clean` says whether that block
/// still had to embed its own (narrower) conflict markers.
fn merge_one(base: &Block, local: &Block, remote: &Block) -> Option<(Block, bool)> {
    match (base, local, remote) {
        (Block::Paragraph(b), Block::Paragraph(l), Block::Paragraph(r)) => {
            let (content, clean) = merge_inline_seq(&b.content, &l.content, &r.content);
            Some((Block::Paragraph(Paragraph::new(content, b.span)), clean))
        }
        (Block::Element(b), Block::Element(l), Block::Element(r)) => {
            let (element, clean) = merge_element(b, l, r)?;
            Some((Block::Element(element), clean))
        }
        (Block::List(b), Block::List(l), Block::List(r)) => {
            let (list, clean) = merge_list(b, l, r)?;
            Some((Block::List(list), clean))
        }
        (Block::Section(b), Block::Section(l), Block::Section(r))
            if b.level == l.level && l.level == r.level =>
        {
            // Neither `connects` nor `id` is diffed -- bail rather than
            // silently keep `local`'s and drop a real difference on the
            // floor.
            if l.connects != r.connects || l.id != r.id {
                return None;
            }

            let (title, title_clean) = merge_inline_seq(&b.title, &l.title, &r.title);
            let (args, args_clean) = merge_optional(&b.args, &l.args, &r.args, |ba, la, ra| {
                let (merged, clean) = merge_value(Some(ba), la, ra);
                Some((merged, clean))
            })?;
            let (blocks, blocks_clean) = merge_block_seq(&b.blocks, &l.blocks, &r.blocks);
            let (value, value_clean) =
                merge_optional(&b.value, &l.value, &r.value, |bv, lv, rv| {
                    merge_element_value(bv, lv, rv)
                })?;

            Some((
                Block::Section(Section {
                    level: l.level,
                    title,
                    args,
                    value,
                    id: l.id.clone(),
                    connects: l.connects.clone(),
                    blocks,
                    span: l.span,
                }),
                title_clean && args_clean && blocks_clean && value_clean,
            ))
        }
        _ => None,
    }
}

fn merge_element(b: &Element, l: &Element, r: &Element) -> Option<(Element, bool)> {
    if b.sigil != l.sigil || l.sigil != r.sigil {
        return None;
    }
    // `connects` isn't diffed -- bail rather than silently keep
    // `local`'s and drop a real difference on the floor.
    if l.connects != r.connects {
        return None;
    }

    let (args, args_clean) = merge_optional(&b.args, &l.args, &r.args, |ba, la, ra| {
        let (merged, clean) = merge_value(Some(ba), la, ra);
        Some((merged, clean))
    })?;
    let (content, content_clean) =
        merge_optional(&b.content, &l.content, &r.content, |bc, lc, rc| {
            let (merged, clean) = merge_block_seq(bc, lc, rc);
            Some((merged, clean))
        })?;
    let (value, value_clean) = merge_optional(&b.value, &l.value, &r.value, |bv, lv, rv| {
        merge_element_value(bv, lv, rv)
    })?;

    Some((
        Element {
            args,
            content,
            value,
            ..l.clone()
        },
        args_clean && content_clean && value_clean,
    ))
}

fn merge_list(b: &List, l: &List, r: &List) -> Option<(List, bool)> {
    if b.ordered != l.ordered || l.ordered != r.ordered {
        return None;
    }
    if l.connects != r.connects || l.id != r.id {
        return None;
    }
    let (items, clean) = merge_list_item_seq(&b.items, &l.items, &r.items)?;
    Some((
        List {
            ordered: l.ordered,
            items,
            id: l.id.clone(),
            connects: l.connects.clone(),
            span: b.span,
        },
        clean,
    ))
}

fn merge_list_item(b: &ListItem, l: &ListItem, r: &ListItem) -> Option<(ListItem, bool)> {
    let (element, el_clean) = merge_element(&b.element, &l.element, &r.element)?;
    let (sublist, sub_clean) = merge_optional(&b.sublist, &l.sublist, &r.sublist, |sb, sl, sr| {
        merge_list(sb, sl, sr)
    })?;
    Some((ListItem { element, sublist }, el_clean && sub_clean))
}

fn merge_list_item_seq(
    base: &[ListItem],
    local: &[ListItem],
    remote: &[ListItem],
) -> Option<(Vec<ListItem>, bool)> {
    let mut items = Vec::new();
    let mut clean = true;
    for region in diff3::merge3(base, local, remote) {
        match region {
            diff3::Region::Same(same_items) => items.extend(same_items),
            diff3::Region::Conflict { base, mine, theirs } => {
                let narrowed = match (base.as_slice(), mine.as_slice(), theirs.as_slice()) {
                    ([b], [m], [t]) => merge_list_item(b, m, t),
                    _ => None,
                }?;
                items.push(narrowed.0);
                clean &= narrowed.1;
            }
        }
    }
    Some((items, clean))
}

/// Narrows a conflict inside an element's `{data}` group, by key rather
/// than by position (so reordered-but-otherwise-untouched keys never
/// register as a conflict). `None` when either side's group holds a
/// nested element rather than only `key: value` pairs -- narrowing a
/// conflict *between* interleaved pairs and nested elements would need
/// its own representation for "a conflict sitting where an element
/// could be", which is a fourth placement context for `@mobile.conflict`
/// and hasn't been designed.
fn merge_element_value(
    base: &ElementValue,
    local: &ElementValue,
    remote: &ElementValue,
) -> Option<(ElementValue, bool)> {
    let (ElementValue::Group(b), ElementValue::Group(l), ElementValue::Group(r)) =
        (base, local, remote)
    else {
        return None;
    };

    let only_pairs = |entries: &[Entry]| -> Option<Vec<(String, Value)>> {
        entries
            .iter()
            .map(|e| match e {
                Entry::Pair(k, v) => Some((k.clone(), v.clone())),
                Entry::Element(_) => None,
            })
            .collect()
    };
    let (base_pairs, local_pairs, remote_pairs) = (only_pairs(b)?, only_pairs(l)?, only_pairs(r)?);

    let (merged, clean) = merge_value_map(&base_pairs, &local_pairs, &remote_pairs);
    let Value::Map(entries) = merged else {
        unreachable!("merge_value_map always returns a Value::Map")
    };
    Some((
        ElementValue::Group(
            entries
                .into_iter()
                .map(|(k, v)| Entry::Pair(k, v))
                .collect(),
        ),
        clean,
    ))
}

/// Three-way merge of one value, recursing into `Value::Map` (by key) and
/// `Value::Seq` (positionally); a leaf-level (or shape-mismatched)
/// disagreement embeds `@mobile.conflict(mine: ..., theirs: ...)` as
/// the value itself, using `Value::Element` -- a value slot can hold an
/// arbitrary element, so the same element name used as a block/inline
/// marker elsewhere does double duty here as data instead.
///
/// `base: None` means there is nothing to compare against (the key, or
/// this whole value, was added independently by both sides) -- treated
/// as an empty `Map`/`Seq` rather than failing, so two sides adding the
/// same key with compatible sub-structure can still merge below this
/// level instead of conflicting outright.
fn merge_value(base: Option<&Value>, local: &Value, remote: &Value) -> (Value, bool) {
    if local == remote {
        return (local.clone(), true);
    }
    if base == Some(local) {
        return (remote.clone(), true);
    }
    if base == Some(remote) {
        return (local.clone(), true);
    }

    match (local, remote) {
        (Value::Map(l), Value::Map(r)) => {
            let b = match base {
                Some(Value::Map(b)) => b.as_slice(),
                _ => &[],
            };
            merge_value_map(b, l, r)
        }
        (Value::Seq(l), Value::Seq(r)) => {
            let b = match base {
                Some(Value::Seq(b)) => b.as_slice(),
                _ => &[],
            };
            merge_value_seq(b, l, r)
        }
        _ => (value_conflict(local.clone(), remote.clone()), false),
    }
}

fn merge_value_map(
    base: &[(String, Value)],
    local: &[(String, Value)],
    remote: &[(String, Value)],
) -> (Value, bool) {
    fn lookup<'a>(entries: &'a [(String, Value)], key: &str) -> Option<&'a Value> {
        entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    // Local's key order first, then any key remote alone introduced --
    // an arbitrary but deterministic choice; `{data}` order isn't
    // semantically load-bearing the way block order is.
    let mut order: Vec<&str> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for (k, _) in local.iter().chain(remote.iter()) {
        if seen.insert(k.as_str()) {
            order.push(k.as_str());
        }
    }

    let mut out = Vec::new();
    let mut clean = true;
    for key in order {
        let (b, l, r) = (lookup(base, key), lookup(local, key), lookup(remote, key));
        match (l, r) {
            (Some(lv), None) => {
                // remote deleted `key`; keep it only if local actually
                // changed it -- an unmodified key gives way to the
                // other side's deletion instead of resurrecting it.
                if b != Some(lv) {
                    out.push((key.to_string(), lv.clone()));
                }
            }
            (None, Some(rv)) => {
                if b != Some(rv) {
                    out.push((key.to_string(), rv.clone()));
                }
            }
            (None, None) => unreachable!("key came from local or remote"),
            (Some(lv), Some(rv)) => {
                let (merged, sub_clean) = merge_value(b, lv, rv);
                clean &= sub_clean;
                out.push((key.to_string(), merged));
            }
        }
    }
    (Value::Map(out), clean)
}

fn merge_value_seq(base: &[Value], local: &[Value], remote: &[Value]) -> (Value, bool) {
    let mut out = Vec::new();
    let mut clean = true;
    for region in diff3::merge3(base, local, remote) {
        match region {
            diff3::Region::Same(items) => out.extend(items),
            diff3::Region::Conflict { base, mine, theirs } => {
                if let ([b], [l], [r]) = (base.as_slice(), mine.as_slice(), theirs.as_slice()) {
                    let (merged, sub_clean) = merge_value(Some(b), l, r);
                    out.push(merged);
                    clean &= sub_clean;
                    continue;
                }
                clean = false;
                out.push(value_conflict(Value::Seq(mine), Value::Seq(theirs)));
            }
        }
    }
    (Value::Seq(out), clean)
}

/// The inline-level counterpart of `merge`'s main loop: diffs one
/// `Vec<Inline>` three ways and always succeeds, embedding
/// `@mobile.conflict` markers (as inline nodes) around anything it can't
/// resolve rather than failing outward to a whole-block conflict.
///
/// A run of plain prose parses to one `Inline::Text` covering the whole
/// run, so without a further step, editing any word anywhere in a
/// sentence would make the entire sentence one non-matching item and
/// conflict outright. When a conflict region narrows to exactly one
/// `Inline::Text` per side, [`merge_text`] recurses once more, character
/// by character, which is what actually lets two edits to different
/// words in the same sentence merge silently.
fn merge_inline_seq(base: &[Inline], local: &[Inline], remote: &[Inline]) -> (Vec<Inline>, bool) {
    let mut out = Vec::new();
    let mut clean = true;
    for region in diff3::merge3(base, local, remote) {
        match region {
            diff3::Region::Same(items) => out.extend(items),
            diff3::Region::Conflict { base, mine, theirs } => {
                let narrowed = match (base.as_slice(), mine.as_slice(), theirs.as_slice()) {
                    ([Inline::Text(b)], [Inline::Text(l)], [Inline::Text(r)]) => {
                        Some(merge_text(&b.value, &l.value, &r.value))
                    }
                    _ => None,
                };
                if let Some((items, sub_clean)) = narrowed {
                    out.extend(items);
                    clean &= sub_clean;
                    continue;
                }
                clean = false;
                out.push(conflict_inline(mine, theirs));
            }
        }
    }
    (coalesce_text(out), clean)
}

/// Character-level three-way merge of one `Inline::Text` run, the bottom
/// of the recursion: nothing below a character can narrow a conflict any
/// further.
fn merge_text(base: &str, local: &str, remote: &str) -> (Vec<Inline>, bool) {
    let (b, l, r): (Vec<char>, Vec<char>, Vec<char>) = (
        base.chars().collect(),
        local.chars().collect(),
        remote.chars().collect(),
    );

    let mut out = Vec::new();
    let mut clean = true;
    for region in diff3::merge3(&b, &l, &r) {
        match region {
            diff3::Region::Same(chars) => push_text(&mut out, chars),
            diff3::Region::Conflict { mine, theirs, .. } => {
                clean = false;
                let mut mine_run = Vec::new();
                push_text(&mut mine_run, mine);
                let mut theirs_run = Vec::new();
                push_text(&mut theirs_run, theirs);
                out.push(conflict_inline(mine_run, theirs_run));
            }
        }
    }
    (coalesce_text(out), clean)
}
