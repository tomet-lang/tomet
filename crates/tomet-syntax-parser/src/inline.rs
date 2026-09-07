//! Parsing for inline sequences, text normalization, and inline delimiters (`*em*`, `**strong**`, `==mark==`).

use crate::codeblock::is_fenced_code_block_start;
use crate::element::{element_ends_line, is_element_start, parse_element};
use crate::error::Result;
use crate::heading::{is_thematic_break, is_titled_thematic_break_start};
use crate::interp::{is_interp_start, parse_dollar_element};
use crate::list::peek_list_marker;
use crate::value::{err, skip_block_comment, skip_inline_ws, skip_line_comment};
use tomet_ast::{Element, Inline, Placement, Sigil, Span, Text, Value};
use tomet_lexer::Cursor;
use tomet_tree::{ElementExt, element_new};
use unicode_width::UnicodeWidthChar;

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
    PipeRun { col: usize },
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
                    let mut look = *cur;
                    look.bump();
                    skip_inline_ws(&mut look);
                    // An element on a continuation line does *not* end the
                    // paragraph. A line inside a paragraph is not block
                    // context, so the element belongs to the running text:
                    // wrapping a sentence so that `@link(…)[Tomet]` lands
                    // at a line start used to split one paragraph into
                    // three blocks. A block is opened by a blank line
                    // first, which is what the checks below still detect.
                    if look.is_eof()
                        || look.peek() == Some('\n')
                        || (look.peek() == Some('#') && crate::document::is_heading_start(&look))
                        || matches!(peek_list_marker(&look), Ok(Some(_)))
                        || look.starts_with("//")
                        || look.starts_with("/*")
                        || is_titled_thematic_break_start(&look)
                        || is_thematic_break(&look)
                        || is_fenced_code_block_start(&look)
                    {
                        break;
                    }
                }
            }
            Stop::Delim(d) => {
                if cur.is_eof() {
                    return Err(err(cur, cur.pos(), format!("unterminated, expected '{d}'")));
                }
                if cur.starts_with(d) && (d == "==" || !is_boundary(char_before(cur))) {
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
            let mut probe = *cur;
            probe.bump();
            let mut closed = false;
            while let Some(c) = probe.peek() {
                match c {
                    '`' => {
                        probe.bump();
                        closed = true;
                        break;
                    }
                    '\n' | '\r' => break,
                    _ => {
                        probe.bump();
                    }
                }
            }
            if closed {
                cur.set_pos(probe.pos());
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
        if cur.peek() == Some('@') {
            let block_context = match stop {
                Stop::Bracket(_) => at_line_start(cur),
                Stop::PipeRun { .. } => at_marked_line_start(cur),
                _ => false,
            };
            if is_element_start(cur, block_context) {
                let block = block_context && element_ends_line(cur);
                flush_text(&mut items, cur, &mut text_start, fold_pipes);
                let el = parse_element(cur, allow_colon_connect)?;
                items.push(Inline::Element(if block {
                    el.with_placement(Placement::Block)
                } else {
                    el
                }));
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
        if matches!(cur.peek(), Some('*') | Some('_') | Some('=')) {
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

fn trim_edges(mut items: Vec<Inline>) -> Vec<Inline> {
    if let Some(Inline::Text(t)) = items.first() {
        let trimmed = t.value.trim_start();
        if trimmed.is_empty() {
            items.remove(0);
        } else if trimmed.len() != t.value.len() {
            let trimmed = trimmed.to_string();
            if let Some(Inline::Text(t)) = items.first_mut() {
                t.value = trimmed;
            }
        }
    }
    if let Some(Inline::Text(t)) = items.last() {
        let trimmed = t.value.trim_end();
        if trimmed.is_empty() {
            items.pop();
        } else if trimmed.len() != t.value.len() {
            let trimmed = trimmed.to_string();
            if let Some(Inline::Text(t)) = items.last_mut() {
                t.value = trimmed;
            }
        }
    }
    items
}

const DELIMITERS: [(&str, &str); 5] = [
    ("**", "strong"),
    ("__", "strong"),
    ("==", "mark"),
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
        if probe.starts_with(delim) && (delim == "==" || !is_boundary(char_before(&probe))) {
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
    let raw = &cur.src()[*text_start..end];
    let span = Span::new(cur.position_at(*text_start), cur.position_at(end));
    *text_start = end;
    let normalized = normalize_text(raw, fold_pipes);
    if !normalized.is_empty() {
        items.push(Inline::Text(Text::new(normalized, span)));
    }
}

/// Whether `c` is East Asian wide or fullwidth.
///
/// Only used to decide what a folded line joins with. Ambiguous-width
/// characters count as narrow, which is `unicode-width`'s default and the
/// usual choice outside a locale-aware terminal.
fn is_wide(c: char) -> bool {
    UnicodeWidthChar::width(c) == Some(2)
}

/// Fold the newlines inside one text run.
///
/// A fold joins with a space -- CommonMark's softbreak -- except between two
/// wide characters, where the space would be a visible gap in the middle of a
/// sentence. Only this run is visible here, so a fold landing on a run
/// boundary, as in `折ると、\n**強調**`, still joins with a space.
///
/// `fold_pipes` extends the fold over a `|` run's marker and the whitespace
/// after it, so `x| a\n| b` folds exactly the way `x[ a\n  b ]` does. That is
/// the whole of what makes `|` a respelling rather than a second construct:
/// the joining rule is not reimplemented here, it is the same rule reaching
/// one character further.
fn normalize_text(raw: &str, fold_pipes: bool) -> String {
    let mut out = String::new();
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\n' || c == '\r' {
            while matches!(
                chars.peek(),
                Some(' ') | Some('\t') | Some('\n') | Some('\r')
            ) {
                chars.next();
            }
            if fold_pipes && chars.peek() == Some(&'|') {
                chars.next();
                while matches!(chars.peek(), Some(' ') | Some('\t')) {
                    chars.next();
                }
            }
            let between_wide = out.chars().next_back().is_some_and(is_wide)
                && chars.peek().copied().is_some_and(is_wide);
            if !between_wide {
                out.push(' ');
            }
        } else {
            out.push(c);
        }
    }
    out
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
        span,
    };

    Ok(Some(el))
}
