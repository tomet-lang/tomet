//! Parsing for elements (`@name`, bare elements, colon connect syntax).

use crate::error::Result;
use crate::fence::{is_fence_start, parse_fence};
use crate::inline::{Stop, at_line_start, parse_inline_seq};
use crate::section::merge_values;
use crate::value::{
    POSITIONAL_ENTRY_KEY, eat_name, eat_scalar_raw, err, is_name_start_at, parse_one_entry,
    parse_quoted, parse_value_at, skip_block_comment, skip_inline_ws, skip_line_comment,
    skip_ws_newlines_and_comments,
};
use tomet_ast::{
    Block, Element, ElementValue, Entry, Id, Inline, Paragraph, Placement, Sigil, Value,
};
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
    if opens_group(&look) || is_fence_start(&look) {
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

/// `allow_colon_connect` gates the `:(...)`/`:{...}`/`:name(...)` connect
/// branch in [`parse_groups_with_pipe_stack`] (a bare, colon-less
/// trailing group is *always* claimed regardless -- see
/// `docs/spec/syntax.tmt`'s `@meta(format:yaml) {...}` example, real
/// usage that must keep working). Every context that reaches a sigil's
/// own top-level groups passes `true`: a list item's own groups used to
/// pass `false` (no `connects` field to put a named one in), but
/// `element_list_item` now takes `connects` too, so there is no longer a
/// context that needs the connect branch half-enabled.
pub(crate) fn parse_element(cur: &mut Cursor, allow_colon_connect: bool) -> Result<Element> {
    parse_element_with_pipe_stack(cur, allow_colon_connect, &[])
}

pub(crate) fn parse_element_with_pipe_stack(
    cur: &mut Cursor,
    allow_colon_connect: bool,
    parent_pipe_stack: &[usize],
) -> Result<Element> {
    let start_pos = cur.pos();
    if !cur.eat_str("@") {
        return Err(err(cur, cur.pos(), "expected '@'"));
    }
    let sigil = match eat_name(cur) {
        Some(name) => Sigil::Named(name),
        None => return Err(err(cur, cur.pos(), "expected an element name after '@'")),
    };

    let mut el = element_new(sigil);
    parse_groups_with_pipe_stack(cur, &mut el, allow_colon_connect, parent_pipe_stack)?;
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
/// `#` is here for a narrower reason than the rest: it does not open an
/// `(args)`/`[content]`/`{value}` group at all, it opens the one `#(id)`
/// slot (see [`parse_hash_id`]) -- and only when a `(` actually follows
/// it, which is why this takes a `Cursor` rather than one already-peeked
/// `char` the way it used to: `#` alone (not followed by `(`) answers
/// "no", the same as any other character with no meaning here, since a
/// bare `#` is otherwise completely free text.
///
/// The alternative was to delete the recognizers instead: decide by
/// parsing speculatively on a copy of the cursor, the way
/// [`element_ends_line`] already does, which leaves one definition per
/// sigil and makes the disagreement structurally impossible. It was
/// rejected on cost. `is_element_start` is called from `Stop::Paragraph`'s
/// per-character scan, so trying a full parse there approaches quadratic
/// on some inputs, and `parser-purity` in this
/// crate's writ asks for the same tree from the same input *in
/// predictable time*. Reconsider it if the lookahead ever stops being
/// per-line; a shared set plus a guard buys the same safety until then.
pub(crate) fn opens_group(cur: &Cursor) -> bool {
    match cur.peek() {
        Some('(') | Some('[') | Some('{') | Some('|') => true,
        Some('#') => cur.peek_at(1) == Some('('),
        _ => false,
    }
}

/// Reads an element's `(args)`, `[content]` and `{value}` groups -- in any
/// order, each at most once -- plus the colon-connect form and the `+++`
/// fence that stands in for a body.
///
/// Split out of [`parse_element`] because a sigil is a sigil: `-` and `=`
/// take the same groups as `@name` and must not grow a second, subtly
/// different implementation of this. `[content]` stops at its closing
/// bracket rather than at end of line, and `|content` -- the same group
/// without the brackets -- stops at the end of its marked run, so either
/// spelling may span lines. Only the bracket-less *sugar* body
/// (`parse_sugar_body`, the `- x` and `= x` forms) is still one line.
pub(crate) fn parse_groups(
    cur: &mut Cursor,
    el: &mut Element,
    allow_colon_connect: bool,
) -> Result<()> {
    parse_groups_with_pipe_stack(cur, el, allow_colon_connect, &[])
}

pub(crate) fn parse_groups_with_pipe_stack(
    cur: &mut Cursor,
    el: &mut Element,
    allow_colon_connect: bool,
    parent_pipe_stack: &[usize],
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
                    parse_groups_with_pipe_stack(cur, &mut connect_el, false, parent_pipe_stack)?;
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
                    let mut probe = *cur;
                    let mut parent_ok = true;
                    if newlines > 0 && !parent_pipe_stack.is_empty() {
                        for &expected_col in parent_pipe_stack {
                            skip_inline_ws(&mut probe);
                            if probe.peek() != Some('|') {
                                parent_ok = false;
                                break;
                            }
                            let (_, col) = probe.line_col(probe.pos());
                            if col != expected_col {
                                parent_ok = false;
                                break;
                            }
                            probe.bump();
                        }
                        skip_inline_ws(&mut probe);
                    }
                    if parent_ok && probe.peek() == Some('|') {
                        *cur = probe;
                        el.content = Some(parse_pipe_content(cur, parent_pipe_stack)?);
                        continue;
                    } else {
                        cur.set_pos(checkpoint);
                        break;
                    }
                }
                Some('{') if el.value.is_none() => {
                    el.value = Some(parse_value_group(cur)?);
                    continue;
                }
                // `#(id)` -- a single slot, read after the three groups
                // above. "Before any `:name(...)` connect" needs no
                // explicit check here: a connect's own `#` arm (this
                // same one, reached through its own recursive
                // `parse_groups` call) always claims an adjacent `#(...)`
                // for *itself* first, so writing an id after a connect
                // names the connect's id, not this element's -- there is
                // no ambiguous case left to reject.
                Some('#') if el.id.is_none() && cur.peek_at(1) == Some('(') => {
                    el.id = Some(parse_hash_id(cur)?);
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
                // else -- an entry inside a `{...}` group -- where this
                // fall-through is what hands it over.
                _ if allow_colon_connect && cur.pos() == checkpoint && opens_group(cur) => {
                    if cur.peek() == Some('#') {
                        return Err(err(
                            cur,
                            cur.pos(),
                            "a second `#(...)`: an element takes at most one id",
                        ));
                    }
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

/// Confirms inline content parsing, stopped at `stop`, actually lands
/// exactly there rather than overshooting it -- see
/// [`find_trailing_connect`]'s doc comment for why this can't just be
/// trusted.
fn content_lands_at(cur: &Cursor, stop: usize) -> bool {
    let mut probe = *cur;
    parse_inline_seq(&mut probe, Stop::Offset(stop), false).is_ok() && probe.pos() == stop
}

/// Whether `el`'s own groups-read actually claimed anything -- used to
/// tell a genuine connect apart from a trial parse that advanced the
/// cursor (eating a `:` and a name) without ever finding a group to
/// read. A named connect with nothing of its own still counts as
/// "nothing", even though [`parse_groups_with_pipe_stack`] pushes it to
/// `connects` unconditionally: otherwise ordinary prose ending in
/// `word:word` (`...see note:more`) would read as a zero-argument
/// `:more` connect merely because there happened to be nothing left
/// after it to disagree.
fn claimed_something(el: &Element) -> bool {
    el.args.is_some()
        || el.content.is_some()
        || el.value.is_some()
        || el.id.is_some()
        || el
            .connects
            .iter()
            .any(|c| c.args.is_some() || c.content.is_some() || c.value.is_some() || c.id.is_some())
}

/// Leftmost position on the current line where the rest of the line,
/// read the same way [`parse_groups_with_pipe_stack`] reads any sigil's
/// own groups, both claims something and lands exactly at the end of the
/// line -- the trailing `#(id)`/`(args)`/`{value}`/`:name(...)` a
/// bracket-less sugar body hands over to its owner (a list item or
/// heading). Leftmost, not rightmost: a valid starting point reads every
/// contiguous group from there to the end of the line in one pass, so
/// the leftmost one that validates is always the full run (`#(id){value}`,
/// not just whichever piece happens to follow a later candidate).
///
/// Candidates are every `(`, `{`, `#(` and `:` on the line -- a cheap
/// forward scan -- each confirmed or rejected by actually parsing from
/// there, the only way to know without duplicating
/// `parse_groups_with_pipe_stack`'s own claiming logic. Three things
/// narrow this to exactly the shapes that belong to the owner rather
/// than to something else in the content:
///
/// - "Lands exactly at the end of the line" rejects a colon that's just
///   part of ordinary prose (`see note: more text`): something would
///   still be unconsumed after it.
/// - [`claimed_something`] further rejects a colon at the very end of a
///   line with nothing structured after it (`...note:more`) -- otherwise
///   indistinguishable from plain text that happens to end in a word
///   with a colon in front of it.
/// - [`content_lands_at`] rejects a *colon-less* candidate that an inner
///   element occupying the end of the content would itself reach past --
///   `@meta(format:yaml) {...}` must still belong to `@meta`, not to
///   this sugar body, the same real, existing usage a bare trailing
///   group has always been claimed by first (`parse_groups_with_pipe_stack`'s
///   colon branch doc comment). A colon-prefixed candidate can't suffer
///   this: nothing within the content is ever parsed with a mode that
///   lets it claim a colon connect for itself (`parse_sugar_body` always
///   parses its own content with `allow_colon_connect: false`), so this
///   check is cheap insurance there rather than load-bearing.
fn find_trailing_connect(cur: &Cursor) -> Option<usize> {
    let src = cur.src();
    let preceded_by_colon = |pos: usize| {
        let bytes = src.as_bytes();
        let mut i = pos;
        while i > 0 && matches!(bytes[i - 1], b' ' | b'\t') {
            i -= 1;
        }
        i > 0 && bytes[i - 1] == b':'
    };
    // Names are ASCII-only (`is_name_char`), so a raw byte compare is
    // UTF-8-safe here: no continuation byte of a multi-byte character
    // ever equals one of these.
    let preceded_by_name_char = |pos: usize| {
        pos > 0
            && matches!(src.as_bytes()[pos - 1], b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-')
    };
    let mut look = *cur;
    let mut candidates = Vec::new();
    while !look.is_eof() && !matches!(look.peek(), Some('\n') | Some('\r')) {
        if look.peek() == Some(':') {
            // Always a candidate -- and, unlike an opener, never itself
            // excluded for being colon-preceded (`::` doesn't happen in
            // practice, but there's no reason to special-case it away).
            candidates.push(look.pos());
            look.bump();
            continue;
        }
        let is_hash_paren = look.peek() == Some('#') && look.peek_at(1) == Some('(');
        // A *colonless* `(` is deliberately not a candidate opener here,
        // unlike `{` -- parenthetical remarks are common, ordinary prose
        // (`tmtroot/locales/ja/readme.tmt`'s own
        // `...記号の渋滞 (LSP機能を大切に)` is exactly this), so treating
        // every sentence that happens to end in `(...)` as the owner's
        // own `args` would be far too eager. `(` only ever participates
        // here through a `:` candidate, matching what the user actually
        // writes (`:(...)`), never bare.
        let is_opener = is_hash_paren || look.peek() == Some('{');
        // A colon-preceded opener is already reachable by starting the
        // trial right at that colon (which will itself eat the opener
        // as part of the same read) -- adding the opener too would let
        // a trial starting there "succeed" while leaving the colon
        // itself dangling as unconsumed content text. A name-preceded
        // one (the `(` of `rule(...)` in `:rule(allow:list)`) is the
        // same problem one level removed: starting right at it reads a
        // plain `(args)` group that happens to look valid on its own,
        // leaving the name *and* its colon as unconsumed text instead
        // of being read together as one `:name(...)` connect.
        if is_opener && !preceded_by_colon(look.pos()) && !preceded_by_name_char(look.pos()) {
            candidates.push(look.pos());
        }
        look.bump();
        // `#(` is one unit: stepping past both here keeps the `(` from
        // also being added as its own, separate (and wrong -- it would
        // read as a plain `(args)` group instead of the id it's part
        // of) candidate one character later.
        if is_hash_paren {
            look.bump();
        }
    }
    // Leftmost first, not rightmost: a valid starting point reads every
    // contiguous group from there to the end of the line in one go (the
    // same loop any sigil's own groups use), so the leftmost one that
    // validates always captures the full run (`#(id){value}`, not just
    // whichever piece happens to follow a later candidate) --
    // `list_item_sugar_reads_a_trailing_id_and_attrs_together` is
    // exactly this: starting at the `{` alone would "succeed" too, but
    // only by skipping straight past the `#(id)` in front of it.
    candidates.into_iter().find(|&pos| {
        let mut probe = *cur;
        probe.set_pos(pos);
        let mut scratch = element_new(Sigil::Bare);
        if parse_groups_with_pipe_stack(&mut probe, &mut scratch, true, &[]).is_err()
            || !claimed_something(&scratch)
        {
            return false;
        }
        skip_inline_ws(&mut probe);
        matches!(probe.peek(), None | Some('\n') | Some('\r')) && content_lands_at(cur, pos)
    })
}

/// What a bracket-less sugar body (`- x`, `= x`) found for its own
/// `args`/`value`/`id`/`connects`, alongside its `content` -- merged
/// into the caller's constructed item/heading exactly the way any other
/// element's own slots are.
pub(crate) struct SugarBody {
    pub content: Vec<Inline>,
    pub args: Option<Value>,
    pub value: Option<ElementValue>,
    pub id: Option<Id>,
    pub connects: Vec<Element>,
}

impl SugarBody {
    fn from_scratch(content: Vec<Inline>, scratch: Element) -> Self {
        SugarBody {
            content,
            args: scratch.args,
            value: scratch.value,
            id: scratch.id,
            connects: scratch.connects,
        }
    }
}

/// Reads a bracket-less body: inline content to the end of the line,
/// plus whatever trailing connect that line (or, failing that, the next
/// one) hands over to its owner.
///
/// This is the sugar shared by `-` and `=`. The content itself is
/// single-line on purpose: content that spans lines has to say so with
/// an explicit `[ ... ]` group, which is what [`parse_groups`] reads.
/// The trailing connect is not bound by that: once a line's content ends
/// with nothing of the owner's own found on it,
/// [`parse_groups_with_pipe_stack`] is tried again right there, which is
/// what lets it tolerate the same single-blank-line gap every other
/// sigil's groups do -- `- content` followed by `  :{}` on the next line
/// connects exactly as `- content :{}` does on one.
pub(crate) fn parse_sugar_body(cur: &mut Cursor) -> Result<SugarBody> {
    if let Some(connect_pos) = find_trailing_connect(cur) {
        let content = parse_inline_seq(cur, Stop::Offset(connect_pos), false)?;
        cur.set_pos(connect_pos);
        let mut scratch = element_new(Sigil::Bare);
        parse_groups_with_pipe_stack(cur, &mut scratch, true, &[])?;
        // `find_trailing_connect` only ever starts a candidate at `:`,
        // `(`, `{` or `#(`, never at `[`/`|` -- but a *second*, colon-less
        // group read further along the same candidate's run still could
        // be (`:(x:1)[y]`, say). A bracket-less sugar body has nowhere
        // of its own to put that `[...]`/`|...`, so treat it the same as
        // not finding a connect at all rather than silently losing it.
        if scratch.content.is_some() {
            cur.set_pos(connect_pos);
            let content = parse_inline_seq(cur, Stop::Line, false)?;
            return Ok(SugarBody::from_scratch(content, element_new(Sigil::Bare)));
        }
        return Ok(SugarBody::from_scratch(content, scratch));
    }

    // Plain content, nothing of the owner's own on this line -- try a
    // trailing connect after a gap (the same single-blank-line tolerance
    // `parse_groups` grants every other sigil) -- `- content` followed
    // by `  :{}` on the next line connects exactly as `- content :{}`
    // does on one.
    let content = parse_inline_seq(cur, Stop::Line, false)?;
    let after_content = cur.pos();
    let mut scratch = element_new(Sigil::Bare);
    // Confirm what follows the gap is actually a connect opener before
    // calling into the real reader: a bracket-less sugar body has no
    // `content`/`children` field of its own to hold a `[...]`/`|...`
    // group, but `parse_groups_with_pipe_stack`'s ordinary `[`/`|` arms
    // fire unconditionally whenever a fresh scratch element's `content`
    // slot is empty (which it always is here) -- so without this guard,
    // a *following* pipe-marked list continuation (`current_stack`'s own
    // next `|`, not this scratch's) would get silently read and
    // discarded as if it belonged to this line instead. Only `:`,
    // `(`, `{` and `#(` are the connect family; `[`/`|` are never routed
    // here at all.
    let mut probe = *cur;
    let is_connect_opener = skip_element_gap(&mut probe) <= 1
        && (matches!(probe.peek(), Some('(') | Some('{') | Some(':'))
            || (probe.peek() == Some('#') && probe.peek_at(1) == Some('(')));
    if is_connect_opener {
        parse_groups_with_pipe_stack(cur, &mut scratch, true, &[])?;
        // Same guard as above, for the same reason: a second, colon-less
        // group within this gap-tolerant read could still reach `[`/`|`.
        if scratch.content.is_some() {
            cur.set_pos(after_content);
            return Ok(SugarBody::from_scratch(content, element_new(Sigil::Bare)));
        }
    }
    Ok(SugarBody::from_scratch(content, scratch))
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

/// `#(foobar)` -- an element's own id. Eats the `#`, then delegates to
/// [`parse_id_scalar`] for the parenthesized content.
///
/// Deliberately narrower than [`parse_paren_value`]: an id is a single
/// scalar, not the general value grammar, so `#(key: value)` and
/// `#(list(1, 2))` are parse errors here rather than silently admitting
/// a map/seq/call that could never mean anything as an id.
pub(crate) fn parse_hash_id(cur: &mut Cursor) -> Result<Id> {
    cur.bump(); // eat '#'
    if !cur.eat_str("(") {
        return Err(err(cur, cur.pos(), "expected '(' after '#'"));
    }
    skip_ws_newlines_and_comments(cur);
    let s = parse_id_scalar(cur)?;
    skip_ws_newlines_and_comments(cur);
    if !cur.eat_str(")") {
        return Err(err(cur, cur.pos(), "expected ')'"));
    }
    Ok(Id(s))
}

/// A single scalar: a quoted string, or a bare run of text (same raw
/// reader every other bare scalar value uses). Unlike [`parse_value_at`],
/// never infers a non-string type -- `#(123)` is the id `"123"`, not an
/// integer, since [`Id`] is always a plain string.
fn parse_id_scalar(cur: &mut Cursor) -> Result<String> {
    if cur.peek() == Some('"') {
        return parse_quoted(cur);
    }
    let raw = eat_scalar_raw(cur).trim_end();
    if raw.is_empty() {
        return Err(err(cur, cur.pos(), "expected an id"));
    }
    Ok(raw.to_string())
}

/// Parses a `|`-prefixed content run: `[content]` spelled without brackets,
/// as a block sequence.
///
/// A marked line that bare-opens a block element (`@name...` ending its
/// own line, same test `document.rs`'s own top-level loop uses) becomes its
/// own `Block::Element`, exactly as it would inside `[...]` -- this is what
/// keeps `tests/src/pipe.rs`'s parity check passing for something like
/// `@references` holding several `@link`s in a row, each its own block
/// rather than all three merged into one `Paragraph` by `SoftBreak`s. A
/// line that is the marker alone, nothing after it, ends the current
/// paragraph without ending the run -- the only way `|content` expresses
/// more than one paragraph, since a line with no marker at all always ends
/// the whole run (`crate::inline::PipeContinuation`).
///
/// Multi-level nesting (an element's own `|content` living inside another
/// element's `|content`, each level adding its own marker column) is
/// handled by tracking `parent_stack` and pushing each active marker's column.
fn parse_pipe_content(cur: &mut Cursor, parent_stack: &[usize]) -> Result<Vec<Block>> {
    let (_, col) = cur.line_col(cur.pos());
    if !cur.eat_str("|") {
        return Err(err(cur, cur.pos(), "expected '|'"));
    }
    let mut current_stack = parent_stack.to_vec();
    current_stack.push(col);
    let mut blocks = Vec::new();
    loop {
        skip_inline_ws(cur);
        if cur.is_eof() {
            break;
        }
        if cur.peek() == Some('@')
            && is_element_start(cur, true)
            && element_ends_line(cur) == LineEnd::Bare
        {
            let el = parse_element_with_pipe_stack(cur, true, &current_stack)?
                .with_placement(Placement::Block);
            blocks.push(Block::Element(el));
        } else if let Some(head) = crate::list::peek_list_marker(cur)? {
            let ordered = head.ordered;
            let mut items = Vec::new();
            let mut next_head = head;
            loop {
                let item = crate::list::parse_list_item_body(cur, &next_head, &current_stack)?;
                items.push(item);
                skip_inline_ws(cur);
                if cur.is_eof() {
                    break;
                }
                match crate::inline::pipe_run_state(cur, &current_stack)? {
                    crate::inline::PipeContinuation::Content => {
                        let mut probe = *cur;
                        probe.bump();
                        for _ in 0..current_stack.len() {
                            skip_inline_ws(&mut probe);
                            probe.bump();
                        }
                        skip_inline_ws(&mut probe);
                        if let Ok(Some(h)) = crate::list::peek_list_marker(&probe) {
                            if h.ordered == ordered {
                                *cur = probe;
                                next_head = h;
                                continue;
                            }
                        }
                        break;
                    }
                    _ => break,
                }
            }
            let list_span = items
                .first()
                .unwrap()
                .element
                .span
                .union(&items.last().unwrap().element.span);
            let l = tomet_tree::list(ordered, items, list_span);
            blocks.push(Block::List(l));
        } else {
            let para_start = cur.pos();
            let content = parse_inline_seq(
                cur,
                Stop::PipeRun {
                    cols: &current_stack,
                },
                true,
            )?;
            if !content.is_empty() {
                let span = cur.span_from(para_start);
                blocks.push(Block::Paragraph(Paragraph::new(content, span)));
            }
        }
        if cur.is_eof() {
            break;
        }
        // After either branch, `cur` sits at the `\n` that ends the line
        // just consumed -- the next thing to check either way.
        skip_inline_ws(cur);
        match crate::inline::pipe_run_state(cur, &current_stack)? {
            crate::inline::PipeContinuation::End => break,
            crate::inline::PipeContinuation::Empty | crate::inline::PipeContinuation::Content => {
                cur.bump(); // the newline
                for _ in 0..current_stack.len() {
                    skip_inline_ws(cur);
                    cur.bump(); // the marker itself
                }
                skip_inline_ws(cur);
            }
        }
    }
    Ok(blocks)
}

/// Parses a `[content]` group as a block sequence -- the same recursive
/// grammar `Document.blocks`/`Section.blocks` use (sections, lists,
/// elements, paragraphs), bounded by the closing `]` instead of EOF. A bare
/// run with no block markers inside comes out as a single `Paragraph`, so
/// ordinary inline usage (`@link(...)[Tomet]`) is unaffected; see
/// `tmtroot/docs/spec/feature/content-shape.tmt`.
fn parse_content(cur: &mut Cursor) -> Result<Vec<Block>> {
    if !cur.eat_str("[") {
        return Err(err(cur, cur.pos(), "expected '['"));
    }
    let content = crate::document::parse_block_seq(cur, crate::document::BlockStop::Bracket(']'))?;
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
