//! Parsing for fenced code blocks.

use crate::error::Result;
use crate::value::skip_inline_ws;
use tomet_ast::{Element, Inline, Placement, RawText, Sigil, Span, Value};
use tomet_lexer::Cursor;
use tomet_tree::{ElementExt, element_new};

pub(crate) fn is_fenced_code_block_start(cur: &Cursor) -> bool {
    let mut look = *cur;
    look.eat_while(|c| c == '`').len() >= 3
}

pub(crate) fn parse_fenced_code_block(cur: &mut Cursor) -> Result<Element> {
    let start_pos = cur.pos();
    let fence_len = cur.eat_while(|c| c == '`').len();

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

    let (body_start, body_end) = crate::fence::scan_fenced_body(cur, '`', fence_len);

    let mut code = cur.src()[body_start..body_end].to_string();
    if code.ends_with('\n') {
        code.pop();
    }

    let args = if lang.is_empty() {
        None
    } else {
        Some(Value::Map(vec![("lang".to_string(), Value::String(lang))]))
    };
    let content_span = Span::new(cur.position_at(body_start), cur.position_at(body_end));
    let mut el = element_new(Sigil::named("codeblock"))
        .with_placement(Placement::Block)
        .with_span(cur.span_from(start_pos))
        .with_content(vec![Inline::Raw(RawText::new(code, content_span))]);
    el.args = args;
    Ok(el)
}
