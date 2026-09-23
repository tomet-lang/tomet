//! Parsing for elements (`@name`, bare elements, colon connect syntax).

use crate::error::Result;
use crate::fence::{is_fence_start, parse_fence};
use crate::section::{merge_values, parse_braced_value};
use crate::inline::{Stop, at_line_start, parse_inline_seq};
use crate::value::{
    POSITIONAL_ENTRY_KEY, eat_name, err, is_name_start_at, parse_one_entry, parse_value_at,
    skip_block_comment, skip_inline_ws, skip_line_comment, skip_ws_newlines_and_comments,
};
use tomet_ast::{Element, ElementValue, Entry, Inline, Placement, Sigil, Value};
use tomet_lexer::Cursor;
use tomet_tree::{ElementExt, element_new};

/// Whether `cur` starts an element (`@name`).
///
/// `block_context` is whether the cursor sits where a block may begin --
/// the document's top level, or a line start inside another element's
/// `[content]`. It only widens what may follow the name, never the sigil
/// or the name itself:
///
/// - anywhere: a group (see [`opens_group`]), a `:` that a `:name` family
///   member follows, or a `+++` fence must follow. This is what keeps
///   `me@example.com` and a lone `@foo` in running prose as plain text.
/// - in block context only: end-of-line also counts, so `@memo` alone on
///   its line is an element. It then fails later, in `tomet-semantics`,
///   as an unknown bare name -- the intended report, and the reason no
///   hashtag-style syntax exists.
///
/// The old `#name` spelling had the end-of-line allowance bound to the
/// sigil rather than to the position. There is one element sigil now, so
/// the allowance belongs to the position.
pub(crate) fn is_element_start(cur: &Cursor, block_context: bool) -> bool {
    let mut look = *cur;
    if look.bump() != Some('@') {
        return false;
    }
    // The name is mandatory. A nameless `@(url:...)` used to infer its
    // kind from an args key; that inference was retired in favour of the
    // one `@link(target:...)` element, but the syntax outlived it and
    // kept parsing into a meaningless `Custom("at")`. It is text now.
    if !is_name_start_at(&look) || eat_name(&mut look).is_none() {
        return false;
    }
    skip_lookahead_gap(&mut look);
    if opens_group(look.peek()) || is_fence_start(&look) {
        return true;
    }
    // A `:` right after the name only starts an element when a `:name`
    // family member follows it. A bare one there can do nothing: `:` says
    // "not a fresh group of this element", and right after the name every
    // slot is still empty, so `@memo:{a:1}` was `@memo{a:1}` with a
    // character in front of it -- a second spelling of one tree, which
    // `explicit-form-first` in the workspace writ calls a duplicate.
    // Nothing under `docs/`, `tests/fixtures/` or `tmtroot/` spelled it.
    if look.peek() == Some(':') {
        let mut after_colon = look;
        after_colon.bump();
        return is_name_start_at(&after_colon);
    }
    block_context && matches!(look.peek(), None | Some('\n') | Some('\r'))
}

/// What follows the element starting at `cur`, for the join/isolate
/// decision (`docs/spec/syntax.tmt`'s `##[ 継続 ]`).
///
/// The default is isolation: a bare element that opens its own line
/// stands as its own block, whether or not a blank line follows --
/// `docs/examples/dirs.tmt`'s file listing depends on this. Joining is
/// opt-in, via an explicit `\` trigger, because it is the rarer intent
/// (three consecutive badge links flowing into one row) and silently
/// swallowing unrelated adjacent content into one paragraph is the wrong
/// default for everything else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LineEnd {
    /// Ends with just the element: isolates as its own block.
    Bare,
    /// Ends with the element followed by an explicit trailing `\`: joins
    /// whatever follows, regardless of adjacency.
    Continuation,
    /// Something else follows: not a candidate. Ordinary running text.
    No,
}

