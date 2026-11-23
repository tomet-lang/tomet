//! Cotomet blueprint (`.blueprint.ctmt`) schema builders, AST conversion, and rendering.
//!
//! Provides span-free schema types ([`CtmtBlueprintSpec`], [`HeaderAttrSpec`], [`TupleSpec`],
//! [`ValueSpec`], [`SeatExpr`], [`EnumVariantExpr`]) so Rust tools can declare target `.ctmt`
//! schemas ergonomically in code and emit deterministically formatted `.blueprint.ctmt` files
//! via [`render`] or [`to_ast`].

use tomet_ast::{
    Declaration, File, InterpExpr, InterpExprKind, Tuple, TupleEntry, TypeSeat, Value,
};
use tomet_config::PrinterConfig;
use tomet_scalar::Scalar;
use tomet_spelling::{is_bare_key, key_text, scalar_spelling};
use tomet_tree::declaration_new;

use crate::util::dummy;

/// Maximum inline entry count and character width before a nested tuple formats across lines.
const INLINE_TUPLE_MAX_ENTRIES: usize = 2;
const INLINE_TUPLE_MAX_WIDTH: usize = 80;

/// Schema definition for a `.blueprint.ctmt` document (`::kind(blueprint)` + `::blueprint(<target>)`).
#[derive(Debug, Clone, PartialEq)]
pub struct CtmtBlueprintSpec {
    pub target_kind: String,
    pub uses: Vec<String>,
    pub header_attrs: Vec<HeaderAttrSpec>,
    pub body: TupleSpec,
}

impl CtmtBlueprintSpec {
    /// Creates an empty blueprint schema targeting `target_kind`.
    pub fn new(target_kind: impl Into<String>) -> Self {
        Self {
            target_kind: target_kind.into(),
            uses: Vec::new(),
            header_attrs: Vec::new(),
            body: TupleSpec::map(),
        }
    }

    /// Appends a `::use(<namespace>)` declaration to the blueprint header.
    pub fn use_vocab(mut self, namespace: impl Into<String>) -> Self {
        self.uses.push(namespace.into());
        self
    }

    /// Appends a custom `::<name>(...)` header attribute schema.
    pub fn header_attr(mut self, attr: HeaderAttrSpec) -> Self {
        self.header_attrs.push(attr);
        self
    }

    /// Sets the top-level body map schema of the blueprint.
    pub fn body(mut self, body: TupleSpec) -> Self {
        self.body = body;
        self
    }
}

/// Custom header attribute schema (`::<name>(...)`) in a `.blueprint.ctmt` header.
#[derive(Debug, Clone, PartialEq)]
pub struct HeaderAttrSpec {
    pub name: String,
    pub value: ValueSpec,
    pub comments: Vec<String>,
}

impl HeaderAttrSpec {
    /// Creates a header attribute schema for `::<name>(...)`.
    pub fn new(name: impl Into<String>, value: impl Into<ValueSpec>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            comments: Vec::new(),
        }
    }

    /// Appends a single `//` doc comment line emitted immediately above this declaration.
    pub fn comment(mut self, comment: impl Into<String>) -> Self {
        self.comments.push(comment.into());
        self
    }

    /// Appends multiple `//` doc comment lines emitted immediately above this declaration.
    pub fn comments(mut self, comments: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.comments.extend(comments.into_iter().map(Into::into));
        self
    }
}

/// Schema for a cotomet tuple `(...)` — maps, lists, positional tuples, or mixed tuples.
///
/// - Empty `()`: `positional` empty, `fixed` empty, `dynamic` is `None`.
/// - Homogeneous 0+ list `(S)`: single item in `positional`, `fixed` empty, `dynamic` is `None`.
/// - Keyed map: `positional` empty, `fixed` and/or `dynamic` populated.
/// - Positional or mixed tuple: `positional` items followed by `fixed` (and optionally `dynamic`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TupleSpec {
    pub positional: Vec<ValueSpec>,
    pub fixed: Vec<(String, ValueSpec)>,
    pub dynamic: Option<(SeatExpr, ValueSpec)>,
}

impl TupleSpec {
    /// Creates an empty tuple schema `()`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Alias for [`TupleSpec::new`] representing the empty tuple `()`.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Creates an empty map schema ready for [`TupleSpec::fixed`] and [`TupleSpec::dynamic`].
    pub fn map() -> Self {
        Self::default()
    }

    /// Creates a homogeneous 0+ list schema `(item)`.
    pub fn list(item: impl Into<ValueSpec>) -> Self {
        Self {
            positional: vec![item.into()],
            fixed: Vec::new(),
            dynamic: None,
        }
    }

    /// Creates a positional tuple schema from an iterator of item schemas.
    pub fn tuple(positional: impl IntoIterator<Item = ValueSpec>) -> Self {
        Self {
            positional: positional.into_iter().collect(),
            fixed: Vec::new(),
            dynamic: None,
        }
    }

    /// Appends a positional slot schema to this tuple.
    pub fn positional(mut self, item: impl Into<ValueSpec>) -> Self {
        self.positional.push(item.into());
        self
    }

    /// Appends a fixed named entry `key: value` to this tuple or map.
    pub fn fixed(mut self, key: impl Into<String>, value: impl Into<ValueSpec>) -> Self {
        self.fixed.push((key.into(), value.into()));
        self
    }

    /// Sets the dynamic key entry `<$key>: value` for this map.
    pub fn dynamic(mut self, key: SeatExpr, value: impl Into<ValueSpec>) -> Self {
        self.dynamic = Some((key, value.into()));
        self
    }

