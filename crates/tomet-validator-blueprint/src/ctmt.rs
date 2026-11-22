//! Schema extraction and validation for cotomet blueprints (`.blueprint.ctmt`).
//!
//! A `.blueprint.ctmt` document is a [`File<Value>`] declaring `::kind(blueprint)` and
//! `::blueprint(<target-kind>)`. It describes the allowed shape of target `.ctmt` documents:
//!
//! - **Header attributes** (`header_attrs`): non-standard header declarations (such as
//!   `::upstream(<$string>: <$catasto.git>)`) are required in target documents unless all of
//!   their entries (or their top-level seat) are marked `optional: true` (or have a `default`),
//!   or the attribute declaration is empty (`::attr()`).
//! - **Body and nested maps** (`MapSchema`): fixed named keys (`key: ...`) are required unless
//!   their value schema is a [`SeatSpec`] with `optional: true` or a `default`. Dynamic keys
//!   (`<$type>: <schema>`) accept any key matching `<$type>` that is not already a fixed key.
//!   Maps without a dynamic key are closed: unrecognized keys produce [`Diagnostic::UnknownKey`].

use std::fmt;

use tomet_ast::{EntryKey, File, Tuple, Value};
use tomet_scalar::Scalar;
use tomet_semantics::{Bindings, DeclKind, ElementKind, classify_declaration_lenient};
use tomet_span::Span;
use tomet_spelling::scalar_spelling;
use tomet_tree::ValueExt;
use tomet_type::Type;
use tomet_validator_core::Diagnostic;
use tomet_validator_core::diagnostic::written_value;

use crate::seat::{EnumVariant, SeatSpec, SeatType};

/// Structural schema extracted from a `.blueprint.ctmt` document.
#[derive(Debug, Clone, PartialEq)]
pub struct CtmtBlueprintSchema {
    pub target_kind: String,
    pub header_attrs: Vec<HeaderAttrSchema>,
    pub body: MapSchema,
}

/// A custom header attribute declaration extracted from a `.blueprint.ctmt` header.
#[derive(Debug, Clone, PartialEq)]
pub struct HeaderAttrSchema {
    pub name: String,
    pub value: ValueSchema,
    pub optional: bool,
    pub span: Span,
}

/// Schema for the keyed entries of a cotomet map or tuple.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MapSchema {
    pub fixed: Vec<(String, MapEntrySchema)>,
    pub dynamic: Option<(SeatSpec, ValueSchema)>,
}

impl MapSchema {
    /// Returns `true` when there are neither fixed nor dynamic keyed entries.
    pub fn is_empty(&self) -> bool {
        self.fixed.is_empty() && self.dynamic.is_none()
    }

    /// Returns `true` when every fixed entry is optional and any dynamic key is also optional
    /// (or there are no entries at all).
    pub fn is_all_optional(&self) -> bool {
        self.fixed.iter().all(|(_, entry)| entry.optional)
            && self
                .dynamic
                .as_ref()
                .is_none_or(|(key_spec, _)| key_spec.optional)
    }
}

/// A fixed named entry inside a [`MapSchema`].
#[derive(Debug, Clone, PartialEq)]
pub struct MapEntrySchema {
    pub schema: ValueSchema,
    pub optional: bool,
    pub span: Span,
}

/// Schema for a cotomet tuple `(...)`, covering empty tuples, homogeneous lists,
/// positional tuples, keyed maps, and mixed positional + named tuples:
///
/// - empty `()` (`positional.is_empty() && named.is_empty()`) matches `()`
/// - single positional item with no named entries (`positional.len() == 1 && named.is_empty()`)
///   matches a homogeneous 0+ list
/// - pure keyed map (`positional.is_empty() && !named.is_empty()`) matches keyed entries only
/// - fixed/bounded positional or mixed tuple (`!positional.is_empty()` with either
///   `positional.len() >= 2` or `!named.is_empty()`) validates positional slots and named keys.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TupleSchema {
    pub positional: Vec<ValueSchema>,
    pub named: MapSchema,
}

impl TupleSchema {
    /// Returns `true` for the empty tuple schema `()`.
    pub fn is_empty(&self) -> bool {
        self.positional.is_empty() && self.named.is_empty()
    }

    /// Returns `true` when this tuple schema describes a keyed-only map.
    pub fn is_pure_map(&self) -> bool {
        self.positional.is_empty() && !self.named.is_empty()
    }

    /// Returns `true` when this tuple schema describes a 0+ homogeneous list `(S)`.
    pub fn is_list(&self) -> bool {
        self.positional.len() == 1 && self.named.is_empty()
    }

    /// Returns `true` when every positional item and every named entry is optional
    /// (or the tuple is empty).
    pub fn is_all_optional(&self) -> bool {
        self.positional.iter().all(value_schema_is_optional) && self.named.is_all_optional()
    }

    /// Returns `(min_positional, max_positional)` for fixed/mixed tuples.
    ///
    /// Trailing positional items marked optional (or carrying a `default`) may be omitted.
    pub fn positional_arity(&self) -> (usize, usize) {
        let max = self.positional.len();
        let min = self
            .positional
            .iter()
            .rposition(|item| !value_schema_is_optional(item))
            .map_or(0, |idx| idx + 1);
        (min, max)
    }
}

/// Recursive schema for any cotomet value in a `.blueprint.ctmt` document.
#[derive(Debug, Clone, PartialEq)]
pub enum ValueSchema {
    /// A `<$...>` type seat constraint.
    Seat(SeatSpec),
    /// A tuple, list, or map schema.
    Tuple(Box<TupleSchema>),
    /// A literal scalar constant.
    Literal(Scalar),
    /// A literal constructor call written directly outside a `<$...>` seat.
    LiteralCall { head: String, args: Tuple },
}

pub(crate) fn value_schema_is_optional(schema: &ValueSchema) -> bool {
    matches!(
        schema,
        ValueSchema::Seat(spec) if spec.optional || spec.default.is_some()
    )
}

