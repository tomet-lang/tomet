//! Type seat decoding (`SeatSpec`, `SeatType`, `EnumVariant`) for `.blueprint.ctmt` schemas.
//!
//! Translates AST [`TypeSeat`] expressions into structured schema specifications:
//! built-in [`tomet_type::Type`]s, namespaced constructors (`namespace.name`),
//! `enum(...)` unions (including nested `enum(...)` and tuple `Seq` variants),
//! and tuple/map schemas, while separating blueprint-level `optional: true`
//! from pure base-type evaluation.

use std::collections::HashMap;
use std::fmt;

use tomet_ast::{InterpExpr, InterpExprKind, TypeSeat};
use tomet_expr::{DocRef, EvalContext, EvalValue, NoResolver, evaluate_type};
use tomet_scalar::Scalar;
use tomet_semantics::Bindings;
use tomet_semantics_core::{admits, admits_default, datum_value};
use tomet_span::Span;
use tomet_spelling::scalar_spelling;
use tomet_type::{Datum, Kind, Type};

use crate::ctmt::{
    MapEntrySchema, MapSchema, TupleSchema, ValueSchema, value_matches_tuple,
    value_schema_is_optional,
};

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
    /// A positional and/or named tuple schema (`(catasto.git, executable: bool(optional: true))`).
    Tuple(Box<TupleSchema>),
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
    /// A nested `enum(...)` union.
    Enum(Vec<EnumVariant>),
    /// A positional and/or named tuple (`(enum(...), executable: bool(optional: true))`).
    Tuple(Box<TupleSchema>),
}

impl fmt::Display for EnumVariant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EnumVariant::Literal(s) => write!(f, "`{}`", scalar_spelling(s)),
            EnumVariant::Constructor { namespace, name } => write!(f, "`{namespace}.{name}(...)`"),
            EnumVariant::Type(ty) => write!(f, "`{ty}`"),
            EnumVariant::Enum(variants) => {
                write!(f, "enum(")?;
                for (i, v) in variants.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{v}")?;
                }
                write!(f, ")")
            }
            EnumVariant::Tuple(tuple) => write!(f, "{tuple}"),
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
            SeatType::Tuple(tuple) => write!(f, "{tuple}"),
        }
    }
}

impl SeatSpec {
    /// Decodes a [`TypeSeat`] AST node into a [`SeatSpec`].
    pub fn from_type_seat(seat: &TypeSeat) -> Result<Self, String> {
        Self::from_interp_expr(&seat.expr, seat.span)
    }

