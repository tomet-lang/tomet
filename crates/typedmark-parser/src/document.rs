//! Parsing for the full markup grammar: headings, lists, paragraphs, and
//! typed inline/block elements (`<T>(input)[area]{value}`, `@name...`,
//! bare `@(key:...)`, and the bare-element children of containers like
//! `@links{}`). See `docs/tmt/typedmark.tm` in the repo root for the
//! syntax this follows.

use crate::error::Result;
use crate::value::{
    eat_ident, err, is_ident_char, parse_value_at, skip_inline_ws, skip_ws_and_newlines,
};
use typedmark_ast::{
    Block, Document, Element, ElementValue, Heading, Inline, ListItem, Sigil, Value,
};
use typedmark_lexar::Cursor;

pub fn parse_document(src: &str) -> Result<Document> {
    let mut cur = Cursor::new(src);
    let mut blocks = Vec::new();
    loop {
        skip_blank_lines(&mut cur);
        if cur.is_eof() {
            break;
        }
        if cur.peek() == Some('#') {
            blocks.push(Block::Heading(parse_heading(&mut cur)?));
        } else if is_thematic_break(&cur) {
            consume_thematic_break(&mut cur);
            blocks.push(Block::Element(Element::new(Sigil::Type("hr".to_string()))));
        } else if is_ordered_list_marker(&cur) {
            blocks.push(Block::List {
                ordered: true,
                items: parse_list(&mut cur, true)?,
            });
        } else if is_list_marker(&cur) {
            blocks.push(Block::List {
                ordered: false,
                items: parse_list(&mut cur, false)?,
            });
        } else {
            blocks.push(parse_paragraph(&mut cur)?);
        }
    }
    Ok(Document { blocks })
}

fn skip_blank_lines(cur: &mut Cursor) {
    loop {
        let checkpoint = cur.pos();
        skip_inline_ws(cur);
        match cur.peek() {
            Some('\n') | Some('\r') => {
                cur.bump();
            }
            _ => {
                cur.set_pos(checkpoint);
                break;
            }
        }
    }
}

fn is_list_marker(cur: &Cursor) -> bool {
    let mut look = *cur;
    if look.bump() != Some('-') {
        return false;
    }
    matches!(look.peek(), Some(' ') | Some('\t'))
}

/// `-. item` -- the ordered-list sibling of `- item`. Kept in the same
/// "marker" family (rather than e.g. `1.[item]`) so numbering is always
/// computed at render time instead of authored and going stale on reorder.
fn is_ordered_list_marker(cur: &Cursor) -> bool {
    let mut look = *cur;
    if look.bump() != Some('-') {
        return false;
    }
    if look.bump() != Some('.') {
        return false;
    }
    matches!(look.peek(), Some(' ') | Some('\t'))
}

/// A line of 3+ `-` and nothing else (trailing inline whitespace aside).
fn is_thematic_break(cur: &Cursor) -> bool {
    let mut look = *cur;
    if look.eat_while(|c| c == '-').len() < 3 {
        return false;
    }
    skip_inline_ws(&mut look);
    matches!(look.peek(), None | Some('\n') | Some('\r'))
}

fn consume_thematic_break(cur: &mut Cursor) {
    cur.eat_while(|c| c == '-' || c == ' ' || c == '\t');
    if matches!(cur.peek(), Some('\n') | Some('\r')) {
        cur.bump();
    }
}

fn parse_heading(cur: &mut Cursor) -> Result<Heading> {
    let level = cur.eat_while(|c| c == '#').len() as u8;
    skip_inline_ws(cur);
    if !cur.eat_str("[") {
        return Err(err(cur, cur.pos(), "expected '[' after '#'"));
    }
    let content = parse_inline_seq(cur, Stop::Bracket(']'))?;
    if !cur.eat_str("]") {
        return Err(err(cur, cur.pos(), "expected ']'"));
    }
    let checkpoint = cur.pos();
    skip_ws_and_newlines(cur);
    let attrs = if cur.peek() == Some('{') {
        Some(parse_braced_value(cur)?)
    } else {
        cur.set_pos(checkpoint);
        None
    };
    skip_inline_ws(cur);
    if matches!(cur.peek(), Some('\n') | Some('\r')) {
        cur.bump();
    }
    Ok(Heading {
        level,
        content,
        attrs,
    })
}

fn parse_braced_value(cur: &mut Cursor) -> Result<Value> {
    if !cur.eat_str("{") {
        return Err(err(cur, cur.pos(), "expected '{'"));
    }
    let v = parse_value_at(cur)?;
    skip_ws_and_newlines(cur);
    if !cur.eat_str("}") {
        return Err(err(cur, cur.pos(), "expected '}'"));
    }
    Ok(v)
}

