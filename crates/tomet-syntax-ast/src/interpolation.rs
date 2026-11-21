//! An interpolation ([`Interpolation`]) and the expression ([`InterpExpr`]) it holds, with the source spelling of each.

use serde::{Deserialize, Serialize};
use std::fmt;
use tomet_span::Span;

/// A `<$type(args)>` seat: the type of a value that is not there yet, written where the value
/// goes. `<$date(default: auto)>`, `<$list(string, length: 6)>`, `<$uuid>`.
///
/// It is a box, `<...>`, whose head is the `$` mark and the type, as the expression it is: a
/// bare name (`uuid`) or a call of one (`list(string, length: 6)`), with its arguments in `()`
/// and nowhere else. `<$uuid>` and `<$uuid()>` are two written forms, a name and a call with no
/// arguments, and stay two here. It takes no `{value}`, no `[content]`, no `#(id)` and no
/// `::connect`, so it holds the type and a span and nothing else: it is not a [`Hole`](crate::Hole), which
/// has a matcher and groups, and not an [`Interpolation`](crate::Interpolation), which is evaluated where it stands.
///
/// `tomet-ast` never interprets the expression: which type names are valid, what their arguments
/// mean, and that `auto` in `default: auto` is not a variable, are for `tomet-type` and the
/// semantics. To the AST `auto` is a name like any other.
///
/// A seat sits where a value, a block or running text does, so it is a variant of [`Block`](crate::Block),
/// [`Inline`](crate::Inline) and [`Value`](crate::Value), and not of [`Entry`](crate::Entry): a `{...}` group holds `key: value` entries
/// and elements, and a seat is neither.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypeSeat {
    pub expr: InterpExpr,
    pub span: Span,
}

/// Displays the seat as source: `<$`, the type as [`InterpExpr`] writes it, and `>`. The text
/// reads back as the same seat.
impl fmt::Display for TypeSeat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<${}>", self.expr)
    }
}

/// An interpolation: an expression to be evaluated where it stands. It is written in one of three
/// spellings, told apart by what the expression is ([`InterpSpelling`]): `{$ expr }` for any
/// expression, `$(ref)` for one reference, `$name(args)` for one call.
///
/// It is not an [`Element`](crate::Element): it has no name, no `(args)` / `[content]` and no
/// `#(id)`, and it never takes a `::connect`. It holds an expression and the
/// span of the whole `$`-form, and nothing else. It sits in three places, as
/// [`Block::Interp`](crate::Block::Interp), [`Inline::Interp`](crate::Inline::Interp) and [`Value::Interp`](crate::Value::Interp).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Interpolation {
    pub expr: InterpExpr,
    pub span: Span,
}

/// Displays the interpolation in its canonical spelling, the one [`InterpExpr::spelling`] says: the
/// source text that reads back as the same expression. The three forms are one AST, so a document
/// that spelled `{$ x }` where `$(x)` is canonical is printed as `$(x)`.
impl fmt::Display for Interpolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.expr.write_interpolation(f)
    }
}

/// The three ways an interpolation is written, each for one kind of expression. The spelling is not
/// kept in the AST: it follows from the expression, which is what makes the printed form canonical.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterpSpelling {
    /// `$name(args)`: one call whose callee is a plain name (`include`, `macro.gh`), or an `addr`.
    Call,
    /// `$(ref)`: one reference -- a name, `#id`, `doc#id`, or a `.member` / `::connect` chain on
    /// those -- or a place number, `$(1)`.
    Reference,
    /// `{$ expr }`: anything else, which is where operators, lambdas, literals and groups live.
    Gate,
}