/// The element is parsed speculatively on a copy of the cursor and the
/// copy is thrown away -- the same probe `eat_list_marker_with_indent`
/// uses for a list marker's `(...)` form.
pub(crate) fn element_ends_line(cur: &Cursor) -> LineEnd {
    let mut probe = *cur;
    if parse_element(&mut probe, true).is_err() {
        return LineEnd::No;
    }
    skip_inline_ws(&mut probe);
    while probe.starts_with("//") || probe.starts_with("/*") {
        if probe.starts_with("//") {
            skip_line_comment(&mut probe);
        } else if skip_block_comment(&mut probe).is_err() {
            return LineEnd::No;
        }
        skip_inline_ws(&mut probe);
    }
    let continues = probe.peek() == Some('\\');
    if continues {
        probe.bump();
        skip_inline_ws(&mut probe);
    }
    // A `+++` fence swallows its own closing line, newline included, so
    // the probe can already sit at the start of the *next* line. That
    // still means the element ended its line.
    if matches!(probe.peek(), None | Some('\n') | Some('\r'))
        || probe.src()[..probe.pos()].ends_with(['\n', '\r'])
    {
        if continues {
            LineEnd::Continuation
        } else {
            LineEnd::Bare
        }
    } else {
        LineEnd::No
    }
}

/// Consumes the trailing `\` continuation trigger a line ends with, plus
/// the inline whitespace/comments before it and the whitespace after it
/// -- the same shape [`element_ends_line`]'s probe already recognized.
///
/// Call only once `element_ends_line` has returned [`LineEnd::Continuation`]
/// for this position; it assumes the shape is there and does not re-check.
pub(crate) fn consume_trailing_continuation(cur: &mut Cursor) {
    skip_inline_ws(cur);
    loop {
        if cur.starts_with("//") {
            skip_line_comment(cur);
        } else if cur.starts_with("/*") {
            let _ = skip_block_comment(cur);
        } else {
            break;
        }
        skip_inline_ws(cur);
    }
    cur.bump();
    skip_inline_ws(cur);
}

/// Whether a fresh line start at `cur` begins with the `\` continuation
/// trigger, and if so, the cursor position right after it (and the
/// inline whitespace following it) -- ready to parse whatever it joins
/// to the previous block. Purely lexical: it does not judge whether
/// there is anything valid to join to, the same way
/// [`consume_trailing_continuation`] does not.
pub(crate) fn leading_continuation<'a>(cur: &Cursor<'a>) -> Option<Cursor<'a>> {
    if cur.peek() != Some('\\') {
        return None;
    }
    let mut after = *cur;
    after.bump();
    skip_inline_ws(&mut after);
    Some(after)
}

fn skip_element_gap(cur: &mut Cursor) -> u8 {
    let mut newlines = 0u8;
    loop {
        skip_inline_ws(cur);
        if cur.starts_with("//") {
            skip_line_comment(cur);
            continue;
        }
        if cur.starts_with("/*") {
            let mut look = *cur;
            if skip_block_comment(&mut look).is_ok() {
                *cur = look;
                continue;
            }
            break;
        }
        match cur.peek() {
            Some('\n') | Some('\r') => {
                cur.bump();
                newlines += 1;
                if newlines > 1 {
                    break;
                }
            }
            _ => break,
        }
    }
    newlines
}

fn skip_lookahead_gap(cur: &mut Cursor) {
    skip_element_gap(cur);
}

