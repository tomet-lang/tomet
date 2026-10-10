//! Parsing for list items (`-`, `-.`).
//!
//! A list marker's optional bracket form is `(...)`: a real `Value` parsed
//! with the same grammar as an element's `(args)` (see
//! `crate::element::parse_paren_value`), normalized against the builtin
//! `"marker"` positional key by `tomet-semantics::positional`.
//!
//! A list item written `-@name(...)` (no space between the marker and `@`)
//! is the **combine** notation: it parses `@name(...)` through the exact
//! same element-parsing path a standalone, top-level `@name(...)` would,
//! producing an identical `Element` (`Sigil::Named(name)`) that just
//! happens to sit in `List.items` instead of `Document.blocks`. There is no
//! second grammar and no naming constraint.

use crate::element::{parse_element_with_pipe_stack, parse_paren_value, parse_sugar_body};
use crate::error::Result;
use crate::inline::{Stop, extend_merging, parse_inline_seq, push_text};
use crate::section::merge_values;
use crate::value::skip_inline_ws;
use tomet_ast::{Block, ListItem, Paragraph, Placement, Sigil, Value};
use tomet_lexer::Cursor;
use tomet_tree::{element_new, list, list_item};

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
    /// `-@name(...)` -- the combine notation. Mutually exclusive with
    /// `marker`/`full_form`: a combine item is parsed entirely through
    /// [`parse_element_with_pipe_stack`] instead of the marker/sugar/full
    /// machinery below.
    pub combine: bool,
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

    // Combine notation: `-@name(...)`, no space between the marker and
    // `@`. Checked before anything else probes for a `(marker)` or a
    // group opener -- `@` is neither, and once this delegates the whole
    // rest of the item to the ordinary element-parsing path there is
    // nothing else left for this function to read.
    if look.peek() == Some('@') {
        cur.set_pos(look.pos());
        return Ok(Some(ListMarker {
            indent,
            ordered,
            marker: None,
            full_form: false,
            combine: true,
        }));
    }

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
        combine: false,
    }))
}

pub(crate) fn peek_list_marker(cur: &Cursor) -> Result<Option<ListMarker>> {
    let mut look = *cur;
    eat_list_marker(&mut look)
}

pub(crate) fn parse_list(cur: &mut Cursor, ordered: bool) -> Result<Vec<ListItem>> {
    parse_list_internal(cur, ordered, 0)
}