impl fmt::Display for ValueSchema {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValueSchema::Seat(spec) => {
                if spec.optional {
                    write!(f, "{} (optional)", spec.ty)
                } else {
                    write!(f, "{}", spec.ty)
                }
            }
            ValueSchema::Tuple(tuple) => write!(f, "{tuple}"),
            ValueSchema::Literal(s) => write!(f, "`{}`", scalar_spelling(s)),
            ValueSchema::LiteralCall { head, .. } => write!(f, "`{head}(...)`"),
        }
    }
}

impl fmt::Display for TupleSchema {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "(")?;
        let mut first = true;
        let mut sep = |f: &mut fmt::Formatter<'_>| -> fmt::Result {
            if !first {
                write!(f, ", ")?;
            }
            first = false;
            Ok(())
        };
        for item in &self.positional {
            sep(f)?;
            write!(f, "{item}")?;
        }
        for (name, entry) in &self.named.fixed {
            sep(f)?;
            let already_annotated =
                matches!(&entry.schema, ValueSchema::Seat(spec) if spec.optional);
            if entry.optional && !already_annotated {
                write!(f, "{name}: {} (optional)", entry.schema)?;
            } else {
                write!(f, "{name}: {}", entry.schema)?;
            }
        }
        if let Some((key_spec, val_schema)) = &self.named.dynamic {
            sep(f)?;
            write!(f, "<${}>: {val_schema}", key_spec.ty)?;
        }
        write!(f, ")")
    }
}

/// Extracts a [`CtmtBlueprintSchema`] from a `.blueprint.ctmt` [`File<Value>`].
///
/// Returns `None` if the document has no `::blueprint(...)` declaration.
pub fn extract_ctmt_blueprint_schema(blueprint: &File<Value>) -> Option<CtmtBlueprintSchema> {
    let mut ignored = Vec::new();
    extract_ctmt_schema_with_errors(blueprint, &mut ignored)
}

/// Validates a `.blueprint.ctmt` document itself: checks that all `TypeSeat`s decode cleanly
/// and that any referenced constructors (`ns.name`) exist in `bindings`.
pub fn validate_ctmt_blueprint(blueprint: &File<Value>, bindings: &Bindings) -> Vec<Diagnostic> {
    let mut errors = Vec::new();
    let Some(schema) = extract_ctmt_schema_with_errors(blueprint, &mut errors) else {
        return errors;
    };
    for attr in &schema.header_attrs {
        check_value_schema_bindings(&attr.value, bindings, &mut errors);
    }
    check_map_schema_bindings(&schema.body, bindings, &mut errors);
    errors
}

/// Validates a target `.ctmt` document against a `.blueprint.ctmt` file.
pub fn validate_ctmt_against_blueprint(
    doc: &File<Value>,
    blueprint: &File<Value>,
    bindings: &Bindings,
) -> Vec<Diagnostic> {
    let Some(schema) = extract_ctmt_blueprint_schema(blueprint) else {
        return Vec::new();
    };
    validate_ctmt_against_schema(doc, &schema, bindings)
}

/// Validates a target `.ctmt` document against an extracted [`CtmtBlueprintSchema`].
pub fn validate_ctmt_against_schema(
    doc: &File<Value>,
    schema: &CtmtBlueprintSchema,
    bindings: &Bindings,
) -> Vec<Diagnostic> {
    let mut errors = Vec::new();

    // 1. Validate custom header attributes.
    let mut doc_attrs: Vec<(String, Value, Span)> = Vec::new();
    for decl in &doc.header {
        if !matches!(classify_declaration_lenient(decl), ElementKind::Declaration(_)) {
            let val = declaration_value(decl);
            doc_attrs.push((decl.name.to_string(), val, decl.span));
        }
    }

    for req_attr in &schema.header_attrs {
        let matching: Vec<_> = doc_attrs
            .iter()
            .filter(|(name, _, _)| name == &req_attr.name)
            .collect();
        if matching.is_empty() {
            if !req_attr.optional {
                errors.push(Diagnostic::MissingRequiredHeaderAttr {
                    name: req_attr.name.clone(),
                    kind: schema.target_kind.clone(),
                    span: doc.span,
                });
            }
        } else {
            let path = format!("::{}", req_attr.name);
            for (_, val, attr_span) in matching {
                validate_value(
                    val,
                    &req_attr.value,
                    &path,
                    &schema.target_kind,
                    *attr_span,
                    bindings,
                    &mut errors,
                );
            }
        }
    }

    // 2. Validate body against `schema.body`.
    match &doc.body {
        Value::Tuple(tuple) => {
            validate_map(
                tuple,
                &schema.body,
                "",
                &schema.target_kind,
                doc.span,
                bindings,
                &mut errors,
            );
        }
        other => {
            errors.push(Diagnostic::InvalidBlueprintValue {
                path: "body".to_string(),
                expected: "a map".to_string(),
                value: written_value(other),
                span: doc.span,
            });
        }
    }

    errors
}

fn declaration_value(decl: &tomet_ast::Declaration) -> Value {
    if let Some(args) = &decl.data.args {
        Value::Tuple(args.clone())
    } else if let Some(group) = &decl.data.value {
        Value::Tuple(group.as_data())
    } else {
        Value::Tuple(Tuple::default())
    }
}

fn extract_ctmt_schema_with_errors(
    blueprint: &File<Value>,
    errors: &mut Vec<Diagnostic>,
) -> Option<CtmtBlueprintSchema> {
    let mut target_kind = "unknown".to_string();
    let mut found_blueprint = false;
    let mut header_attrs = Vec::new();

    for decl in &blueprint.header {
        match classify_declaration_lenient(decl) {
            ElementKind::Declaration(DeclKind::Blueprint) => {
                found_blueprint = true;
                if let Some(args) = &decl.data.args
                    && let Some(target) = args
                        .get("target")
                        .or_else(|| args.get("kind"))
                        .or_else(|| args.nth_unnamed(0))
                    && let Some(s) = target.as_str()
                {
                    target_kind = s.to_string();
                }
            }
            ElementKind::Declaration(_) => {}
            _ => {
                let raw_val = declaration_value(decl);
                let schema = extract_value_schema(&raw_val, decl.span, errors);
                let optional = match &schema {
                    ValueSchema::Seat(spec) => spec.optional || spec.default.is_some(),
                    ValueSchema::Tuple(tuple) => tuple.is_all_optional(),
                    _ => false,
                };
                header_attrs.push(HeaderAttrSchema {
                    name: decl.name.to_string(),
                    value: schema,
                    optional,
                    span: decl.span,
                });
            }
        }
    }

    if !found_blueprint {
        return None;
    }

    let body = match &blueprint.body {
        Value::Tuple(tuple) => extract_map_schema(tuple, blueprint.span, errors),
        _ => {
            errors.push(Diagnostic::InvalidTypeSeat {
                seat: written_value(&blueprint.body),
                reason: "blueprint body must be a map of entries".to_string(),
                span: blueprint.span,
            });
            MapSchema::default()
        }
    };

    Some(CtmtBlueprintSchema {
        target_kind,
        header_attrs,
        body,
    })
}

