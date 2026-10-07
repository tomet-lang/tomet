//! Parsing for list items (`-`, `-.`).
//!
//! A list marker's optional bracket form is `(...)`: a real `Value` parsed
//! with the same grammar as an element's `(args)` (see
//! `crate::element::parse_paren_value`), normalized against the builtin
//! `"marker"` positional key by `tomet-semantics::positional`.

use crate::element::{parse_paren_value, parse_sugar_body};
use crate::error::Result;
use crate::inline::{Stop, extend_merging, parse_inline_seq, push_text};
use crate::section::merge_values;
use crate::value::skip_inline_ws;
use tomet_ast::{Block, Element, Paragraph, Sigil, Value};
use tomet_lexer::Cursor;
use tomet_tree::{element_list, element_list_item, element_new};

/// The group openers left once an item's `(marker)` has been taken.
///
/// A second `(` would be a duplicate `args` group, which `parse_groups`
/// refuses, so the two places that ask after the marker ask for this
/// rather than for [`opens_group`]. Derived from it, so a fifth opener
/// reaches both without being spelled again.
fn opens_group_after_args(cur: &Cursor) -> bool {
    crate::element::opens_group(cur) && cur.peek() != Some('(')
}

/// What reading a list marker found.
///
/// `full_form` used to be re-derived by `parse_list_internal` peeking at
/// the same position a second time. It is answered here, where the marker
/// was read and where whether its `(...)` was taken is still known, and
/// handed over rather than asked again.
pub(crate) struct ListMarker {
    /// Columns of indentation before the `-`.
    pub indent: usize,
    /// `-.` rather than `-`.
    pub ordered: bool,
    /// The `(marker)` value, when the item carries one.
    pub marker: Option<Value>,
    /// A group opens on the marker, so the item takes the full
    /// `()[]{}` form rather than the one-line sugar.
    pub full_form: bool,
}

pub(crate) fn eat_list_marker(cur: &mut Cursor) -> Result<Option<ListMarker>> {
    let mut look = *cur;
    let mut indent = 0;
    while matches!(look.peek(), Some(' ') | Some('\t')) {
        if look.peek() == Some('\t') {
            indent += 4;
        } else {
            indent += 1;
        }
        look.bump();
    }
    if look.peek() != Some('-') {
        return Ok(None);
    }
    look.bump();
    let ordered = if look.peek() == Some('.') {
        look.bump();
        true
    } else {
        false
    };

    let has_ws = matches!(look.peek(), Some(' ') | Some('\t'));
    skip_inline_ws(&mut look);

    let mut marker = None;
    let mut found_group = false;

    if look.peek() == Some('(') {
        let mut probe = look;
        let value = parse_paren_value(&mut probe)?;
        // `- (x) content` and `- (x)[ content ]` are the same element with
        // the same args; only the sugar needs a space to separate the
        // marker from the text that follows it. `|` joins the group
        // openers because it is one -- `[content]` without the brackets.
        if matches!(probe.peek(), Some(' ') | Some('\t')) || opens_group_after_args(&probe) {
            marker = Some(value);
            found_group = true;
            look = probe;
        }
    }

    // A group opening directly on the marker is the full form, and needs
    // no space in front of it -- `^[ x ]` and `@name[ x ]` read that way,
    // and a list item is the same element.
    //
    // Only `(` used to count, because the flag was set solely by the
    // branch above: `-()[ x ]` was an item and `-[ x ]` was a paragraph.
    // That is the asymmetry `2a99315` set out to remove and reached only
    // halfway -- it unified how the groups are *read* (`parse_groups`) and
    // left how the marker is *recognized* where it was.
    //
    // `(` belongs here too. The probe above has already tried it and
    // declined it *as a marker*; that says nothing about whether it opens
    // this item's `args`, which is what `^(id: a)` and `@memo(x: 1)` do
    // with the same characters.
    if !found_group && crate::element::opens_group(&look) {
        found_group = true;
    }

    if !found_group && !has_ws {
        return Ok(None);
    }

    skip_inline_ws(&mut look);
    cur.set_pos(look.pos());
    // Once the probe has taken a `(marker)`, a second `(` would be a
    // duplicate `args` group; until then it opens the first one.
    let full_form = if marker.is_some() {
        opens_group_after_args(&look)
    } else {
        crate::element::opens_group(&look)
    };
    Ok(Some(ListMarker {
        indent,
        ordered,
        marker,
        full_form,
    }))
}

pub(crate) fn peek_list_marker(cur: &Cursor) -> Result<Option<ListMarker>> {
    let mut look = *cur;
    eat_list_marker(&mut look)
}

pub(crate) fn parse_list(cur: &mut Cursor, ordered: bool) -> Result<Vec<Element>> {
    parse_list_internal(cur, ordered, 0)
}

fn parse_list_internal(cur: &mut Cursor, ordered: bool, min_indent: usize) -> Result<Vec<Element>> {
    let mut items = Vec::new();
    while let Some(head) = peek_list_marker(cur)? {
        if head.indent < min_indent || head.ordered != ordered {
            break;
        }
        let item = parse_single_list_item(cur, head)?;
        items.push(item);
    }
    Ok(items)
}