fn parse_list_internal(
    cur: &mut Cursor,
    ordered: bool,
    min_indent: usize,
) -> Result<Vec<ListItem>> {
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

/// Appends whatever inline content follows an item's groups on the same
/// line onto `content`'s last paragraph (or a fresh one) -- shared by the
/// full bracketed form and the combine form, both of which read their
/// groups through `parse_groups`/`parse_element_with_pipe_stack` and then
/// may still have trailing text before the line ends, the same way
/// `@x[T] content` keeps both halves in one paragraph.
fn append_trailing_line_content(cur: &mut Cursor, content: &mut Vec<Block>) -> Result<()> {
    let after_groups = cur.pos();
    skip_inline_ws(cur);
    if matches!(cur.peek(), None | Some('\n')) {
        return Ok(());
    }
    // `parse_inline_seq` trims its own edges, which is right for a
    // standalone sequence and wrong when appending to one: the space that
    // separated the group from the text has to be put back by hand. Not
    // when the group was empty, though -- `- [ ] text` has nothing to
    // separate the text from.
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
    Ok(())
}

pub(crate) fn parse_list_item_body(
    cur: &mut Cursor,
    head: &ListMarker,
    pipe_stack: &[usize],
) -> Result<ListItem> {
    let item_start = cur.pos();
    eat_list_marker(cur)?;

    if head.combine {
        // Reuses the exact same element-parsing path a standalone
        // `@name(...)` takes -- no second grammar, no naming constraint.
        // Cursor sits right on `@`, left there by `eat_list_marker` above.
        let mut el = parse_element_with_pipe_stack(cur, true, pipe_stack)?;
        el.placement = Placement::Block;
        let mut content = el.content.take().unwrap_or_default();
        append_trailing_line_content(cur, &mut content)?;
        el.content = Some(content);
        el.span = cur.span_from(item_start);
        return Ok(ListItem {
            element: el,
            sublist: None,
        });
    }

    let mut marker = head.marker.clone();

    let (content, attrs, id, connects) = if head.full_form {
        // The full form: `- ()[ content ]{value}`. Groups are read by
        // the same code that reads `@name`'s, so `[content]` stops at
        // its closing bracket and `|content` at the end of its marked
        // run -- either may span lines. Only the bracket-less sugar
        // below is one line, and it is one line because it has no
        // group to close rather than because spreading out is barred.
        //
        // `allow_colon_connect: true`: both a bare `:(args)`/`:{value}`/
        // `:(){}` merge and a named `:rule(...)` connect now attach to
        // the item's own slots, the same arms/branch `@name`'s groups
        // already use.
        let mut item = element_new(Sigil::Bare);
        crate::element::parse_groups_with_pipe_stack(cur, &mut item, true, pipe_stack)?;
        // An `(args)` group read here is the item's own -- the same
        // slot the probe's `(marker)` fills, reached from the other
        // side. `- (x)[ y ]` takes that path and `-(x)` this one, and
        // dropping it here is what made `-(x)` an empty item once `(`
        // could reach this branch at all.
        if item.args.is_some() {
            marker = merge_values(marker.as_ref(), item.args.as_ref());
        }
        let id = item.id;
        let connects = item.connects;
        let attrs = item.value.and_then(|v| v.as_data());
        let mut content = item.content.unwrap_or_default();
        append_trailing_line_content(cur, &mut content)?;
        (content, attrs, id, connects)
    } else {
        // The bracket-less sugar, shared with `=`: one line, plus this
        // line's own trailing `#(id)`/`{attrs}`/`:name(...)`. Spreading
        // out means opening a group -- `[ ]` or `|` -- exactly as it
        // does for `@name`. `parse_sugar_body` returns `Vec<Inline>`
        // (`=`'s title stays that shape too); a list item's own
        // `content` is `Vec<Block>` now, so wrap it in one `Paragraph`.
        let item_start = cur.pos();
        let body = parse_sugar_body(cur)?;
        let content = if body.content.is_empty() {
            Vec::new()
        } else {
            vec![Block::Paragraph(Paragraph::new(
                body.content,
                cur.span_from(item_start),
            ))]
        };
        // A trailing `:(...)` is the item's own `args` -- same slot the
        // `(marker)` probe fills, reached from the other side.
        if body.args.is_some() {
            marker = merge_values(marker.as_ref(), body.args.as_ref());
        }
        let attrs = body.value.and_then(|v| v.as_data());
        (content, attrs, body.id, body.connects)
    };

    let span = cur.span_from(item_start);
    Ok(list_item(
        Sigil::Bare,
        content,
        marker,
        attrs,
        id,
        connects,
        None,
        span,
    ))
}

pub(crate) fn parse_single_list_item(cur: &mut Cursor, head: ListMarker) -> Result<ListItem> {
    let item_start = cur.pos();
    let mut item = parse_list_item_body(cur, &head, &[])?;

    if cur.peek() == Some('\n') {
        cur.bump();
    }

    // At most one nested list per item -- matches `sublist: Option<List>`.
    // A contiguous run of deeper-indented items (all the same `ordered`)
    // becomes that one sublist; anything beyond it (e.g. a second run with
    // the opposite ordering directly under the same item) is not captured
    // as a further sublist, the same single-slot limit `Option<List>`
    // states structurally.
    let mut sublist = None;
    if let Some(next) = peek_list_marker(cur)?
        && next.indent > head.indent
    {
        let sub_items = parse_list_internal(cur, next.ordered, next.indent)?;
        if !sub_items.is_empty() {
            let list_span = sub_items
                .first()
                .unwrap()
                .element
                .span
                .union(&sub_items.last().unwrap().element.span);
            sublist = Some(list(next.ordered, sub_items, list_span));
        }
    }

    item.sublist = sublist;
    item.element.span = cur.span_from(item_start);
    Ok(item)
}