/// One node of an interpolation's parsed expression tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InterpExpr {
    pub kind: InterpExprKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InterpExprKind {
    Identifier(String),
    Id(String),
    Literal(tomet_scalar::Scalar),
    Call {
        callee: Box<InterpExpr>,
        args: Vec<InterpExpr>,
    },
    /// `object.member`: a named entry of the data an element, a tuple or a section holds, or the start of a
    /// method call (`object.member(...)` is [`InterpExprKind::Call`] on this). A node's own
    /// attributes are not read with a dot; see [`InterpExprKind::Connect`].
    Member {
        object: Box<InterpExpr>,
        member: String,
    },
    /// `object::member`: from a node to what hangs off it -- an attribute of the node (`::id`,
    /// `::name`, a document's `::path`), the connect of that name (`::meta`), or the document or
    /// directory it leads to. The dot reads data inside a node; the double colon goes from node to node.
    Connect {
        object: Box<InterpExpr>,
        member: String,
    },
    NamedArg {
        name: String,
        value: Box<InterpExpr>,
    },
    /// `lhs op rhs`: `+`, `-`, a comparison, `&&` or `||`; see [`BinaryOp`] for the precedence.
    Binary {
        op: BinaryOp,
        lhs: Box<InterpExpr>,
        rhs: Box<InterpExpr>,
    },
    /// `doc#id` -- the element with that unique id inside `doc`, where `doc`
    /// is any expression that yields a document. The leading form `#id`
    /// (no document on the left) is [`InterpExprKind::Id`] and means the
    /// same thing with `self` on the left.
    DocId {
        doc: Box<InterpExpr>,
        id: String,
    },
    /// `addr(tmt:docs/a.tmt#intro)` or `addr("...")` -- an address written as
    /// text. `raw` records the spelling the author used, so printing gives
    /// it back: the raw form reads everything up to the matching `)` without
    /// treating `:`, `.`, `-` or `/` as expression symbols.
    Addr {
        text: String,
        raw: bool,
    },
    /// `( expr )`. Kept as a node, not dropped, so printing a parsed
    /// expression gives back the parentheses the author wrote.
    Group(Box<InterpExpr>),
    /// `()` (empty), `(a,)` (one item), or `(a, b, ...)` (two or more items). Kept as a sequence
    /// expression. `(a)` with no comma is [`InterpExprKind::Group`].
    Seq(Vec<InterpExpr>),
    /// `x => body` or `(a, b) => body`. The body is one expression.
    Lambda {
        params: Vec<String>,
        body: Box<InterpExpr>,
    },
}

/// A binary operator in an expression.
///
/// `+` and `-` are written with white space on both sides: `-` is also part of a name
/// (`daily-note`), so `a-b` is one name and `a - b` is a subtraction. The other arithmetic is
/// functions (`mul`, `div`, `mod`), and there is no `!` (`not`). `&&` and `||` are the only
/// operators that may skip their right-hand side, which is why they are symbols and not functions.
///
/// Precedence, tightest first: the postfix forms, additive, comparison, `&&`, `||`, then `=>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryOp {
    Add,
    Sub,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

impl BinaryOp {
    /// Every operator, so that a reader can look for the one spelled at a position.
    pub const ALL: [BinaryOp; 10] = [
        BinaryOp::Add,
        BinaryOp::Sub,
        BinaryOp::Eq,
        BinaryOp::Ne,
        BinaryOp::Lt,
        BinaryOp::Le,
        BinaryOp::Gt,
        BinaryOp::Ge,
        BinaryOp::And,
        BinaryOp::Or,
    ];

    /// The spelling in source. It is the only place an operator is spelled: the parser finds the
    /// operator at a position by these symbols, longest first (`<=` over `<`).
    pub fn symbol(self) -> &'static str {
        match self {
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Eq => "==",
            BinaryOp::Ne => "!=",
            BinaryOp::Lt => "<",
            BinaryOp::Le => "<=",
            BinaryOp::Gt => ">",
            BinaryOp::Ge => ">=",
            BinaryOp::And => "&&",
            BinaryOp::Or => "||",
        }
    }

    /// Whether this is `+` or `-`, which chain to the left and need white space around them.
    pub fn is_additive(self) -> bool {
        matches!(self, BinaryOp::Add | BinaryOp::Sub)
    }

    /// Whether this is one of the six comparisons, which do not chain.
    pub fn is_comparison(self) -> bool {
        matches!(
            self,
            BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge
        )
    }
}