pub(crate) fn parse_list_item_body(
    cur: &mut Cursor,
    head: &ListMarker,
    pipe_stack: &[usize],
) -> Result<Element> {
    let mut marker = head.marker.clone();
    let item_start = cur.pos();
    eat_list_marker(cur)?;

    let (content, attrs, id) = if head.full_form {
        // The full form: `- ()[ content ]{value}`. Groups are read by
        // the same code that reads `@name`'s, so `[content]` stops at
        // its closing bracket and `|content` at the end of its marked
        // run -- either may span lines. Only the bracket-less sugar
        // below is one line, and it is one line because it has no
        // group to close rather than because spreading out is barred.
        let mut item = element_new(Sigil::Bare);
        crate::element::parse_groups_with_pipe_stack(cur, &mut item, false, pipe_stack)?;
        // An `(args)` group read here is the item's own -- the same
        // slot the probe's `(marker)` fills, reached from the other
        // side. `- (x)[ y ]` takes that path and `-(x)` this one, and
        // dropping it here is what made `-(x)` an empty item once `(`
        // could reach this branch at all.
        if item.args.is_some() {
            marker = merge_values(marker.as_ref(), item.args.as_ref());
        }
        let id = item.id;
        let attrs = item.value.and_then(|v| v.as_data());
        let mut content = item.content.unwrap_or_default();
        // Text after the groups belongs to the item, the way
        // `@x[T] content` keeps both halves in one paragraph. Letting
        // it fall out as a block of its own would silently move a
        // sentence out of the list it was written in. `content` is a
        // `Vec<Block>` now, so "the item's paragraph" is its last
        // block if that's a `Paragraph`, not `content` itself.
        let after_groups = cur.pos();
        skip_inline_ws(cur);
        if !matches!(cur.peek(), None | Some('\n')) {
            // `parse_inline_seq` trims its own edges, which is right
            // for a standalone sequence and wrong when appending to
            // one: the space that separated the group from the text
            // has to be put back by hand. Not when the group was
            // empty, though -- `- [ ] text` has nothing to separate
            // the text from. `push_text`/`extend_merging` fold that
            // space (and the parsed sequence's own leading `Text`, if
            // it has one) into the last paragraph's content rather
            // than leaving separate nodes at the seam that mean
            // nothing on their own -- same reasoning as `inline.rs`'s
            // comment elision.
            if cur.pos() != after_groups
                && let Some(Block::Paragraph(p)) = content.last_mut()
            {
                push_text(&mut p.content, " ".to_string(), cur.span_from(after_groups));
            }
            let rest_start = cur.pos();
            let rest = parse_inline_seq(cur, Stop::Line, false)?;
            if !rest.is_empty() {
                match content.last_mut() {
                    Some(Block::Paragraph(p)) => extend_merging(&mut p.content, rest),
                    _ => content.push(Block::Paragraph(Paragraph::new(
                        rest,
                        cur.span_from(rest_start),
                    ))),
                }
            }
        }
        (content, attrs, id)
    } else {
        // The bracket-less sugar, shared with `=`: one line, plus this
        // line's own trailing `#(id)`/`{attrs}`. Spreading out means
        // opening a group -- `[ ]` or `|` -- exactly as it does for
        // `@name`. `parse_sugar_body` returns `Vec<Inline>` (`=`'s
        // title stays that shape too); a list item's own `content` is
        // `Vec<Block>` now, so wrap it in one `Paragraph`.
        let item_start = cur.pos();
        let (inline, attrs, id) = parse_sugar_body(cur)?;
        let content = if inline.is_empty() {
            Vec::new()
        } else {
            vec![Block::Paragraph(Paragraph::new(
                inline,
                cur.span_from(item_start),
            ))]
        };
        (content, attrs, id)
    };

    let span = cur.span_from(item_start);
    Ok(element_list_item(
        content,
        marker,
        attrs,
        id,
        Vec::new(),
        span,
    ))
}

pub(crate) fn parse_single_list_item(cur: &mut Cursor, head: ListMarker) -> Result<Element> {
    let item_start = cur.pos();
    let mut item = parse_list_item_body(cur, &head, &[])?;

    if cur.peek() == Some('\n') {
        cur.bump();
    }

    let mut children = Vec::new();
    while let Some(next) = peek_list_marker(cur)? {
        if next.indent > head.indent {
            let sub_items = parse_list_internal(cur, next.ordered, next.indent)?;
            if !sub_items.is_empty() {
                let list_span = sub_items
                    .first()
                    .unwrap()
                    .span
                    .union(&sub_items.last().unwrap().span);
                children.push(tomet_ast::Block::Element(element_list(
                    next.ordered,
                    sub_items,
                    list_span,
                )));
            }
        } else {
            break;
        }
    }

    item.children = if children.is_empty() {
        None
    } else {
        Some(children)
    };
    item.span = cur.span_from(item_start);
    Ok(item)
}
