use crate::error::Result;
use tomet_ast::{
    BinaryOp, InterpExpr, InterpExprKind, InterpSpelling, Interpolation, Literal, TypeSeat,
};
use tomet_cst::SyntaxNode;
use tomet_lexer::Cursor;
use tomet_span::Span;

/// First character of an interpolation expression identifier. A name is ASCII.
pub fn is_interp_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

/// Subsequent characters of an interpolation expression identifier.
pub fn is_interp_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// Reads a `"..."` string, with `\"`, `\\`, `\n`, `\t` escapes.
pub fn parse_quoted(cur: &mut Cursor) -> Result<String> {
    let start = cur.pos();
    if !cur.eat_str("\"") {
        return Err(err(start, "expected '\"'"));
    }
    let mut out = String::new();
    loop {
        match cur.bump() {
            None => return Err(err(start, "unterminated string")),
            Some('"') => break,
            Some('\\') => match cur.bump() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(other) => out.push(other),
                None => return Err(err(start, "unterminated string")),
            },
            Some(c) => out.push(c),
        }
    }
    Ok(out)
}

fn err(pos: usize, message: impl Into<String>) -> crate::error::Error {
    crate::error::Error {
        message: message.into(),
        line: 0,
        column: 0,
        offset: pos,
    }
}

/// Lowers an `INLINE_FORM` CST node representing an interpolation into an AST [`Interpolation`].
pub(crate) fn lower_interpolation(node: &SyntaxNode) -> Result<Interpolation> {
    let text = node.text().to_string();
    let base = usize::from(node.text_range().start());
    let mut cur = Cursor::new(&text);
    let mut interp = parse_interpolation(&mut cur).map_err(|e| err(base + e.offset, e.message))?;
    skip_expr_ws(&mut cur);
    if !cur.is_eof() {
        return Err(err(base + cur.pos(), "unexpected content in interpolation"));
    }
    offset_interpolation(&mut interp, base);
    Ok(interp)
}

/// Lowers the `SLOT` node of a type seat, `<$type>` or `<$type(args)>`, into a [`TypeSeat`].
///
/// The text between the `<$` and the `>` is read as the expression it is, by the expression
/// parser the interpolations use: types and expressions are one language. The seat holds that
/// expression as written and no more. Which names are types and what their arguments mean is not
/// asked here.
pub(crate) fn lower_type_seat(slot: &SyntaxNode) -> Result<TypeSeat> {
    const WHAT: &str = "a type seat holds a type: a name or a call of one, \
                        as in `<$uuid>` and `<$list(string, length: 6)>`";
    let text = slot.text().to_string();
    let base = usize::from(slot.text_range().start());
    let Some(written) = text.strip_prefix("<$").and_then(|t| t.strip_suffix('>')) else {
        return Err(err(base, format!("this type seat does not close: {WHAT}")));
    };
    let inner = base + "<$".len();
    let mut cur = Cursor::new(written);
    let mut expr = parse_interp_expr(&mut cur).map_err(|e| err(inner + e.offset, e.message))?;
    skip_expr_ws(&mut cur);
    if !cur.is_eof() {
        return Err(err(
            inner + cur.pos(),
            format!("unexpected content: {WHAT}"),
        ));
    }
    let is_type = match &expr.kind {
        InterpExprKind::Identifier(_) => true,
        InterpExprKind::Call { callee, .. } => {
            matches!(callee.kind, InterpExprKind::Identifier(_))
        }
        _ => false,
    };
    if !is_type {
        return Err(err(base, WHAT));
    }
    offset_expr(&mut expr, inner);
    Ok(TypeSeat {
        expr,
        span: Span::new(base, base + text.len()),
    })
}

fn offset_interpolation(interp: &mut Interpolation, base: usize) {
    interp.span = Span::new(base + interp.span.start, base + interp.span.end);
    offset_expr(&mut interp.expr, base);
}