fn extract_value_schema(val: &Value, span: Span, errors: &mut Vec<Diagnostic>) -> ValueSchema {
    match val {
        Value::TypeSeat(seat) => match SeatSpec::from_type_seat(seat) {
            Ok(spec) => ValueSchema::Seat(spec),
            Err(reason) => {
                errors.push(Diagnostic::InvalidTypeSeat {
                    seat: seat.to_string(),
                    reason,
                    span: seat.span,
                });
                ValueSchema::Tuple(Box::default())
            }
        },
        Value::Tuple(tuple) => {
            ValueSchema::Tuple(Box::new(extract_tuple_schema(tuple, span, errors)))
        }
        Value::Scalar(s) => ValueSchema::Literal(s.clone()),
        Value::Call(head, args) => ValueSchema::LiteralCall {
            head: head.clone(),
            args: args.clone(),
        },
        other => {
            errors.push(Diagnostic::InvalidTypeSeat {
                seat: written_value(other),
                reason: "unsupported value in blueprint schema".to_string(),
                span,
            });
            ValueSchema::Tuple(Box::default())
        }
    }
}

fn extract_tuple_schema(tuple: &Tuple, span: Span, errors: &mut Vec<Diagnostic>) -> TupleSchema {
    let mut positional = Vec::new();
    let mut fixed = Vec::new();
    let mut dynamic: Option<(SeatSpec, ValueSchema)> = None;

    for entry in tuple.entries() {
        match entry.key() {
            Some(EntryKey::Name(name)) => {
                let val_schema = extract_value_schema(entry.value(), span, errors);
                let optional = value_schema_is_optional(&val_schema);
                fixed.push((
                    name.clone(),
                    MapEntrySchema {
                        schema: val_schema,
                        optional,
                        span,
                    },
                ));
            }
            Some(EntryKey::TypeSeat(seat)) => {
                let val_schema = extract_value_schema(entry.value(), seat.span, errors);
                match SeatSpec::from_type_seat(seat) {
                    Ok(key_spec) => {
                        if dynamic.is_some() {
                            errors.push(Diagnostic::InvalidTypeSeat {
                                seat: seat.to_string(),
                                reason: "a map schema may declare at most one dynamic key"
                                    .to_string(),
                                span: seat.span,
                            });
                        } else {
                            dynamic = Some((key_spec, val_schema));
                        }
                    }
                    Err(reason) => {
                        errors.push(Diagnostic::InvalidTypeSeat {
                            seat: seat.to_string(),
                            reason,
                            span: seat.span,
                        });
                    }
                }
            }
            None => {
                positional.push(extract_value_schema(entry.value(), span, errors));
            }
        }
    }

    TupleSchema {
        positional,
        named: MapSchema { fixed, dynamic },
    }
}

fn extract_map_schema(tuple: &Tuple, span: Span, errors: &mut Vec<Diagnostic>) -> MapSchema {
    let tuple_schema = extract_tuple_schema(tuple, span, errors);
    if !tuple_schema.positional.is_empty() {
        for item in tuple.unnamed() {
            errors.push(Diagnostic::InvalidTypeSeat {
                seat: written_value(item),
                reason: "map schema cannot contain positional entries".to_string(),
                span,
            });
        }
    }
    tuple_schema.named
}

fn check_map_schema_bindings(
    map: &MapSchema,
    bindings: &Bindings,
    errors: &mut Vec<Diagnostic>,
) {
    for (_, entry) in &map.fixed {
        check_value_schema_bindings(&entry.schema, bindings, errors);
    }
    if let Some((key_spec, val_schema)) = &map.dynamic {
        check_seat_bindings(key_spec, bindings, errors);
        check_value_schema_bindings(val_schema, bindings, errors);
    }
}

fn check_tuple_schema_bindings(
    tuple: &TupleSchema,
    bindings: &Bindings,
    errors: &mut Vec<Diagnostic>,
) {
    for item in &tuple.positional {
        check_value_schema_bindings(item, bindings, errors);
    }
    check_map_schema_bindings(&tuple.named, bindings, errors);
}

fn check_value_schema_bindings(
    schema: &ValueSchema,
    bindings: &Bindings,
    errors: &mut Vec<Diagnostic>,
) {
    match schema {
        ValueSchema::Seat(spec) => check_seat_bindings(spec, bindings, errors),
        ValueSchema::Tuple(tuple) => check_tuple_schema_bindings(tuple, bindings, errors),
        ValueSchema::Literal(_) | ValueSchema::LiteralCall { .. } => {}
    }
}

fn check_seat_bindings(spec: &SeatSpec, bindings: &Bindings, errors: &mut Vec<Diagnostic>) {
    match &spec.ty {
        SeatType::Builtin(_) => {}
        SeatType::Constructor { namespace, name } => {
            check_constructor_exists(namespace, name, spec.span, bindings, errors);
        }
        SeatType::Enum(variants) => {
            check_enum_variants_bindings(variants, spec.span, bindings, errors);
        }
        SeatType::Tuple(tuple) => check_tuple_schema_bindings(tuple, bindings, errors),
    }
}