/// `allow_colon_connect` gates the `:(...)`/`:{...}` "connect" branch
/// below (a bare, colon-less trailing group is *always* claimed
/// regardless -- see `docs/spec/syntax.tmt`'s
/// `@meta(format:yaml) {...}` example, real usage this must keep
/// working). List items pass `false` for the single top-level element
/// they parse as their own content (`list.rs::parse_list_internal`):
/// a list item has its own optional trailing `{attrs}`, and without
/// this, a colon-prefixed group meant for the *item* (`- @link(ref:x)
/// :{id:breakfast}`) always got silently claimed by `@link` instead --
/// by the time `list.rs` got a turn, the group was already gone, with
/// nothing left at the position it expected to still find one at (see
/// `list_item_ending_in_an_element_does_not_error_on_a_trailing_brace`'s
/// history for the crash this used to cause before the item-attrs guess
/// was made speculative). `docs/spec/syntax.tmt`'s
/// `##[ コネクト ]` section had flagged exactly this shape (`- ()
/// xxxxxx :{}`) as an unimplemented idea for attaching a group to the
/// *enclosing* construct rather than the nearest element -- this is
/// that, scoped narrowly to where the ambiguity actually is. Every
/// other caller (top-level block elements, content nested inside an
/// already-bracketed group, paragraph prose) passes `true`, unchanged:
/// none of those have a competing attrs slot of their own to lose the
/// group to, so `<id:taskA>:{...}`-style remote connect (see
/// `remote_id_target_element_supports_colon_connection`) keeps working
/// exactly as before there.
pub(crate) fn parse_element(cur: &mut Cursor, allow_colon_connect: bool) -> Result<Element> {
    let start_pos = cur.pos();
    if !cur.eat_str("@") {
        return Err(err(cur, cur.pos(), "expected '@'"));
    }
    let sigil = match eat_name(cur) {
        Some(name) => Sigil::Named(name),
        None => return Err(err(cur, cur.pos(), "expected an element name after '@'")),
    };

    let mut el = element_new(sigil);
    parse_groups(cur, &mut el, allow_colon_connect)?;
    el.span = cur.span_from(start_pos);
    Ok(el)
}

/// The characters that open one of an element's groups.
///
/// [`parse_groups`] is the only code that *reads* a group; every other site
/// only asks whether one starts here, and each used to spell the set again.
/// `|` had to be added to six independent spellings, and `-` was missing
/// `[` and `{` from two of them from `2a99315` until `181b542`. One
/// definition now, and `every_sigil_takes_every_group_opener` holds each
/// sigil's recognizer and parser to it.
///
/// `:` is deliberately not here. It opens nothing; it says where the group
/// that follows belongs. See [`is_element_start`].
///
/// The alternative was to delete the recognizers instead: decide by
/// parsing speculatively on a copy of the cursor, the way
/// [`element_ends_line`] already does, which leaves one definition per
/// sigil and makes the disagreement structurally impossible. It was
/// rejected on cost. `document::is_heading_start` is called from
/// `Stop::Paragraph`'s per-line lookahead, so trying a full parse there
/// approaches quadratic on some inputs, and `parser-purity` in this
/// crate's writ asks for the same tree from the same input *in
/// predictable time*. Reconsider it if the lookahead ever stops being
/// per-line; a shared set plus a guard buys the same safety until then.
pub(crate) fn opens_group(c: Option<char>) -> bool {
    matches!(c, Some('(') | Some('[') | Some('{') | Some('|'))
}