    /// Returns `true` when there are no positional, fixed, or dynamic entries.
    pub fn is_empty(&self) -> bool {
        self.positional.is_empty() && self.fixed.is_empty() && self.dynamic.is_none()
    }
}

/// Any value constraint inside a `.blueprint.ctmt` schema.
#[derive(Debug, Clone, PartialEq)]
pub enum ValueSpec {
    /// A `<$...>` type seat constraint.
    Seat(SeatExpr),
    /// A tuple, list, or map schema `(...)`.
    Tuple(Box<TupleSpec>),
    /// A literal scalar constant.
    Literal(Scalar),
}

impl ValueSpec {
    /// Wraps a [`SeatExpr`] as a [`ValueSpec::Seat`].
    pub fn seat(seat: SeatExpr) -> Self {
        Self::Seat(seat)
    }

    /// Wraps a [`TupleSpec`] as a [`ValueSpec::Tuple`].
    pub fn tuple(tuple: TupleSpec) -> Self {
        Self::Tuple(Box::new(tuple))
    }

    /// Wraps a [`Scalar`] as a [`ValueSpec::Literal`].
    pub fn literal(scalar: Scalar) -> Self {
        Self::Literal(scalar)
    }

    /// Creates a literal text [`ValueSpec::Literal`].
    pub fn text(text: impl Into<String>) -> Self {
        Self::Literal(Scalar::Text(text.into()))
    }
}

impl From<SeatExpr> for ValueSpec {
    fn from(seat: SeatExpr) -> Self {
        Self::Seat(seat)
    }
}

impl From<TupleSpec> for ValueSpec {
    fn from(tuple: TupleSpec) -> Self {
        Self::Tuple(Box::new(tuple))
    }
}

impl From<Scalar> for ValueSpec {
    fn from(scalar: Scalar) -> Self {
        Self::Literal(scalar)
    }
}

/// An argument passed to a built-in type inside a type seat (`<$list(string, length: 6)>`).
#[derive(Debug, Clone, PartialEq)]
pub struct TypeArg {
    pub name: Option<String>,
    pub value: TypeArgValue,
}

impl TypeArg {
    /// Creates a positional type argument.
    pub fn positional(value: impl Into<TypeArgValue>) -> Self {
        Self {
            name: None,
            value: value.into(),
        }
    }

    /// Creates a named type argument (`name: value`).
    pub fn named(name: impl Into<String>, value: impl Into<TypeArgValue>) -> Self {
        Self {
            name: Some(name.into()),
            value: value.into(),
        }
    }

    /// Creates `optional: <bool>`.
    pub fn optional(optional: bool) -> Self {
        Self::named("optional", Scalar::Bool(optional))
    }

    /// Creates `default: <value>`.
    pub fn default_value(value: impl Into<TypeArgValue>) -> Self {
        Self::named("default", value)
    }
}

/// The value of a [`TypeArg`].
#[derive(Debug, Clone, PartialEq)]
pub enum TypeArgValue {
    /// A scalar literal (`true`, `6`, `"abc"`, `auto`, `none`).
    Literal(Scalar),
    /// A built-in type name or nested type call (`string`, `list(string)`).
    Type { name: String, args: Vec<TypeArg> },
    /// A sequence literal (`("a", "b")`), e.g. for `list(..., default: ("a", "b"))`.
    Seq(Vec<TypeArgValue>),
}

impl TypeArgValue {
    /// References a bare built-in type by name (e.g. `string`, `int`).
    pub fn ty(name: impl Into<String>) -> Self {
        Self::Type {
            name: name.into(),
            args: Vec::new(),
        }
    }

    /// References a parameterized built-in type (e.g. `list(string)`).
    pub fn type_with(
        name: impl Into<String>,
        args: impl IntoIterator<Item = TypeArg>,
    ) -> Self {
        Self::Type {
            name: name.into(),
            args: args.into_iter().collect(),
        }
    }

    /// Wraps a [`Scalar`] literal.
    pub fn literal(scalar: Scalar) -> Self {
        Self::Literal(scalar)
    }

    /// Wraps a text literal.
    pub fn text(s: impl Into<String>) -> Self {
        Self::Literal(Scalar::Text(s.into()))
    }

    /// Wraps a sequence of argument values.
    pub fn seq(items: impl IntoIterator<Item = TypeArgValue>) -> Self {
        Self::Seq(items.into_iter().collect())
    }
}

impl From<Scalar> for TypeArgValue {
    fn from(scalar: Scalar) -> Self {
        Self::Literal(scalar)
    }
}

/// Type expression inside a `<$...>` seat.
#[derive(Debug, Clone, PartialEq)]
pub enum SeatExpr {
    /// A built-in Tomet type (`string`, `bool(optional: true)`, `list(string, length: 6)`).
    Builtin { name: String, args: Vec<TypeArg> },
    /// A namespaced vocabulary constructor (`catasto.git`, `catasto.git(optional: true)`).
    Constructor {
        namespace: String,
        name: String,
        optional: bool,
        default: Option<Scalar>,
    },
    /// An `enum(...)` union of allowed variants.
    Enum {
        variants: Vec<EnumVariantExpr>,
        optional: bool,
        default: Option<Scalar>,
    },
    /// A positional and/or named tuple schema (`(catasto.git, executable: bool(optional: true))`).
    ///
    /// Note: dynamic (`<$type>: ...`) map keys cannot appear inside a `<$...>` seat tuple; use
    /// [`ValueSpec::Tuple`] for maps with dynamic keys.
    Tuple(Box<TupleSpec>),
}