impl InterpExpr {
    /// The interpolation of this expression as source, in the spelling [`InterpExpr::spelling`]
    /// says. For when there is only the expression and no [`Interpolation`] around it.
    pub fn interpolation_source(&self) -> String {
        struct Source<'a>(&'a InterpExpr);
        impl fmt::Display for Source<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.write_interpolation(f)
            }
        }
        Source(self).to_string()
    }

    fn write_interpolation(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.spelling() {
            InterpSpelling::Call => write!(f, "${self}"),
            InterpSpelling::Reference => write!(f, "$({self})"),
            InterpSpelling::Gate => write!(f, "{{$ {self} }}"),
        }
    }

    /// How an interpolation of this expression is written; see [`InterpSpelling`].
    pub fn spelling(&self) -> InterpSpelling {
        match &self.kind {
            InterpExprKind::Addr { .. } => InterpSpelling::Call,
            InterpExprKind::Call { callee, .. } if callee.is_name_chain() => InterpSpelling::Call,
            InterpExprKind::Literal(tomet_scalar::Scalar::Int(n)) if *n >= 0 => {
                InterpSpelling::Reference
            }
            _ if self.is_reference() => InterpSpelling::Reference,
            _ => InterpSpelling::Gate,
        }
    }

    /// A name, or `.member`s of one: what `$name(...)` can take as its callee.
    pub fn is_name_chain(&self) -> bool {
        match &self.kind {
            InterpExprKind::Identifier(_) => true,
            InterpExprKind::Member { object, .. } => object.is_name_chain(),
            _ => false,
        }
    }

    /// A name, `#id`, or a `.member`, `::connect` or `#id` on one: what `$(...)` can hold.
    fn is_reference(&self) -> bool {
        match &self.kind {
            InterpExprKind::Identifier(_) | InterpExprKind::Id(_) => true,
            InterpExprKind::Member { object, .. } | InterpExprKind::Connect { object, .. } => {
                object.is_reference()
            }
            InterpExprKind::DocId { doc, .. } => doc.is_reference(),
            _ => false,
        }
    }
}

