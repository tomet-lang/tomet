//! Parsing for inline sequences, text normalization, and inline delimiters (`*em*`, `**strong**`, `==mark==`).

use crate::caret::{is_caret_start, parse_caret_element};
use crate::codeblock::is_fenced_code_block_start;
use crate::element::{LineEnd, element_ends_line, is_element_start, parse_element};
use crate::error::Result;
use crate::interp::{is_interp_start, parse_dollar_element};
use crate::list::peek_list_marker;
use crate::section::{is_thematic_break, is_titled_thematic_break_start};
use crate::value::{err, skip_block_comment, skip_inline_ws, skip_line_comment};
use tomet_ast::{Element, Inline, Placement, RawText, Sigil, SoftBreak, Span, Text, Value};
use tomet_lexer::Cursor;
use tomet_tree::{ElementExt, element_new};

#[derive(Debug, Clone, Copy)]
pub(crate) enum Stop {
    Bracket(char),
    Paragraph,
    Line,
    Offset(usize),
    Delim(&'static str),
    /// A `|`-prefixed run: `[content]` spelled without brackets. `col` is
    /// the 1-based column of the opening `|`, and a following line stays in
    /// the run only when its own `|` stands in that same column.
    PipeRun {
        col: usize,
    },
}

/// `allow_colon_connect` is threaded straight through to every element
/// parsed here (including recursively, inside `**em**`/`__strong__`/
/// `==mark==` spans) -- see `element::parse_element`'s own doc comment
/// for what it gates and why. Only `list.rs::parse_list_internal`'s two
/// top-level calls (a list item's own inline content) pass `false`;
/// every other caller passes `true`, unchanged from before this
/// parameter existed.
pub(crate) fn parse_inline_seq(
    cur: &mut Cursor,
    stop: Stop,
    allow_colon_connect: bool,
) -> Result<Vec<Inline>> {
    let mut items = Vec::new();
    let mut text_start = cur.pos();
    let mut bracket_depth: u32 = 0;
    // A `|` run's markers are folded away with the newline they follow, so
    // the run's text stays one contiguous slice of the source and comes out
    // identical to the same content written between brackets.
    let fold_pipes = matches!(stop, Stop::PipeRun { .. });
    loop {
        match stop {
            Stop::Bracket(c) => {
                if cur.is_eof() {
                    return Err(err(cur, cur.pos(), format!("unterminated, expected '{c}'")));
                }
                if cur.peek() == Some(c) {
                    if bracket_depth == 0 {
                        break;
                    }
                    bracket_depth -= 1;
                } else if cur.peek() == Some('[') {
                    bracket_depth += 1;
                }
            }
            Stop::Line => {
                if cur.is_eof() || cur.peek() == Some('\n') {
                    break;
                }
            }
            Stop::Offset(target_pos) => {
                if cur.pos() >= target_pos || cur.is_eof() {
                    break;
                }
            }
            Stop::Paragraph => {
                if cur.is_eof() {
                    break;
                }
                if cur.peek() == Some('\n') {
                    // A trailing `\` right before this newline is a
                    // continuation marker -- possibly redundant, since
                    // this run may already be open, in which case it is
                    // this element's own trailing form of the same
                    // marker `document.rs`'s generic `@`-element branch
                    // never gets a chance to see or consume, because an
                    // element already inside an open run is parsed with
                    // no trailing-marker awareness at all. Either way it
                    // must never show up as literal text: strip it from
                    // what gets flushed, whether the run ends here or
                    // continues.
                    let pending = &cur.src()[text_start..cur.pos()];
                    let trimmed = pending.trim_end_matches([' ', '\t']);
                    if let Some(content) = trimmed.strip_suffix('\\') {
                        let content_end = text_start + content.trim_end_matches([' ', '\t']).len();
                        flush_text_upto(&mut items, cur, &mut text_start, content_end, fold_pipes);
                        text_start = cur.pos();
                    }

                    let mut look = *cur;
                    look.bump();
                    skip_inline_ws(&mut look);
                    // A leading `\` on the next line is likewise
                    // redundant here for the same reason -- fold it away
                    // below the same way `|`'s own repeated marker never
                    // becomes literal text.
                    let redundant_leading = look.peek() == Some('\\');
                    if redundant_leading {
                        look.bump();
                        skip_inline_ws(&mut look);
                    }
                    if paragraph_breaks_here(&look) {
                        break;
                    }
                    if redundant_leading {
                        // Flush up to (and including, as a `SoftBreak`)
                        // the newline, then silently skip the marker so
                        // it never shows up as literal text either.
                        let break_start = cur.pos();
                        flush_text(&mut items, cur, &mut text_start, fold_pipes);
                        cur.bump();
                        skip_inline_ws(cur);
                        cur.bump();
                        skip_inline_ws(cur);
                        push_soft_break(&mut items, cur.span_from(break_start));
                        text_start = cur.pos();
                        continue;
                    }
                }
            }
            Stop::Delim(d) => {
                if cur.is_eof() {
                    return Err(err(cur, cur.pos(), format!("unterminated, expected '{d}'")));
                }
                if cur.starts_with(d) && !is_boundary(char_before(cur)) {
                    break;
                }
                if cur.peek() == Some('\n') {
                    let mut look = *cur;
                    look.bump();
                    skip_inline_ws(&mut look);
                    if look.is_eof() || look.peek() == Some('\n') {
                        return Err(err(cur, cur.pos(), format!("unterminated, expected '{d}'")));
                    }
                }
            }
            Stop::PipeRun { col } => {
                if cur.is_eof() {
                    break;
                }
                if cur.peek() == Some('\n') && !pipe_run_continues(cur, col)? {
                    break;
                }
            }
        }
        if cur.peek() == Some('`') {
            let before = cur.pos();
            let mut probe = *cur;
            let fence_len = probe.eat_while(|c| c == '`').len();
            let inner_start = probe.pos();
            let mut closed = false;
            let mut inner_end = inner_start;
            while let Some(c) = probe.peek() {
                if c == '\n' || c == '\r' {
                    break;
                }
                if c == '`' {
                    let run_start = probe.pos();
                    let run_len = probe.eat_while(|ch| ch == '`').len();
                    if run_len == fence_len {
                        inner_end = run_start;
                        closed = true;
                        break;
                    }
                } else {
                    probe.bump();
                }
            }
            if closed {
                flush_text_upto(&mut items, cur, &mut text_start, before, fold_pipes);
                let inner_text = cur.src()[inner_start..inner_end].to_string();
                let span = cur.span_from(before);
                let content_span =
                    Span::new(cur.position_at(inner_start), cur.position_at(inner_end));
                let el = element_new(Sigil::named("raw"))
                    .with_placement(Placement::Inline)
                    .with_span(span)
                    .with_content(vec![Inline::Raw(RawText::new(inner_text, content_span))]);
                items.push(Inline::Element(el));
                cur.set_pos(probe.pos());
                text_start = cur.pos();
                continue;
            }
        }
        if is_autolink_start(cur) {
            let before = cur.pos();
            if let Some(el) = try_autolink(cur, stop)? {
                flush_text_upto(&mut items, cur, &mut text_start, before, fold_pipes);
                items.push(Inline::Element(el));
                text_start = cur.pos();
                continue;
            }
        }
        if cur.starts_with("/*") {
            flush_text(&mut items, cur, &mut text_start, fold_pipes);
            skip_block_comment(cur)?;
            text_start = cur.pos();
            continue;
        }
        if cur.starts_with("//") && is_boundary(char_before(cur)) {
            flush_text(&mut items, cur, &mut text_start, fold_pipes);
            skip_line_comment(cur);
            text_start = cur.pos();
            continue;
        }
        // The placement rule applies inside a content group as well: a
        // line start there is block context, so an element that also ends
        // its line stands as a block. That is what lets `@references[`
        // hold entries on their own lines, while `@link(…)[x]` mid-line
        // stays part of the running text.
        //
        // A paragraph is the exception, and the reason this depends on
        // `stop` at all: its continuation lines are *inside* running text,
        // so a line start there is not block context. Treating it as one
        // is what used to tear a wrapped sentence into separate blocks.
        //
        // A leading `\` continuation trigger (`docs/spec/syntax.tmt`'s
        // `##[ 継続 ]`) demotes the previous element -- still the last
        // thing pushed to `items` at this point, nothing has flushed the
        // gap between them yet -- from `Placement::Block` to `Inline`,
        // joining it to what follows. Dangling (nothing before it, or it
        // is already `Inline`) is a silent no-op, which also covers a
        // redundant repeated marker.
        if matches!(stop, Stop::Bracket(_)) && at_line_start(cur) && cur.peek() == Some('\\') {
            if let Some(Inline::Element(prev)) = items.last_mut() {
                prev.placement = Placement::Inline;
            }
            cur.bump();
            skip_inline_ws(cur);
            text_start = cur.pos();
            continue;
        }
        if cur.peek() == Some('@') {
            let block_context = match stop {
                Stop::Bracket(_) => at_line_start(cur),
                Stop::PipeRun { .. } => at_marked_line_start(cur),
                _ => false,
            };
            if is_element_start(cur, block_context) {
                let line_end = if block_context {
                    element_ends_line(cur)
                } else {
                    LineEnd::No
                };
                // Join/isolate rule (`docs/spec/syntax.tmt`'s
                // `##[ 継続 ]`): a bare element that opens its own line
                // isolates by default (`Placement::Block`) the same way
                // it does at the top level; only an explicit trailing
                // `\` joins it to what follows.
                flush_text(&mut items, cur, &mut text_start, fold_pipes);
                let el = parse_element(cur, allow_colon_connect)?;
                if line_end == LineEnd::Continuation {
                    crate::element::consume_trailing_continuation(cur);
                }
                let placement = if line_end == LineEnd::Bare {
                    Placement::Block
                } else {
                    Placement::Inline
                };
                items.push(Inline::Element(el.with_placement(placement)));
                text_start = cur.pos();
                continue;
            }
        }
        if cur.peek() == Some('$') && is_interp_start(cur) {
            flush_text(&mut items, cur, &mut text_start, fold_pipes);
            items.push(Inline::Element(parse_dollar_element(cur)?));
            text_start = cur.pos();
            continue;
        }
        if cur.starts_with("#(") {
            flush_text(&mut items, cur, &mut text_start, fold_pipes);
            items.push(Inline::Element(parse_tag_sugar(cur)?));
            text_start = cur.pos();
            continue;
        }
        if cur.peek() == Some('^') && is_caret_start(cur) {
            flush_text(&mut items, cur, &mut text_start, fold_pipes);
            items.push(Inline::Element(parse_caret_element(
                cur,
                allow_colon_connect,
            )?));
            text_start = cur.pos();
            continue;
        }
        if matches!(cur.peek(), Some('*') | Some('_') | Some('~')) {
            let before = cur.pos();
            if let Some(el) = try_delimited(cur, allow_colon_connect)? {
                flush_text_upto(&mut items, cur, &mut text_start, before, fold_pipes);
                items.push(Inline::Element(el));
                text_start = cur.pos();
                continue;
            }
        }
        if cur.bump().is_none() {
            break;
        }
    }
    flush_text(&mut items, cur, &mut text_start, fold_pipes);
    Ok(trim_edges(items))
}

/// Whether a paragraph ends right after the newline `look` is already
/// positioned past, with inline whitespace also already skipped.
///
/// This is the stopping condition for a run already open (from genuine
/// prose, or from an explicit `\` continuation trigger --
/// `docs/spec/syntax.tmt`'s `##[ 継続 ]`): a blank line, a heading, a list
/// marker, a comment, a thematic break, or a fenced code block all start
/// something with its own identity, not more of this paragraph. It is not
/// consulted to decide whether a fresh bare element joins what precedes
/// it -- that is opt-in now, via the trigger, not a default this
/// look-ahead governs.
pub(crate) fn paragraph_breaks_here(look: &Cursor) -> bool {
    look.is_eof()
        || look.peek() == Some('\n')
        || (look.peek() == Some('=') && crate::section::is_section_start(look))
        || matches!(peek_list_marker(look), Ok(Some(_)))
        || look.starts_with("//")
        || look.starts_with("/*")
        || is_titled_thematic_break_start(look)
        || is_thematic_break(look)
        || is_fenced_code_block_start(look)
}

/// Trims leading/trailing whitespace from a finished inline sequence.
///
/// A leading or trailing [`Inline::SoftBreak`] is whitespace by definition
/// (see its doc comment), so it is dropped outright, the same as an
/// all-whitespace edge `Text`. The two can alternate -- e.g. a run that
/// starts `"  \nfoo"` flushes as `[Text("  "), SoftBreak, Text("foo")]` --
/// so each side loops until it hits real content, matching what folding the
/// whole edge into one string and calling `trim_start`/`trim_end` on it used
/// to do in one step.
fn trim_edges(mut items: Vec<Inline>) -> Vec<Inline> {
    loop {
        match items.first() {
            Some(Inline::SoftBreak(_)) => {
                items.remove(0);
            }
            Some(Inline::Text(t)) => {
                let trimmed = t.value.trim_start();
                if trimmed.is_empty() {
                    items.remove(0);
                } else {
                    if trimmed.len() != t.value.len() {
                        let trimmed = trimmed.to_string();
                        if let Some(Inline::Text(t)) = items.first_mut() {
                            t.value = trimmed;
                        }
                    }
                    break;
                }
            }
            _ => break,
        }
    }
    loop {
        match items.last() {
            Some(Inline::SoftBreak(_)) => {
                items.pop();
            }
            Some(Inline::Text(t)) => {
                let trimmed = t.value.trim_end();
                if trimmed.is_empty() {
                    items.pop();
                } else {
                    if trimmed.len() != t.value.len() {
                        let trimmed = trimmed.to_string();
                        if let Some(Inline::Text(t)) = items.last_mut() {
                            t.value = trimmed;
                        }
                    }
                    break;
                }
            }
            _ => break,
        }
    }
    items
}

fn parse_tag_sugar(cur: &mut Cursor) -> Result<Element> {
    let start_pos = cur.pos();
    cur.bump(); // eat '#'
    let args = crate::element::parse_paren_value(cur)?;
    let span = cur.span_from(start_pos);
    let mut el = element_new(Sigil::named("tag"))
        .with_placement(Placement::Inline)
        .with_span(span);
    el.args = Some(args);
    Ok(el)
}

const DELIMITERS: [(&str, &str); 5] = [
    ("**", "strong"),
    ("__", "strong"),
    ("~~", "strikeout"),
    ("*", "em"),
    ("_", "em"),
];

fn char_before(cur: &Cursor) -> Option<char> {
    cur.src()[..cur.pos()].chars().next_back()
}

fn is_boundary(c: Option<char>) -> bool {
    matches!(c, None | Some(' ') | Some('\t') | Some('\n') | Some('\r'))
}

fn try_delimited(cur: &mut Cursor, allow_colon_connect: bool) -> Result<Option<Element>> {
    for (delim, kind) in DELIMITERS {
        if let Some(el) = try_one_delimited(cur, delim, kind, allow_colon_connect)? {
            return Ok(Some(el));
        }
    }
    Ok(None)
}

fn try_one_delimited(
    cur: &mut Cursor,
    delim: &'static str,
    kind: &str,
    allow_colon_connect: bool,
) -> Result<Option<Element>> {
    let start_pos = cur.pos();
    if !cur.starts_with(delim) {
        return Ok(None);
    }
    if delim.starts_with('_') && char_before(cur).is_some_and(|c| c.is_alphanumeric()) {
        return Ok(None);
    }
    let mut open = *cur;
    open.eat_str(delim);
    if is_boundary(open.peek()) {
        return Ok(None);
    }
    let mut probe = open;
    loop {
        if probe.starts_with(delim) && (delim == "~~" || !is_boundary(char_before(&probe))) {
            break;
        }
        if probe.is_eof() {
            return Ok(None);
        }
        if probe.peek() == Some('\n') {
            let mut look = probe;
            look.bump();
            skip_inline_ws(&mut look);
            if look.is_eof() || look.peek() == Some('\n') {
                return Ok(None);
            }
        }
        probe.bump();
    }

    cur.set_pos(open.pos());
    let inner = parse_inline_seq(cur, Stop::Delim(delim), allow_colon_connect)?;
    if !cur.eat_str(delim) {
        return Err(err(cur, cur.pos(), format!("expected '{delim}'")));
    }
    let span = cur.span_from(start_pos);
    let mut el = element_new(Sigil::named(kind)).with_span(span);
    el.content = Some(inner);
    Ok(Some(el))
}

/// Whether only whitespace precedes `cur` on its line.
pub(crate) fn at_line_start(cur: &Cursor) -> bool {
    let src = cur.src();
    let before = &src[..cur.pos()];
    match before.rfind('\n') {
        Some(nl) => before[nl + 1..].trim().is_empty(),
        None => before.trim().is_empty(),
    }
}

/// Whether the line after the newline at `cur` continues a `|` run opened in
/// column `col`.
///
/// The marker is left where it is rather than consumed: `normalize_text`
/// folds it away with the newline, which is what keeps the run's text one
/// contiguous slice of the source.
///
/// A `|` in the wrong column is an error, not the end of the run. Ending
/// quietly would drop the line into prose -- the failure that made a wrapped
/// list item break every export -- and once lists nest, the column is the
/// only thing that says which content a marker belongs to.
fn pipe_run_continues(cur: &Cursor, col: usize) -> Result<bool> {
    let mut look = *cur;
    look.bump();
    skip_inline_ws(&mut look);
    // A blank line closes every block, this one included.
    if matches!(look.peek(), None | Some('\n') | Some('\r')) {
        return Ok(false);
    }
    if look.peek() != Some('|') {
        return Ok(false);
    }
    let (_, marker_col) = look.line_col(look.pos());
    if marker_col != col {
        return Err(err(
            &look,
            look.pos(),
            format!(
                "this `|` stands in column {marker_col}, and the content it would \
                 continue opens in column {col}: it lines up with no content"
            ),
        ));
    }
    Ok(true)
}

/// [`at_line_start`] for a `|` run: the marker is the line's left edge, not
/// content, so an element standing after one is at a line start exactly as
/// it would be inside brackets.
///
/// Without this the two spellings disagree about placement -- an element on
/// its own line becomes a block inside `[ ]` and stayed inline inside a run
/// -- which is the sort of quiet divergence `tests/src/pipe.rs` exists to
/// refuse.
fn at_marked_line_start(cur: &Cursor) -> bool {
    let before = &cur.src()[..cur.pos()];
    let line = match before.rfind(['\n', '\r']) {
        Some(nl) => &before[nl + 1..],
        None => before,
    };
    match line.split_once('|') {
        Some((head, tail)) => head.trim().is_empty() && tail.trim().is_empty(),
        None => false,
    }
}

fn flush_text(items: &mut Vec<Inline>, cur: &Cursor, text_start: &mut usize, fold_pipes: bool) {
    flush_text_upto(items, cur, text_start, cur.pos(), fold_pipes);
}

fn flush_text_upto(
    items: &mut Vec<Inline>,
    cur: &Cursor,
    text_start: &mut usize,
    end: usize,
    fold_pipes: bool,
) {
    let base = *text_start;
    let raw = &cur.src()[base..end];
    *text_start = end;
    split_softbreaks(items, cur, raw, base, fold_pipes);
}

/// Pushes a `Text`, merging into the previous item if it is also a `Text`.
///
/// The only time that happens is right after something was elided with
/// nothing pushed in its place -- a `/* */`/`//` comment being skipped is
/// the one case in this file. Flushing the text before and after a comment
/// as two separate nodes would leave `Vec<Inline>` with adjacent `Text`s
/// that mean nothing (no break, no content, sits between them), a state
/// [`Inline::SoftBreak`] very deliberately does *not* rely on `Text`
/// adjacency to mean "join these" -- see its doc comment. Merging here
/// keeps that invariant true instead of merely convenient: two `Text`s
/// are never adjacent in the finished tree without a reason.
pub(crate) fn push_text(items: &mut Vec<Inline>, value: String, span: Span) {
    if let Some(Inline::Text(prev)) = items.last_mut() {
        prev.value.push_str(&value);
        prev.span = Span::new(prev.span.start, span.end);
        return;
    }
    items.push(Inline::Text(Text::new(value, span)));
}

/// Appends `more` to `items`, merging across the join the same way
/// [`push_text`]/[`push_soft_break`] do within a single flush -- used by
/// `list.rs` when it appends a freshly parsed sequence onto content it
/// already built up by hand (a list item's own trailing text after its
/// groups), so the two don't leave an unmerged `Text`/`Text` or
/// `SoftBreak`/`SoftBreak` pair sitting at the seam.
pub(crate) fn extend_merging(items: &mut Vec<Inline>, more: Vec<Inline>) {
    let mut more = more.into_iter();
    if let Some(first) = more.next() {
        match first {
            Inline::Text(t) => push_text(items, t.value, t.span),
            Inline::SoftBreak(b) => push_soft_break(items, b.span),
            other => items.push(other),
        }
    }
    items.extend(more);
}

/// Pushes a `SoftBreak`, widening the previous one instead if it is also a
/// `SoftBreak` sitting right before it.
///
/// A comment sandwiched between two line breaks -- `"keep\n// x\nkeep2"` --
/// flushes as two separate calls into `split_softbreaks`, one ending in a
/// break and the next starting with one, with nothing pushed for the
/// elided comment in between. Left alone that is two adjacent `SoftBreak`s
/// standing for what a reader sees as one join between `keep` and `keep2`.
/// Same reasoning as [`push_text`], for the other node kind that comment
/// elision can leave stuttering.
pub(crate) fn push_soft_break(items: &mut Vec<Inline>, span: Span) {
    if let Some(Inline::SoftBreak(prev)) = items.last_mut() {
        prev.span = Span::new(prev.span.start, span.end);
        return;
    }
    items.push(Inline::SoftBreak(SoftBreak { span }));
}

/// Splits one flushed run of source text into `Text`/[`Inline::SoftBreak`]
/// items.
///
/// A source line break used to be folded away here, right into a literal
/// `' '` in the text (or nothing, between two East-Asian-wide characters) --
/// see [`Inline::SoftBreak`]'s doc comment for why that destroyed information
/// this needs to keep. What a break swallows is unchanged: the newline, the
/// whitespace after it, and -- inside a `|` run, when `fold_pipes` is set --
/// the next line's own `|` marker and the whitespace after that, so
/// `x| a\n| b` still swallows exactly what `x[ a\n  b ]` does. Only the
/// output differs: the swallowed gap becomes a `SoftBreak` node spanning it,
/// not a character. Deciding what that node renders as (a space, nothing,
/// a real newline) is left to whoever consumes the tree.
fn split_softbreaks(
    items: &mut Vec<Inline>,
    cur: &Cursor,
    raw: &str,
    base: usize,
    fold_pipes: bool,
) {
    let mut text_start = 0usize;
    let mut chars = raw.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c != '\n' && c != '\r' {
            continue;
        }
        if i > text_start {
            let span = Span::new(
                cur.position_at(base + text_start),
                cur.position_at(base + i),
            );
            push_text(items, raw[text_start..i].to_string(), span);
        }
        let mut gap_end = i + c.len_utf8();
        while let Some(&(j, wc)) = chars.peek() {
            if matches!(wc, ' ' | '\t' | '\n' | '\r') {
                gap_end = j + wc.len_utf8();
                chars.next();
            } else {
                break;
            }
        }
        if fold_pipes {
            if let Some(&(j, '|')) = chars.peek() {
                gap_end = j + '|'.len_utf8();
                chars.next();
                while let Some(&(k, wc)) = chars.peek() {
                    if matches!(wc, ' ' | '\t') {
                        gap_end = k + wc.len_utf8();
                        chars.next();
                    } else {
                        break;
                    }
                }
            }
        }
        let span = Span::new(cur.position_at(base + i), cur.position_at(base + gap_end));
        push_soft_break(items, span);
        text_start = gap_end;
    }
    if text_start < raw.len() {
        let span = Span::new(
            cur.position_at(base + text_start),
            cur.position_at(base + raw.len()),
        );
        push_text(items, raw[text_start..].to_string(), span);
    }
}