/// Reads an element's `(args)`, `[content]` and `{value}` groups -- in any
/// order, each at most once -- plus the colon-connect form and the `+++`
/// fence that stands in for a body.
///
/// Split out of [`parse_element`] because a sigil is a sigil: `-` and `#`
/// take the same groups as `@name` and must not grow a second, subtly
/// different implementation of this. `[content]` stops at its closing
/// bracket rather than at end of line, and `|content` -- the same group
/// without the brackets -- stops at the end of its marked run, so either
/// spelling may span lines. Only the bracket-less *sugar* body
/// (`parse_sugar_body`, the `- x` and `# x` forms) is still one line.
pub(crate) fn parse_groups(
    cur: &mut Cursor,
    el: &mut Element,
    allow_colon_connect: bool,
) -> Result<()> {
    loop {
        let checkpoint = cur.pos();
        let newlines = skip_element_gap(cur);
        if newlines <= 1 {
            let colon_pos = cur.pos();
            if allow_colon_connect && cur.eat_str(":") {
                skip_inline_ws(cur);
                // `:name(...)` -- a connect, not the bare merge below. A
                // name is checked for *before* `(`/`{` so this wins the
                // ambiguity: a name right after `:` can never be the
                // start of a bare merge's own group. Read with the same
                // `parse_groups` every other sigil uses, so a connect
                // takes `(args)`/`[content]`/`{value}` in any order just
                // like `@name` does -- but with `allow_colon_connect:
                // false`, so it does not itself swallow a *sibling*
                // `:xxx(...)`; that is left for this same loop's next
                // iteration, which is what turns `@x():as(y):rule(z)`
                // into two flat `connects` entries rather than one
                // nested inside the other.
                if is_name_start_at(cur) {
                    let name = eat_name(cur).expect("is_name_start_at just confirmed a name");
                    let mut connect_el = element_new(Sigil::Named(name));
                    parse_groups(cur, &mut connect_el, false)?;
                    connect_el.span = cur.span_from(colon_pos);
                    el.connects.push(connect_el);
                    continue;
                }
                match cur.peek() {
                    Some('(') => {
                        let conn_args = parse_paren_value(cur)?;
                        el.args = merge_values(el.args.as_ref(), Some(&conn_args));
                        continue;
                    }
                    Some('{') => {
                        let conn_val = parse_value_group(cur)?;
                        // Connect merges pairs into an existing group; a
                        // group holding elements is replaced wholesale,
                        // as there is nothing to merge key-wise.
                        let merged = match (&el.value, &conn_val) {
                            (Some(existing), ElementValue::Group(_))
                                if !existing.has_children() && !conn_val.has_children() =>
                            {
                                merge_values(
                                    existing.as_data().as_ref(),
                                    conn_val.as_data().as_ref(),
                                )
                                .map(ElementValue::from_map)
                            }
                            _ => None,
                        };
                        el.value = Some(merged.unwrap_or(conn_val));
                        continue;
                    }
                    _ => {}
                }
            }
            match cur.peek() {
                Some('(') if el.args.is_none() => {
                    el.args = Some(parse_paren_value(cur)?);
                    continue;
                }
                Some('[') if el.content.is_none() => {
                    el.content = Some(parse_content(cur)?);
                    continue;
                }
                // `|` is `[` without the brackets -- same slot, closed by
                // the end of the marked run instead of by `]`.
                Some('|') if el.content.is_none() => {
                    el.content = Some(parse_pipe_content(cur)?);
                    continue;
                }
                Some('{') if el.value.is_none() => {
                    el.value = Some(parse_value_group(cur)?);
                    continue;
                }
                // A `+++` fence is exclusive with `[content]` and
                // `{value}`: it *is* the body, captured verbatim.
                _ if el.value.is_none() && el.content.is_none() && is_fence_start(cur) => {
                    el.value = Some(ElementValue::Raw(parse_fence(cur)?));
                    continue;
                }
                // A group whose slot is already filled. Every arm above
                // guards on its slot being empty, so reaching here with an
                // opener means a second one of the same kind. It used to
                // fall through: the group was left where it stood and read
                // as prose, which also cost the element its block
                // placement, so `@memo(a: 1)(b: 2)` quietly became a
                // paragraph. `docs/spec/syntax.tmt` calls it an error.
                //
                // `:` is the spelling for reaching a filled slot, and it
                // merges rather than replaces -- see the connect branch
                // above.
                //
                // Two things narrow this. The second group must be
                // written *against* the first, with nothing skipped
                // between them: `@file(x) (it was ...)` is an element and
                // then a parenthesis in running prose, which `docs/.writ.tmt`
                // and `tmtroot/agents.tmt` both do, and a `(` opening the
                // next line is prose too. The spec's own example --
                // `@xxx()(){}{}[][]` -- is adjacent, and adjacency is the
                // only reading that leaves ordinary sentences alone.
                //
                // And `allow_colon_connect` is false in exactly the
                // contexts where a trailing group may belong to something
                // else -- a list item's own `{attrs}`, an entry inside a
                // `{...}` group -- where this fall-through is what hands
                // it over.
                _ if allow_colon_connect && cur.pos() == checkpoint && opens_group(cur.peek()) => {
                    let group = match cur.peek() {
                        Some('(') => "(args)",
                        Some('[') | Some('|') => "[content]",
                        _ => "{value}",
                    };
                    return Err(err(
                        cur,
                        cur.pos(),
                        format!(
                            "a second `{group}` group: an element takes one of each. \
                             Write `:` in front of it to merge into the one already there"
                        ),
                    ));
                }
                _ => {}
            }
        }
        cur.set_pos(checkpoint);
        break;
    }
    Ok(())
}

