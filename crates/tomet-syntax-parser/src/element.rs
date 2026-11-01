//! What opens an element, and the values a group holds.
//!
//! Where an element starts and what makes `@` a sigil rather than a character
//! ([`is_element_start`]), which characters open a group ([`opens_group`]), and
//! the readers of the groups' values: `(args)` ([`parse_paren_value`]), `{value}`
//! ([`parse_value_group`]) and `#(id)` ([`parse_hash_id`]). They read the text of
//! a group; the shape of what a group embeds is the CST's (see [`Embedded`]).

use crate::dialect::Dialect;
use crate::error::Result;
use crate::inline::at_line_start;
use crate::value::{
    Embedded, POSITIONAL_ENTRY_KEY, eat_name, eat_scalar_raw, err, is_name_start_at, parse_quoted,
    skip_block_comment, skip_inline_ws, skip_line_comment, skip_ws_newlines_and_comments,
};
use tomet_ast::{Bare, Block, Connect, Data, Entry, Group, Id, Placement, Span, Value};
use tomet_cst::{SyntaxKind as K, SyntaxNode};
use tomet_lexer::Cursor;
use tomet_tree::ElementExt;

/// Whether `cur` starts an element: `@name`, or in a pattern `?matcher(`.
///
/// A hole (`?`) is only an element in [`Dialect::Pattern`]; see [`is_hole_tail`].
///
/// `block_context` is whether the cursor sits where a block may begin --
/// the document's top level, or a line start inside another element's
/// `[content]`. It only widens what may follow the name, never the sigil
/// or the name itself:
///
/// - anywhere: a group (see [`opens_group`]), or a `:` that a `:name`
///   family member follows. This is what keeps `me@example.com` and a
///   lone `@foo` in running prose as plain text.
/// - in block context only: end-of-line also counts, so `@memo` alone on
///   its line is an element. It then fails later, in `tomet-semantics`,
///   as an unknown bare name -- the intended report, and the reason no
///   hashtag-style syntax exists.
///
/// The old `#name` spelling had the end-of-line allowance bound to the
/// sigil rather than to the position. There is one element sigil now, so
/// the allowance belongs to the position.
pub(crate) fn is_element_start(dialect: Dialect, cur: &Cursor, block_context: bool) -> bool {
    let mut look = *cur;
    match look.bump() {
        Some('@') => name_and_tail_start(look, block_context),
        Some('?') => dialect == Dialect::Pattern && is_hole_tail(look),
        _ => false,
    }
}

/// What has to follow a `?` for it to be a hole and not a question mark: an
/// optional matcher name, then `(` with nothing between. Parens are always
/// written, so `?name` alone, or `?name {...}`, is text, and so is `really? (yes)`.
fn is_hole_tail(mut look: Cursor) -> bool {
    let _ = eat_name(&mut look);
    look.peek() == Some('(')
}

/// Whether a declaration about the document, `::name`, starts at `cur`.
///
/// Only a block can be one -- the `::` has to open a line -- so there is no
/// `block_context` to pass: every call site is a block dispatch. Nowhere else
/// asks, so in running text, in a `{...}` entry list, `std::vec` and a `::`
/// in the middle of a line stay what they were: text.
///
/// What follows the `::` is read exactly as what follows an `@`.
pub(crate) fn is_document_element_start(cur: &Cursor) -> bool {
    let mut look = *cur;
    look.eat_str("::") && name_and_tail_start(look, true)
}