impl SeatExpr {
    /// Creates a bare built-in type seat (`<$name>`).
    pub fn builtin(name: impl Into<String>) -> Self {
        Self::Builtin {
            name: name.into(),
            args: Vec::new(),
        }
    }

    /// Creates a parameterized built-in type seat (`<$name(args...)>`).
    pub fn builtin_with(
        name: impl Into<String>,
        args: impl IntoIterator<Item = TypeArg>,
    ) -> Self {
        Self::Builtin {
            name: name.into(),
            args: args.into_iter().collect(),
        }
    }

    /// Shorthand for `<$string>`.
    pub fn string() -> Self {
        Self::builtin("string")
    }

    /// Shorthand for `<$bool>`.
    pub fn bool() -> Self {
        Self::builtin("bool")
    }

    /// Shorthand for `<$int>`.
    pub fn int() -> Self {
        Self::builtin("int")
    }

    /// Shorthand for `<$float>`.
    pub fn float() -> Self {
        Self::builtin("float")
    }

    /// Shorthand for `<$date>`.
    pub fn date() -> Self {
        Self::builtin("date")
    }

    /// Shorthand for `<$datetime>`.
    pub fn datetime() -> Self {
        Self::builtin("datetime")
    }

    /// Shorthand for `<$uuid>`.
    pub fn uuid() -> Self {
        Self::builtin("uuid")
    }

    /// Shorthand for `<$list(<item>)>`.
    pub fn list(item: impl Into<TypeArgValue>) -> Self {
        Self::builtin_with("list", [TypeArg::positional(item)])
    }

    /// Creates a namespaced constructor seat (`<$namespace.name>`).
    pub fn constructor(namespace: impl Into<String>, name: impl Into<String>) -> Self {
        Self::Constructor {
            namespace: namespace.into(),
            name: name.into(),
            optional: false,
            default: None,
        }
    }

    /// Creates an `enum(...)` union seat (`<$enum(variants...)>`).
    pub fn enum_of(variants: impl IntoIterator<Item = EnumVariantExpr>) -> Self {
        Self::Enum {
            variants: variants.into_iter().collect(),
            optional: false,
            default: None,
        }
    }

    /// Creates a tuple seat expression (`(...)`).
    pub fn tuple(tuple: TupleSpec) -> Self {
        Self::Tuple(Box::new(tuple))
    }

    /// Marks this seat as `optional: true`.
    ///
    /// For [`SeatExpr::Tuple`], optionality is determined by every entry being optional.
    pub fn optional(self) -> Self {
        self.with_optional(true)
    }

    /// Sets whether this seat carries `optional: true`.
    pub fn with_optional(mut self, is_optional: bool) -> Self {
        match &mut self {
            Self::Builtin { args, .. } => {
                if is_optional {
                    upsert_named_arg(args, "optional", TypeArgValue::Literal(Scalar::Bool(true)));
                } else {
                    remove_named_arg(args, "optional");
                }
            }
            Self::Constructor { optional, .. } | Self::Enum { optional, .. } => {
                *optional = is_optional;
            }
            Self::Tuple(_) => {}
        }
        self
    }

    /// Sets `default: <value>` on this seat.
    pub fn default_value(mut self, value: Scalar) -> Self {
        match &mut self {
            Self::Builtin { args, .. } => {
                upsert_named_arg(args, "default", TypeArgValue::Literal(value));
            }
            Self::Constructor { default, .. } | Self::Enum { default, .. } => {
                *default = Some(value);
            }
            Self::Tuple(_) => {}
        }
        self
    }
}

/// A single allowed alternative inside `enum(...)`.
#[derive(Debug, Clone, PartialEq)]
pub enum EnumVariantExpr {
    /// A literal scalar value (`"follow"`, `42`, `true`, `none`, `auto`).
    Literal(Scalar),
    /// A namespaced constructor (`catasto.git`).
    Constructor { namespace: String, name: String },
    /// A built-in type (`string`, `list(string)`).
    Type { name: String, args: Vec<TypeArg> },
    /// A nested `enum(...)` union.
    Enum(Vec<EnumVariantExpr>),
    /// A positional and/or named tuple (`(catasto.local, executable: bool(optional: true))`).
    Tuple(Box<TupleSpec>),
}

impl EnumVariantExpr {
    /// Creates a literal scalar variant.
    pub fn literal(scalar: Scalar) -> Self {
        Self::Literal(scalar)
    }

    /// Creates a quoted string literal variant.
    pub fn text(s: impl Into<String>) -> Self {
        Self::Literal(Scalar::Text(s.into()))
    }

    /// Creates a namespaced constructor variant (`namespace.name`).
    pub fn constructor(namespace: impl Into<String>, name: impl Into<String>) -> Self {
        Self::Constructor {
            namespace: namespace.into(),
            name: name.into(),
        }
    }

    /// Creates a bare built-in type variant (`string`, `int`, etc.).
    pub fn ty(name: impl Into<String>) -> Self {
        Self::Type {
            name: name.into(),
            args: Vec::new(),
        }
    }

    /// Creates a parameterized built-in type variant (`list(string)`, etc.).
    pub fn type_with(
        name: impl Into<String>,
        args: impl IntoIterator<Item = TypeArg>,
    ) -> Self {
        Self::Type {
            name: name.into(),
            args: args.into_iter().collect(),
        }
    }

    /// Creates a nested `enum(...)` variant.
    pub fn enum_of(variants: impl IntoIterator<Item = EnumVariantExpr>) -> Self {
        Self::Enum(variants.into_iter().collect())
    }

