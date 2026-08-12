//! Parsing for the full markup grammar: headings, lists, paragraphs, and
//! typed inline/block elements (`<T>(input)[area]{value}`, `@name...`,
//! bare `@(key:...)`, and the bare-element children of containers like
//! `@links{}`). See `docs/tmt/typedmark.tm` in the repo root for the
//! syntax this follows.

use crate::embedded_format::{EmbeddedFormat, parse_embedded_format_value};
use crate::error::Result;
use crate::value::{
    eat_ident, err, find_matching_bracket, find_matching_delimiter, is_ident_char, parse_value_at,
    skip_inline_ws, skip_ws_and_newlines, skip_ws_newlines_and_comments,
};
use typedmark_ast::{
    Block, Document, Element, ElementValue, Heading, Inline, List, ListItem, Paragraph, Sigil,
    Text, Value,
};
use typedmark_lexar::Cursor;

pub fn parse_document(src: &str) -> Result<Document> {
    let mut cur = Cursor::new(src);
    let mut blocks = Vec::new();
    let doc_start = cur.pos();
    // Running default for the `{...}` embedded-format mechanism, set by a
    // `@config(format:...)` block and applied to every element parsed after
    // it -- order-dependent, single-pass (see `config_format_update`).
    let mut default_format: Option<EmbeddedFormat> = None;
    loop {
        skip_blank_lines(&mut cur);
        if cur.is_eof() {
            break;
        }
        let block_start = cur.pos();
        let block = if cur.peek() == Some('#') {
            Some(Block::Heading(parse_heading(&mut cur, default_format)?))
        } else if is_line_comment_start(&cur) {
            skip_inline_ws(&mut cur);
            skip_line_comment(&mut cur);
            None
        } else if is_block_comment_start(&cur) {
            skip_inline_ws(&mut cur);
            skip_block_comment(&mut cur)?;
            None
        } else if is_titled_thematic_break_start(&cur) {
            Some(Block::Element(parse_titled_thematic_break(
                &mut cur,
                default_format,
            )?))
        } else if is_thematic_break(&cur) {
            consume_thematic_break(&mut cur);
            let span = cur.span_from(block_start);
            Some(Block::Element(
                Element::new(Sigil::Type("hr".to_string())).with_span(span),
            ))
        } else if let Some((ordered, _)) = peek_list_marker(&cur) {
            let items = parse_list(&mut cur, ordered, default_format)?;
            let span = cur.span_from(block_start);
            Some(Block::List(List::new(ordered, items, span)))
        } else {
            Some(parse_paragraph(&mut cur, default_format)?)
        };

        if let Some(block) = block {
            if let Block::Element(el) = &block {
                if let Some(new_default) = config_format_update(el) {
                    default_format = new_default;
                }
            }
            blocks.push(block);
        }
    }
    let doc_span = cur.span_from(doc_start);
    Ok(Document::new(blocks, doc_span))
}

/// `// ...` to end of line/EOF. Discarded entirely -- comments never enter
/// the AST, same as blank lines. Unterminated (i.e. running to EOF with no
/// trailing newline) isn't an error: "the rest of the file" is a visible,
/// bounded consequence of a to-end-of-line comment, not a silent swallow.
fn skip_line_comment(cur: &mut Cursor) {
    cur.eat_str("//");
    cur.eat_while(|c| c != '\n' && c != '\r');
}

