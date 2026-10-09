//! Parsing for sections (`=[...]`, `=...`) and thematic breaks (`---`).

use crate::element::{parse_groups, parse_sugar_body};
use crate::error::Result;
use crate::inline::{Stop, parse_inline_seq};
use crate::value::{err, skip_inline_ws};
use tomet_ast::{Block, Element, Inline, Paragraph, Placement, Section, Sigil, Value};
use tomet_lexer::Cursor;
use tomet_tree::{ElementExt, element_new};

/// Converts a parsed `[content]`/`|content` block sequence down to the
/// `Vec<Inline>` a heading's title needs. Empty content is an empty title;
/// exactly one plain paragraph is that paragraph's own inline content; a
/// single bare element (`=[ ${x} ]`, nothing else on the line) is demoted
/// to `Placement::Inline` and taken as the title's sole inline node --
/// same demotion `document.rs`'s `continue_into_paragraph` already does
/// when a block-placed element joins running text, since a lone `@`/`$`
/// element inside `[...]` reaches `parse_block_seq` as its own block the
/// same way it would at the top level, not wrapped in a `Paragraph`.
/// Anything else (multiple blocks, or a `Section`) is a parse error --
/// `Section.title` has no field to hold it, so this can't be deferred to
/// `tomet-semantics` the way other content-shape violations are.
fn title_from_content_blocks(cur: &Cursor, content: Option<Vec<Block>>) -> Result<Vec<Inline>> {
    match content {
        None => Ok(Vec::new()),
        Some(blocks) if blocks.is_empty() => Ok(Vec::new()),
        Some(mut blocks) if blocks.len() == 1 => match blocks.pop() {
            Some(Block::Paragraph(p)) => Ok(p.content),
            Some(Block::Element(mut el)) => {
                el.placement = Placement::Inline;
                Ok(vec![Inline::Element(el)])
            }
            Some(Block::Section(_)) => Err(err(
                cur,
                cur.pos(),
                "a heading's title can only hold plain text, not a nested section",
            )),
            None => unreachable!(),
        },
        Some(_) => Err(err(
            cur,
            cur.pos(),
            "a heading's title can only hold one paragraph of plain text, \
             not multiple blocks",
        )),
    }
}

pub(crate) fn is_section_start(cur: &Cursor) -> bool {
    if cur.peek() != Some('=') {
        return false;
    }
    let mut look = *cur;
    look.eat_while(|c| c == '=');
    if crate::element::opens_group(&look) {
        return true;
    }
    let had_ws = matches!(look.peek(), Some(' ') | Some('\t'));
    skip_inline_ws(&mut look);
    if matches!(look.peek(), None | Some('\n') | Some('\r'))
        || look.starts_with("//")
        || look.starts_with("/*")
    {
        return true;
    }
    had_ws
}

pub(crate) fn parse_section(cur: &mut Cursor) -> Result<Section> {
    let start_pos = cur.pos();
    let level = cur.eat_while(|c| c == '=').len();

    let had_ws = matches!(cur.peek(), Some(' ') | Some('\t'));
    skip_inline_ws(cur);

    let mut el = element_new(Sigil::named("section")).with_placement(Placement::Block);
    let mut title: Vec<Inline> = Vec::new();

    match cur.peek() {
        _ if crate::element::opens_group(cur) => {
            // The full form. Groups are read by the same code that reads
            // `@name`'s, so `=` takes `(args)`, `[content]` and `{value}`
            // in any order, and its content -- bracketed or `|`-marked --
            // may span lines.
            parse_groups(cur, &mut el, true)?;
            skip_inline_ws(cur);
            if cur.peek() == Some('=') {
                cur.eat_while(|c| c == '=');
                skip_inline_ws(cur);
                // Allow groups (e.g. `{ attrs }`) to follow decorative '='
                if crate::element::opens_group(cur) {
                    parse_groups(cur, &mut el, true)?;
                    skip_inline_ws(cur);
                    if cur.peek() == Some('=') {
                        cur.eat_while(|c| c == '=');
                        skip_inline_ws(cur);
                    }
                }
            }
            // A heading's title is `Vec<Inline>` (unlike `Element.content`,
            // `Section` has no way to hold more than one block), so `[...]`
            // written here -- now parsed as a block sequence like any other
            // `[content]` -- is only legal when it comes out as a single
            // plain paragraph. `heading` is inline-only content (see
            // `tmtroot/docs/spec/feature/content-shape.tmt`); this is that
            // rule enforced where the AST itself can't represent a
            // violation, rather than deferred to `tomet-semantics`.
            title = title_from_content_blocks(cur, el.content.take())?;
        }
        _ if cur.starts_with("//") || cur.starts_with("/*") => {
            // A heading-less section with a trailing comment.
        }
        _ if had_ws && !matches!(cur.peek(), None | Some('\n') | Some('\r')) => {
            // A sugar heading supports a named connect too, matching its
            // full, bracketed form just above.
            let body = parse_sugar_body(cur)?;
            let mut content = body.content;
            trim_trailing_equals(&mut content);
            title = content;
            el.args = body.args;
            el.value = body.value;
            el.id = body.id;
            el.connects = body.connects;
        }
        None | Some('\n') | Some('\r') => {
            // A heading-less section (`=` or `==` alone on its line).
            // `title` stays empty.
        }
        _ => return Err(err(cur, cur.pos(), "expected '[' or a space after '='")),
    }

    skip_inline_ws(cur);
    if cur.starts_with("//") {
        crate::value::skip_line_comment(cur);
    }
    if matches!(cur.peek(), Some('\n') | Some('\r')) {
        cur.bump();
    }

    let span = cur.span_from(start_pos);

    Ok(Section {
        level,
        title,
        args: el.args,
        value: el.value,
        id: el.id,
        connects: el.connects,
        blocks: Vec::new(),
        span,
    })
}