    /// Creates a tuple variant.
    pub fn tuple(tuple: TupleSpec) -> Self {
        Self::Tuple(Box::new(tuple))
    }
}

impl From<Scalar> for EnumVariantExpr {
    fn from(scalar: Scalar) -> Self {
        Self::Literal(scalar)
    }
}

fn upsert_named_arg(args: &mut Vec<TypeArg>, key: &str, value: TypeArgValue) {
    if let Some(existing) = args
        .iter_mut()
        .find(|arg| arg.name.as_deref() == Some(key))
    {
        existing.value = value;
    } else {
        args.push(TypeArg::named(key, value));
    }
}

fn remove_named_arg(args: &mut Vec<TypeArg>, key: &str) {
    args.retain(|arg| arg.name.as_deref() != Some(key));
}

// ---------------------------------------------------------------------------
// AST conversion
// ---------------------------------------------------------------------------

/// Converts a [`CtmtBlueprintSpec`] into a cotomet [`File<Value>`] AST with dummy spans.
pub fn to_ast(spec: &CtmtBlueprintSpec) -> File<Value> {
    let mut header = Vec::with_capacity(2 + spec.uses.len() + spec.header_attrs.len());

    header.push(simple_decl("kind", "blueprint"));
    header.push(simple_decl("blueprint", &spec.target_kind));

    for ns in &spec.uses {
        header.push(simple_decl("use", ns));
    }

    for attr in &spec.header_attrs {
        let mut decl = declaration_new(&attr.name);
        let args = match &attr.value {
            ValueSpec::Tuple(tuple) => tuple_spec_to_ast(tuple),
            other => Tuple::from_unnamed([value_spec_to_ast(other)]),
        };
        decl.data.args = Some(args);
        header.push(decl);
    }

    let body = Value::Tuple(tuple_spec_to_ast(&spec.body));
    File::new(header, body, dummy())
}

fn simple_decl(name: &str, arg: &str) -> Declaration {
    let mut decl = declaration_new(name);
    decl.data.args = Some(Tuple::from_unnamed([Value::Scalar(Scalar::Text(
        arg.to_string(),
    ))]));
    decl
}

fn tuple_spec_to_ast(tuple: &TupleSpec) -> Tuple {
    let mut entries = Vec::with_capacity(
        tuple.positional.len() + tuple.fixed.len() + usize::from(tuple.dynamic.is_some()),
    );
    for item in &tuple.positional {
        entries.push(TupleEntry::unnamed(value_spec_to_ast(item)));
    }
    for (key, val) in &tuple.fixed {
        entries.push(TupleEntry::named(key.clone(), value_spec_to_ast(val)));
    }
    if let Some((key_seat, val)) = &tuple.dynamic {
        entries.push(TupleEntry::type_seat(
            seat_to_type_seat(key_seat),
            value_spec_to_ast(val),
        ));
    }
    Tuple::from_entries(entries)
}

fn value_spec_to_ast(spec: &ValueSpec) -> Value {
    match spec {
        // Outside `<$...>`, a standalone tuple cannot be written as `<$(...)>`; emit a cotomet tuple.
        ValueSpec::Seat(SeatExpr::Tuple(tuple)) => Value::Tuple(tuple_spec_to_ast(tuple)),
        ValueSpec::Tuple(tuple) => Value::Tuple(tuple_spec_to_ast(tuple)),
        ValueSpec::Seat(seat) => Value::TypeSeat(seat_to_type_seat(seat)),
        ValueSpec::Literal(scalar) => Value::Scalar(scalar.clone()),
    }
}

fn seat_to_type_seat(seat: &SeatExpr) -> TypeSeat {
    assert!(
        !matches!(seat, SeatExpr::Tuple(_)),
        "a tuple seat cannot be used as a `<$...>` map key or standalone type seat"
    );
    TypeSeat {
        expr: seat_expr_to_interp(seat),
        span: dummy(),
    }
}

fn seat_expr_to_interp(seat: &SeatExpr) -> InterpExpr {
    match seat {
        SeatExpr::Builtin { name, args } => call_or_ident(name, args),
        SeatExpr::Constructor {
            namespace,
            name,
            optional,
            default,
        } => {
            let callee = member_expr(namespace, name);
            let mut args = Vec::new();
            if *optional {
                args.push(named_interp("optional", literal_interp(Scalar::Bool(true))));
            }
            if let Some(d) = default {
                args.push(named_interp("default", literal_interp(d.clone())));
            }
            if args.is_empty() {
                callee
            } else {
                InterpExpr {
                    kind: InterpExprKind::Call {
                        callee: Box::new(callee),
                        args,
                    },
                    span: dummy(),
                }
            }
        }
        SeatExpr::Enum {
            variants,
            optional,
            default,
        } => {
            let mut args: Vec<InterpExpr> =
                variants.iter().map(enum_variant_to_interp).collect();
            if *optional {
                args.push(named_interp("optional", literal_interp(Scalar::Bool(true))));
            }
            if let Some(d) = default {
                args.push(named_interp("default", literal_interp(d.clone())));
            }
            InterpExpr {
                kind: InterpExprKind::Call {
                    callee: Box::new(ident_expr("enum")),
                    args,
                },
                span: dummy(),
            }
        }
        SeatExpr::Tuple(tuple) => interp_tuple(tuple),
    }
}

