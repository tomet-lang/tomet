//! Type seat decoding (`SeatSpec`, `SeatType`, `EnumVariant`) for `.blueprint.ctmt` schemas.
//!
//! Translates AST [`TypeSeat`] expressions into structured schema specifications:
//! built-in [`tomet_type::Type`]s, namespaced constructors (`namespace.name`),
//! and `enum(...)` unions, while separating blueprint-level `optional: true`
//! from pure base-type evaluation.

use std::collections::HashMap;
use std::fmt;

use tomet_ast::{InterpExpr, InterpExprKind, TypeSeat};
use tomet_expr::{DocRef, EvalContext, EvalValue, NoResolver, evaluate_type};
use tomet_scalar::Scalar;
use tomet_semantics_core::{admits, admits_default, datum_value};
use tomet_span::Span;
use tomet_spelling::scalar_spelling;
use tomet_type::{Datum, Kind, Type};

/// A decoded type seat specification in a `.blueprint.ctmt` document.
#[derive(Debug, Clone, PartialEq)]
pub struct SeatSpec {
    pub ty: SeatType,
    pub optional: bool,
    pub default: Option<Datum>,
    pub span: Span,
}

/// The value constraint expressed by a [`SeatSpec`].
#[derive(Debug, Clone, PartialEq)]
pub enum SeatType {
    /// A built-in [`tomet_type::Type`] (`string`, `int`, `list(...)`, `uuid(...)`, etc.).
    Builtin(Type),
    /// A namespaced constructor call (`catasto.git`, etc.).
    Constructor { namespace: String, name: String },
    /// An enumeration of allowed variants (`enum("follow", "copy", catasto.git, ...)`).
    Enum(Vec<EnumVariant>),
}

/// A single allowed alternative inside `enum(...)`.
#[derive(Debug, Clone, PartialEq)]
pub enum EnumVariant {
    /// A literal scalar value (`"follow"`, `42`, `true`, `none`, `auto`).
    Literal(Scalar),
    /// A namespaced constructor call (`catasto.git`).
    Constructor { namespace: String, name: String },
    /// A built-in type (`string`, `int`, `list(...)`, etc.).
    Type(Type),
}

impl fmt::Display for EnumVariant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EnumVariant::Literal(s) => write!(f, "`{}`", scalar_spelling(s)),
            EnumVariant::Constructor { namespace, name } => write!(f, "`{namespace}.{name}(...)`"),
            EnumVariant::Type(ty) => write!(f, "`{ty}`"),
        }
    }
}

impl fmt::Display for SeatType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SeatType::Builtin(ty) => write!(f, "`{ty}`"),
            SeatType::Constructor { namespace, name } => write!(f, "`{namespace}.{name}(...)`"),
            SeatType::Enum(variants) => {
                write!(f, "one of ")?;
                for (i, v) in variants.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{v}")?;
                }
                Ok(())
            }
        }
    }
}

impl SeatSpec {
    /// Decodes a [`TypeSeat`] AST node into a [`SeatSpec`].
    pub fn from_type_seat(seat: &TypeSeat) -> Result<Self, String> {
        let span = seat.span;
        match &seat.expr.kind {
            InterpExprKind::Identifier(name) => {
                if name == "enum" {
                    return Err("`enum` requires at least one variant".to_string());
                }
                let ty = eval_builtin_type(&seat.expr)?;
                Ok(SeatSpec {
                    ty: SeatType::Builtin(ty),
                    optional: false,
                    default: None,
                    span,
                })
            }
            InterpExprKind::Member { object, member } => {
                let ns = single_namespace(object)?;
                Ok(SeatSpec {
                    ty: SeatType::Constructor {
                        namespace: ns,
                        name: member.clone(),
                    },
                    optional: false,
                    default: None,
                    span,
                })
            }
            InterpExprKind::Call { callee, args } => {
                let (optional, remaining) = extract_optional_arg(args)?;
                match &callee.kind {
                    InterpExprKind::Identifier(name) if name == "enum" => {
                        decode_enum_seat(remaining, optional, span)
                    }
                    InterpExprKind::Identifier(_) => {
                        let stripped = if remaining.is_empty() {
                            callee.as_ref().clone()
                        } else {
                            InterpExpr {
                                kind: InterpExprKind::Call {
                                    callee: callee.clone(),
                                    args: remaining,
                                },
                                span: seat.expr.span,
                            }
                        };
                        let ty = eval_builtin_type(&stripped)?;
                        let default = ty.default().cloned();
                        Ok(SeatSpec {
                            ty: SeatType::Builtin(ty),
                            optional,
                            default,
                            span,
                        })
                    }
                    InterpExprKind::Member { object, member } => {
                        let ns = single_namespace(object)?;
                        let default = extract_constructor_default(&remaining, &format!("{ns}.{member}"))?;
                        Ok(SeatSpec {
                            ty: SeatType::Constructor {
                                namespace: ns,
                                name: member.clone(),
                            },
                            optional,
                            default,
                            span,
                        })
                    }
                    _ => Err(format!("`{}` is not a valid type seat", seat.expr)),
                }
            }
            _ => Err(format!("`{}` is not a valid type seat", seat.expr)),
        }
    }
}