fn check_enum_variants_bindings(
    variants: &[EnumVariant],
    span: Span,
    bindings: &Bindings,
    errors: &mut Vec<Diagnostic>,
) {
    for variant in variants {
        match variant {
            EnumVariant::Literal(_) | EnumVariant::Type(_) => {}
            EnumVariant::Constructor { namespace, name } => {
                check_constructor_exists(namespace, name, span, bindings, errors);
            }
            EnumVariant::Enum(nested) => {
                check_enum_variants_bindings(nested, span, bindings, errors);
            }
            EnumVariant::Tuple(tuple) => {
                check_tuple_schema_bindings(tuple, bindings, errors);
            }
        }
    }
}

fn check_constructor_exists(
    namespace: &str,
    name: &str,
    span: Span,
    bindings: &Bindings,
    errors: &mut Vec<Diagnostic>,
) {
    let full = format!("{namespace}.{name}");
    let Some(vocab) = bindings.vocabulary_in_scope(namespace) else {
        errors.push(Diagnostic::UnknownConstructor {
            name: full,
            unbound_namespace: Some(namespace.to_string()),
            known: Vec::new(),
            span,
        });
        return;
    };
    if !vocab.constructors.contains_key(name) {
        errors.push(Diagnostic::UnknownConstructor {
            name: full,
            unbound_namespace: None,
            known: vocab.constructors.keys().cloned().collect(),
            span,
        });
    }
}

fn validate_map(
    tuple: &Tuple,
    schema: &MapSchema,
    path: &str,
    kind: &str,
    span: Span,
    bindings: &Bindings,
    errors: &mut Vec<Diagnostic>,
) {
    // Reject unexpected positional items when the schema is a map.
    if !schema.is_empty() {
        for item in tuple.unnamed() {
            errors.push(Diagnostic::InvalidBlueprintValue {
                path: display_path(path),
                expected: "keyed entries".to_string(),
                value: written_value(item),
                span,
            });
        }
    }
    validate_map_entries(tuple, schema, path, kind, span, bindings, errors);
}

fn validate_map_entries(
    tuple: &Tuple,
    schema: &MapSchema,
    path: &str,
    kind: &str,
    span: Span,
    bindings: &Bindings,
    errors: &mut Vec<Diagnostic>,
) {
    // Check required fixed keys.
    for (req_key, entry_schema) in &schema.fixed {
        let child_path = join_path(path, req_key);
        match tuple.get(req_key) {
            Some(val) => {
                validate_value(
                    val,
                    &entry_schema.schema,
                    &child_path,
                    kind,
                    span,
                    bindings,
                    errors,
                );
            }
            None if !entry_schema.optional => {
                errors.push(Diagnostic::MissingRequiredKey {
                    key: child_path,
                    kind: kind.to_string(),
                    span,
                });
            }
            None => {}
        }
    }

    // Check remaining named entries against `dynamic` or reject as unknown.
    for (doc_key, doc_val) in tuple.named() {
        if schema.fixed.iter().any(|(k, _)| k == doc_key) {
            continue;
        }
        let child_path = join_path(path, doc_key);
        if let Some((key_spec, val_schema)) = &schema.dynamic {
            let key_scalar =
                Scalar::from_bare(doc_key).unwrap_or_else(|_| Scalar::Text(doc_key.to_string()));
            let key_val = Value::Scalar(key_scalar);
            validate_seat(
                &key_val,
                key_spec,
                &format!("{child_path} (key)"),
                kind,
                span,
                bindings,
                errors,
            );
            validate_value(doc_val, val_schema, &child_path, kind, span, bindings, errors);
        } else {
            errors.push(Diagnostic::UnknownKey {
                key: child_path,
                kind: kind.to_string(),
                span,
            });
        }
    }
}

fn validate_tuple_value(
    val: &Value,
    schema: &TupleSchema,
    path: &str,
    kind: &str,
    span: Span,
    bindings: &Bindings,
    errors: &mut Vec<Diagnostic>,
) {
    let Value::Tuple(tuple) = val else {
        let expected = if schema.is_empty() {
            "an empty tuple `()`"
        } else if schema.is_pure_map() {
            "a map"
        } else if schema.is_list() {
            "a list"
        } else {
            "a tuple"
        };
        errors.push(Diagnostic::InvalidBlueprintValue {
            path: display_path(path),
            expected: expected.to_string(),
            value: written_value(val),
            span,
        });
        return;
    };

    if schema.is_empty() {
        if !tuple.is_empty() {
            errors.push(Diagnostic::InvalidBlueprintValue {
                path: display_path(path),
                expected: "an empty tuple `()`".to_string(),
                value: written_value(val),
                span,
            });
        }
        return;
    }

    if schema.is_pure_map() {
        validate_map(tuple, &schema.named, path, kind, span, bindings, errors);
        return;
    }

    if schema.is_list() {
        if !tuple.has_no_named() {
            errors.push(Diagnostic::InvalidBlueprintValue {
                path: display_path(path),
                expected: "a list".to_string(),
                value: written_value(val),
                span,
            });
            return;
        }
        for (i, item) in tuple.values().enumerate() {
            let item_path = format!("{}[{i}]", display_path(path));
            validate_value(
                item,
                &schema.positional[0],
                &item_path,
                kind,
                span,
                bindings,
                errors,
            );
        }
        return;
    }

    let (min_pos, max_pos) = schema.positional_arity();
    if schema.named.is_empty() && !tuple.has_no_named() {
        errors.push(Diagnostic::InvalidBlueprintValue {
            path: display_path(path),
            expected: positional_arity_expected(min_pos, max_pos, false),
            value: written_value(val),
            span,
        });
        return;
    }

    let unnamed: Vec<&Value> = tuple.unnamed().collect();
    if unnamed.len() < min_pos || unnamed.len() > max_pos {
        errors.push(Diagnostic::InvalidBlueprintValue {
            path: display_path(path),
            expected: positional_arity_expected(min_pos, max_pos, !schema.named.is_empty()),
            value: written_value(val),
            span,
        });
    } else {
        for (i, (item, item_schema)) in unnamed.iter().zip(schema.positional.iter()).enumerate() {
            let item_path = format!("{}[{i}]", display_path(path));
            validate_value(item, item_schema, &item_path, kind, span, bindings, errors);
        }
    }

    if !schema.named.is_empty() {
        validate_map_entries(tuple, &schema.named, path, kind, span, bindings, errors);
    }
}