fn enum_variant_to_interp(variant: &EnumVariantExpr) -> InterpExpr {
    match variant {
        EnumVariantExpr::Literal(scalar) => literal_interp(scalar.clone()),
        EnumVariantExpr::Constructor { namespace, name } => member_expr(namespace, name),
        EnumVariantExpr::Type { name, args } => call_or_ident(name, args),
        EnumVariantExpr::Enum(variants) => InterpExpr {
            kind: InterpExprKind::Call {
                callee: Box::new(ident_expr("enum")),
                args: variants.iter().map(enum_variant_to_interp).collect(),
            },
            span: dummy(),
        },
        EnumVariantExpr::Tuple(tuple) => interp_tuple(tuple),
    }
}

fn interp_tuple(tuple: &TupleSpec) -> InterpExpr {
    assert!(
        tuple.dynamic.is_none(),
        "dynamic (<$type>: ...) keys are only supported in ValueSpec::Tuple, not inside a <$...> seat tuple"
    );
    let mut items = Vec::with_capacity(tuple.positional.len() + tuple.fixed.len());
    for item in &tuple.positional {
        items.push(value_spec_to_interp(item));
    }
    for (key, val) in &tuple.fixed {
        items.push(named_interp(key, value_spec_to_interp(val)));
    }
    if items.len() == 1 && matches!(&items[0].kind, InterpExprKind::NamedArg { .. }) {
        InterpExpr {
            kind: InterpExprKind::Group(Box::new(items.into_iter().next().unwrap())),
            span: dummy(),
        }
    } else {
        InterpExpr {
            kind: InterpExprKind::Seq(items),
            span: dummy(),
        }
    }
}

fn value_spec_to_interp(spec: &ValueSpec) -> InterpExpr {
    match spec {
        ValueSpec::Seat(seat) => seat_expr_to_interp(seat),
        ValueSpec::Tuple(tuple) => interp_tuple(tuple),
        ValueSpec::Literal(scalar) => literal_interp(scalar.clone()),
    }
}

fn type_arg_to_interp(arg: &TypeArg) -> InterpExpr {
    let val = type_arg_value_to_interp(&arg.value);
    match &arg.name {
        Some(name) => named_interp(name, val),
        None => val,
    }
}

fn type_arg_value_to_interp(value: &TypeArgValue) -> InterpExpr {
    match value {
        TypeArgValue::Literal(scalar) => literal_interp(scalar.clone()),
        TypeArgValue::Type { name, args } => call_or_ident(name, args),
        TypeArgValue::Seq(items) => InterpExpr {
            kind: InterpExprKind::Seq(items.iter().map(type_arg_value_to_interp).collect()),
            span: dummy(),
        },
    }
}

fn call_or_ident(name: &str, args: &[TypeArg]) -> InterpExpr {
    let callee = ident_expr(name);
    if args.is_empty() {
        callee
    } else {
        InterpExpr {
            kind: InterpExprKind::Call {
                callee: Box::new(callee),
                args: args.iter().map(type_arg_to_interp).collect(),
            },
            span: dummy(),
        }
    }
}

fn ident_expr(name: &str) -> InterpExpr {
    InterpExpr {
        kind: InterpExprKind::Identifier(name.to_string()),
        span: dummy(),
    }
}

fn member_expr(namespace: &str, name: &str) -> InterpExpr {
    InterpExpr {
        kind: InterpExprKind::Member {
            object: Box::new(ident_expr(namespace)),
            member: name.to_string(),
        },
        span: dummy(),
    }
}

fn literal_interp(scalar: Scalar) -> InterpExpr {
    InterpExpr {
        kind: InterpExprKind::Literal(scalar),
        span: dummy(),
    }
}

fn named_interp(name: &str, value: InterpExpr) -> InterpExpr {
    InterpExpr {
        kind: InterpExprKind::NamedArg {
            name: name.to_string(),
            value: Box::new(value),
        },
        span: dummy(),
    }
}

// ---------------------------------------------------------------------------
// Deterministic .blueprint.ctmt rendering
// ---------------------------------------------------------------------------

/// Renders a [`CtmtBlueprintSpec`] into a deterministically formatted `.blueprint.ctmt` string.
pub fn render(spec: &CtmtBlueprintSpec, config: &PrinterConfig) -> String {
    let mut out = String::new();
    let blank_between = config.format.declaration_blank_between.unwrap_or(false);
    let blank_after = config.format.declaration_blank_after.unwrap_or(true);

    let mut header_lines = Vec::new();
    header_lines.push(format!("::kind({})", render_decl_arg("blueprint")));
    header_lines.push(format!("::blueprint({})", render_decl_arg(&spec.target_kind)));
    for ns in &spec.uses {
        header_lines.push(format!("::use({})", render_decl_arg(ns)));
    }
    for attr in &spec.header_attrs {
        header_lines.push(render_header_attr(attr));
    }

    for (i, decl) in header_lines.iter().enumerate() {
        if i > 0 && blank_between {
            out.push('\n');
        }
        out.push_str(decl);
        out.push('\n');
    }

    if !spec.body.is_empty() {
        if blank_after {
            out.push('\n');
        }
        render_top_level_body(&spec.body, &mut out);
    }

    tomet_formatter::format_data_source(&out)
}

fn render_decl_arg(text: &str) -> String {
    if is_bare_key(text) {
        text.to_string()
    } else {
        scalar_spelling(&Scalar::Text(text.to_string()))
    }
}

fn render_header_attr(attr: &HeaderAttrSpec) -> String {
    let mut out = String::new();
    for comment in &attr.comments {
        for line in comment.lines() {
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                out.push_str("//\n");
            } else {
                out.push_str("// ");
                out.push_str(trimmed);
                out.push('\n');
            }
        }
    }
    out.push_str("::");
    out.push_str(&attr.name);
    out.push('(');
    match &attr.value {
        ValueSpec::Tuple(tuple) => out.push_str(&render_tuple_contents(tuple, 0)),
        other => out.push_str(&render_value_spec(other, 0)),
    }
    out.push(')');
    out
}