fn offset_expr(expr: &mut InterpExpr, base: usize) {
    expr.span = Span::new(base + expr.span.start, base + expr.span.end);
    match &mut expr.kind {
        InterpExprKind::Identifier(_)
        | InterpExprKind::Id(_)
        | InterpExprKind::Literal(_)
        | InterpExprKind::Addr { .. } => {}
        InterpExprKind::Member { object, .. } | InterpExprKind::Connect { object, .. } => {
            offset_expr(object, base)
        }
        InterpExprKind::DocId { doc, .. } => offset_expr(doc, base),
        InterpExprKind::Group(inner) => offset_expr(inner, base),
        InterpExprKind::Seq(items) => {
            for item in items {
                offset_expr(item, base);
            }
        }
        InterpExprKind::Call { callee, args } => {
            offset_expr(callee, base);
            for arg in args {
                offset_expr(arg, base);
            }
        }
        InterpExprKind::Binary { lhs, rhs, .. } => {
            offset_expr(lhs, base);
            offset_expr(rhs, base);
        }
        InterpExprKind::Lambda { body, .. } => offset_expr(body, base),
        InterpExprKind::NamedArg { value, .. } => offset_expr(value, base),
    }
}

/// One of the three spellings of an interpolation: `{$ expr }`, `$(ref)` or `$name(args)`.
///
/// The first takes the whole expression grammar. `$(...)` takes one reference
/// ([`InterpSpelling::Reference`]) and `$name(...)` one call, so that an operator in either is
/// refused with the gate that would take it, and not read as something else.
pub fn parse_interpolation(cur: &mut Cursor) -> Result<Interpolation> {
    let start_pos = cur.pos();
    let expr = if cur.eat_str("{$") {
        let expr = parse_gate(cur)?;
        skip_expr_ws(cur);
        if !cur.eat_str("}") {
            return Err(err(cur.pos(), "unterminated '{$', expected '}'"));
        }
        expr
    } else if cur.eat_str("$(") {
        let expr = parse_reference(cur, start_pos)?;
        skip_expr_ws(cur);
        if !cur.eat_str(")") {
            return Err(err(cur.pos(), "unterminated '$(', expected ')'"));
        }
        expr
    } else if cur.eat_str("$") {
        parse_call_form(cur, start_pos)?
    } else {
        return Err(err(start_pos, "expected '{$', '$(' or '$name('"));
    };
    Ok(Interpolation {
        expr,
        span: cur.span_from(start_pos),
    })
}

/// The expression inside `{$ ... }`, with the cursor after the `{$`.
fn parse_gate(cur: &mut Cursor) -> Result<InterpExpr> {
    skip_expr_ws(cur);
    if cur.peek() == Some('}') {
        return Err(err(
            cur.pos(),
            "empty interpolation, expected an expression",
        ));
    }
    parse_interp_expr(cur)
}

/// The reference inside `$( ... )`, with the cursor after the `$(`.
fn parse_reference(cur: &mut Cursor, start_pos: usize) -> Result<InterpExpr> {
    skip_expr_ws(cur);
    if cur.peek() == Some(')') {
        return Err(err(cur.pos(), "empty '$()', expected a reference"));
    }
    let expr = parse_interp_expr(cur)?;
    if expr.spelling() != InterpSpelling::Reference {
        return Err(err(
            start_pos,
            "'$(...)' holds one reference: a name, '#id', a '.member' or '::connect' chain, or a \
             place number. Write any other expression as '{$ ... }', and a call as '$name(...)'",
        ));
    }
    Ok(expr)
}

/// `name(args)` or `macro.name(args)`, with the cursor after the `$`.
fn parse_call_form(cur: &mut Cursor, start_pos: usize) -> Result<InterpExpr> {
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
    Ok(expr)
}