/// `ordered` selects which marker continues the list -- a run of `- `
/// lines and a run of `-. ` lines are two separate lists even if adjacent,
/// so switching marker mid-stream stops this list rather than mixing.
fn parse_list(cur: &mut Cursor, ordered: bool) -> Result<Vec<ListItem>> {
    let mut items = Vec::new();
    while if ordered {
        is_ordered_list_marker(cur)
    } else {
        is_list_marker(cur)
    } {
        cur.bump();
        if ordered {
            cur.bump();
        }
        skip_inline_ws(cur);
        let content = parse_inline_seq(cur, Stop::Line)?;
        items.push(ListItem { content });
        if cur.peek() == Some('\n') {
            cur.bump();
        }
    }
    Ok(items)
}

fn parse_paragraph(cur: &mut Cursor) -> Result<Block> {
    let mut content = parse_inline_seq(cur, Stop::Paragraph)?;
    if content.len() == 1 && matches!(content[0], Inline::Element(_)) {
        if let Inline::Element(el) = content.pop().unwrap() {
            return Ok(Block::Element(el));
        }
    }
    Ok(Block::Paragraph(content))
}

#[derive(Debug, Clone, Copy)]
enum Stop {
    Bracket(char),
    Paragraph,
    Line,
    /// Closing delimiter of a `*em*`/`**strong**`/`==mark==` span; mirrors
    /// `Bracket` but the terminator is a short string instead of one char.
    Delim(&'static str),
}

fn parse_inline_seq(cur: &mut Cursor, stop: Stop) -> Result<Vec<Inline>> {
    let mut items = Vec::new();
    let mut text_start = cur.pos();
    loop {
        match stop {
            Stop::Bracket(c) => {
                if cur.is_eof() {
                    return Err(err(cur, cur.pos(), format!("unterminated, expected '{c}'")));
                }
                if cur.peek() == Some(c) {
                    break;
                }
            }
            Stop::Line => {
                if cur.is_eof() || cur.peek() == Some('\n') {
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
                    if look.is_eof()
                        || look.peek() == Some('\n')
                        || look.peek() == Some('#')
                        || is_list_marker(&look)
                    {
                        break;
                    }
                    // Lazy continuation: no blank line and no new block marker,
                    // so this newline is just part of the running text.
                }
            }
            Stop::Delim(d) => {
                if cur.is_eof() {
                    return Err(err(cur, cur.pos(), format!("unterminated, expected '{d}'")));
                }
                // Only a non-boundary-preceded occurrence counts as the
                // real close -- matches the dry run `try_one_delimited`
                // already did before committing to this recursive call.
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
        }
        if cur.peek() == Some('`') {
            // A backtick span is verbatim, Markdown-code-span style -- lets
            // prose mention `@links{}`/`<caution>[...]` etc. literally
            // without it being parsed as a real trigger.
            cur.bump();
            while let Some(c) = cur.peek() {
                cur.bump();
                if c == '`' {
                    break;
                }
            }
            continue;
        }
        if cur.peek() == Some('<') && is_type_element_start(cur) {
            flush_text(&mut items, cur, &mut text_start);
            items.push(Inline::Element(parse_element(cur)?));
            text_start = cur.pos();
            continue;
        }
        if cur.peek() == Some('@') && is_at_element_start(cur) {
            flush_text(&mut items, cur, &mut text_start);
            items.push(Inline::Element(parse_element(cur)?));
            text_start = cur.pos();
            continue;
        }
        if matches!(cur.peek(), Some('*') | Some('_') | Some('=')) {
            // `try_delimited` mutates `cur` past the whole span on success,
            // so the pending-text flush has to use the position from
            // *before* that call, not `cur.pos()` afterwards -- otherwise
            // the raw delimiter text gets flushed as literal text on top
            // of the parsed element.
            let before = cur.pos();
            if let Some(el) = try_delimited(cur)? {
                flush_text_upto(&mut items, cur, &mut text_start, before);
                items.push(Inline::Element(el));
                text_start = cur.pos();
                continue;
            }
        }
        if cur.bump().is_none() {
            break;
        }
    }
    flush_text(&mut items, cur, &mut text_start);
    Ok(trim_edges(items))
}

/// Only the leading edge of the first text chunk and the trailing edge of
/// the last need trimming (e.g. `[ hi ]` -> `hi`) -- chunks in the middle
/// of the sequence sit next to an element on at least one side, and their
/// whitespace there is real inter-token spacing, not padding to strip.
/// (Trimming every chunk independently, which `normalize_text` used to do,
/// ate the space around inline elements embedded mid-paragraph.)
fn trim_edges(mut items: Vec<Inline>) -> Vec<Inline> {
    if let Some(Inline::Text(t)) = items.first() {
        let trimmed = t.trim_start();
        if trimmed.is_empty() {
            items.remove(0);
        } else if trimmed.len() != t.len() {
            let trimmed = trimmed.to_string();
            if let Some(Inline::Text(t)) = items.first_mut() {
                *t = trimmed;
            }
        }
    }
    if let Some(Inline::Text(t)) = items.last() {
        let trimmed = t.trim_end();
        if trimmed.is_empty() {
            items.pop();
        } else if trimmed.len() != t.len() {
            let trimmed = trimmed.to_string();
            if let Some(Inline::Text(t)) = items.last_mut() {
                *t = trimmed;
            }
        }
    }
    items
}

/// Delimiters tried longest-first (so `**`/`__` aren't read as two `*`/`_`
/// spans) with the `Sigil::Type` name each desugars to.
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

/// Simplified stand-in for CommonMark's flanking-delimiter rule: a
/// delimiter only opens/closes a span when it's not touching whitespace
/// (or the start/end of the text) on the content side.
fn is_boundary(c: Option<char>) -> bool {
    matches!(c, None | Some(' ') | Some('\t') | Some('\n') | Some('\r'))
}

fn try_delimited(cur: &mut Cursor) -> Result<Option<Element>> {
    for (delim, kind) in DELIMITERS {
        if let Some(el) = try_one_delimited(cur, delim, kind)? {
            return Ok(Some(el));
        }
    }
    Ok(None)
}

fn try_one_delimited(cur: &mut Cursor, delim: &'static str, kind: &str) -> Result<Option<Element>> {
    if !cur.starts_with(delim) {
        return Ok(None);
    }
    // `_`/`__` additionally require a word boundary before the opening
    // delimiter, so `foo_bar_baz` stays literal instead of misfiring.
    if delim.starts_with('_') && char_before(cur).is_some_and(|c| c.is_alphanumeric()) {
        return Ok(None);
    }
    let mut open = *cur;
    open.eat_str(delim);
    if is_boundary(open.peek()) {
        return Ok(None);
    }
    // Dry run: bail out to plain text unless a valid close exists before
    // EOF or a blank-line paragraph break -- mirrors exactly what the
    // `Stop::Delim` arm above will do once we commit to the real parse.
    let mut probe = open;
    loop {
        if probe.starts_with(delim) && !is_boundary(char_before(&probe)) {
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
    let inner = parse_inline_seq(cur, Stop::Delim(delim))?;
    if !cur.eat_str(delim) {
        return Err(err(cur, cur.pos(), format!("expected '{delim}'")));
    }
    let mut el = Element::new(Sigil::Type(kind.to_string()));
    el.area = Some(inner);
    Ok(Some(el))
}

fn flush_text(items: &mut Vec<Inline>, cur: &Cursor, text_start: &mut usize) {
    flush_text_upto(items, cur, text_start, cur.pos());
}

/// Like `flush_text`, but flushes only up to an explicit end position
/// rather than `cur`'s current one -- needed when `cur` has already been
/// advanced past a span (e.g. a delimiter match) whose raw source text
/// must NOT be included in the flush.
fn flush_text_upto(items: &mut Vec<Inline>, cur: &Cursor, text_start: &mut usize, end: usize) {
    let raw = &cur.src()[*text_start..end];
    *text_start = end;
    let normalized = normalize_text(raw);
    if !normalized.is_empty() {
        items.push(Inline::Text(normalized));
    }
}

/// Collapses any run of whitespace containing a newline into a single
/// space (Markdown-style "lazy continuation"). Edge trimming is handled
/// separately by `trim_edges`, once the full sequence is assembled --
/// doing it per-chunk here would also eat real spacing around embedded
/// elements.
fn normalize_text(raw: &str) -> String {
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
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}

/// Between a sigil's name and its first `(`/`[`/`{` group, `parse_element`
/// tolerates inline whitespace and up to one newline (so e.g. `@links {`
/// or a heading's attrs on their own line still parse) -- the lookahead
/// here has to tolerate exactly the same gap, or it'll disagree with
/// `parse_element` about whether a trigger is even present.
fn skip_lookahead_gap(cur: &mut Cursor) {
    skip_inline_ws(cur);
    let mut newlines = 0u8;
    while matches!(cur.peek(), Some('\n') | Some('\r')) {
        cur.bump();
        newlines += 1;
        skip_inline_ws(cur);
        if newlines > 1 {
            break;
        }
    }
}

fn is_type_element_start(cur: &Cursor) -> bool {
    let mut look = *cur;
    if look.bump() != Some('<') {
        return false;
    }
    let ident = look.eat_while(is_ident_char);
    if ident.is_empty() {
        return false;
    }
    if look.bump() != Some('>') {
        return false;
    }
    skip_lookahead_gap(&mut look);
    matches!(look.peek(), Some('(') | Some('[') | Some('{'))
}

fn is_at_element_start(cur: &Cursor) -> bool {
    let mut look = *cur;
    if look.bump() != Some('@') {
        return false;
    }
    look.eat_while(is_ident_char);
    skip_lookahead_gap(&mut look);
    matches!(look.peek(), Some('(') | Some('[') | Some('{'))
}

fn parse_element(cur: &mut Cursor) -> Result<Element> {
    let sigil = if cur.peek() == Some('<') {
        cur.bump();
        let name = eat_ident(cur).to_string();
        if name.is_empty() {
            return Err(err(cur, cur.pos(), "expected a type name after '<'"));
        }
        if !cur.eat_str(">") {
            return Err(err(cur, cur.pos(), "expected '>' after type name"));
        }
        Sigil::Type(name)
    } else {
        if !cur.eat_str("@") {
            return Err(err(cur, cur.pos(), "expected '@'"));
        }
        let name = eat_ident(cur).to_string();
        if name.is_empty() {
            Sigil::At(None)
        } else {
            Sigil::At(Some(name))
        }
    };

    let mut el = Element::new(sigil);
    loop {
        let checkpoint = cur.pos();
        skip_inline_ws(cur);
        let mut newlines = 0u8;
        while matches!(cur.peek(), Some('\n') | Some('\r')) {
            cur.bump();
            newlines += 1;
            skip_inline_ws(cur);
            if newlines > 1 {
                break;
            }
        }
        if newlines <= 1 {
            match cur.peek() {
                Some('(') if el.input.is_none() => {
                    el.input = Some(parse_paren_value(cur)?);
                    continue;
                }
                Some('[') if el.area.is_none() => {
                    el.area = Some(parse_area(cur)?);
                    continue;
                }
                Some('{') if el.value.is_none() => {
                    el.value = Some(parse_value_group(cur)?);
                    continue;
                }
                _ => {}
            }
        }
        cur.set_pos(checkpoint);
        break;
    }
    Ok(el)
}

fn parse_paren_value(cur: &mut Cursor) -> Result<Value> {
    if !cur.eat_str("(") {
        return Err(err(cur, cur.pos(), "expected '('"));
    }
    let v = parse_value_at(cur)?;
    skip_ws_and_newlines(cur);
    if !cur.eat_str(")") {
        return Err(err(cur, cur.pos(), "expected ')'"));
    }
    Ok(v)
}

fn parse_area(cur: &mut Cursor) -> Result<Vec<Inline>> {
    if !cur.eat_str("[") {
        return Err(err(cur, cur.pos(), "expected '['"));
    }
    let content = parse_inline_seq(cur, Stop::Bracket(']'))?;
    if !cur.eat_str("]") {
        return Err(err(cur, cur.pos(), "expected ']'"));
    }
    Ok(content)
}

fn parse_value_group(cur: &mut Cursor) -> Result<ElementValue> {
    if !cur.eat_str("{") {
        return Err(err(cur, cur.pos(), "expected '{'"));
    }
    skip_ws_and_newlines(cur);
    let result = if cur.peek() == Some('(') {
        let mut children = Vec::new();
        loop {
            skip_ws_and_newlines(cur);
            if cur.peek() != Some('(') {
                break;
            }
            children.push(parse_bare_element(cur)?);
        }
        ElementValue::Children(children)
    } else {
        ElementValue::Data(parse_value_at(cur)?)
    };
    skip_ws_and_newlines(cur);
    if !cur.eat_str("}") {
        return Err(err(cur, cur.pos(), "expected '}'"));
    }
    Ok(result)
}

fn parse_bare_element(cur: &mut Cursor) -> Result<Element> {
    let input = parse_paren_value(cur)?;
    let mut el = Element::new(Sigil::Bare);
    el.input = Some(input);
    let checkpoint = cur.pos();
    skip_inline_ws(cur);
    if cur.peek() == Some('[') {
        el.area = Some(parse_area(cur)?);
    } else {
        cur.set_pos(checkpoint);
    }
    Ok(el)
}