fn positional_arity_expected(min: usize, max: usize, has_named: bool) -> String {
    if min == max {
        if has_named {
            let noun = if max == 1 { "item" } else { "items" };
            format!("{max} positional {noun}")
        } else {
            format!("a tuple of {max} items")
        }
    } else if has_named {
        format!("{min}..={max} positional items")
    } else {
        format!("a tuple of {min}..={max} items")
    }
}

pub(crate) fn value_matches_tuple(
    val: &Value,
    schema: &TupleSchema,
    path: &str,
    kind: &str,
    span: Span,
    bindings: &Bindings,
) -> bool {
    let mut scratch = Vec::new();
    validate_tuple_value(val, schema, path, kind, span, bindings, &mut scratch);
    scratch.is_empty()
}

fn validate_value(
    val: &Value,
    schema: &ValueSchema,
    path: &str,
    kind: &str,
    span: Span,
    bindings: &Bindings,
    errors: &mut Vec<Diagnostic>,
) {
    if tomet_semantics_core::is_placeholder(val) {
        return;
    }
    match schema {
        ValueSchema::Seat(spec) => validate_seat(val, spec, path, kind, span, bindings, errors),
        ValueSchema::Tuple(tuple_schema) => {
            validate_tuple_value(val, tuple_schema, path, kind, span, bindings, errors);
        }
        ValueSchema::Literal(expected) => {
            if !matches!(val, Value::Scalar(s) if s == expected) {
                errors.push(Diagnostic::InvalidBlueprintValue {
                    path: display_path(path),
                    expected: format!("`{}`", scalar_spelling(expected)),
                    value: written_value(val),
                    span,
                });
            }
        }
        ValueSchema::LiteralCall { head, args } => {
            if !matches!(val, Value::Call(h, a) if h == head && a == args) {
                errors.push(Diagnostic::InvalidBlueprintValue {
                    path: display_path(path),
                    expected: format!("`{head}(...)`"),
                    value: written_value(val),
                    span,
                });
            }
        }
    }
}

fn validate_seat(
    val: &Value,
    spec: &SeatSpec,
    path: &str,
    kind: &str,
    span: Span,
    bindings: &Bindings,
    errors: &mut Vec<Diagnostic>,
) {
    if tomet_semantics_core::is_placeholder(val) {
        return;
    }
    let conforms = match &spec.ty {
        SeatType::Builtin(ty) => matches_builtin_type(ty, val),
        SeatType::Constructor { namespace, name } => matches!(
            val,
            Value::Call(head, _) if matches_constructor(head, namespace, name, bindings)
        ),
        SeatType::Enum(variants) => variants
            .iter()
            .any(|variant| matches_enum_variant(val, variant, path, kind, span, bindings)),
        SeatType::Tuple(tuple_schema) => {
            validate_tuple_value(val, tuple_schema, path, kind, span, bindings, errors);
            return;
        }
    };
    if !conforms {
        errors.push(Diagnostic::InvalidBlueprintValue {
            path: display_path(path),
            expected: spec.ty.to_string(),
            value: written_value(val),
            span,
        });
    }
}

fn matches_enum_variant(
    val: &Value,
    variant: &EnumVariant,
    path: &str,
    kind: &str,
    span: Span,
    bindings: &Bindings,
) -> bool {
    match variant {
        EnumVariant::Literal(expected) => matches!(val, Value::Scalar(s) if s == expected),
        EnumVariant::Constructor { namespace, name } => matches!(
            val,
            Value::Call(head, _) if matches_constructor(head, namespace, name, bindings)
        ),
        EnumVariant::Type(ty) => matches_builtin_type(ty, val),
        EnumVariant::Enum(nested) => nested
            .iter()
            .any(|v| matches_enum_variant(val, v, path, kind, span, bindings)),
        EnumVariant::Tuple(tuple_schema) => {
            value_matches_tuple(val, tuple_schema, path, kind, span, bindings)
        }
    }
}

fn matches_builtin_type(ty: &Type, val: &Value) -> bool {
    if let Some(id_spec) = tomet_id::IdSpec::new(ty) {
        val.as_scalar()
            .and_then(|s| s.text())
            .is_some_and(|text| id_spec.accepts(&text))
    } else {
        tomet_semantics_core::admits(ty, val).is_ok()
    }
}

fn matches_constructor(head: &str, namespace: &str, name: &str, bindings: &Bindings) -> bool {
    if let Some((written_ns, written_name)) = head.split_once('.') {
        if written_name != name {
            return false;
        }
        if written_ns == namespace {
            return true;
        }
        bindings
            .aliases
            .get(written_ns)
            .is_some_and(|resolved| resolved == namespace)
    } else {
        bindings
            .kind
            .as_ref()
            .is_some_and(|vocab| vocab.namespace == namespace)
            && head == name
    }
}

fn join_path(prefix: &str, key: &str) -> String {
    if prefix.is_empty() {
        key.to_string()
    } else {
        format!("{prefix}.{key}")
    }
}

