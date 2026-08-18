//! Parsing for the full markup grammar: headings, lists, paragraphs, and
//! typed inline/block elements (`<T>(args)[content]{value}`, `@name...`,
//! bare `@(key:...)`, and the bare-element children of containers like
//! `@links{}`). See `docs/tmt/typedmark.tm` in the repo root for the
//! syntax this follows.

use crate::embedded_format::{EmbeddedFormat, parse_embedded_format_value};
use crate::error::Result;
use crate::value::{
    eat_ident, err, find_matching_bracket, find_matching_delimiter, is_ident_char, parse_quoted,
    parse_value_at, skip_inline_ws, skip_ws_and_newlines, skip_ws_newlines_and_comments,
};
use typedmark_ast::{
    Block, Document, Element, ElementValue, Heading, Inline, InterpExpr, InterpExprKind, List,
    ListItem, Literal, Paragraph, Sigil, Text, Value,
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
        } else if is_fenced_code_block_start(&cur) {
            Some(Block::Element(parse_fenced_code_block(&mut cur)?))
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
    el.content = Some(title);
    Ok(el)
}

/// Whether the current position starts a fenced code block: 3 or more
/// consecutive backticks at the start of a line. TypedMark's grammar is
/// indentation-independent throughout (headings/list markers/thematic
/// breaks all only fire at column 1 too), so no leading-whitespace
/// tolerance is offered here either.
fn is_fenced_code_block_start(cur: &Cursor) -> bool {
    let mut look = *cur;
    look.eat_while(|c| c == '`').len() >= 3
}