/// `/* ... */`, block position: raw/unparsed content up to the first `*/`,
/// possibly spanning multiple lines, blank lines, or paragraphs. No
/// nesting (matches C). Also used from `parse_inline_seq` for the inline
/// form. Unlike `skip_line_comment`, an unterminated comment here silently
/// swallows everything after it with no visible trace, so it's a parse
/// error instead of running quietly to EOF.
fn skip_block_comment(cur: &mut Cursor) -> Result<()> {
    let start = cur.pos();
    cur.eat_str("/*");
    loop {
        if cur.eat_str("*/") {
            return Ok(());
        }
        if cur.bump().is_none() {
            return Err(err(cur, start, "unterminated block comment, expected '*/'"));
        }
    }
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

/// Whether the current line, after any leading inline whitespace, starts a
/// `//` line comment. Unlike block markers (`#`/list markers/thematic
/// breaks), a comment carries no structural meaning of its own, so
/// tolerating indentation costs nothing -- callers that find this `true`
/// still need to consume that leading whitespace themselves before
/// `skip_line_comment`.
fn is_line_comment_start(cur: &Cursor) -> bool {
    let mut look = *cur;
    skip_inline_ws(&mut look);
    look.starts_with("//")
}

/// Same idea as `is_line_comment_start`, for the block-position `/* ... */`
/// form.
fn is_block_comment_start(cur: &Cursor) -> bool {
    let mut look = *cur;
    skip_inline_ws(&mut look);
    look.starts_with("/*")
}

fn eat_list_marker(cur: &mut Cursor) -> Option<(bool, Option<String>)> {
    let mut look = *cur;
    if look.bump() != Some('-') {
        return None;
    }
    let ordered = if look.peek() == Some('.') {
        look.bump();
        true
    } else {
        false
    };

    let has_ws = matches!(look.peek(), Some(' ') | Some('\t'));
    skip_inline_ws(&mut look);

    let marker = if look.peek() == Some('[') || look.peek() == Some('(') {
        let open_char = look.bump().unwrap();
        let close_char = if open_char == '[' { ']' } else { ')' };
        let mut inner = String::new();
        while let Some(c) = look.peek() {
            if c == close_char {
                look.bump();
                break;
            }
            if c == '\n' || c == '\r' {
                break;
            }
            inner.push(c);
            look.bump();
        }
        if look.peek() == Some(' ') || look.peek() == Some('\t') {
            Some(inner)
        } else {
            None
        }
    } else {
        None
    };

    if marker.is_none() && !has_ws {
        return None;
    }

    skip_inline_ws(&mut look);
    cur.set_pos(look.pos());
    Some((ordered, marker))
}

fn peek_list_marker(cur: &Cursor) -> Option<(bool, Option<String>)> {
    let mut look = *cur;
    eat_list_marker(&mut look)
}

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

fn is_titled_thematic_break_start(cur: &Cursor) -> bool {
    let mut look = *cur;
    if look.eat_while(|c| c == '-').len() < 3 {
        return false;
    }
    skip_inline_ws(&mut look);
    look.peek() == Some('[')
}

fn parse_titled_thematic_break(
    cur: &mut Cursor,
    default_format: Option<EmbeddedFormat>,
) -> Result<Element> {
    let start_pos = cur.pos();
    cur.eat_while(|c| c == '-');
    skip_inline_ws(cur);
    if !cur.eat_str("[") {
        return Err(err(cur, cur.pos(), "expected '['"));
    }
    let title = parse_inline_seq(cur, Stop::Bracket(']'), default_format)?;
    if !cur.eat_str("]") {
        return Err(err(cur, cur.pos(), "expected ']'"));
    }
    skip_inline_ws(cur);
    if cur.eat_while(|c| c == '-').len() < 3 {
        return Err(err(
            cur,
            cur.pos(),
            "expected 3 or more '-' to close the titled thematic break",
        ));
    }
    skip_inline_ws(cur);
    if !matches!(cur.peek(), None | Some('\n') | Some('\r')) {
        return Err(err(
            cur,
            cur.pos(),
            "unexpected trailing content after titled thematic break",
        ));
    }
    if matches!(cur.peek(), Some('\n') | Some('\r')) {
        cur.bump();
    }
    let mut el = Element::new(Sigil::Type("hr".to_string())).with_span(cur.span_from(start_pos));
    el.area = Some(title);
    Ok(el)
}

fn parse_heading(cur: &mut Cursor, default_format: Option<EmbeddedFormat>) -> Result<Heading> {
    let start_pos = cur.pos();
    let level = cur.eat_while(|c| c == '#').len() as u8;
    skip_inline_ws(cur);
    if !cur.eat_str("[") {
        return Err(err(cur, cur.pos(), "expected '[' after '#'"));
    }
    let content = parse_inline_seq(cur, Stop::Bracket(']'), default_format)?;
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
    let span = cur.span_from(start_pos);
    Ok(Heading::new(level, content, attrs, span))
}

fn parse_braced_value(cur: &mut Cursor) -> Result<Value> {
    if !cur.eat_str("{") {
        return Err(err(cur, cur.pos(), "expected '{'"));
    }
    let v = parse_value_at(cur)?;
    skip_ws_newlines_and_comments(cur);
    if !cur.eat_str("}") {
        return Err(err(cur, cur.pos(), "expected '}'"));
    }
    Ok(v)
}

fn peek_trailing_attrs(cur: &Cursor) -> Option<usize> {
    let mut look = *cur;
    let mut last_brace_pos = None;
    while !look.is_eof() && look.peek() != Some('\n') && look.peek() != Some('\r') {
        if look.peek() == Some('{') {
            last_brace_pos = Some(look.pos());
        }
        look.bump();
    }
    if let Some(pos) = last_brace_pos {
        let mut test_cur = *cur;
        test_cur.set_pos(pos);
        if parse_braced_value(&mut test_cur).is_ok() {
            skip_inline_ws(&mut test_cur);
            if matches!(test_cur.peek(), None | Some('\n') | Some('\r')) {
                return Some(pos);
            }
        }
    }
    None
}

/// `ordered` selects which marker continues the list -- a run of `- `
/// lines and a run of `-. ` lines are two separate lists even if adjacent,
/// so switching marker mid-stream stops this list rather than mixing.
fn parse_list(
    cur: &mut Cursor,
    ordered: bool,
    default_format: Option<EmbeddedFormat>,
) -> Result<Vec<ListItem>> {
    let mut items = Vec::new();
    while let Some((item_ordered, marker)) = peek_list_marker(cur) {
        if item_ordered != ordered {
            break;
        }
        let item_start = cur.pos();
        eat_list_marker(cur);

        let (content, attrs) = if let Some(brace_pos) = peek_trailing_attrs(cur) {
            let content = parse_inline_seq(cur, Stop::Offset(brace_pos), default_format)?;
            let attrs = parse_braced_value(cur)?;
            (content, Some(attrs))
        } else {
            let content = parse_inline_seq(cur, Stop::Line, default_format)?;
            (content, None)
        };

        if cur.peek() == Some('\n') {
            cur.bump();
        }
        let span = cur.span_from(item_start);
        items.push(ListItem::new(content, marker, attrs, span));
    }
    Ok(items)
}

fn parse_paragraph(cur: &mut Cursor, default_format: Option<EmbeddedFormat>) -> Result<Block> {
    let start_pos = cur.pos();
    let mut content = parse_inline_seq(cur, Stop::Paragraph, default_format)?;
    let span = cur.span_from(start_pos);
    if content.len() == 1 && matches!(content[0], Inline::Element(_)) {
        if let Inline::Element(mut el) = content.pop().unwrap() {
            if el.span == typedmark_ast::Span::default() {
                el.span = span;
            }
            return Ok(Block::Element(el));
        }
    }
    Ok(Block::Paragraph(Paragraph::new(content, span)))
}

#[derive(Debug, Clone, Copy)]
enum Stop {
    Bracket(char),
    Paragraph,
    Line,
    Offset(usize),
    /// Closing delimiter of a `*em*`/`**strong**`/`==mark==` span; mirrors
    /// `Bracket` but the terminator is a short string instead of one char.
    Delim(&'static str),
}

fn parse_inline_seq(
    cur: &mut Cursor,
    stop: Stop,
    default_format: Option<EmbeddedFormat>,
) -> Result<Vec<Inline>> {
    let mut items = Vec::new();
    let mut text_start = cur.pos();
    // Only meaningful for `Stop::Bracket`, whose closer is always `]`: an
    // unowned literal `[`/`]` reaching this loop's plain-text fallback
    // (one belonging to a nested element/`*em*`/backtick span is fully
    // consumed by its own recursive call and never reaches here, so it
    // can't double-count) nests instead of ending the area at the first
    // `]`, e.g. a bare "[brackets]" or "[ ]" inside otherwise-ordinary
    // text.
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
                        || peek_list_marker(&look).is_some()
                        || look.starts_with("//")
                        || look.starts_with("/*")
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
        if cur.starts_with("/*") {
            // Inline `/* ... */`: has an explicit closer, so it's safe to
            // recognize anywhere -- it can't run past a `]`/`)`/`}` it
            // doesn't own without erroring first.
            flush_text(&mut items, cur, &mut text_start);
            skip_block_comment(cur)?;
            text_start = cur.pos();
            continue;
        }
        if cur.starts_with("//") && is_boundary(char_before(cur)) {
            // Inline `//`, to end of line: only recognized when it isn't
            // glued to preceding text -- a `//` right after whitespace,
            // a newline, or the very start of the text is a comment, but
            // `https://example.com` (no whitespace before `//`) stays
            // literal. Same boundary rule `eat_scalar_raw`
            // (`value.rs`) uses for a trailing comment inside `(...)`/
            // `{...}`. Doesn't consume the trailing newline itself, so
            // `Stop::Paragraph`'s lazy-continuation check still runs
            // normally on whatever follows.
            flush_text(&mut items, cur, &mut text_start);
            skip_line_comment(cur);
            text_start = cur.pos();
            continue;
        }
        if cur.peek() == Some('<') && is_type_element_start(cur) {
            flush_text(&mut items, cur, &mut text_start);
            items.push(Inline::Element(parse_element(cur, default_format)?));
            text_start = cur.pos();
            continue;
        }
        if cur.peek() == Some('@') && is_at_element_start(cur) {
            flush_text(&mut items, cur, &mut text_start);
            items.push(Inline::Element(parse_element(cur, default_format)?));
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
            if let Some(el) = try_delimited(cur, default_format)? {
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

fn try_delimited(
    cur: &mut Cursor,
    default_format: Option<EmbeddedFormat>,
) -> Result<Option<Element>> {
    for (delim, kind) in DELIMITERS {
        if let Some(el) = try_one_delimited(cur, delim, kind, default_format)? {
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
) -> Result<Option<Element>> {
    let start_pos = cur.pos();
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
    let inner = parse_inline_seq(cur, Stop::Delim(delim), default_format)?;
    if !cur.eat_str(delim) {
        return Err(err(cur, cur.pos(), format!("expected '{delim}'")));
    }
    let span = cur.span_from(start_pos);
    let mut el = Element::new(Sigil::Type(kind.to_string())).with_span(span);
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
    let span = typedmark_ast::Span::new(cur.position_at(*text_start), cur.position_at(end));
    *text_start = end;
    let normalized = normalize_text(raw);
    if !normalized.is_empty() {
        items.push(Inline::Text(Text::new(normalized, span)));
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

fn parse_element(cur: &mut Cursor, default_format: Option<EmbeddedFormat>) -> Result<Element> {
    let start_pos = cur.pos();
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
                    el.area = Some(if is_codeblock(&el) {
                        parse_raw_area(cur)?
                    } else if is_verbatim_area(&el) {
                        parse_verbatim_area(cur)?
                    } else {
                        parse_area(cur, default_format)?
                    });
                    continue;
                }
                Some('{') if el.value.is_none() => {
                    // An explicit local `format` key always wins (including
                    // an unrecognized value opting out of an active document
                    // default); with no local key, inherit the document's
                    // running default, if any.
                    let format = match local_format_key(&el) {
                        Some(explicit) => explicit,
                        None => default_format,
                    };
                    el.value = Some(match format {
                        Some(format) => {
                            ElementValue::Data(parse_embedded_format_value(cur, format)?)
                        }
                        None => parse_value_group(cur, default_format)?,
                    });
                    continue;
                }
                _ => {}
            }
        }
        cur.set_pos(checkpoint);
        break;
    }
    el.span = cur.span_from(start_pos);
    Ok(el)
}

fn parse_paren_value(cur: &mut Cursor) -> Result<Value> {
    if !cur.eat_str("(") {
        return Err(err(cur, cur.pos(), "expected '('"));
    }
    let v = parse_value_at(cur)?;
    skip_ws_newlines_and_comments(cur);
    if !cur.eat_str(")") {
        return Err(err(cur, cur.pos(), "expected ')'"));
    }
    Ok(v)
}

fn parse_area(cur: &mut Cursor, default_format: Option<EmbeddedFormat>) -> Result<Vec<Inline>> {
    if !cur.eat_str("[") {
        return Err(err(cur, cur.pos(), "expected '['"));
    }
    let content = parse_inline_seq(cur, Stop::Bracket(']'), default_format)?;
    if !cur.eat_str("]") {
        return Err(err(cur, cur.pos(), "expected ']'"));
    }
    Ok(content)
}

/// `<codeblock>(lang:xxx)[code]` and any element opting in via
/// `area:raw` (see `is_verbatim_area`) both need a `[...]` that's raw
/// verbatim text rather than going through the full inline grammar
/// (`parse_inline_seq`: em/strong/mark, element triggers, ...) --
/// real source code, and free-form prose that must round-trip byte-for-
/// byte (a memo/notes field), both need `*`/`<`/`@`/backticks and
/// embedded newlines to stay literal instead of being reinterpreted as
/// TypedMark markup or collapsed by `normalize_text`'s lazy-continuation
/// folding. `find_close` supplies the bracket-depth matcher: codeblock
/// uses the quote-aware one (real source code doesn't have unmatched
/// quotes), the generic `area:raw` path uses the quote-agnostic one (see
/// `find_matching_bracket`'s doc comment for why that split exists).
fn parse_raw_area_with(
    cur: &mut Cursor,
    find_close: fn(&mut Cursor, char, char, usize) -> Result<usize>,
) -> Result<Vec<Inline>> {
    let group_start = cur.pos();
    if !cur.eat_str("[") {
        return Err(err(cur, cur.pos(), "expected '['"));
    }
    let body_start = cur.pos();
    let body_end = find_close(cur, '[', ']', group_start)?;
    let raw = cur.src()[body_start..body_end].to_string();
    let span = typedmark_ast::Span::new(cur.position_at(body_start), cur.position_at(body_end));
    cur.set_pos(body_end);
    if !cur.eat_str("]") {
        return Err(err(cur, cur.pos(), "expected ']'"));
    }
    Ok(vec![Inline::Text(Text::new(raw, span))])
}

fn parse_raw_area(cur: &mut Cursor) -> Result<Vec<Inline>> {
    parse_raw_area_with(cur, find_matching_delimiter)
}

/// The `area:raw` opt-in's raw area -- see `is_verbatim_area`.
fn parse_verbatim_area(cur: &mut Cursor) -> Result<Vec<Inline>> {
    parse_raw_area_with(cur, find_matching_bracket)
}

fn is_codeblock(el: &Element) -> bool {
    matches!(&el.sigil, Sigil::Type(name) if name == "codeblock")
}

/// Whether `(input)` carries an `area:raw` key, opting *any* element
/// (not just the built-in `codeblock`) into the same raw/verbatim `[area]`
/// treatment codeblock gets -- e.g. `<memo>(area:raw)[ ... ]`. Unlike
/// `format`/`local_format_key`, this is local-only with no document-wide
/// default and no opt-out state to represent: it's either present with
/// the recognized value or it isn't, so a plain bool is enough. An
/// unrecognized `area:` value (or no `area` key at all) falls back to
/// ordinary prose parsing, same fallback shape as an unrecognized
/// `format:` value.
fn is_verbatim_area(el: &Element) -> bool {
    let Some(Value::Map(entries)) = el.input.as_ref() else {
        return false;
    };
    entries
        .iter()
        .any(|(key, v)| key == "area" && matches!(v, Value::String(tag) if tag == "raw"))
}

/// Tri-state read of `(input)`'s `format` key, shared by a regular
/// element's local override and by `@config`'s own `format` key (see
/// `config_format_update`):
/// - `None`: no `format` key at all (or no `(input)` map) -- inherit the
///   document's running default, if any.
/// - `Some(None)`: `format` key present but its value isn't a recognized
///   format (e.g. `format:none`, `format:xml`) -- an explicit opt-out to
///   the lightweight grammar, even over an active document default.
/// - `Some(Some(fmt))`: `format` key present and recognized.
fn local_format_key(el: &Element) -> Option<Option<EmbeddedFormat>> {
    let Value::Map(entries) = el.input.as_ref()? else {
        return None;
    };
    entries.iter().find_map(|(key, v)| {
        if key != "format" {
            return None;
        }
        Some(match v {
            Value::String(tag) => EmbeddedFormat::from_tag(tag),
            _ => None,
        })
    })
}

fn is_config(el: &Element) -> bool {
    matches!(&el.sigil, Sigil::At(Some(name)) if name == "config")
}

/// If `el` is a `@config` block that touches the `format` key, returns the
/// new running default to install for every element parsed after it
/// (`Some(None)` resets it, e.g. via `@config(format:none)`). `None` means
/// "not `@config`, or `@config` with no `format` key at all" -- leave the
/// running default as-is, so a future `@config` key unrelated to `format`
/// doesn't clobber it.
fn config_format_update(el: &Element) -> Option<Option<EmbeddedFormat>> {
    if !is_config(el) {
        return None;
    }
    local_format_key(el)
}

fn parse_value_group(
    cur: &mut Cursor,
    default_format: Option<EmbeddedFormat>,
) -> Result<ElementValue> {
    if !cur.eat_str("{") {
        return Err(err(cur, cur.pos(), "expected '{'"));
    }
    skip_ws_newlines_and_comments(cur);
    let result = if cur.peek() == Some('(') {
        let mut children = Vec::new();
        loop {
            skip_ws_newlines_and_comments(cur);
            if cur.peek() != Some('(') {
                break;
            }
            children.push(parse_bare_element(cur, default_format)?);
        }
        ElementValue::Children(children)
    } else {
        ElementValue::Data(parse_value_at(cur)?)
    };
    skip_ws_newlines_and_comments(cur);
    if !cur.eat_str("}") {
        return Err(err(cur, cur.pos(), "expected '}'"));
    }
    Ok(result)
}

fn parse_bare_element(cur: &mut Cursor, default_format: Option<EmbeddedFormat>) -> Result<Element> {
    let start_pos = cur.pos();
    let input = parse_paren_value(cur)?;
    let mut el = Element::new(Sigil::Bare);
    el.input = Some(input);
    let checkpoint = cur.pos();
    skip_inline_ws(cur);
    if cur.peek() == Some('[') {
        el.area = Some(parse_area(cur, default_format)?);
    } else {
        cur.set_pos(checkpoint);
    }
    el.span = cur.span_from(start_pos);
    Ok(el)
}