fn display_path(path: &str) -> String {
    if path.is_empty() {
        "body".to_string()
    } else {
        path.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_semantics::Vocabulary;

    fn parse_ctmt(src: &str) -> File<Value> {
        tomet_ast_parser::parse_data_file(src).expect("valid .ctmt")
    }

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
)
"#,
        )
        .expect("valid vocabulary");
        Vocabulary::from_file(&doc).expect("vocabulary")
    }

    #[test]
    fn seat_spec_decodes_builtin_optional_constructor_and_enum() {
        let bp = parse_ctmt(
            r#"::kind(blueprint)
::blueprint(demo)

req_str: <$string>
opt_str: <$string(optional: true)>
def_int: <$int(default: 7)>
opt_list: <$list(string, optional: true)>
ctor: <$catasto.git>
opt_ctor: <$catasto.git(optional: true)>
mode: <$enum(catasto.upstream, "follow", "copy", int, optional: true)>
"#,
        );
        let schema = extract_ctmt_blueprint_schema(&bp).expect("schema");
        assert_eq!(schema.target_kind, "demo");
        assert_eq!(schema.body.fixed.len(), 7);

        assert!(!schema.body.fixed[0].1.optional);
        assert!(schema.body.fixed[1].1.optional);
        assert!(schema.body.fixed[2].1.optional); // has default
        assert!(schema.body.fixed[3].1.optional);
        assert!(!schema.body.fixed[4].1.optional);
        assert!(schema.body.fixed[5].1.optional);
        assert!(schema.body.fixed[6].1.optional);

        assert!(matches!(
            &schema.body.fixed[4].1.schema,
            ValueSchema::Seat(SeatSpec {
                ty: SeatType::Constructor { namespace, name },
                optional: false,
                ..
            }) if namespace == "catasto" && name == "git"
        ));

        assert!(matches!(
            &schema.body.fixed[6].1.schema,
            ValueSchema::Seat(SeatSpec {
                ty: SeatType::Enum(variants),
                optional: true,
                ..
            }) if variants.len() == 4
        ));
    }

    #[test]
    fn validates_catasto_style_blueprint_and_target_data() {
        let vocab = catasto_vocab();
        let bp = parse_ctmt(
            r#"::kind(blueprint)
::blueprint(catasto)
::use(catasto)

::upstream(<$string>: <$catasto.git>)

<$string>: <$enum(catasto.upstream, "follow", "copy")>
"#,
        );
        let bp_bindings = Bindings::for_document(&bp, [vocab.clone()]);
        let bp_errors = validate_ctmt_blueprint(&bp, &bp_bindings);
        assert!(bp_errors.is_empty(), "blueprint errors: {bp_errors:?}");

        let doc = parse_ctmt(
            r#"::kind(catasto)
::use(catasto)

::upstream(
  commonfiles: catasto.git("https://github.com/tomet-lang/commonfiles.git", rev: main)
)

".editorconfig": catasto.upstream(commonfiles, path: ".editorconfig")
"LICENSE": "follow"
"#,
        );
        let doc_bindings = Bindings::for_document(&doc, [vocab]);
        let errors = validate_ctmt_against_blueprint(&doc, &bp, &doc_bindings);
        assert!(errors.is_empty(), "expected no errors, got: {errors:?}");
    }

    #[test]
    fn blueprint_self_check_reports_unknown_constructors_and_bad_seats() {
        let vocab = catasto_vocab();

        let bad_ns = parse_ctmt(
            r#"::kind(blueprint)
::blueprint(catasto)

repo: <$missing.git>
"#,
        );
        let bindings = Bindings::for_document(&bad_ns, [vocab.clone()]);
        let errs = validate_ctmt_blueprint(&bad_ns, &bindings);
        assert!(
            matches!(
                errs.as_slice(),
                [Diagnostic::UnknownConstructor { unbound_namespace: Some(ns), .. }] if ns == "missing"
            ),
            "{errs:?}"
        );

        let bad_ctor = parse_ctmt(
            r#"::kind(blueprint)
::blueprint(catasto)
::use(catasto)

repo: <$catasto.svn>
"#,
        );
        let bindings = Bindings::for_document(&bad_ctor, [vocab.clone()]);
        let errs = validate_ctmt_blueprint(&bad_ctor, &bindings);
        assert!(
            matches!(
                errs.as_slice(),
                [Diagnostic::UnknownConstructor { unbound_namespace: None, name, .. }] if name == "catasto.svn"
            ),
            "{errs:?}"
        );

        let bad_seat = parse_ctmt(
            r#"::kind(blueprint)
::blueprint(catasto)

mode: <$enum()>
"#,
        );
        let bindings = Bindings::for_document(&bad_seat, [vocab]);
        let errs = validate_ctmt_blueprint(&bad_seat, &bindings);
        assert!(
            matches!(errs.as_slice(), [Diagnostic::InvalidTypeSeat { .. }]),
            "{errs:?}"
        );
    }

    #[test]
    fn reports_missing_header_attr_and_invalid_attr_value() {
        let vocab = catasto_vocab();
        let bp = parse_ctmt(
            r#"::kind(blueprint)
::blueprint(catasto)
::use(catasto)

::upstream(<$string>: <$catasto.git>)

<$string>: <$string>
"#,
        );

        let missing = parse_ctmt(
            r#"::kind(catasto)
::use(catasto)

foo: "bar"
"#,
        );
        let bindings = Bindings::for_document(&missing, [vocab.clone()]);
        let errs = validate_ctmt_against_blueprint(&missing, &bp, &bindings);
        assert!(
            matches!(
                errs.as_slice(),
                [Diagnostic::MissingRequiredHeaderAttr { name, kind, .. }]
                if name == "upstream" && kind == "catasto"
            ),
            "{errs:?}"
        );

        let bad_val = parse_ctmt(
            r#"::kind(catasto)
::use(catasto)
::upstream(commonfiles: "not-a-git-call")

foo: "bar"
"#,
        );
        let bindings = Bindings::for_document(&bad_val, [vocab]);
        let errs = validate_ctmt_against_blueprint(&bad_val, &bp, &bindings);
        assert!(
            matches!(
                errs.as_slice(),
                [Diagnostic::InvalidBlueprintValue { path, .. }] if path == "::upstream.commonfiles"
            ),
            "{errs:?}"
        );
    }

    #[test]
    fn optional_header_attr_may_be_omitted() {
        let vocab = catasto_vocab();
        let bp = parse_ctmt(
            r#"::kind(blueprint)
::blueprint(catasto)
::use(catasto)

::upstream(<$string(optional: true)>: <$catasto.git>)

<$string>: <$string>
"#,
        );
        let doc = parse_ctmt("::kind(catasto)\n\nfoo: \"bar\"\n");
        let bindings = Bindings::for_document(&doc, [vocab]);
        let errs = validate_ctmt_against_blueprint(&doc, &bp, &bindings);
        assert!(errs.is_empty(), "{errs:?}");
    }

    #[test]
    fn closed_map_rejects_unknown_and_missing_keys_and_invalid_enum() {
        let vocab = catasto_vocab();
        let bp = parse_ctmt(
            r#"::kind(blueprint)
::blueprint(project)
::use(catasto)

name: <$string>
mode: <$enum("follow", "copy")>
note: <$string(optional: true)>
"#,
        );

        let bad = parse_ctmt(
            r#"::kind(project)

mode: "invalid"
extra: 123
"#,
        );
        let bindings = Bindings::for_document(&bad, [vocab]);
        let errs = validate_ctmt_against_blueprint(&bad, &bp, &bindings);
        assert_eq!(errs.len(), 3, "{errs:?}");
        assert!(matches!(
            &errs[0],
            Diagnostic::MissingRequiredKey { key, kind, .. } if key == "name" && kind == "project"
        ));
        assert!(matches!(
            &errs[1],
            Diagnostic::InvalidBlueprintValue { path, .. } if path == "mode"
        ));
        assert!(matches!(
            &errs[2],
            Diagnostic::UnknownKey { key, kind, .. } if key == "extra" && kind == "project"
        ));
    }

    #[test]
    fn nested_fixed_and_dynamic_keys_and_lists_and_typed_dynamic_keys() {
        let bp = parse_ctmt(
            r#"::kind(blueprint)
::blueprint(nested)

tags: (<$int>)
repo: (
  url: <$string>
  rev: <$string(optional: true)>
  <$string>: <$int>
)
ports: (
  <$int>: <$string>
)
"#,
        );

        let ok = parse_ctmt(
            r#"::kind(nested)

tags: (1, 2)
repo: (
  url: "https://example.com"
  stars: 10
)
ports: (
  8080: "http"
)
"#,
        );
        let bindings = Bindings::default();
        let errs = validate_ctmt_against_blueprint(&ok, &bp, &bindings);
        assert!(errs.is_empty(), "{errs:?}");

        let bad = parse_ctmt(
            r#"::kind(nested)

tags: (1, "two")
repo: (
  stars: "not-int"
)
ports: (
  not_an_int: "http"
)
"#,
        );
        let errs = validate_ctmt_against_blueprint(&bad, &bp, &bindings);
        assert_eq!(errs.len(), 4, "{errs:?}");
        assert!(matches!(
            &errs[0],
            Diagnostic::InvalidBlueprintValue { path, .. } if path == "tags[1]"
        ));
        assert!(matches!(
            &errs[1],
            Diagnostic::MissingRequiredKey { key, .. } if key == "repo.url"
        ));
        assert!(matches!(
            &errs[2],
            Diagnostic::InvalidBlueprintValue { path, .. } if path == "repo.stars"
        ));
        assert!(matches!(
            &errs[3],
            Diagnostic::InvalidBlueprintValue { path, .. } if path == "ports.not_an_int (key)"
        ));
    }

    fn catasto_extended_vocab() -> Vocabulary {
        let doc = tomet_ast_parser::parse_file(
            r#"::kind(vocabulary)
::vocabulary(catasto)

::constructor(
  git: (
    args: (
      url: (type: string, required: true, positional: 0)
      rev: (type: string)
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

    #[test]
    fn validates_mixed_positional_and_named_tuple_in_value_position() {
        let vocab = catasto_extended_vocab();
        let bp = parse_ctmt(
            r#"::kind(blueprint)
::blueprint(catasto)
::use(catasto)

opt_exec: (<$catasto.git>, executable: <$bool(optional: true)>)
req_exec: (<$catasto.git>, executable: <$bool>)
tags: (<$string>)
empty: ()
"#,
        );

        let ok = parse_ctmt(
            r#"::kind(catasto)
::use(catasto)

opt_exec: (catasto.git("https://example.com/a.git"), executable: true)
req_exec: (catasto.git("https://example.com/b.git"), executable: false)
tags: ("a", "b")
empty: ()
"#,
        );
        let bindings = Bindings::for_document(&ok, [vocab.clone()]);
        let errs = validate_ctmt_against_blueprint(&ok, &bp, &bindings);
        assert!(errs.is_empty(), "{errs:?}");

        // Optional named key omitted on mixed tuple.
        let ok_omitted = parse_ctmt(
            r#"::kind(catasto)
::use(catasto)

opt_exec: (catasto.git("https://example.com/a.git"),)
req_exec: (catasto.git("https://example.com/b.git"), executable: true)
tags: ()
empty: ()
"#,
        );
        let errs = validate_ctmt_against_blueprint(&ok_omitted, &bp, &bindings);
        assert!(errs.is_empty(), "{errs:?}");

        // Errors: missing required key, unknown key, wrong positional type, wrong element in list, non-empty for empty tuple.
        let bad = parse_ctmt(
            r#"::kind(catasto)
::use(catasto)

opt_exec: ("not-git", extra: true)
req_exec: (catasto.git("https://example.com/b.git"),)
tags: (none)
empty: ("non-empty")
"#,
        );
        let errs = validate_ctmt_against_blueprint(&bad, &bp, &bindings);
        assert_eq!(errs.len(), 5, "{errs:?}");
        assert!(matches!(
            &errs[0],
            Diagnostic::InvalidBlueprintValue { path, .. } if path == "opt_exec[0]"
        ));
        assert!(matches!(
            &errs[1],
            Diagnostic::UnknownKey { key, .. } if key == "opt_exec.extra"
        ));
        assert!(matches!(
            &errs[2],
            Diagnostic::MissingRequiredKey { key, .. } if key == "req_exec.executable"
        ));
        assert!(matches!(
            &errs[3],
            Diagnostic::InvalidBlueprintValue { path, .. } if path == "tags[0]"
        ));
        assert!(matches!(
            &errs[4],
            Diagnostic::InvalidBlueprintValue { path, .. } if path == "empty"
        ));

        let bad_count_and_named_type = parse_ctmt(
            r#"::kind(catasto)
::use(catasto)

opt_exec: (catasto.git("https://example.com/a.git"), "extra-pos", executable: "yes")
req_exec: (catasto.git("https://example.com/b.git"), executable: true)
tags: ("a")
empty: ()
"#,
        );
        let errs = validate_ctmt_against_blueprint(&bad_count_and_named_type, &bp, &bindings);
        assert_eq!(errs.len(), 2, "{errs:?}");
        assert!(matches!(
            &errs[0],
            Diagnostic::InvalidBlueprintValue { path, .. } if path == "opt_exec"
        ));
        assert!(matches!(
            &errs[1],
            Diagnostic::InvalidBlueprintValue { path, .. } if path == "opt_exec.executable"
        ));
    }

    #[test]
    fn validates_nested_enum_and_seq_tuple_inside_enum() {
        let vocab = catasto_extended_vocab();
        let bp = parse_ctmt(
            r#"::kind(blueprint)
::blueprint(catasto)
::use(catasto)

<$string>: <$enum(
  catasto.repo,
  enum(catasto.git, catasto.upstream, catasto.local),
  (enum(catasto.git, catasto.upstream, catasto.local), executable: bool(optional: true))
)>
"#,
        );
        let bp_bindings = Bindings::for_document(&bp, [vocab.clone()]);
        let bp_errs = validate_ctmt_blueprint(&bp, &bp_bindings);
        assert!(bp_errs.is_empty(), "blueprint errors: {bp_errs:?}");

        let ok = parse_ctmt(
            r#"::kind(catasto)
::use(catasto)

repo_entry: catasto.repo("vendor/foo")
upstream_entry: catasto.upstream(commonfiles, path: ".editorconfig")
local_entry: catasto.local("shared/base")
tuple_with_exec: (catasto.upstream(commonfiles, path: "bin/run"), executable: true)
tuple_without_exec: (catasto.local("bin/tool"),)
"#,
        );
        let doc_bindings = Bindings::for_document(&ok, [vocab]);
        let errs = validate_ctmt_against_blueprint(&ok, &bp, &doc_bindings);
        assert!(errs.is_empty(), "expected no errors, got: {errs:?}");

        let bad = parse_ctmt(
            r#"::kind(catasto)
::use(catasto)

bad_first_slot: (catasto.repo("vendor/foo"), executable: true)
bad_unknown_key: (catasto.upstream(commonfiles, path: "a"), unknown: true)
bad_exec_type: (catasto.upstream(commonfiles, path: "a"), executable: "yes")
"#,
        );
        let errs = validate_ctmt_against_blueprint(&bad, &bp, &doc_bindings);
        assert_eq!(errs.len(), 3, "{errs:?}");
        assert!(matches!(
            &errs[0],
            Diagnostic::InvalidBlueprintValue { path, .. } if path == "bad_first_slot"
        ));
        assert!(matches!(
            &errs[1],
            Diagnostic::InvalidBlueprintValue { path, .. } if path == "bad_unknown_key"
        ));
        assert!(matches!(
            &errs[2],
            Diagnostic::InvalidBlueprintValue { path, .. } if path == "bad_exec_type"
        ));
    }

    #[test]
    fn validates_required_named_key_inside_enum_seq_tuple() {
        let vocab = catasto_extended_vocab();
        let bp = parse_ctmt(
            r#"::kind(blueprint)
::blueprint(catasto)
::use(catasto)

entry: <$enum(catasto.repo, (catasto.upstream, executable: bool))>
"#,
        );

        let ok = parse_ctmt(
            r#"::kind(catasto)
::use(catasto)

entry: (catasto.upstream(commonfiles, path: "bin/run"), executable: true)
"#,
        );
        let bindings = Bindings::for_document(&ok, [vocab]);
        assert!(validate_ctmt_against_blueprint(&ok, &bp, &bindings).is_empty());

        let missing_req = parse_ctmt(
            r#"::kind(catasto)
::use(catasto)

entry: (catasto.upstream(commonfiles, path: "bin/run"),)
"#,
        );
        let errs = validate_ctmt_against_blueprint(&missing_req, &bp, &bindings);
        assert!(
            matches!(
                errs.as_slice(),
                [Diagnostic::InvalidBlueprintValue { path, .. }] if path == "entry"
            ),
            "{errs:?}"
        );
    }

    #[test]
    fn validates_zone_style_optional_enum_of_string_or_list_of_string() {
        let bp = parse_ctmt(
            r#"::kind(blueprint)
::blueprint(zone)

zones: <$enum(string, list(string), optional: true)>
"#,
        );
        let bindings = Bindings::default();

        let omitted = parse_ctmt("::kind(zone)\n\n");
        assert!(validate_ctmt_against_blueprint(&omitted, &bp, &bindings).is_empty());

        let as_str = parse_ctmt("::kind(zone)\n\nzones: \"work\"\n");
        assert!(validate_ctmt_against_blueprint(&as_str, &bp, &bindings).is_empty());

        let as_list = parse_ctmt("::kind(zone)\n\nzones: (\"work\", \"home\")\n");
        assert!(validate_ctmt_against_blueprint(&as_list, &bp, &bindings).is_empty());

        let invalid = parse_ctmt("::kind(zone)\n\nzones: (\"work\", none)\n");
        let errs = validate_ctmt_against_blueprint(&invalid, &bp, &bindings);
        assert!(
            matches!(
                errs.as_slice(),
                [Diagnostic::InvalidBlueprintValue { path, .. }] if path == "zones"
            ),
            "{errs:?}"
        );
    }

    #[test]
    fn blueprint_self_check_catches_unknown_constructors_inside_nested_enum_and_seq() {
        let vocab = catasto_extended_vocab();
        let bp = parse_ctmt(
            r#"::kind(blueprint)
::blueprint(catasto)
::use(catasto)

entry: <$enum(
  catasto.repo,
  enum(catasto.git, catasto.missing_nested),
  (enum(catasto.upstream, missing_ns.ctor), flag: catasto.missing_named(optional: true))
)>
"#,
        );
        let bindings = Bindings::for_document(&bp, [vocab]);
        let errs = validate_ctmt_blueprint(&bp, &bindings);
        assert_eq!(errs.len(), 3, "{errs:?}");
        assert!(matches!(
            &errs[0],
            Diagnostic::UnknownConstructor { name, unbound_namespace: None, .. }
            if name == "catasto.missing_nested"
        ));
        assert!(matches!(
            &errs[1],
            Diagnostic::UnknownConstructor { name, unbound_namespace: Some(ns), .. }
            if name == "missing_ns.ctor" && ns == "missing_ns"
        ));
        assert!(matches!(
            &errs[2],
            Diagnostic::UnknownConstructor { name, unbound_namespace: None, .. }
            if name == "catasto.missing_named"
        ));
    }
}