/// Byte offset of a `:` immediately (only inline whitespace between)
/// preceding `brace_pos`, if there is one -- the colon-connect marker
/// that dedicates this line's trailing `{...}` to the sigil itself
/// rather than to whatever element precedes it (see
/// `element::parse_element`'s `allow_colon_connect` doc comment).
/// Byte-indexed but UTF-8 safe: only ever steps back over bytes it has
/// just confirmed are the ASCII space/tab/colon it's looking for, so it
/// never lands on, or reads across, a multi-byte character's interior.
pub(crate) fn connect_colon_pos(src: &str, brace_pos: usize) -> Option<usize> {
    let bytes = src.as_bytes();
    let mut i = brace_pos;
    while i > 0 && matches!(bytes[i - 1], b' ' | b'\t') {
        i -= 1;
    }
    if i > 0 && bytes[i - 1] == b':' {
        Some(i - 1)
    } else {
        None
    }
}

/// Finds this line's own trailing `{attrs}`, if the rest of the line
/// has one. Returns `(content_stop, brace_pos)`: `content_stop` is
/// where the line's own inline *content* parsing must stop -- normally
/// the same as `brace_pos` (a bare, colon-less `{}` is always claimed
/// by an element instead, so content parsing runs right up to it
/// regardless of whether it turns out to belong to the item or not),
/// but the position of a preceding colon-connect marker instead when
/// there is one, so that marker isn't left dangling as ordinary
/// trailing text once `allow_colon_connect: false` stops an inner
/// element from consuming it itself.
pub(crate) fn peek_trailing_attrs(cur: &Cursor) -> Option<(usize, usize)> {
    let mut look = *cur;
    let mut last_brace_pos = None;
    while !look.is_eof() && look.peek() != Some('\n') && look.peek() != Some('\r') {
        if look.peek() == Some('{') {
            last_brace_pos = Some(look.pos());
        }
        look.bump();
    }
    let brace_pos = last_brace_pos?;
    let content_stop = connect_colon_pos(cur.src(), brace_pos).unwrap_or(brace_pos);

    // Confirm inline content parsing, stopped at `content_stop`,
    // actually lands exactly there rather than overshooting it. A bare,
    // colon-less trailing group is still always claimed by an element
    // regardless of `allow_colon_connect` (`@meta(format:yaml) {...}`
    // is real, existing usage that must keep working) -- so this line's
    // one `{` can still belong to an inner element instead of the item,
    // and `Stop::Offset` alone can't be trusted to have actually
    // stopped parsing there: `parse_inline_seq` only checks its target
    // *between* separate line items, not while a single element is
    // mid-way through claiming one more trailing group -- so it can
    // walk straight past `content_stop` without ever noticing.
    // Speculatively parsing here (the result is discarded either way --
    // `parse_list_internal` reparses for real once this confirms the
    // guess) is the only way to know without duplicating
    // `parse_element`'s own claiming logic.
    let mut probe = *cur;
    let landed_at_pos = matches!(
        parse_inline_seq(&mut probe, Stop::Offset(content_stop), false),
        Ok(_) if probe.pos() == content_stop
    );
    if !landed_at_pos {
        return None;
    }

    let mut test_cur = *cur;
    test_cur.set_pos(brace_pos);
    if parse_braced_value(&mut test_cur).is_ok() {
        skip_inline_ws(&mut test_cur);
        if matches!(test_cur.peek(), None | Some('\n') | Some('\r')) {
            return Some((content_stop, brace_pos));
        }
    }
    None
}