    /// Decodes an [`InterpExpr`] in seat position into a [`SeatSpec`].
    pub(crate) fn from_interp_expr(expr: &InterpExpr, span: Span) -> Result<Self, String> {
        match &expr.kind {
            InterpExprKind::Identifier(name) => {
                if name == "enum" {
                    return Err("`enum` requires at least one variant".to_string());
                }
                let ty = eval_builtin_type(expr)?;
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
                        decode_enum_seat(&remaining, optional, span)
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
                                span: expr.span,
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
                        let default =
                            extract_constructor_default(&remaining, &format!("{ns}.{member}"))?;
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
                    _ => Err(format!("`{expr}` is not a valid type seat")),
                }
            }
            InterpExprKind::Seq(items) => {
                let tuple = decode_interp_tuple(items)?;
                Ok(SeatSpec {
                    ty: SeatType::Tuple(Box::new(tuple)),
                    optional: false,
                    default: None,
                    span,
                })
            }
            InterpExprKind::Group(inner) => {
                if matches!(&inner.kind, InterpExprKind::NamedArg { .. }) {
                    let tuple = decode_interp_tuple(std::slice::from_ref(inner.as_ref()))?;
                    Ok(SeatSpec {
                        ty: SeatType::Tuple(Box::new(tuple)),
                        optional: false,
                        default: None,
                        span,
                    })
                } else {
                    Self::from_interp_expr(inner, span)
                }
            }
            _ => Err(format!("`{expr}` is not a valid type seat")),
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
    args: &[InterpExpr],
    optional: bool,
    span: Span,
) -> Result<SeatSpec, String> {
    let (variants, default) = decode_enum_variants(args, true)?;
    if let Some(d) = &default {
        let val = datum_value(d);
        let fits = variants.iter().any(|v| datum_fits_variant(&val, v));
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

fn decode_enum_variants(
    args: &[InterpExpr],
    allow_default: bool,
) -> Result<(Vec<EnumVariant>, Option<Datum>), String> {
    let mut variants = Vec::new();
    let mut default: Option<Datum> = None;
    for arg in args {
        match &arg.kind {
            InterpExprKind::NamedArg { name, value } if allow_default && name == "default" => {
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
    Ok((variants, default))
}

fn datum_fits_variant(val: &tomet_ast::Value, variant: &EnumVariant) -> bool {
    match variant {
        EnumVariant::Literal(s) => matches!(val, tomet_ast::Value::Scalar(vs) if vs == s),
        EnumVariant::Type(ty) => admits(ty, val).is_ok(),
        EnumVariant::Constructor { .. } => false,
        EnumVariant::Enum(nested) => nested.iter().any(|v| datum_fits_variant(val, v)),
        EnumVariant::Tuple(tuple) => {
            value_matches_tuple(val, tuple, "", "", Span::dummy(), &Bindings::default())
        }
    }
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
            if name == "enum" {
                Err("`enum` requires at least one variant".to_string())
            } else if let Some(kind) = Kind::from_name(name) {
                Ok(EnumVariant::Type(Type::bare(kind)))
            } else {
                Err(format!(
                    "unknown enum variant `{name}` (quote string literals like `\"{name}\"`)"
                ))
            }
        }
        InterpExprKind::Call { callee, args } => {
            if matches!(&callee.kind, InterpExprKind::Identifier(name) if name == "enum") {
                let (variants, _) = decode_enum_variants(args, false)?;
                Ok(EnumVariant::Enum(variants))
            } else {
                let ty = eval_builtin_type(expr)?;
                Ok(EnumVariant::Type(ty))
            }
        }
        InterpExprKind::Seq(items) => {
            Ok(EnumVariant::Tuple(Box::new(decode_interp_tuple(items)?)))
        }
        InterpExprKind::Group(inner) => {
            if matches!(&inner.kind, InterpExprKind::NamedArg { .. }) {
                Ok(EnumVariant::Tuple(Box::new(decode_interp_tuple(
                    std::slice::from_ref(inner.as_ref()),
                )?)))
            } else {
                decode_enum_variant(inner)
            }
        }
        _ => Err(format!("unsupported `enum` variant `{expr}`")),
    }
}

fn decode_interp_tuple(items: &[InterpExpr]) -> Result<TupleSchema, String> {
    let mut positional = Vec::new();
    let mut fixed: Vec<(String, MapEntrySchema)> = Vec::new();

    for item in items {
        match &item.kind {
            InterpExprKind::NamedArg { name, value } => {
                if fixed.iter().any(|(k, _)| k == name) {
                    return Err(format!("duplicate key `{name}` in tuple"));
                }
                let schema = decode_interp_value_schema(value)?;
                let optional = value_schema_is_optional(&schema);
                fixed.push((
                    name.clone(),
                    MapEntrySchema {
                        schema,
                        optional,
                        span: item.span,
                    },
                ));
            }
            _ => {
                positional.push(decode_interp_value_schema(item)?);
            }
        }
    }

    Ok(TupleSchema {
        positional,
        named: MapSchema {
            fixed,
            dynamic: None,
        },
    })
}

fn decode_interp_value_schema(expr: &InterpExpr) -> Result<ValueSchema, String> {
    match &expr.kind {
        InterpExprKind::Literal(s) => Ok(ValueSchema::Literal(s.clone())),
        InterpExprKind::Seq(items) => {
            Ok(ValueSchema::Tuple(Box::new(decode_interp_tuple(items)?)))
        }
        InterpExprKind::Group(inner) => {
            if matches!(&inner.kind, InterpExprKind::NamedArg { .. }) {
                Ok(ValueSchema::Tuple(Box::new(decode_interp_tuple(
                    std::slice::from_ref(inner.as_ref()),
                )?)))
            } else {
                decode_interp_value_schema(inner)
            }
        }
        InterpExprKind::Identifier(_)
        | InterpExprKind::Member { .. }
        | InterpExprKind::Call { .. } => {
            SeatSpec::from_interp_expr(expr, expr.span).map(ValueSchema::Seat)
        }
        _ => Err(format!("unsupported item `{expr}` in blueprint tuple")),
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

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_ast::Value;

    fn decode_seat(expr_src: &str) -> Result<SeatSpec, String> {
        let src = format!("::kind(blueprint)\n::blueprint(test)\n\nv: <${expr_src}>\n");
        let file = tomet_ast_parser::parse_data_file(&src).expect("valid .ctmt");
        let Value::Tuple(body) = file.body else {
            panic!("expected tuple body");
        };
        let Value::TypeSeat(seat) = body.get("v").expect("v entry") else {
            panic!("expected type seat");
        };
        SeatSpec::from_type_seat(seat)
    }

    #[test]
    fn decodes_nested_enum_and_seq_tuple_with_optional_named_arg() {
        let spec = decode_seat(
            "enum(catasto.repo, enum(catasto.git, catasto.upstream, catasto.local), (enum(catasto.git, catasto.upstream, catasto.local), executable: bool(optional: true)))",
        )
        .expect("should decode nested enum and tuple");

        assert!(!spec.optional);
        let SeatType::Enum(variants) = &spec.ty else {
            panic!("expected enum seat type, got {:?}", spec.ty);
        };
        assert_eq!(variants.len(), 3);
        assert!(matches!(
            &variants[0],
            EnumVariant::Constructor { namespace, name } if namespace == "catasto" && name == "repo"
        ));
        assert!(matches!(
            &variants[1],
            EnumVariant::Enum(inner) if inner.len() == 3
        ));
        let EnumVariant::Tuple(tuple) = &variants[2] else {
            panic!("expected tuple variant, got {:?}", variants[2]);
        };
        assert_eq!(tuple.positional.len(), 1);
        assert!(matches!(
            &tuple.positional[0],
            ValueSchema::Seat(SeatSpec { ty: SeatType::Enum(inner), optional: false, .. }) if inner.len() == 3
        ));
        assert_eq!(tuple.named.fixed.len(), 1);
        assert_eq!(tuple.named.fixed[0].0, "executable");
        assert!(tuple.named.fixed[0].1.optional);
        assert!(matches!(
            &tuple.named.fixed[0].1.schema,
            ValueSchema::Seat(SeatSpec { ty: SeatType::Builtin(_), optional: true, .. })
        ));
    }

    #[test]
    fn decodes_single_named_group_and_seq_inside_enum() {
        let single_named =
            decode_seat("enum((executable: bool(optional: true)))").expect("named group in enum");
        let SeatType::Enum(variants) = &single_named.ty else {
            panic!("expected enum seat type, got {:?}", single_named.ty);
        };
        assert_eq!(variants.len(), 1);
        let EnumVariant::Tuple(named_tuple) = &variants[0] else {
            panic!("expected tuple variant, got {:?}", variants[0]);
        };
        assert!(named_tuple.positional.is_empty());
        assert_eq!(named_tuple.named.fixed.len(), 1);
        assert_eq!(named_tuple.named.fixed[0].0, "executable");
        assert!(named_tuple.named.fixed[0].1.optional);

        // Also verify `SeatSpec::from_interp_expr` handles top-level `Seq` directly.
        let seq_expr = InterpExpr {
            kind: InterpExprKind::Seq(vec![
                InterpExpr {
                    kind: InterpExprKind::Identifier("string".to_string()),
                    span: Span::dummy(),
                },
                InterpExpr {
                    kind: InterpExprKind::NamedArg {
                        name: "executable".to_string(),
                        value: Box::new(InterpExpr {
                            kind: InterpExprKind::Call {
                                callee: Box::new(InterpExpr {
                                    kind: InterpExprKind::Identifier("bool".to_string()),
                                    span: Span::dummy(),
                                }),
                                args: vec![InterpExpr {
                                    kind: InterpExprKind::NamedArg {
                                        name: "optional".to_string(),
                                        value: Box::new(InterpExpr {
                                            kind: InterpExprKind::Literal(Scalar::Bool(true)),
                                            span: Span::dummy(),
                                        }),
                                    },
                                    span: Span::dummy(),
                                }],
                            },
                            span: Span::dummy(),
                        }),
                    },
                    span: Span::dummy(),
                },
            ]),
            span: Span::dummy(),
        };
        let seq_spec = SeatSpec::from_interp_expr(&seq_expr, Span::dummy()).expect("seq expr");
        let SeatType::Tuple(tuple) = &seq_spec.ty else {
            panic!("expected tuple seat type, got {:?}", seq_spec.ty);
        };
        assert_eq!(tuple.positional.len(), 1);
        assert_eq!(tuple.named.fixed.len(), 1);
        assert!(tuple.named.fixed[0].1.optional);
    }

    #[test]
    fn rejects_invalid_nested_enum_and_duplicate_seq_keys() {
        let empty_nested = decode_seat("enum(catasto.repo, enum())");
        assert!(empty_nested.is_err());

        let optional_nested = decode_seat("enum(catasto.repo, enum(catasto.git, optional: true))");
        assert!(optional_nested.is_err());

        let dup_key = decode_seat("enum((a: string, a: int))");
        assert!(dup_key.is_err());
    }

    #[test]
    fn displays_nested_enum_and_tuple_variants_cleanly() {
        let spec = decode_seat(
            "enum(catasto.repo, enum(catasto.git, catasto.upstream), (catasto.local, executable: bool(optional: true)))",
        )
        .expect("decode");
        let rendered = spec.ty.to_string();
        assert_eq!(
            rendered,
            "one of `catasto.repo(...)`, enum(`catasto.git(...)`, `catasto.upstream(...)`), (`catasto.local(...)`, executable: `bool` (optional))"
        );
    }
}