fn render_top_level_body(body: &TupleSpec, out: &mut String) {
    let entries = render_tuple_entries(body, 0);
    for entry in entries {
        out.push_str(&entry);
        out.push('\n');
    }
}

fn render_tuple_contents(tuple: &TupleSpec, indent: usize) -> String {
    if tuple.is_empty() {
        return String::new();
    }

    let entries = render_tuple_entries(tuple, indent);
    if should_render_inline(tuple, &entries) {
        return entries.join(", ");
    }

    // If a single keyed entry (e.g. `::zone(<$string>: (\n...\n))`) already formats its own
    // nested multiline block, keep `key: (` on the declaration line rather than double-wrapping.
    if entries.len() == 1 && entries[0].contains('\n') {
        return entries.into_iter().next().unwrap();
    }

    let child_indent = "  ".repeat(indent + 1);
    let close_indent = "  ".repeat(indent);
    let mut out = String::from("\n");
    for entry in entries {
        out.push_str(&child_indent);
        out.push_str(&entry);
        out.push('\n');
    }
    out.push_str(&close_indent);
    out
}

fn render_value_spec(spec: &ValueSpec, indent: usize) -> String {
    match spec {
        ValueSpec::Seat(SeatExpr::Tuple(tuple)) => render_tuple_value(tuple, indent),
        ValueSpec::Tuple(tuple) => render_tuple_value(tuple, indent),
        ValueSpec::Seat(seat) => seat_to_type_seat(seat).to_string(),
        ValueSpec::Literal(scalar) => scalar_spelling(scalar),
    }
}

fn render_tuple_value(tuple: &TupleSpec, indent: usize) -> String {
    if tuple.is_empty() {
        return "()".to_string();
    }
    let entries = render_tuple_entries(tuple, indent);
    if should_render_inline(tuple, &entries) {
        return format!("({})", entries.join(", "));
    }
    let child_indent = "  ".repeat(indent + 1);
    let close_indent = "  ".repeat(indent);
    let mut out = String::from("(\n");
    for entry in entries {
        out.push_str(&child_indent);
        out.push_str(&entry);
        out.push('\n');
    }
    out.push_str(&close_indent);
    out.push(')');
    out
}

fn render_tuple_entries(tuple: &TupleSpec, indent: usize) -> Vec<String> {
    let next_indent = if should_indent_children(tuple) {
        indent + 1
    } else {
        indent
    };
    let mut entries = Vec::with_capacity(
        tuple.positional.len() + tuple.fixed.len() + usize::from(tuple.dynamic.is_some()),
    );
    for item in &tuple.positional {
        entries.push(render_value_spec(item, next_indent));
    }
    for (key, val) in &tuple.fixed {
        entries.push(format!(
            "{}: {}",
            key_text(key),
            render_value_spec(val, next_indent)
        ));
    }
    if let Some((key_seat, val)) = &tuple.dynamic {
        let rendered_key = seat_to_type_seat(key_seat).to_string();
        entries.push(format!(
            "{rendered_key}: {}",
            render_value_spec(val, next_indent)
        ));
    }
    entries
}

fn should_indent_children(tuple: &TupleSpec) -> bool {
    let total = tuple.positional.len() + tuple.fixed.len() + usize::from(tuple.dynamic.is_some());
    total > 1
}