/// Reads a bracket-less body: inline content to the end of the line, plus
/// the line's own trailing `{attrs}` if it has one.
///
/// This is the sugar shared by `-` and `#`. It is single-line on purpose:
/// content that spans lines has to say so with an explicit `[ ... ]`
/// group, which is what [`parse_groups`] reads.
pub(crate) fn parse_sugar_body(cur: &mut Cursor) -> Result<(Vec<Inline>, Option<Value>)> {
    if let Some((content_stop, brace_pos)) = peek_trailing_attrs(cur) {
        let content = parse_inline_seq(cur, Stop::Offset(content_stop), false)?;
        // `content_stop` is the colon-connect marker's position when there
        // is one, short of `brace_pos` -- jump the rest of the way past it
        // and its surrounding whitespace, neither of which needs to
        // survive as an AST node.
        cur.set_pos(brace_pos);
        let attrs = parse_braced_value(cur)?;
        Ok((content, Some(attrs)))
    } else {
        Ok((parse_inline_seq(cur, Stop::Line, false)?, None))
    }
}

pub(crate) fn parse_paren_value(cur: &mut Cursor) -> Result<Value> {
    if !cur.eat_str("(") {
        return Err(err(cur, cur.pos(), "expected '('"));
    }
    skip_ws_newlines_and_comments(cur);
    let v = if cur.peek() == Some(')') {
        Value::Map(Vec::new())
    } else {
        parse_value_at(cur)?
    };
    skip_ws_newlines_and_comments(cur);
    if !cur.eat_str(")") {
        return Err(err(cur, cur.pos(), "expected ')'"));
    }
    Ok(v)
}

/// Parses a `|`-prefixed content run: `[content]` spelled without brackets.
///
/// Nothing about the content is decided here. The run's text is the same
/// contiguous slice of source it would be between brackets, and
/// `normalize_text` folds each marker away with the newline it follows, so
/// line joining, whitespace, and every element's own reading of its content
/// are inherited rather than restated. A `|` run and the bracketed form of
/// the same content parse to the same tree, and `tests/src/pipe.rs` pins
/// that.
fn parse_pipe_content(cur: &mut Cursor) -> Result<Vec<Inline>> {
    let (_, col) = cur.line_col(cur.pos());
    if !cur.eat_str("|") {
        return Err(err(cur, cur.pos(), "expected '|'"));
    }
    parse_inline_seq(cur, Stop::PipeRun { col }, true)
}

fn parse_content(cur: &mut Cursor) -> Result<Vec<Inline>> {
    if !cur.eat_str("[") {
        return Err(err(cur, cur.pos(), "expected '['"));
    }
    let content = parse_inline_seq(cur, Stop::Bracket(']'), true)?;
    if !cur.eat_str("]") {
        return Err(err(cur, cur.pos(), "expected ']'"));
    }
    Ok(content)
}