/// The source spelling of an interpolation expression, without the `$`-form around it. To print
/// an interpolation, display the [`Interpolation`], which adds the form its expression calls for
/// (`$name(args)` writes a call with no gate at all).
///
/// This belongs to the node rather than to any consumer. It was written
/// five times -- once in the printer and once in each of the four convert
/// crates -- and they disagreed: the converters escaped a string literal
/// with Rust's `{:?}` and the printer wrapped it in bare quotes, so a
/// string containing `"` printed back as source that will not parse.
///
/// A string literal is written by [`tomet_spelling::quoted_text`], which is what the parser reads
/// back. Not `{:?}`, which also emits `\u{...}` escapes the parser does not know.
impl fmt::Display for InterpExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            InterpExprKind::Identifier(name) => f.write_str(name),
            InterpExprKind::Id(name) => write!(f, "#{name}"),
            InterpExprKind::Literal(scalar) => match scalar {
                tomet_scalar::Scalar::Int(i) => write!(f, "{i}"),
                tomet_scalar::Scalar::Float(x) => write!(f, "{x}"),
                tomet_scalar::Scalar::Number(s) => write!(f, "{s}"),
                tomet_scalar::Scalar::Text(s) => f.write_str(&tomet_spelling::quoted_text(s)),
                tomet_scalar::Scalar::Bool(b) => write!(f, "{b}"),
                tomet_scalar::Scalar::None => f.write_str("none"),
                tomet_scalar::Scalar::Auto => f.write_str("auto"),
            },
            InterpExprKind::Call { callee, args } => {
                write!(f, "{callee}(")?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{arg}")?;
                }
                f.write_str(")")
            }
            InterpExprKind::Member { object, member } => write!(f, "{object}.{member}"),
            InterpExprKind::Connect { object, member } => write!(f, "{object}::{member}"),
            InterpExprKind::NamedArg { name, value } => write!(f, "{name}: {value}"),
            InterpExprKind::Binary { op, lhs, rhs } => {
                write!(f, "{lhs} {} {rhs}", op.symbol())
            }
            InterpExprKind::DocId { doc, id } => write!(f, "{doc}#{id}"),
            InterpExprKind::Addr { text, raw } => {
                if *raw && addr_text_is_safe_raw(text) {
                    write!(f, "addr({text})")
                } else {
                    f.write_str("addr(")?;
                    f.write_str(&tomet_spelling::quoted_text(text))?;
                    f.write_str(")")
                }
            }
            InterpExprKind::Group(inner) => write!(f, "({inner})"),
            InterpExprKind::Seq(items) => match items.as_slice() {
                [] => f.write_str("()"),
                [only] => write!(f, "({only},)"),
                items => {
                    f.write_str("(")?;
                    for (i, item) in items.iter().enumerate() {
                        if i > 0 {
                            f.write_str(", ")?;
                        }
                        write!(f, "{item}")?;
                    }
                    f.write_str(")")
                }
            },
            InterpExprKind::Lambda { params, body } => {
                match params.as_slice() {
                    [only] => f.write_str(only)?,
                    many => write!(f, "({})", many.join(", "))?,
                }
                write!(f, " => {body}")
            }
        }
    }
}

/// Whether `addr(` can be written back in its raw form: the text must read
/// back as the same text, which means no line break, balanced parentheses,
/// no surrounding space and no leading quote.
fn addr_text_is_safe_raw(text: &str) -> bool {
    if text.is_empty() || text.trim() != text || text.starts_with('"') || text.contains('\n') {
        return false;
    }
    let mut depth = 0usize;
    for c in text.chars() {
        match c {
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    return false;
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    depth == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name_expr(name: &str) -> InterpExpr {
        InterpExpr {
            kind: InterpExprKind::Identifier(name.to_string()),
            span: Span::dummy(),
        }
    }

    fn int_expr(n: i64) -> InterpExpr {
        InterpExpr {
            kind: InterpExprKind::Literal(tomet_scalar::Scalar::Int(n)),
            span: Span::dummy(),
        }
    }

    fn call_expr(name: &str, args: Vec<InterpExpr>) -> InterpExpr {
        InterpExpr {
            kind: InterpExprKind::Call {
                callee: Box::new(name_expr(name)),
                args,
            },
            span: Span::dummy(),
        }
    }

    fn seat(expr: InterpExpr) -> TypeSeat {
        TypeSeat {
            expr,
            span: Span::dummy(),
        }
    }

    #[test]
    fn a_type_seat_displays_as_the_source_it_reads_back_from() {
        // A bare name and a call with no arguments are two written forms, and stay two.
        assert_eq!(seat(name_expr("uuid")).to_string(), "<$uuid>");
        assert_eq!(seat(call_expr("uuid", vec![])).to_string(), "<$uuid()>");

        let named = |name: &str, value: InterpExpr| InterpExpr {
            kind: InterpExprKind::NamedArg {
                name: name.to_string(),
                value: Box::new(value),
            },
            span: Span::dummy(),
        };
        let list = call_expr(
            "list",
            vec![name_expr("string"), named("length", int_expr(6))],
        );
        assert_eq!(seat(list).to_string(), "<$list(string, length: 6)>");
        let date = call_expr("date", vec![named("default", name_expr("auto"))]);
        assert_eq!(seat(date).to_string(), "<$date(default: auto)>");
    }
}
