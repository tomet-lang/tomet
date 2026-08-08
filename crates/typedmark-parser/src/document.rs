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
        } else if is_list_marker(&cur) {
            blocks.push(Block::List(parse_list(&mut cur)?));
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

fn parse_list(cur: &mut Cursor) -> Result<Vec<ListItem>> {
    let mut items = Vec::new();
    while is_list_marker(cur) {
        cur.bump();
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
        if cur.bump().is_none() {
            break;
        }
    }
    flush_text(&mut items, cur, &mut text_start);
    Ok(items)
}

fn flush_text(items: &mut Vec<Inline>, cur: &Cursor, text_start: &mut usize) {
    let raw = cur.slice_from(*text_start);
    *text_start = cur.pos();
    let normalized = normalize_text(raw);
    if !normalized.is_empty() {
        items.push(Inline::Text(normalized));
    }
}

/// Collapses any run of whitespace containing a newline into a single
/// space (Markdown-style "lazy continuation"), then trims the ends.
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
    out.trim().to_string()
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