/// ```` ```lang\ncode\n``` ```` -- sugar for `<codeblock>(lang:xxx)[code]`.
/// Builds the exact same `Element` shape the bracket form does
/// (`Sigil::Type("codeblock")`, `args` holding `lang`, `content` holding
/// the body as a single raw `Inline::Text`), so every downstream consumer
/// (`typedmark_semantics::classify`, `typedmark-html`,
/// `typedmark-markdown`, `typedmark-formatter`'s raw-span detection) needs
/// no changes -- they all key off the sigil name, not which syntax
/// produced it.
fn parse_fenced_code_block(cur: &mut Cursor) -> Result<Element> {
    let start_pos = cur.pos();
    let fence_len = cur.eat_while(|c| c == '`').len();

    // Info string: everything to end of line; only its first
    // whitespace-separated word becomes `lang`, matching
    // `typedmark_markdown::import`'s `CodeBlockKind::Fenced(lang)` handling.
    skip_inline_ws(cur);
    let info_start = cur.pos();
    cur.eat_while(|c| c != '\n' && c != '\r');
    let lang = cur
        .slice_from(info_start)
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string();
    if matches!(cur.peek(), Some('\n') | Some('\r')) {
        cur.bump();
    }

    let body_start = cur.pos();
    let body_end;
    loop {
        if cur.is_eof() {
            // Unterminated is not an error -- same reasoning
            // `skip_line_comment` uses (running to EOF is a visible,
            // bounded consequence, not a silent swallow), and it matches
            // CommonMark's own fenced-code-block spec (an unclosed fence
            // just runs to the end of the document). Deliberately
            // asymmetric with `<codeblock>[...]`'s `find_matching_delimiter`,
            // which hard-errors on EOF -- that form has an explicit `]`
            // closer, this one's closer is a variable-length marker with
            // no single required character to fail on.
            body_end = cur.pos();
            break;
        }
        let line_start = cur.pos();
        let mut look = *cur;
        let run = look.eat_while(|c| c == '`').len();
        if run >= fence_len {
            skip_inline_ws(&mut look);
            if matches!(look.peek(), None | Some('\n') | Some('\r')) {
                // A valid closing fence: a line made of nothing but
                // backticks (>= the opening count) plus trailing
                // whitespace.
                body_end = line_start;
                cur.set_pos(look.pos());
                if matches!(cur.peek(), Some('\n') | Some('\r')) {
                    cur.bump();
                }
                break;
            }
        }
        // Not a closing fence -- consume this whole source line as body.
        cur.eat_while(|c| c != '\n' && c != '\r');
        if matches!(cur.peek(), Some('\n') | Some('\r')) {
            cur.bump();
        }
    }

    let mut code = cur.src()[body_start..body_end].to_string();
    if code.ends_with('\n') {
        code.pop();
    }

    let args = if lang.is_empty() {
        None
    } else {
        Some(Value::Map(vec![("lang".to_string(), Value::String(lang))]))
    };
    let content_span =
        typedmark_ast::Span::new(cur.position_at(body_start), cur.position_at(body_end));
    let mut el = Element::new(Sigil::Type("codeblock".to_string()));
    el.args = args;
    el.content = Some(vec![Inline::Text(Text::new(code, content_span))]);
    el.span = cur.span_from(start_pos);
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
    // can't double-count) nests instead of ending the content at the first
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
                        || (look.peek() == Some('<') && is_type_element_start(&look))
                        || (look.peek() == Some('@') && is_at_element_start(&look))
                        || is_titled_thematic_break_start(&look)
                        || is_thematic_break(&look)
                        || is_fenced_code_block_start(&look)
                    {
                        break;
                    }
                    // Lazy continuation: no blank line and no new block marker
                    // (heading/list/comment/element trigger/thematic break/
                    // fenced code block), so this newline is just part of the
                    // running text. An element trigger on the next line
                    // always ends the paragraph rather than continuing it --
                    // `<T>`/`@name` are block-shaped constructs in their own
                    // right, not prose, so `@meta{...}` immediately followed
                    // by `@settings{...}` (no blank line between them)
                    // becomes two separate `Block::Element`s rather than one
                    // `Block::Paragraph` with both folded into it. Same
                    // reasoning for `---`/`---[title]---`/fenced ` ``` `
                    // blocks: they're block-shaped, not prose, even directly
                    // after a paragraph line with no blank line between.
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
            // without it being parsed as a real trigger. Dry-run first
            // (same idea as `try_one_delimited`'s dry run for
            // `*em*`/`**strong**`): only commit to treating this as a code
            // span if a closing '`' exists on the *same line*. Without
            // this, an unterminated '`' would otherwise swallow everything
            // up to the next stray backtick anywhere later in the source,
            // across paragraph/block boundaries -- restricting the search
            // to the current line keeps a missing closer a local, visible
            // failure (falls back to a literal '`' below) instead of a
            // silent runaway one.
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
            // No closing '`' on this line -- fall through and treat this
            // '`' as an ordinary character.
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
        if cur.peek() == Some('$') && is_interp_start(cur) {
            flush_text(&mut items, cur, &mut text_start);
            items.push(Inline::Element(parse_dollar_element(cur)?));
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
    el.content = Some(inner);
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

/// Between a sigil's name and its first `(`/`[`/`{` group, and between
/// consecutive groups, `parse_element` tolerates inline whitespace,
/// `//`/`/* */` comments, and up to one bare newline (so e.g. `@links {`,
/// a heading's attrs on their own line, or a trailing `// note` between
/// two groups all still parse) -- the lookahead here has to tolerate
/// exactly the same gap, or it'll disagree with `parse_element` about
/// whether a trigger is even present. A comment is consumed in full
/// regardless of how many newlines it itself spans (a multi-line `/*
/// ... */` doesn't count against the budget); only bare newlines outside
/// a comment do. An unterminated `/*` is left untouched here (not
/// consumed, not an error) -- this is a best-effort lookahead/gap-skip,
/// not the real parse; the caller's own comment handling, reached once
/// this gap tolerance gives up, is what reports it properly. Returns the
/// number of bare newlines seen (capped at 2, where 2 means "budget
/// exceeded").
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
        let newlines = skip_element_gap(cur);
        if newlines <= 1 {
            match cur.peek() {
                Some('(') if el.args.is_none() => {
                    el.args = Some(parse_paren_value(cur)?);
                    continue;
                }
                // A second `(args)`/`[content]`/`{value}` group of a kind
                // already seen is invalid (each group at most once) --
                // distinct from an unrelated `(`/`[`/`{` starting fresh
                // text after the element's groups end, which the `_` arm
                // below still lets through to `break`.
                Some('(') => {
                    return Err(err(cur, cur.pos(), "duplicate '(' group"));
                }
                Some('[') if el.content.is_none() => {
                    el.content = Some(if is_codeblock(&el) {
                        parse_raw_content(cur)?
                    } else if is_verbatim_content(&el) {
                        parse_verbatim_content(cur)?
                    } else {
                        parse_content(cur, default_format)?
                    });
                    continue;
                }
                Some('[') => {
                    return Err(err(cur, cur.pos(), "duplicate '[' group"));
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
                Some('{') => {
                    return Err(err(cur, cur.pos(), "duplicate '{' group"));
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

/// `$` immediately followed by `{` -- unlike `@name`, `$` never takes an
/// identifier of its own before its brace group, so no gap/name lookahead
/// is needed. A `$` not immediately followed by `{` (`$5`, `$ {x}`) isn't
/// recognized at all and falls through to plain text, same as an `@` that
/// doesn't resolve to a real element.
fn is_interp_start(cur: &Cursor) -> bool {
    let mut look = *cur;
    look.bump() == Some('$') && look.peek() == Some('{')
}

/// `${ Expr }` -- structurally just `Sigil::Dollar` with a mandatory
/// `{value}` group, the same shape as `@name{value}` (no `name` of its
/// own, no `(args)`/`[content]` groups in v1), so it produces a real
/// `Element` rather than a bespoke node -- `parse_inline_seq`'s caller
/// wraps it in `Inline::Element` exactly like `@`/`<T>`. Grammar-only:
/// the `InterpExpr` inside doesn't look anything up. Inline whitespace is
/// allowed around the expression; newlines are not (there's no
/// `skip_ws_and_newlines` call anywhere in this function), which
/// deliberately keeps `${...}` a single-line construct with no
/// interaction with `Stop::Paragraph`'s lazy-continuation logic.
fn parse_dollar_element(cur: &mut Cursor) -> Result<Element> {
    let start_pos = cur.pos();
    cur.eat_str("$");
    cur.eat_str("{");
    skip_inline_ws(cur);
    if cur.peek() == Some('}') {
        return Err(err(
            cur,
            cur.pos(),
            "empty interpolation, expected an expression",
        ));
    }
    let expr = parse_interp_expr(cur)?;
    skip_inline_ws(cur);
    if !cur.eat_str("}") {
        return Err(err(cur, cur.pos(), "unterminated '${', expected '}'"));
    }
    let mut el = Element::new(Sigil::Dollar);
    el.value = Some(ElementValue::Interp(expr));
    el.span = cur.span_from(start_pos);
    Ok(el)
}

/// `primary (. Ident | ( Args ))*` -- a standard postfix-chain parser.
/// `primary` is an identifier or a literal; each trailing `.member` wraps
/// the expression-so-far in `Member`, each trailing `(args)` wraps it in
/// `Call`. So `a.b.c` (a pure dotted chain), `sum(a, b)` (a call), and
/// `b(x).id` (member access on a call's result) all fall out of the same
/// loop -- there's no separate flat "path" grammar; `Member` alone covers
/// a dotted chain when no `Call` appears in it.
fn parse_interp_expr(cur: &mut Cursor) -> Result<InterpExpr> {
    skip_inline_ws(cur);
    let start_pos = cur.pos();
    let mut expr = parse_interp_primary(cur, start_pos)?;
    loop {
        let mut look = *cur;
        skip_inline_ws(&mut look);
        match look.peek() {
            Some('.') => {
                look.bump();
                skip_inline_ws(&mut look);
                *cur = look;
                let member = eat_interp_ident(cur)?.to_string();
                expr = InterpExpr {
                    kind: InterpExprKind::Member {
                        object: Box::new(expr),
                        member,
                    },
                    span: cur.span_from(start_pos),
                };
            }
            Some('(') => {
                *cur = look;
                let args = parse_interp_call_args(cur)?;
                expr = InterpExpr {
                    kind: InterpExprKind::Call {
                        callee: Box::new(expr),
                        args,
                    },
                    span: cur.span_from(start_pos),
                };
            }
            _ => break,
        }
    }
    Ok(expr)
}

/// An identifier or a literal (string/number). `start_pos` is the
/// caller's already-whitespace-skipped position, shared so the returned
/// node's span starts exactly at the token, not before any leading gap.
fn parse_interp_primary(cur: &mut Cursor, start_pos: usize) -> Result<InterpExpr> {
    match cur.peek() {
        Some('"') => {
            let s = parse_quoted(cur)?;
            Ok(InterpExpr {
                kind: InterpExprKind::Literal(Literal::String(s)),
                span: cur.span_from(start_pos),
            })
        }
        Some(c) if c.is_ascii_digit() => parse_interp_number(cur, start_pos),
        Some('-') if cur.peek_at(1).is_some_and(|c| c.is_ascii_digit()) => {
            parse_interp_number(cur, start_pos)
        }
        Some(c) if is_interp_ident_start(c) => {
            let name = eat_interp_ident(cur)?.to_string();
            Ok(InterpExpr {
                kind: InterpExprKind::Identifier(name),
                span: cur.span_from(start_pos),
            })
        }
        _ => Err(err(
            cur,
            cur.pos(),
            "expected a value, identifier, or call in '${...}'",
        )),
    }
}

fn parse_interp_call_args(cur: &mut Cursor) -> Result<Vec<InterpExpr>> {
    cur.eat_str("(");
    skip_inline_ws(cur);
    let mut args = Vec::new();
    if cur.peek() != Some(')') {
        loop {
            args.push(parse_interp_expr(cur)?);
            skip_inline_ws(cur);
            match cur.peek() {
                Some(',') => {
                    cur.bump();
                    skip_inline_ws(cur);
                }
                Some(')') => break,
                _ => return Err(err(cur, cur.pos(), "expected ',' or ')' in call arguments")),
            }
        }
    }
    if !cur.eat_str(")") {
        return Err(err(cur, cur.pos(), "expected ')'"));
    }
    Ok(args)
}

/// Deliberately its own scanner, not `value.rs`'s `eat_scalar_raw`/
/// `scalar_from_text`: those can't tell a float's `.` apart from a
/// following `.member` access's separator. Requires a digit right after
/// `.` to commit to a float, so `${1.foo}` parses `Literal::Int(1)` and
/// leaves `.foo` for `parse_interp_expr`'s postfix loop to read as a
/// (semantically odd, but not a parse error) `Member` access, rather than
/// silently swallowing it.
fn parse_interp_number(cur: &mut Cursor, start: usize) -> Result<InterpExpr> {
    if cur.peek() == Some('-') {
        cur.bump();
    }
    let digits = cur.eat_while(|c| c.is_ascii_digit());
    if digits.is_empty() {
        return Err(err(cur, cur.pos(), "expected digits"));
    }
    let mut is_float = false;
    if cur.peek() == Some('.') && cur.peek_at(1).is_some_and(|c| c.is_ascii_digit()) {
        is_float = true;
        cur.bump();
        cur.eat_while(|c| c.is_ascii_digit());
    }
    let text = cur.slice_from(start);
    let literal = if is_float {
        text.parse::<f64>()
            .map(Literal::Float)
            .map_err(|_| err(cur, start, "invalid float literal"))?
    } else {
        text.parse::<i64>()
            .map(Literal::Int)
            .map_err(|_| err(cur, start, "invalid integer literal"))?
    };
    Ok(InterpExpr {
        kind: InterpExprKind::Literal(literal),
        span: cur.span_from(start),
    })
}

/// Deliberately its own predicate, not `value.rs`'s `is_ident_char`: that
/// one treats both `.` and `-` as ident characters (map keys/bare scalars
/// like `file-name.ext`), which would swallow a `Path`'s `a.b.c` into one
/// opaque token instead of three dot-separated segments.
fn is_interp_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_interp_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn eat_interp_ident<'a>(cur: &mut Cursor<'a>) -> Result<&'a str> {
    let start = cur.pos();
    if !cur.peek().is_some_and(is_interp_ident_start) {
        return Err(err(cur, start, "expected an identifier"));
    }
    Ok(cur.eat_while(is_interp_ident_char))
}

fn parse_paren_value(cur: &mut Cursor) -> Result<Value> {
    if !cur.eat_str("(") {
        return Err(err(cur, cur.pos(), "expected '('"));
    }
    skip_ws_newlines_and_comments(cur);
    // `()` (possibly with only whitespace/comments inside) is a valid,
    // deliberately-empty args map -- distinct from omitting `(...)`
    // entirely (`el.args` stays `None` in that case).
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

fn parse_content(cur: &mut Cursor, default_format: Option<EmbeddedFormat>) -> Result<Vec<Inline>> {
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
/// `content:raw` (see `is_verbatim_content`) both need a `[...]` that's raw
/// verbatim text rather than going through the full inline grammar
/// (`parse_inline_seq`: em/strong/mark, element triggers, ...) --
/// real source code, and free-form prose that must round-trip byte-for-
/// byte (a memo/notes field), both need `*`/`<`/`@`/backticks and
/// embedded newlines to stay literal instead of being reinterpreted as
/// TypedMark markup or collapsed by `normalize_text`'s lazy-continuation
/// folding. `find_close` supplies the bracket-depth matcher: codeblock
/// uses the quote-aware one (real source code doesn't have unmatched
/// quotes), the generic `content:raw` path uses the quote-agnostic one (see
/// `find_matching_bracket`'s doc comment for why that split exists).
fn parse_raw_content_with(
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

fn parse_raw_content(cur: &mut Cursor) -> Result<Vec<Inline>> {
    parse_raw_content_with(cur, find_matching_delimiter)
}

/// The `content:raw` opt-in's raw content -- see `is_verbatim_content`.
fn parse_verbatim_content(cur: &mut Cursor) -> Result<Vec<Inline>> {
    parse_raw_content_with(cur, find_matching_bracket)
}

fn is_codeblock(el: &Element) -> bool {
    matches!(&el.sigil, Sigil::Type(name) if name == "codeblock")
}

/// Whether `(args)` carries a `content:raw` key, opting *any* element
/// (not just the built-in `codeblock`) into the same raw/verbatim
/// `[content]` treatment codeblock gets -- e.g.
/// `<memo>(content:raw)[ ... ]`. Unlike `format`/`local_format_key`, this
/// is local-only with no document-wide default and no opt-out state to
/// represent: it's either present with the recognized value or it isn't,
/// so a plain bool is enough. An unrecognized `content:` value (or no
/// `content` key at all) falls back to ordinary prose parsing, same
/// fallback shape as an unrecognized `format:` value.
fn is_verbatim_content(el: &Element) -> bool {
    let Some(Value::Map(entries)) = el.args.as_ref() else {
        return false;
    };
    entries
        .iter()
        .any(|(key, v)| key == "content" && matches!(v, Value::String(tag) if tag == "raw"))
}

/// Tri-state read of `(args)`'s `format` key, shared by a regular
/// element's local override and by `@config`'s own `format` key (see
/// `config_format_update`):
/// - `None`: no `format` key at all (or no `(args)` map) -- inherit the
///   document's running default, if any.
/// - `Some(None)`: `format` key present but its value isn't a recognized
///   format (e.g. `format:none`, `format:xml`) -- an explicit opt-out to
///   the lightweight grammar, even over an active document default.
/// - `Some(Some(fmt))`: `format` key present and recognized.
fn local_format_key(el: &Element) -> Option<Option<EmbeddedFormat>> {
    let Value::Map(entries) = el.args.as_ref()? else {
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
    } else if cur.peek() == Some('}') {
        // Same "deliberately empty" case as `parse_paren_value`'s `()`.
        ElementValue::Data(Value::Map(Vec::new()))
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
    let args = parse_paren_value(cur)?;
    let mut el = Element::new(Sigil::Bare);
    el.args = Some(args);
    let checkpoint = cur.pos();
    skip_inline_ws(cur);
    if cur.peek() == Some('[') {
        el.content = Some(parse_content(cur, default_format)?);
    } else {
        cur.set_pos(checkpoint);
    }
    el.span = cur.span_from(start_pos);
    Ok(el)
}