fn is_autolink_start(cur: &Cursor) -> bool {
    if char_before(cur).is_some_and(|c| c.is_alphanumeric()) {
        return false;
    }
    cur.starts_with("https://") || cur.starts_with("http://") || cur.starts_with("mailto:")
}

fn try_autolink(cur: &mut Cursor, stop: Stop) -> Result<Option<Element>> {
    let start_pos = cur.pos();
    let scheme_len = if cur.starts_with("https://") {
        8
    } else if cur.starts_with("http://") || cur.starts_with("mailto:") {
        7
    } else {
        return Ok(None);
    };

    let mut probe = *cur;
    let mut bracket_depth: u32 = 0;

    loop {
        if probe.is_eof() {
            break;
        }
        match stop {
            Stop::Bracket(c) => {
                if probe.peek() == Some(c) && bracket_depth == 0 {
                    break;
                }
                if probe.peek() == Some('[') {
                    bracket_depth += 1;
                } else if probe.peek() == Some(']') && bracket_depth > 0 {
                    bracket_depth -= 1;
                }
            }
            // A `|` run joins with the same line ending as any other
            // fold, and a URL never survives one, so the run's marker is
            // simply out of reach here.
            Stop::Line | Stop::Paragraph | Stop::PipeRun { .. } => {
                if probe.peek() == Some('\n') || probe.peek() == Some('\r') {
                    break;
                }
            }
            Stop::Offset(target_pos) => {
                if probe.pos() >= target_pos {
                    break;
                }
            }
            Stop::Delim(d) => {
                if probe.starts_with(d) && !is_boundary(char_before(&probe)) {
                    break;
                }
                if probe.peek() == Some('\n') || probe.peek() == Some('\r') {
                    break;
                }
            }
        }

        let ch = probe.peek().unwrap();
        if ch == ' ' || ch == '\t' || ch == '\n' || ch == '\r' || ch < ' ' {
            break;
        }
        probe.bump();
    }

    let scanned_end = probe.pos();
    let raw_len = scanned_end - start_pos;
    if raw_len <= scheme_len {
        return Ok(None);
    }

    let src = cur.src();
    let mut end_pos = scanned_end;

    while end_pos > start_pos + scheme_len {
        let slice = &src[start_pos..end_pos];
        let last_char = slice.chars().next_back().unwrap();
        if matches!(last_char, '.' | ',' | ';' | ':' | '!' | '?' | '"' | '\'') {
            end_pos -= last_char.len_utf8();
        } else if last_char == '>' {
            let open_count = slice.chars().filter(|&c| c == '<').count();
            let close_count = slice.chars().filter(|&c| c == '>').count();
            if close_count > open_count {
                end_pos -= 1;
            } else {
                break;
            }
        } else if last_char == ')' {
            let open_count = slice.chars().filter(|&c| c == '(').count();
            let close_count = slice.chars().filter(|&c| c == ')').count();
            if close_count > open_count {
                end_pos -= 1;
            } else {
                break;
            }
        } else if last_char == ']' {
            let open_count = slice.chars().filter(|&c| c == '[').count();
            let close_count = slice.chars().filter(|&c| c == ']').count();
            if close_count > open_count {
                end_pos -= 1;
            } else {
                break;
            }
        } else {
            break;
        }
    }

    if end_pos <= start_pos + scheme_len {
        return Ok(None);
    }

    let url_str = src[start_pos..end_pos].to_string();
    cur.set_pos(end_pos);
    let span = cur.span_from(start_pos);

    let el = Element {
        sigil: Sigil::named("link"),
        // An autolink is found while scanning running text, so it is
        // always part of it.
        placement: Placement::Inline,
        args: Some(Value::Map(vec![(
            "target".to_string(),
            Value::String(url_str),
        )])),
        content: None,
        children: None,
        value: None,
        connects: Vec::new(),
        span,
    };

    Ok(Some(el))
}