fn should_render_inline(tuple: &TupleSpec, entries: &[String]) -> bool {
    if entries.iter().any(|e| e.contains('\n')) {
        return false;
    }
    if tuple.positional.len() == 1 && tuple.fixed.is_empty() && tuple.dynamic.is_none() {
        return true;
    }
    if entries.len() > INLINE_TUPLE_MAX_ENTRIES {
        return false;
    }
    let width: usize = entries.iter().map(|e| e.len()).sum::<usize>()
        + entries.len().saturating_sub(1) * 2
        + 2;
    width <= INLINE_TUPLE_MAX_WIDTH
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_semantics::{Bindings, Vocabulary};
    use tomet_validator_blueprint::{
        extract_ctmt_blueprint_schema, validate_ctmt_against_blueprint, validate_ctmt_blueprint,
    };

    fn catasto_vocab() -> Vocabulary {
        let doc = tomet_ast_parser::parse_file(
            r#"::kind(vocabulary)
::vocabulary(catasto)

::constructor(
  git: (
    args: (
      url: (type: string, required: true, positional: 0)
      rev: (type: string)
      root: (type: string)
    )
  )
  upstream: (
    args: (
      name: (type: string, required: true, positional: 0)
      path: (type: string, required: true)
    )
  )
  local: (
    args: (
      path: (type: string, required: true, positional: 0)
    )
  )
  repo: (
    args: (
      path: (type: string, required: true, positional: 0)
    )
  )
)
"#,
        )
        .expect("valid vocabulary");
        Vocabulary::from_file(&doc).expect("vocabulary")
    }

    fn catasto_blueprint_spec() -> CtmtBlueprintSpec {
        let zone_pattern_list = SeatExpr::enum_of([
            EnumVariantExpr::ty("string"),
            EnumVariantExpr::type_with("list", [TypeArg::positional(TypeArgValue::ty("string"))]),
        ])
        .optional();

        let source_variants = EnumVariantExpr::enum_of([
            EnumVariantExpr::constructor("catasto", "git"),
            EnumVariantExpr::constructor("catasto", "upstream"),
            EnumVariantExpr::constructor("catasto", "local"),
        ]);

        let top_level_entry = SeatExpr::enum_of([
            EnumVariantExpr::constructor("catasto", "repo"),
            source_variants.clone(),
            EnumVariantExpr::tuple(
                TupleSpec::new()
                    .positional(ValueSpec::Seat(SeatExpr::enum_of([
                        EnumVariantExpr::constructor("catasto", "git"),
                        EnumVariantExpr::constructor("catasto", "upstream"),
                        EnumVariantExpr::constructor("catasto", "local"),
                    ])))
                    .fixed("executable", SeatExpr::bool().optional()),
            ),
        ]);

        CtmtBlueprintSpec::new("catasto")
            .header_attr(
                HeaderAttrSpec::new(
                    "upstream",
                    TupleSpec::map().dynamic(
                        SeatExpr::string().optional(),
                        SeatExpr::constructor("catasto", "git"),
                    ),
                )
                .comment("Named upstream git repositories."),
            )
            .header_attr(
                HeaderAttrSpec::new(
                    "zone",
                    TupleSpec::map().dynamic(
                        SeatExpr::string().optional(),
                        TupleSpec::map()
                            .fixed("require", zone_pattern_list.clone())
                            .fixed("allow", zone_pattern_list.clone())
                            .fixed("deny", zone_pattern_list.clone())
                            .fixed("ignore", zone_pattern_list.clone())
                            .fixed("recursive", SeatExpr::bool().optional())
                            .fixed("files", zone_pattern_list.clone())
                            .fixed("dirs", zone_pattern_list),
                    ),
                )
                .comment("Zone access and path matching rules."),
            )
            .body(TupleSpec::map().dynamic(SeatExpr::string(), top_level_entry))
    }

    #[test]
    fn renders_deterministic_catasto_blueprint_and_roundtrips_through_validator() {
        let spec = catasto_blueprint_spec();
        let cfg = PrinterConfig::default();

        let rendered_a = render(&spec, &cfg);
        let rendered_b = render(&spec, &cfg);
        assert_eq!(rendered_a, rendered_b, "rendering must be deterministic");

        assert!(rendered_a.starts_with("::kind(blueprint)\n::blueprint(catasto)\n"));
        assert!(rendered_a.contains("::upstream(<$string(optional: true)>: <$catasto.git>)"));
        assert!(rendered_a.contains("require: <$enum(string, list(string), optional: true)>"));
        assert!(rendered_a.contains("recursive: <$bool(optional: true)>"));

        // 1. Must parse cleanly as a cotomet data file.
        let parsed_bp =
            tomet_ast_parser::parse_data_file(&rendered_a).expect("generated blueprint parses");

        // 2. AST from `to_ast` and parsed AST must extract the exact same schema.
        let ast_bp = to_ast(&spec);
        let schema_from_rendered =
            extract_ctmt_blueprint_schema(&parsed_bp).expect("extract from rendered");
        let schema_from_ast = extract_ctmt_blueprint_schema(&ast_bp).expect("extract from to_ast");
        assert_eq!(schema_from_rendered.target_kind, "catasto");
        assert_eq!(
            schema_from_rendered.header_attrs.len(),
            schema_from_ast.header_attrs.len()
        );
        assert_eq!(schema_from_rendered.body.fixed.len(), schema_from_ast.body.fixed.len());
        assert_eq!(
            schema_from_rendered.body.dynamic.is_some(),
            schema_from_ast.body.dynamic.is_some()
        );

        // 3. Self-check against catasto vocabulary must report 0 errors.
        let vocab = catasto_vocab();
        let bp_bindings = Bindings::for_document(&parsed_bp, [vocab.clone()]);
        let bp_errors = validate_ctmt_blueprint(&parsed_bp, &bp_bindings);
        assert!(
            bp_errors.is_empty(),
            "blueprint self-check errors: {bp_errors:?}\n{rendered_a}"
        );

        // 4. Realistic valid `catasto.ctmt` manifest validates with 0 errors.
        let valid_manifest = tomet_ast_parser::parse_data_file(
            r#"::kind(catasto)
::use(catasto)
::upstream(
  commonfiles: catasto.git("https://github.com/tomet-lang/commonfiles.git", rev: main)
)
::zone(
  core: (
    require: "crates/*"
    allow: ("crates/*", "tests/*")
    deny: "target/*"
    recursive: true
    files: ("Cargo.toml", "*.rs")
  )
)

".editorconfig": catasto.upstream(commonfiles, path: ".editorconfig")
"vendor/lib": catasto.repo("vendor/lib")
"scripts/check.sh": (catasto.local("shared/check.sh"), executable: true)
"scripts/fmt.sh": (catasto.upstream(commonfiles, path: "scripts/fmt.sh"),)
"#,
        )
        .expect("valid catasto manifest parses");

        let doc_bindings = Bindings::for_document(&valid_manifest, [vocab.clone()]);
        let errs = validate_ctmt_against_blueprint(&valid_manifest, &parsed_bp, &doc_bindings);
        assert!(errs.is_empty(), "manifest validation errors: {errs:?}");

        // 5. Realistic manifest omitting optional `::upstream` & `::zone` also validates cleanly.
        let minimal_manifest = tomet_ast_parser::parse_data_file(
            r#"::kind(catasto)
::use(catasto)

"vendor/lib": catasto.repo("vendor/lib")
"#,
        )
        .expect("minimal manifest parses");
        let min_bindings = Bindings::for_document(&minimal_manifest, [vocab.clone()]);
        assert!(
            validate_ctmt_against_blueprint(&minimal_manifest, &parsed_bp, &min_bindings)
                .is_empty()
        );

        // 6. Invalid manifest entries are caught by the generated blueprint.
        let invalid_manifest = tomet_ast_parser::parse_data_file(
            r#"::kind(catasto)
::use(catasto)
::upstream(commonfiles: "not-a-git-call")
::zone(
  core: (
    require: ("crates/*", none)
    unknown_zone_opt: true
  )
)

"bad_entry": "just-a-string"
"bad_tuple": (catasto.repo("vendor/foo"), executable: true)
"bad_exec": (catasto.local("shared/run.sh"), executable: "yes")
"#,
        )
        .expect("invalid manifest parses");
        let bad_bindings = Bindings::for_document(&invalid_manifest, [vocab]);
        let bad_errs = validate_ctmt_against_blueprint(&invalid_manifest, &parsed_bp, &bad_bindings);
        assert_eq!(bad_errs.len(), 6, "expected 6 errors, got: {bad_errs:?}");
    }

    #[test]
    fn renders_single_positional_seat_tuples_and_literals() {
        let spec = CtmtBlueprintSpec::new("demo")
            .use_vocab("catasto")
            .body(
                TupleSpec::map()
                    .fixed("tags", TupleSpec::list(SeatExpr::string()))
                    .fixed(
                        "items",
                        TupleSpec::list(SeatExpr::enum_of([
                            EnumVariantExpr::text("a"),
                            EnumVariantExpr::text("b"),
                        ])),
                    )
                    .fixed(
                        "pair",
                        TupleSpec::tuple([
                            SeatExpr::int().into(),
                            SeatExpr::bool().optional().into(),
                        ]),
                    )
                    .fixed(
                        "single_pos_in_enum",
                        SeatExpr::enum_of([
                            EnumVariantExpr::Literal(Scalar::None),
                            EnumVariantExpr::tuple(TupleSpec::list(SeatExpr::string())),
                        ]),
                    )
                    .fixed(
                        "single_named_in_enum",
                        SeatExpr::enum_of([EnumVariantExpr::tuple(
                            TupleSpec::map().fixed("flag", SeatExpr::bool().optional()),
                        )]),
                    )
                    .fixed(
                        "version",
                        SeatExpr::builtin_with(
                            "string",
                            [
                                TypeArg::optional(true),
                                TypeArg::default_value(TypeArgValue::text("1.0.0")),
                            ],
                        ),
                    )
                    .fixed("kind_tag", ValueSpec::Literal(Scalar::Text("v1".into())))
                    .fixed(
                        "created",
                        SeatExpr::date().default_value(Scalar::Auto),
                    )
                    .fixed(
                        "labels",
                        SeatExpr::builtin_with(
                            "list",
                            [
                                TypeArg::positional(TypeArgValue::ty("string")),
                                TypeArg::default_value(TypeArgValue::seq([
                                    TypeArgValue::text("a"),
                                    TypeArgValue::text("b"),
                                ])),
                            ],
                        ),
                    ),
            );

        let rendered = render(&spec, &PrinterConfig::default());
        let parsed = tomet_ast_parser::parse_data_file(&rendered).expect("parses");
        let errs = validate_ctmt_blueprint(&parsed, &Bindings::default());
        assert!(errs.is_empty(), "errors: {errs:?}\n{rendered}");

        let ok = tomet_ast_parser::parse_data_file(
            r#"::kind(demo)

tags: ("x", "y")
items: ("a")
pair: (1)
single_pos_in_enum: ("only")
single_named_in_enum: (flag: true)
kind_tag: "v1"
"#,
        )
        .expect("ok parses");
        let doc_errs = validate_ctmt_against_blueprint(&ok, &parsed, &Bindings::default());
        assert!(doc_errs.is_empty(), "doc errors: {doc_errs:?}");
    }

    #[test]
    fn quotes_non_bare_keys_and_target_kinds() {
        let spec = CtmtBlueprintSpec::new("my kind").body(
            TupleSpec::map()
                .fixed("a/b", SeatExpr::string())
                .fixed("true", SeatExpr::bool()),
        );
        let rendered = render(&spec, &PrinterConfig::default());
        assert!(rendered.contains("::blueprint(\"my kind\")"));
        assert!(rendered.contains("\"a/b\": <$string>"));
        let parsed = tomet_ast_parser::parse_data_file(&rendered).expect("parses");
        let schema = extract_ctmt_blueprint_schema(&parsed).expect("extracts");
        assert_eq!(schema.target_kind, "my kind");
        assert_eq!(schema.body.fixed[0].0, "a/b");
        assert_eq!(schema.body.fixed[1].0, "true");
    }

    #[test]
    fn seat_tuple_in_value_position_renders_as_valid_cotomet_tuple() {
        let spec = CtmtBlueprintSpec::new("demo").body(TupleSpec::map().fixed(
            "opt",
            ValueSpec::Seat(SeatExpr::tuple(
                TupleSpec::new()
                    .positional(SeatExpr::int())
                    .fixed("label", SeatExpr::string().optional()),
            )),
        ));
        let rendered = render(&spec, &PrinterConfig::default());
        assert!(rendered.contains("opt: (<$int>, label: <$string(optional: true)>)"));
        let parsed = tomet_ast_parser::parse_data_file(&rendered).expect("parses");
        let errs = validate_ctmt_blueprint(&parsed, &Bindings::default());
        assert!(errs.is_empty(), "errors: {errs:?}");
    }
}
