//! Parsing for `${...}` interpolation expressions.

use crate::error::Result;
use crate::value::{err, parse_quoted, skip_inline_ws};
use tomet_ast::{Element, InterpExpr, InterpExprKind, Literal, Sigil};
use tomet_lexar::Cursor;

/// `$` immediately followed by `{`
pub(crate) fn is_interp_start(cur: &Cursor) -> bool {
    let mut look = *cur;
    look.bump() == Some('$') && look.peek() == Some('{')
}

/// `${ Expr }` -- structurally just `Sigil::Dollar` with a mandatory `{value}` group
pub(crate) fn parse_dollar_element(cur: &mut Cursor) -> Result<Element> {
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
    el.value = Some(tomet_ast::ElementValue::Interp(expr));
    el.span = cur.span_from(start_pos);
    Ok(el)
}

/// `primary (. Ident | ( Args ))*`
pub(crate) fn parse_interp_expr(cur: &mut Cursor) -> Result<InterpExpr> {
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