pub(crate) fn is_thematic_break(cur: &Cursor) -> bool {
    let mut look = *cur;
    if look.eat_while(|c| c == '-').len() < 3 {
        return false;
    }
    skip_inline_ws(&mut look);
    matches!(look.peek(), None | Some('\n') | Some('\r'))
}

pub(crate) fn consume_thematic_break(cur: &mut Cursor) {
    cur.eat_while(|c| c == '-' || c == ' ' || c == '\t');
    if matches!(cur.peek(), Some('\n') | Some('\r')) {
        cur.bump();
    }
}

pub(crate) fn is_titled_thematic_break_start(cur: &Cursor) -> bool {
    let mut look = *cur;
    if look.eat_while(|c| c == '-').len() < 3 {
        return false;
    }
    skip_inline_ws(&mut look);
    look.peek() == Some('[')
}

pub(crate) fn parse_titled_thematic_break(cur: &mut Cursor) -> Result<Element> {
    let start_pos = cur.pos();
    cur.eat_while(|c| c == '-');
    skip_inline_ws(cur);
    if !cur.eat_str("[") {
        return Err(err(cur, cur.pos(), "expected '['"));
    }
    let title = parse_inline_seq(cur, Stop::Bracket(']'), true)?;
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
    let title_span = cur.span_from(start_pos);
    let mut el = element_new(Sigil::named("hr"))
        .with_placement(Placement::Block)
        .with_span(title_span);
    el.content = Some(vec![Block::Paragraph(Paragraph::new(title, title_span))]);
    Ok(el)
}

pub(crate) fn merge_values(direct: Option<&Value>, connected: Option<&Value>) -> Option<Value> {
    match (direct, connected) {
        (None, None) => None,
        (Some(d), None) => Some(d.clone()),
        (None, Some(c)) => Some(c.clone()),
        (Some(d), Some(c)) => Some(merge_values_inner(d, c)),
    }
}

fn merge_values_inner(direct: &Value, connected: &Value) -> Value {
    match (direct, connected) {
        (Value::Map(d_map), Value::Map(c_map)) => {
            let mut merged = c_map.clone();
            for (d_k, d_v) in d_map {
                if let Some(pos) = merged.iter().position(|(c_k, _)| c_k == d_k) {
                    let c_v = &merged[pos].1;
                    merged[pos].1 = merge_values_inner(d_v, c_v);
                } else {
                    merged.push((d_k.clone(), d_v.clone()));
                }
            }
            Value::Map(merged)
        }
        (Value::Seq(d_seq), Value::Seq(c_seq)) => {
            let mut merged = c_seq.clone();
            for item in d_seq {
                if !merged.contains(item) {
                    merged.push(item.clone());
                }
            }
            Value::Seq(merged)
        }
        (direct_val, _) => direct_val.clone(),
    }
}

fn trim_trailing_equals(inlines: &mut Vec<Inline>) {
    if let Some(Inline::Text(t)) = inlines.last_mut() {
        let trimmed = t.value.trim_end();
        if trimmed.ends_with('=') {
            let before_eq = trimmed.trim_end_matches('=');
            if before_eq.is_empty() || before_eq.ends_with(char::is_whitespace) {
                t.value = before_eq.trim_end().to_string();
                if t.value.is_empty() {
                    inlines.pop();
                }
            }
        }
    }
}