/// Parses a `{...}` value group.
///
/// `{}` is always data: entries are read uniformly, each either a
/// `key: value` pair or a nested element, in source order. Nothing here
/// consults the enclosing element's name -- which is what lets
/// `@links{ (1)[a] note:x (2)[b] }` parse at all. It previously either
/// errored or, worse, swallowed the elements into a scalar string,
/// depending on which came first.
///
/// "A nested element" means a *named* one too, not only the bare
/// `(marker)[content]` form. That half arrived late: this comment claimed
/// the general rule while the code took only `(`, so a vocabulary's own
/// `@element(c){ @args{ @param(id){...} } }` -- the shape
/// `docs/spec/vocabulary.tmt` is written in -- could not be parsed at all.
///
/// A non-map body (`{[1,2,3]}`, `{"str"}`, `{bare}`) is rejected. Those had
/// no uniform-entry spelling, and the `+++` fence now covers the case they
/// served -- `@meta(format:json)+++ [1,2,3] +++`.
pub(crate) fn parse_value_group(cur: &mut Cursor) -> Result<ElementValue> {
    let group_start = cur.pos();
    if !cur.eat_str("{") {
        return Err(err(cur, cur.pos(), "expected '{'"));
    }
    let mut entries = Vec::new();
    loop {
        skip_ws_newlines_and_comments(cur);
        match cur.peek() {
            None => return Err(err(cur, group_start, "unterminated '{', expected '}'")),
            Some('}') => break,
            Some('(') => entries.push(Entry::Element(parse_bare_element(cur)?)),
            // A named entry, `@args{ ... }`. Unambiguous: `is_ident_char`
            // excludes `@`, so a map key cannot begin with one, and this
            // position errors today -- `eat_ident` returns empty and the
            // `POSITIONAL_ENTRY_KEY` rejection below takes over. So this
            // arm only turns errors into parses.
            //
            // `block_context: false`: a group entry still has to carry a
            // group, a `:` or a fence after the name, so a bare `@foo`
            // inside `{...}` stays the error it already was.
            // The placement rule reaches inside a group too, the same way
            // it reaches inside `[content]` (see `inline.rs`): a line start
            // here is block context, so an element that also ends its line
            // stands as a block. `@args{ ... }` on its own line inside
            // `@element(x){ ... }` is what that is for -- `required_shape`
            // calls those six block elements, and placement is what it is
            // compared against.
            //
            // `{value}` groups are a flat list of entries, not prose, so
            // there is no paragraph for `;` to isolate from here -- the
            // separator (`docs/spec/syntax.tmt`'s `##[ 区切り ]`) is out
            // of scope for this position on purpose. `Separated` collapses
            // into "not a block" the same as `No`, unchanged from
            // `element_ends_line`'s old boolean behavior.
            _ if is_element_start(cur, at_line_start(cur)) => {
                let block = at_line_start(cur) && element_ends_line(cur) == LineEnd::Bare;
                let el = parse_element(cur, false)?;
                entries.push(Entry::Element(if block {
                    el.with_placement(Placement::Block)
                } else {
                    el
                }));
            }
            _ => {
                let entry_start = cur.pos();
                let (key, value) = parse_one_entry(cur)?;
                if key == POSITIONAL_ENTRY_KEY {
                    return Err(err(
                        cur,
                        entry_start,
                        "a '{...}' group holds 'key: value' entries or elements; \
                         write a bare value in '(args)', or use a '+++' fence",
                    ));
                }
                entries.push(Entry::Pair(key, value));
            }
        }
        // Entries may be separated by `,` or just by whitespace.
        skip_ws_newlines_and_comments(cur);
        if cur.peek() == Some(',') {
            cur.bump();
        }
    }
    if !cur.eat_str("}") {
        return Err(err(cur, cur.pos(), "expected '}'"));
    }
    Ok(ElementValue::Group(entries))
}

/// A `(marker)[content]` entry inside a `{...}` group -- an element whose
/// type its container supplies, so it carries no name.
///
/// It reads its groups through [`parse_groups`], like every other sigil.
/// It used to have a small parser of its own that took `(args)` and then
/// an optional `[content]` and nothing else, which is exactly the second
/// implementation `parse_groups`' own doc comment says a sigil must not
/// grow: a bare entry could not take `{value}` or `|content` while the
/// named entry beside it could, and `(1)` and `[a]` on separate lines was
/// an error here and fine everywhere else.
///
/// `allow_colon_connect: false`, matching the named entry above -- there
/// is no enclosing construct inside a group for a connect to reach.
fn parse_bare_element(cur: &mut Cursor) -> Result<Element> {
    let start_pos = cur.pos();
    let mut el = element_new(Sigil::Bare);
    parse_groups(cur, &mut el, false)?;
    el.span = cur.span_from(start_pos);
    Ok(el)
}