/// The part of [`is_element_start`] after the sigil: the name, and what has to
/// follow it for the sigil to start an element rather than prose.
fn name_and_tail_start(mut look: Cursor, block_context: bool) -> bool {
    // The name is mandatory. A nameless `@(url:...)` used to infer its
    // kind from an args key; that inference was retired in favour of the
    // one `@link(target:...)` element, but the syntax outlived it and
    // kept parsing into a meaningless `Custom("at")`. It is text now.
    if !is_name_start_at(&look) || eat_name(&mut look).is_none() {
        return false;
    }
    // `@memo` alone on its line: asked before the gap is skipped, which reads
    // past the line end and would take whatever follows for what follows the name.
    if block_context {
        let mut rest = look;
        skip_inline_ws(&mut rest);
        while rest.starts_with("//") || rest.starts_with("/*") {
            if rest.starts_with("//") {
                skip_line_comment(&mut rest);
            } else if skip_block_comment(&mut rest).is_err() {
                break;
            }
            skip_inline_ws(&mut rest);
        }
        if matches!(rest.peek(), None | Some('\n') | Some('\r')) {
            return true;
        }
    }
    skip_element_gap(&mut look);
    if opens_group(&look) {
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
    false
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

/// What [`parse_groups`] reads: the groups every node takes, before the node
/// that owns them claims the ones it accepts.
///
/// It is not a node. Which of an element, a hole, a declaration, a bare
/// entry, a list item or a section it becomes is decided by the caller, and
/// what that role does not take (a declaration's `:connect`, a connect's
/// `#(id)`) is refused there, so the reading of a group is written once.
#[derive(Default)]
pub(crate) struct Groups {
    pub data: Data,
    pub content: Option<Vec<Block>>,
    pub id: Option<Id>,
    pub connects: Vec<Connect>,
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
/// [`element_stands_alone`] does, which leaves one definition per
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

pub(crate) fn parse_paren_value(
    hook: &mut impl cotomet_syntax::ValueHook,
    cur: &mut Cursor,
) -> Result<Value> {
    if !cur.eat_str("(") {
        return Err(err(cur.pos(), "expected '('"));
    }
    skip_ws_newlines_and_comments(cur);
    let v = if cur.peek() == Some(')') {
        Value::Map(Vec::new())
    } else {
        cotomet_syntax::parse_value_at_with(cur, hook)?
    };
    skip_ws_newlines_and_comments(cur);
    if !cur.eat_str(")") {
        return Err(err(cur.pos(), "expected ')'"));
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
        return Err(err(cur.pos(), "expected '(' after '#'"));
    }
    skip_ws_newlines_and_comments(cur);
    let s = parse_id_scalar(cur)?;
    skip_ws_newlines_and_comments(cur);
    if !cur.eat_str(")") {
        return Err(err(cur.pos(), "expected ')'"));
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
        return Err(err(cur.pos(), "expected an id"));
    }
    Ok(raw.to_string())
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
/// A non-map body (`{[1,2,3]}`, `{"str"}`, `{bare}`) is rejected: `{...}`
/// is always a uniform-entry data group, never a bare scalar/array
/// literal. A bare sequence of values belongs in `(args)`'s own positional
/// slot instead (`@tags(1, 2, 3)`, already a `Seq` there) -- there is no
/// live spelling anywhere in the corpus that needs `{...}` itself to hold
/// one.
pub(crate) fn parse_value_group(
    dialect: Dialect,
    embedded: &mut Embedded,
    cur: &mut Cursor,
) -> Result<Group> {
    let group_start = cur.pos();
    if !cur.eat_str("{") {
        return Err(err(cur.pos(), "expected '{'"));
    }
    let mut entries = Vec::new();
    loop {
        skip_ws_newlines_and_comments(cur);
        match cur.peek() {
            None => return Err(err(group_start, "unterminated '{', expected '}'")),
            Some('}') => break,
            Some('(') => entries.push(Entry::Bare(parse_bare_entry(embedded, cur)?)),
            // A named entry, `@args{ ... }`. Unambiguous: `is_ident_char`
            // excludes `@`, so a map key cannot begin with one, and this
            // position errors today -- `eat_ident` returns empty and the
            // `POSITIONAL_ENTRY_KEY` rejection below takes over. So this
            // arm only turns errors into parses.
            //
            // `block_context: false`: a group entry still has to carry a
            // group or a `:` after the name, so a bare `@foo`
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
            // there is no paragraph here for a `;` to break or a line to
            // fold into: an element that is the whole of its line is a block.
            _ if is_element_start(dialect, cur, at_line_start(cur)) => {
                let at_start = at_line_start(cur);
                let Some(node) = embedded.node_at(cur.pos()) else {
                    return Err(err(cur.pos(), "an element the CST did not read here"));
                };
                let (value, end) = crate::lower::lower_embedded(embedded.src(), node)?;
                cur.set_pos(end);
                // The whole of its line: nothing but white space and comments after it.
                let block = at_start && ends_its_line(cur);
                let entry = match value {
                    Value::Hole(mut hole) => {
                        if block {
                            hole.placement = Placement::Block;
                        }
                        Entry::Hole(*hole)
                    }
                    Value::Element(element) => Entry::Element(if block {
                        (*element).with_placement(Placement::Block)
                    } else {
                        *element
                    }),
                    _ => unreachable!("lower_embedded of an element node"),
                };
                entries.push(entry);
            }
            _ => {
                let entry_start = cur.pos();
                let (key, value) = cotomet_syntax::parse_one_entry_with(cur, embedded)?;
                if key == POSITIONAL_ENTRY_KEY {
                    return Err(err(
                        entry_start,
                        "a '{...}' group holds 'key: value' entries or elements; \
                         write a bare value in '(args)' instead",
                    ));
                }
                if entries
                    .iter()
                    .any(|e| matches!(e, Entry::Pair(k, _) if *k == key))
                {
                    return Err(cotomet_syntax::duplicate_key_error(entry_start, &key).into());
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
        return Err(err(cur.pos(), "expected '}'"));
    }
    Ok(Group(entries))
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
fn parse_bare_entry(embedded: &Embedded, cur: &mut Cursor) -> Result<Bare> {
    let start_pos = cur.pos();
    let src = cur.src();
    let mut groups: Vec<SyntaxNode> = Vec::new();
    let mut end = start_pos;
    loop {
        let mut look = *cur;
        if skip_element_gap(&mut look) > 1 {
            break;
        }
        // Each group once: a second `(args)` is the next entry's.
        let Some(node) = embedded.node_at(look.pos()).filter(|n| {
            matches!(
                n.kind(),
                K::ARGS | K::VALUE_DATA | K::MAP_LITERAL | K::CONTENT | K::ID_GROUP
            ) && !groups.iter().any(|taken| taken.kind() == n.kind())
        }) else {
            break;
        };
        end = usize::from(node.text_range().end());
        cur.set_pos(end);
        groups.push(node.clone());
    }
    let read = crate::lower::lower_group_list(src, groups)?;
    Ok(Bare {
        data: read.data,
        content: read.content,
        id: read.id,
        span: Span::new(start_pos, end),
    })
}

/// Whether nothing but white space and comments follows `cur` on its line.
fn ends_its_line(cur: &Cursor) -> bool {
    let mut probe = *cur;
    skip_inline_ws(&mut probe);
    while probe.starts_with("//") || probe.starts_with("/*") {
        if probe.starts_with("//") {
            skip_line_comment(&mut probe);
        } else if skip_block_comment(&mut probe).is_err() {
            return false;
        }
        skip_inline_ws(&mut probe);
    }
    matches!(probe.peek(), None | Some('\n') | Some('\r'))
}