fn single_namespace(object: &InterpExpr) -> Result<String, String> {
    match &object.kind {
        InterpExprKind::Identifier(ns) => Ok(ns.clone()),
        _ => Err(format!(
            "constructor reference `{object}` must have the form `namespace.name`"
        )),
    }
}

fn eval_builtin_type(expr: &InterpExpr) -> Result<Type, String> {
    let cx = EvalContext {
        resolver: &NoResolver,
        self_doc: DocRef(String::new()),
        macros: &HashMap::new(),
        var: &EvalValue::Scalar(Scalar::None),
        var_unset: &[],
        includes: &[],
        heading_level: 0,
        notes: Default::default(),
    };
    let ty = evaluate_type(expr, &cx).map_err(|e| e.to_string())?;
    if let Err(mismatch) = admits_default(&ty) {
        return Err(format!("`default`: {mismatch}"));
    }
    Ok(ty)
}

fn extract_optional_arg(args: &[InterpExpr]) -> Result<(bool, Vec<InterpExpr>), String> {
    let mut optional: Option<bool> = None;
    let mut remaining = Vec::with_capacity(args.len());
    for arg in args {
        if let InterpExprKind::NamedArg { name, value } = &arg.kind
            && name == "optional"
        {
            if optional.is_some() {
                return Err("duplicate `optional` argument".to_string());
            }
            match &value.kind {
                InterpExprKind::Literal(Scalar::Bool(b)) => optional = Some(*b),
                _ => return Err("`optional` must be a boolean (`true` or `false`)".to_string()),
            }
        } else {
            remaining.push(arg.clone());
        }
    }
    Ok((optional.unwrap_or(false), remaining))
}

fn extract_constructor_default(args: &[InterpExpr], head: &str) -> Result<Option<Datum>, String> {
    let mut default: Option<Datum> = None;
    for arg in args {
        match &arg.kind {
            InterpExprKind::NamedArg { name, value } if name == "default" => {
                if default.is_some() {
                    return Err(format!("`{head}`: duplicate `default` argument"));
                }
                default = Some(expr_to_datum(value)?);
            }
            InterpExprKind::NamedArg { name, .. } => {
                return Err(format!("`{head}` does not take blueprint argument `{name}`"));
            }
            _ => {
                return Err(format!(
                    "`{head}` does not take positional blueprint arguments"
                ));
            }
        }
    }
    Ok(default)
}

fn decode_enum_seat(
    args: Vec<InterpExpr>,
    optional: bool,
    span: Span,
) -> Result<SeatSpec, String> {
    let mut variants = Vec::new();
    let mut default: Option<Datum> = None;
    for arg in &args {
        match &arg.kind {
            InterpExprKind::NamedArg { name, value } if name == "default" => {
                if default.is_some() {
                    return Err("`enum`: duplicate `default` argument".to_string());
                }
                default = Some(expr_to_datum(value)?);
            }
            InterpExprKind::NamedArg { name, .. } => {
                return Err(format!("`enum` does not take argument `{name}`"));
            }
            _ => {
                variants.push(decode_enum_variant(arg)?);
            }
        }
    }
    if variants.is_empty() {
        return Err("`enum` requires at least one variant".to_string());
    }
    if let Some(d) = &default {
        let val = datum_value(d);
        let fits = variants.iter().any(|v| match v {
            EnumVariant::Literal(s) => matches!(&val, tomet_ast::Value::Scalar(vs) if vs == s),
            EnumVariant::Type(ty) => admits(ty, &val).is_ok(),
            EnumVariant::Constructor { .. } => false,
        });
        if !fits {
            return Err(format!(
                "`default` value does not match any variant of {}",
                SeatType::Enum(variants)
            ));
        }
    }
    Ok(SeatSpec {
        ty: SeatType::Enum(variants),
        optional,
        default,
        span,
    })
}

fn decode_enum_variant(expr: &InterpExpr) -> Result<EnumVariant, String> {
    match &expr.kind {
        InterpExprKind::Literal(scalar) => Ok(EnumVariant::Literal(scalar.clone())),
        InterpExprKind::Member { object, member } => {
            let ns = single_namespace(object)?;
            Ok(EnumVariant::Constructor {
                namespace: ns,
                name: member.clone(),
            })
        }
        InterpExprKind::Identifier(name) => {
            if let Some(kind) = Kind::from_name(name) {
                Ok(EnumVariant::Type(Type::bare(kind)))
            } else {
                Err(format!(
                    "unknown enum variant `{name}` (quote string literals like `\"{name}\"`)"
                ))
            }
        }
        InterpExprKind::Call { .. } => {
            let ty = eval_builtin_type(expr)?;
            Ok(EnumVariant::Type(ty))
        }
        _ => Err(format!("unsupported `enum` variant `{expr}`")),
    }
}

fn expr_to_datum(expr: &InterpExpr) -> Result<Datum, String> {
    match &expr.kind {
        InterpExprKind::Literal(s) => Ok(Datum::Scalar(s.clone())),
        InterpExprKind::Group(inner) => expr_to_datum(inner),
        InterpExprKind::Seq(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(expr_to_datum(item)?);
            }
            Ok(Datum::Seq(out))
        }
        _ => Err(format!("`{expr}` is not a literal default value")),
    }
}