/// `Lambda | NamedArg | Or`, the whole of an expression.
///
/// Precedence, loosest first: `=>`, `||`, `&&`, comparison, additive (`+`, `-`), then the
/// postfix forms (`.name`, `::name`, `(args)`). The grammar has no keywords, so
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

    // Check if this is `name: expr` (NamedArg). `name::member` is a connect, not a name.
    let mut look = *cur;
    if look.peek().is_some_and(is_interp_ident_start)
        && let Ok(name) = eat_interp_ident(&mut look)
    {
        skip_expr_ws(&mut look);
        if look.peek() == Some(':') && !matches!(look.peek_at(1), Some('/' | ':')) {
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
    parse_interp_chain(cur, BinaryOp::Or, parse_interp_and)
}

/// `Comparison ('&&' Comparison)*`
fn parse_interp_and(cur: &mut Cursor) -> Result<InterpExpr> {
    parse_interp_chain(cur, BinaryOp::And, parse_interp_comparison)
}

/// `operand (op operand)*` for the logical operator `op`. Its first character alone is the mistake
/// for the whole symbol (`|` for `||`) and says so.
fn parse_interp_chain(
    cur: &mut Cursor,
    op: BinaryOp,
    operand: fn(&mut Cursor) -> Result<InterpExpr>,
) -> Result<InterpExpr> {
    skip_expr_ws(cur);
    let start_pos = cur.pos();
    let mut lhs = operand(cur)?;
    loop {
        let mut look = *cur;
        skip_expr_ws(&mut look);
        if let Some(found) = operator_at(&look, std::iter::once(op)) {
            bump_symbol(&mut look, found);
            *cur = look;
            let rhs = operand(cur)?;
            lhs = binary(op, lhs, rhs, cur, start_pos);
        } else if look.peek() == op.symbol().chars().next() {
            return Err(err(
                look.pos(),
                format!(
                    "unexpected '{}', did you mean '{}'?",
                    &op.symbol()[..1],
                    op.symbol()
                ),
            ));
        } else {
            return Ok(lhs);
        }
    }
}

/// The operator of `ops` spelled at the cursor, the longest spelling when two are (`<=` over `<`).
fn operator_at(cur: &Cursor, ops: impl Iterator<Item = BinaryOp>) -> Option<BinaryOp> {
    ops.filter(|op| cur.starts_with(op.symbol()))
        .max_by_key(|op| op.symbol().len())
}

/// Moves the cursor past the symbol of `op`, which it stands at.
fn bump_symbol(cur: &mut Cursor, op: BinaryOp) {
    for _ in op.symbol().chars() {
        cur.bump();
    }
}

/// The comparison operator at the cursor, if any, with the cursor moved
/// past it. `=>` is the lambda arrow and is not a comparison.
fn take_comparison(cur: &mut Cursor) -> Result<Option<BinaryOp>> {
    let mut look = *cur;
    skip_expr_ws(&mut look);
    let Some(op) = operator_at(
        &look,
        BinaryOp::ALL.into_iter().filter(|op| op.is_comparison()),
    ) else {
        return match (look.peek(), look.peek_at(1)) {
            (Some('='), Some('>')) => Ok(None),
            (Some('='), _) => Err(err(
                look.pos(),
                format!("unexpected '=', did you mean '{}'?", BinaryOp::Eq.symbol()),
            )),
            _ => Ok(None),
        };
    };
    bump_symbol(&mut look, op);
    *cur = look;
    Ok(Some(op))
}

/// `Additive (CompareOp Additive)?` -- comparisons do not chain.
fn parse_interp_comparison(cur: &mut Cursor) -> Result<InterpExpr> {
    skip_expr_ws(cur);
    let start_pos = cur.pos();
    let lhs = parse_interp_additive(cur)?;
    let Some(op) = take_comparison(cur)? else {
        return Ok(lhs);
    };
    let rhs = parse_interp_additive(cur)?;
    let mut look = *cur;
    if take_comparison(&mut look)?.is_some() {
        return Err(err(
            cur.pos(),
            "comparison operators cannot be chained, group one side with parentheses",
        ));
    }
    Ok(binary(op, lhs, rhs, cur, start_pos))
}

/// `Postfix (('+' | '-') Postfix)*`, left to right.
///
/// A `-` is also a letter of a name (`daily-note`), so the operator is told from it by white space:
/// `a - b` subtracts and `a-b` is one name. The same white space is asked of `+`, so that the two
/// are written alike. A `+` or `-` that stands after an operand without it is an error that says so;
/// reading `a -b` as `a` followed by `-b` would only fail later with a worse message.
fn parse_interp_additive(cur: &mut Cursor) -> Result<InterpExpr> {
    skip_expr_ws(cur);
    let start_pos = cur.pos();
    let mut lhs = parse_interp_postfix(cur)?;
    loop {
        let mut look = *cur;
        skip_expr_ws(&mut look);
        let op = match look.peek() {
            Some('+') => BinaryOp::Add,
            Some('-') => BinaryOp::Sub,
            _ => return Ok(lhs),
        };
        let space_before = look.pos() > cur.pos();
        let space_after = look
            .peek_at(1)
            .is_none_or(|c| matches!(c, ' ' | '\t' | '\n' | '\r'));
        if !(space_before && space_after) {
            return Err(err(
                look.pos(),
                format!(
                    "'{0}' is an operator only with white space on both sides: write 'a {0} b'",
                    op.symbol()
                ),
            ));
        }
        bump_symbol(&mut look, op);
        *cur = look;
        let rhs = parse_interp_postfix(cur)?;
        lhs = binary(op, lhs, rhs, cur, start_pos);
    }
}

/// `primary (. Ident | :: Ident | ( Args ))*`
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
            Some(':') if look.peek_at(1) == Some(':') => {
                look.bump();
                look.bump();
                skip_expr_ws(&mut look);
                *cur = look;
                let member = eat_interp_ident(cur)?.to_string();
                expr = InterpExpr {
                    kind: InterpExprKind::Connect {
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
            skip_expr_ws(cur);
            if cur.eat_str(")") {
                return Ok(InterpExpr {
                    kind: InterpExprKind::Seq(Vec::new()),
                    span: cur.span_from(start_pos),
                });
            }
            let first = parse_interp_expr(cur)?;
            skip_expr_ws(cur);
            if cur.peek() == Some(',') {
                cur.bump();
                skip_expr_ws(cur);
                let mut items = vec![first];
                if cur.eat_str(")") {
                    return Ok(InterpExpr {
                        kind: InterpExprKind::Seq(items),
                        span: cur.span_from(start_pos),
                    });
                }
                loop {
                    items.push(parse_interp_expr(cur)?);
                    skip_expr_ws(cur);
                    match cur.peek() {
                        Some(',') => {
                            cur.bump();
                            skip_expr_ws(cur);
                            if cur.eat_str(")") {
                                break;
                            }
                        }
                        Some(')') => {
                            cur.bump();
                            break;
                        }
                        _ => return Err(err(cur.pos(), "expected ',' or ')'")),
                    }
                }
                Ok(InterpExpr {
                    kind: InterpExprKind::Seq(items),
                    span: cur.span_from(start_pos),
                })
            } else if cur.eat_str(")") {
                Ok(InterpExpr {
                    kind: InterpExprKind::Group(Box::new(first)),
                    span: cur.span_from(start_pos),
                })
            } else {
                Err(err(cur.pos(), "expected ')'"))
            }
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
            "expected a value, a name or a call in an expression",
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
/// reads best with one call to a line, so a `{$ ... }` and a call's arguments may
/// run over several lines. (Everywhere else in a `.tmt` value a line break
/// ends the thing being read, which is why this is not `skip_inline_ws`.)
fn skip_expr_ws(cur: &mut Cursor) {
    while matches!(cur.peek(), Some(' ' | '\t' | '\n' | '\r')) {
        cur.bump();
    }
}
