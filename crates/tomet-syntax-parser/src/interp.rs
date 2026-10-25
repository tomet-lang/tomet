//! Parsing for `${...}` interpolation expressions.

use crate::error::Result;
use crate::value::{err, parse_quoted};
use tomet_ast::{BinaryOp, Element, InterpExpr, InterpExprKind, Literal, Sigil};
use tomet_lexer::Cursor;
use tomet_tree::element_new;

pub use cotomet_syntax::{is_interp_ident_char, is_interp_ident_start, is_interp_start};

/// `${ Expr }` or `$func(args)` or `$macro.func(args)` -- structurally `Sigil::Dollar` with an `Interp` element value
pub(crate) fn parse_dollar_element(cur: &mut Cursor) -> Result<Element> {
    let start_pos = cur.pos();
    cur.eat_str("$");
    if cur.peek() == Some('{') {
        cur.eat_str("{");
        skip_expr_ws(cur);
        if cur.peek() == Some('}') {
            return Err(err(
                cur.pos(),
                "empty interpolation, expected an expression",
            ));
        }
        let expr = parse_interp_expr(cur)?;
        skip_expr_ws(cur);
        if !cur.eat_str("}") {
            return Err(err(cur.pos(), "unterminated '${', expected '}'"));
        }
        let mut el = element_new(Sigil::Dollar);
        el.value = Some(tomet_ast::ElementValue::Interp(expr));
        el.span = cur.span_from(start_pos);
        Ok(el)
    } else {
        let ident_start = cur.pos();
        let name = eat_interp_ident(cur)?.to_string();
        let mut expr = if name == "addr" && cur.peek() == Some('(') {
            parse_addr_literal(cur, ident_start)?
        } else {
            InterpExpr {
                kind: InterpExprKind::Identifier(name),
                span: cur.span_from(ident_start),
            }
        };
        loop {
            let mut look = *cur;
            skip_expr_ws(&mut look);
            match look.peek() {
                Some('.') => {
                    look.bump();
                    skip_expr_ws(&mut look);
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
        let mut el = element_new(Sigil::Dollar);
        el.value = Some(tomet_ast::ElementValue::Interp(expr));
        el.span = cur.span_from(start_pos);
        Ok(el)
    }
}

/// `Lambda | NamedArg | Or`, the whole of an expression.
///
/// Precedence, loosest first: `=>`, `||`, `&&`, comparison, then the
/// postfix forms (`.name`, `(args)`). The grammar has no keywords, so
/// `true`, `false` and `null` are the only words that mean anything on
/// their own, and they are values.
pub(crate) fn parse_interp_expr(cur: &mut Cursor) -> Result<InterpExpr> {
    skip_expr_ws(cur);
    let start_pos = cur.pos();

    // `x => body` and `(a, b) => body`
    if let Some((params, after_arrow)) = lambda_head(cur) {
        *cur = after_arrow;
        let body = parse_interp_expr(cur)?;
        return Ok(InterpExpr {
            kind: InterpExprKind::Lambda {
                params,
                body: Box::new(body),
            },
            span: cur.span_from(start_pos),
        });
    }

    // Check if this is `name: expr` (NamedArg)
    let mut look = *cur;
    if look.peek().is_some_and(is_interp_ident_start)
        && let Ok(name) = eat_interp_ident(&mut look)
    {
        skip_expr_ws(&mut look);
        if look.peek() == Some(':') && look.peek_at(1) != Some('/') {
            look.bump(); // consume ':'
            skip_expr_ws(&mut look);
            *cur = look;
            let val_expr = parse_interp_expr(cur)?;
            return Ok(InterpExpr {
                kind: InterpExprKind::NamedArg {
                    name: name.to_string(),
                    value: Box::new(val_expr),
                },
                span: cur.span_from(start_pos),
            });
        }
    }

    parse_interp_or(cur)
}

/// Recognizes the head of a lambda -- `x =>` or `(a, b) =>` -- without
/// consuming anything, and returns the parameter names with the cursor
/// moved past the arrow. `None` when the text is not a lambda head, so the
/// caller falls through to the other readings of the same prefix (`x: v`
/// is a named argument, `(x)` is a group).
fn lambda_head<'a>(cur: &Cursor<'a>) -> Option<(Vec<String>, Cursor<'a>)> {
    let mut look = *cur;
    let mut params = Vec::new();
    if look.peek() == Some('(') {
        look.bump();
        skip_expr_ws(&mut look);
        loop {
            if !look.peek().is_some_and(is_interp_ident_start) {
                return None;
            }
            params.push(eat_interp_ident(&mut look).ok()?.to_string());
            skip_expr_ws(&mut look);
            match look.peek() {
                Some(',') => {
                    look.bump();
                    skip_expr_ws(&mut look);
                }
                Some(')') => {
                    look.bump();
                    break;
                }
                _ => return None,
            }
        }
    } else if look.peek().is_some_and(is_interp_ident_start) {
        params.push(eat_interp_ident(&mut look).ok()?.to_string());
    } else {
        return None;
    }
    skip_expr_ws(&mut look);
    if look.peek() == Some('=') && look.peek_at(1) == Some('>') {
        look.bump();
        look.bump();
        skip_expr_ws(&mut look);
        Some((params, look))
    } else {
        None
    }
}

fn binary(
    op: BinaryOp,
    lhs: InterpExpr,
    rhs: InterpExpr,
    cur: &Cursor,
    start_pos: usize,
) -> InterpExpr {
    InterpExpr {
        kind: InterpExprKind::Binary {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        },
        span: cur.span_from(start_pos),
    }
}

/// `And ('||' And)*`
fn parse_interp_or(cur: &mut Cursor) -> Result<InterpExpr> {
    skip_expr_ws(cur);
    let start_pos = cur.pos();
    let mut lhs = parse_interp_and(cur)?;
    loop {
        let mut look = *cur;
        skip_expr_ws(&mut look);
        if look.peek() == Some('|') && look.peek_at(1) == Some('|') {
            look.bump();
            look.bump();
            *cur = look;
            let rhs = parse_interp_and(cur)?;
            lhs = binary(BinaryOp::Or, lhs, rhs, cur, start_pos);
        } else if look.peek() == Some('|') {
            return Err(err(look.pos(), "unexpected '|', did you mean '||'?"));
        } else {
            return Ok(lhs);
        }
    }
}

/// `Comparison ('&&' Comparison)*`
fn parse_interp_and(cur: &mut Cursor) -> Result<InterpExpr> {
    skip_expr_ws(cur);
    let start_pos = cur.pos();
    let mut lhs = parse_interp_comparison(cur)?;
    loop {
        let mut look = *cur;
        skip_expr_ws(&mut look);
        if look.peek() == Some('&') && look.peek_at(1) == Some('&') {
            look.bump();
            look.bump();
            *cur = look;
            let rhs = parse_interp_comparison(cur)?;
            lhs = binary(BinaryOp::And, lhs, rhs, cur, start_pos);
        } else if look.peek() == Some('&') {
            return Err(err(look.pos(), "unexpected '&', did you mean '&&'?"));
        } else {
            return Ok(lhs);
        }
    }
}

/// The comparison operator at the cursor, if any, with the cursor moved
/// past it. `=>` is the lambda arrow and is not a comparison.
fn take_comparison(cur: &mut Cursor) -> Result<Option<BinaryOp>> {
    let mut look = *cur;
    skip_expr_ws(&mut look);
    let (op, len) = match (look.peek(), look.peek_at(1)) {
        (Some('='), Some('=')) => (BinaryOp::Eq, 2),
        (Some('!'), Some('=')) => (BinaryOp::Ne, 2),
        (Some('<'), Some('=')) => (BinaryOp::Le, 2),
        (Some('>'), Some('=')) => (BinaryOp::Ge, 2),
        (Some('<'), _) => (BinaryOp::Lt, 1),
        (Some('>'), _) => (BinaryOp::Gt, 1),
        (Some('='), Some('>')) => return Ok(None),
        (Some('='), _) => return Err(err(look.pos(), "unexpected '=', did you mean '=='?")),
        _ => return Ok(None),
    };
    for _ in 0..len {
        look.bump();
    }
    *cur = look;
    Ok(Some(op))
}

/// `Postfix (CompareOp Postfix)?` -- comparisons do not chain.
fn parse_interp_comparison(cur: &mut Cursor) -> Result<InterpExpr> {
    skip_expr_ws(cur);
    let start_pos = cur.pos();
    let lhs = parse_interp_postfix(cur)?;
    let Some(op) = take_comparison(cur)? else {
        return Ok(lhs);
    };
    let rhs = parse_interp_postfix(cur)?;
    let mut look = *cur;
    if take_comparison(&mut look)?.is_some() {
        return Err(err(
            cur.pos(),
            "comparison operators cannot be chained, group one side with parentheses",
        ));
    }
    Ok(binary(op, lhs, rhs, cur, start_pos))
}

/// `primary (. Ident | ( Args ))*`
fn parse_interp_postfix(cur: &mut Cursor) -> Result<InterpExpr> {
    skip_expr_ws(cur);
    let start_pos = cur.pos();
    let mut expr = parse_interp_primary(cur, start_pos)?;
    loop {
        // `doc#id`: the hash must touch what is on its left, so `#` after a
        // space can never be read as a document boundary by accident.
        if cur.peek() == Some('#') {
            let id = parse_hash_id(cur)?;
            expr = InterpExpr {
                kind: InterpExprKind::DocId {
                    doc: Box::new(expr),
                    id,
                },
                span: cur.span_from(start_pos),
            };
            continue;
        }
        let mut look = *cur;
        skip_expr_ws(&mut look);
        match look.peek() {
            Some('.') => {
                look.bump();
                skip_expr_ws(&mut look);
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

/// `addr(...)`, with the cursor on the `(`: an address written as text.
///
/// This is the one place the expression grammar reads raw text. The raw
/// form takes everything up to the matching `)` (parentheses nest), so the
/// `:`, `.`, `-` and `/` of `tmt:docs/a.tmt#intro` are not expression
/// symbols; the quoted form is for text the raw form cannot carry. Either
/// way the argument is a literal and is never evaluated.
fn parse_addr_literal(cur: &mut Cursor, start_pos: usize) -> Result<InterpExpr> {
    cur.bump(); // '('
    skip_expr_ws(cur);
    let (text, raw) = if cur.peek() == Some('"') {
        let quoted = parse_quoted(cur)?;
        skip_expr_ws(cur);
        (quoted, false)
    } else {
        let begin = cur.pos();
        let mut depth = 0usize;
        loop {
            match cur.peek() {
                None | Some('\n') => {
                    return Err(err(begin, "unterminated 'addr(', expected ')'"));
                }
                Some('(') => {
                    depth += 1;
                    cur.bump();
                }
                Some(')') if depth == 0 => break,
                Some(')') => {
                    depth -= 1;
                    cur.bump();
                }
                Some(_) => {
                    cur.bump();
                }
            }
        }
        (cur.slice_from(begin).trim_end().to_string(), true)
    };
    if !cur.eat_str(")") {
        return Err(err(cur.pos(), "expected ')' after the address"));
    }
    if text.is_empty() {
        return Err(err(start_pos, "empty address in 'addr()'"));
    }
    Ok(InterpExpr {
        kind: InterpExprKind::Addr { text, raw },
        span: cur.span_from(start_pos),
    })
}

/// `#name` or `#(name)`, with the cursor on the `#`. Returns the id.
fn parse_hash_id(cur: &mut Cursor) -> Result<String> {
    cur.bump(); // '#'
    let has_paren = cur.peek() == Some('(');
    if has_paren {
        cur.bump();
        skip_expr_ws(cur);
    }
    let name = eat_interp_ident(cur)?.to_string();
    if has_paren {
        skip_expr_ws(cur);
        if cur.peek() == Some(')') {
            cur.bump();
        } else {
            return Err(err(cur.pos(), "expected closing ')' after '#('"));
        }
    }
    Ok(name)
}

fn parse_interp_primary(cur: &mut Cursor, start_pos: usize) -> Result<InterpExpr> {
    match cur.peek() {
        Some('#') => {
            let name = parse_hash_id(cur)?;
            Ok(InterpExpr {
                kind: InterpExprKind::Id(name),
                span: cur.span_from(start_pos),
            })
        }
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
        Some('(') => {
            cur.bump();
            let inner = parse_interp_expr(cur)?;
            skip_expr_ws(cur);
            if !cur.eat_str(")") {
                return Err(err(cur.pos(), "expected ')'"));
            }
            Ok(InterpExpr {
                kind: InterpExprKind::Group(Box::new(inner)),
                span: cur.span_from(start_pos),
            })
        }
        Some(c) if is_interp_ident_start(c) => {
            let name = eat_interp_ident(cur)?.to_string();
            if name == "addr" && cur.peek() == Some('(') {
                return parse_addr_literal(cur, start_pos);
            }
            // The same words the `.tmt` value syntax reads as values. They
            // are values, not keywords: nothing else in an expression is
            // reserved by its spelling.
            let kind = match name.as_str() {
                "true" => InterpExprKind::Literal(Literal::Bool(true)),
                "false" => InterpExprKind::Literal(Literal::Bool(false)),
                "null" => InterpExprKind::Literal(Literal::Null),
                _ => InterpExprKind::Identifier(name),
            };
            Ok(InterpExpr {
                kind,
                span: cur.span_from(start_pos),
            })
        }
        _ => Err(err(
            cur.pos(),
            "expected a value, identifier, or call in '${...}'",
        )),
    }
}

fn parse_interp_call_args(cur: &mut Cursor) -> Result<Vec<InterpExpr>> {
    cur.eat_str("(");
    skip_expr_ws(cur);
    let mut args = Vec::new();
    if cur.peek() != Some(')') {
        loop {
            args.push(parse_interp_expr(cur)?);
            skip_expr_ws(cur);
            match cur.peek() {
                Some(',') => {
                    cur.bump();
                    skip_expr_ws(cur);
                }
                Some(')') => break,
                _ => return Err(err(cur.pos(), "expected ',' or ')' in call arguments")),
            }
        }
    }
    if !cur.eat_str(")") {
        return Err(err(cur.pos(), "expected ')'"));
    }
    Ok(args)
}

fn parse_interp_number(cur: &mut Cursor, start: usize) -> Result<InterpExpr> {
    if cur.peek() == Some('-') {
        cur.bump();
    }
    let digits = cur.eat_while(|c| c.is_ascii_digit());
    if digits.is_empty() {
        return Err(err(cur.pos(), "expected digits"));
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
            .map_err(|_| err(start, "invalid float literal"))?
    } else {
        text.parse::<i64>()
            .map(Literal::Int)
            .map_err(|_| err(start, "invalid integer literal"))?
    };
    Ok(InterpExpr {
        kind: InterpExprKind::Literal(literal),
        span: cur.span_from(start),
    })
}

fn eat_interp_ident<'a>(cur: &mut Cursor<'a>) -> Result<&'a str> {
    let start = cur.pos();
    if !cur.peek().is_some_and(is_interp_ident_start) {
        return Err(err(start, "expected an identifier"));
    }
    Ok(cur.eat_while(is_interp_ident_char))
}

/// Whitespace inside an expression, line breaks included. A chain of calls
/// reads best with one call to a line, so `${...}` and a call's arguments may
/// run over several lines. (Everywhere else in a `.tmt` value a line break
/// ends the thing being read, which is why this is not `skip_inline_ws`.)
fn skip_expr_ws(cur: &mut Cursor) {
    while matches!(cur.peek(), Some(' ' | '\t' | '\n' | '\r')) {
        cur.bump();
    }
}
