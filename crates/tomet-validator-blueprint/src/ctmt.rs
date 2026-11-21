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

/// Schema for a cotomet map (`(...)` or top-level `.ctmt` body).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MapSchema {
    pub fixed: Vec<(String, MapEntrySchema)>,
    pub dynamic: Option<(SeatSpec, ValueSchema)>,
}

impl MapSchema {
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

/// Recursive schema for any cotomet value in a `.blueprint.ctmt` document.
#[derive(Debug, Clone, PartialEq)]
pub enum ValueSchema {
    /// A `<$...>` type seat constraint.
    Seat(SeatSpec),
    /// A map with fixed and/or dynamic keys.
    Map(Box<MapSchema>),
    /// A positional tuple or homogeneous list:
    /// - empty `()` matches `()`
    /// - 1 element `(S)` matches a list of 0+ elements each matching `S`
    /// - N elements `(S1, S2, ...)` matches an N-tuple positionally
    List(Vec<ValueSchema>),
    /// A literal scalar constant.
    Literal(Scalar),
    /// A literal constructor call written directly outside a `<$...>` seat.
    LiteralCall { head: String, args: Tuple },
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
                    ValueSchema::Map(map) => map.is_all_optional(),
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
                ValueSchema::List(Vec::new())
            }
        },
        Value::Tuple(tuple) => {
            let has_keyed = tuple.entries().any(|e| e.key().is_some());
            if has_keyed {
                ValueSchema::Map(Box::new(extract_map_schema(tuple, span, errors)))
            } else {
                let items = tuple
                    .values()
                    .map(|item| extract_value_schema(item, span, errors))
                    .collect();
                ValueSchema::List(items)
            }
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
            ValueSchema::List(Vec::new())
        }
    }
}

fn extract_map_schema(tuple: &Tuple, span: Span, errors: &mut Vec<Diagnostic>) -> MapSchema {
    let mut fixed = Vec::new();
    let mut dynamic: Option<(SeatSpec, ValueSchema)> = None;

    for entry in tuple.entries() {
        match entry.key() {
            Some(EntryKey::Name(name)) => {
                let val_schema = extract_value_schema(entry.value(), span, errors);
                let optional = matches!(
                    &val_schema,
                    ValueSchema::Seat(spec) if spec.optional || spec.default.is_some()
                );
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
                errors.push(Diagnostic::InvalidTypeSeat {
                    seat: written_value(entry.value()),
                    reason: "map schema cannot mix positional and keyed entries".to_string(),
                    span,
                });
            }
        }
    }

    MapSchema { fixed, dynamic }
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

fn check_value_schema_bindings(
    schema: &ValueSchema,
    bindings: &Bindings,
    errors: &mut Vec<Diagnostic>,
) {
    match schema {
        ValueSchema::Seat(spec) => check_seat_bindings(spec, bindings, errors),
        ValueSchema::Map(map) => check_map_schema_bindings(map, bindings, errors),
        ValueSchema::List(items) => {
            for item in items {
                check_value_schema_bindings(item, bindings, errors);
            }
        }
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
            for variant in variants {
                if let EnumVariant::Constructor { namespace, name } = variant {
                    check_constructor_exists(namespace, name, spec.span, bindings, errors);
                }
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
    if !schema.fixed.is_empty() || schema.dynamic.is_some() {
        for item in tuple.unnamed() {
            errors.push(Diagnostic::InvalidBlueprintValue {
                path: if path.is_empty() {
                    "body".to_string()
                } else {
                    path.to_string()
                },
                expected: "keyed entries".to_string(),
                value: written_value(item),
                span,
            });
        }
    }

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
        ValueSchema::Seat(spec) => validate_seat(val, spec, path, span, bindings, errors),
        ValueSchema::Map(map_schema) => match val {
            Value::Tuple(tuple) => {
                validate_map(tuple, map_schema, path, kind, span, bindings, errors);
            }
            other => {
                errors.push(Diagnostic::InvalidBlueprintValue {
                    path: display_path(path),
                    expected: "a map".to_string(),
                    value: written_value(other),
                    span,
                });
            }
        },
        ValueSchema::List(elem_schemas) => match val {
            Value::Tuple(tuple) if tuple.has_no_named() => {
                if elem_schemas.is_empty() {
                    if !tuple.is_empty() {
                        errors.push(Diagnostic::InvalidBlueprintValue {
                            path: display_path(path),
                            expected: "an empty tuple `()`".to_string(),
                            value: written_value(val),
                            span,
                        });
                    }
                } else if elem_schemas.len() == 1 {
                    for (i, item) in tuple.values().enumerate() {
                        let item_path = format!("{}[{i}]", display_path(path));
                        validate_value(
                            item,
                            &elem_schemas[0],
                            &item_path,
                            kind,
                            span,
                            bindings,
                            errors,
                        );
                    }
                } else if tuple.len() != elem_schemas.len() {
                    errors.push(Diagnostic::InvalidBlueprintValue {
                        path: display_path(path),
                        expected: format!("a tuple of {} items", elem_schemas.len()),
                        value: written_value(val),
                        span,
                    });
                } else {
                    for (i, (item, item_schema)) in
                        tuple.values().zip(elem_schemas.iter()).enumerate()
                    {
                        let item_path = format!("{}[{i}]", display_path(path));
                        validate_value(item, item_schema, &item_path, kind, span, bindings, errors);
                    }
                }
            }
            other => {
                errors.push(Diagnostic::InvalidBlueprintValue {
                    path: display_path(path),
                    expected: "a list".to_string(),
                    value: written_value(other),
                    span,
                });
            }
        },
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
        SeatType::Enum(variants) => variants.iter().any(|variant| match variant {
            EnumVariant::Literal(expected) => matches!(val, Value::Scalar(s) if s == expected),
            EnumVariant::Constructor { namespace, name } => matches!(
                val,
                Value::Call(head, _) if matches_constructor(head, namespace, name, bindings)
            ),
            EnumVariant::Type(ty) => matches_builtin_type(ty, val),
        }),
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
}
