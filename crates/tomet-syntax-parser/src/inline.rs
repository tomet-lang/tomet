//! Parsing for inline sequences, text normalization, and inline delimiters (`*em*`, `**strong**`, `==mark==`).

use crate::codeblock::is_fenced_code_block_start;
use crate::element::{is_at_element_start, is_type_element_start, parse_element};
use crate::embedded_format::EmbeddedFormat;
use crate::error::Result;
use crate::heading::{is_thematic_break, is_titled_thematic_break_start};
use crate::interp::{is_interp_start, parse_dollar_element};
use crate::list::peek_list_marker;
use crate::value::{err, skip_block_comment, skip_inline_ws, skip_line_comment};
use tomet_ast::{Element, Inline, Sigil, Span, Text, Value};
use tomet_lexar::Cursor;

#[derive(Debug, Clone, Copy)]
pub(crate) enum Stop {
    Bracket(char),
    Paragraph,
    Line,
    Offset(usize),
    Delim(&'static str),
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
    default_format: Option<EmbeddedFormat>,
    allow_colon_connect: bool,
) -> Result<Vec<Inline>> {
    let mut items = Vec::new();
    let mut text_start = cur.pos();
    let mut bracket_depth: u32 = 0;
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
                    if look.is_eof()
                        || look.peek() == Some('\n')
                        || look.peek() == Some('#')
                        || matches!(peek_list_marker(&look), Ok(Some(_)))
                        || look.starts_with("//")
                        || look.starts_with("/*")
                        || (look.peek() == Some('<') && is_type_element_start(&look))
                        || (look.peek() == Some('@') && is_at_element_start(&look))
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
                flush_text_upto(&mut items, cur, &mut text_start, before);
                items.push(Inline::Element(el));
                text_start = cur.pos();
                continue;
            }
        }
        if cur.starts_with("/*") {
            flush_text(&mut items, cur, &mut text_start);
            skip_block_comment(cur)?;
            text_start = cur.pos();
            continue;
        }
        if cur.starts_with("//") && is_boundary(char_before(cur)) {
            flush_text(&mut items, cur, &mut text_start);
            skip_line_comment(cur);
            text_start = cur.pos();
            continue;
        }
        if cur.peek() == Some('<') && is_type_element_start(cur) {
            flush_text(&mut items, cur, &mut text_start);
            items.push(Inline::Element(parse_element(
                cur,
                default_format,
                allow_colon_connect,
            )?));
            text_start = cur.pos();
            continue;
        }
        if cur.peek() == Some('@') && is_at_element_start(cur) {
            flush_text(&mut items, cur, &mut text_start);
            items.push(Inline::Element(parse_element(
                cur,
                default_format,
                allow_colon_connect,
            )?));
            text_start = cur.pos();
            continue;
        }
        if cur.peek() == Some('$') && is_interp_start(cur) {
            flush_text(&mut items, cur, &mut text_start);
            items.push(Inline::Element(parse_dollar_element(cur)?));
            text_start = cur.pos();
            continue;
        }
        if matches!(cur.peek(), Some('*') | Some('_') | Some('=')) {
            let before = cur.pos();
            if let Some(el) = try_delimited(cur, default_format, allow_colon_connect)? {
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

fn try_delimited(
    cur: &mut Cursor,
    default_format: Option<EmbeddedFormat>,
    allow_colon_connect: bool,
) -> Result<Option<Element>> {
    for (delim, kind) in DELIMITERS {
        if let Some(el) = try_one_delimited(cur, delim, kind, default_format, allow_colon_connect)?
        {
            return Ok(Some(el));
        }
    }
    Ok(None)
}

fn try_one_delimited(
    cur: &mut Cursor,
    delim: &'static str,
    kind: &str,
    default_format: Option<EmbeddedFormat>,
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
    let inner = parse_inline_seq(cur, Stop::Delim(delim), default_format, allow_colon_connect)?;
    if !cur.eat_str(delim) {
        return Err(err(cur, cur.pos(), format!("expected '{delim}'")));
    }
    let span = cur.span_from(start_pos);
    let mut el = Element::new(Sigil::Type(kind.to_string())).with_span(span);
    el.content = Some(inner);
    Ok(Some(el))
}

fn flush_text(items: &mut Vec<Inline>, cur: &Cursor, text_start: &mut usize) {
    flush_text_upto(items, cur, text_start, cur.pos());
}

fn flush_text_upto(items: &mut Vec<Inline>, cur: &Cursor, text_start: &mut usize, end: usize) {
    let raw = &cur.src()[*text_start..end];
    let span = Span::new(cur.position_at(*text_start), cur.position_at(end));
    *text_start = end;
    let normalized = normalize_text(raw);
    if !normalized.is_empty() {
        items.push(Inline::Text(Text::new(normalized, span)));
    }
}

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
            Stop::Line | Stop::Paragraph => {
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
        sigil: Sigil::At(None),
        args: Some(Value::Map(vec![(
            "url".to_string(),
            Value::String(url_str),
        )])),
        content: None,
        children: None,
        value: None,
        span,
    };

    Ok(Some(el))
}
