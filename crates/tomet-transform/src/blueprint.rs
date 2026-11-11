//! Blueprint schema extraction, transformation, and instantiation.

use tomet_ast::{Block, File, Inline, Name, Section, Value};
use tomet_expr::{EvalContext, evaluate_type};
use tomet_id::IdSpec;
use tomet_semantics::DeclKind;
use tomet_semantics::{ElementKind, classify_declaration_lenient, normalized_declaration_args};
use tomet_semantics_core::admits;
use tomet_tree::{
    DataExt, ValueExt, declaration_new, for_each_declaration, for_each_declaration_mut,
};
use tomet_type::Type;

use crate::expressions::{Unresolved, resolve_expressions};
use crate::seats::{SeatProblem, fill_seats, seats_left};
use crate::source::Source;

/// What a `::blueprint` directive says about the blueprint. Its variables are
/// not here: the blueprint declares them with `::var`.
#[derive(Debug, Clone, PartialEq)]
pub struct BlueprintInfo {
    pub target_kind: String,
    pub description: Option<String>,
}

/// Extracts [`BlueprintInfo`] from `doc` if it declares a top-level `::blueprint` directive.
pub fn extract_blueprint_info(doc: &File) -> Option<BlueprintInfo> {
    let mut result = None;
    for_each_declaration(doc, |declaration| {
        if result.is_some()
            || classify_declaration_lenient(declaration)
                != ElementKind::Declaration(DeclKind::Blueprint)
        {
            return;
        }
        let target_kind = match &declaration.data.args {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Map(entries)) => entries
                .iter()
                .find(|(k, _)| k == "target" || k == "kind")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            _ => "unknown".to_string(),
        };

        let mut description = None;

        if let Some(entries) = declaration
            .data
            .value
            .as_ref()
            .map(|v| v.pairs().collect::<Vec<_>>())
        {
            for (k, v) in entries {
                if k == "description" {
                    description = v.as_str().map(String::from);
                }
            }
        }

        result = Some(BlueprintInfo {
            target_kind,
            description,
        });
    });
    result
}

/// What [`instantiate_blueprint`] did.
#[derive(Debug, Default)]
pub struct Instantiated {
    /// Whether the document differs from the blueprint it started as.
    pub changed: bool,
    /// Expressions that could not be resolved. They are left in the
    /// document as written, so the person who runs `tomet new` can see them.
    pub unresolved: Vec<Unresolved>,
}

/// Instantiates a blueprint `doc` in-place by:
/// 1. Filling every type seat (`<$type(...)>`) from its source: `default: auto` from `source`, a
///    `default` written out as itself. A seat with no source stops the instantiation.
/// 2. Promoting/converting `::blueprint(target)` to `::kind(target)` and stripping schema blocks.
/// 3. Resolving every `{$ ... }` in it, with `cx`.
///
/// A document made from a blueprint holds no type seat. Every seat that cannot be filled -- it is
/// not a type, its `default` is not of its type, it has no `default`, it stands in running text and
/// its value has none -- comes back as a [`SeatProblem`], all of them and not the first, and then `doc`
/// is left as it was: nothing is made. An expression with no value is no such stop; it stays in the
/// document as written and is reported in [`Instantiated::unresolved`].
///
/// `source` is the clock and the random source `default: auto` is made from; nothing else here reads
/// either.
pub fn instantiate_blueprint(
    doc: &mut File,
    cx: &EvalContext,
    source: &dyn Source,
) -> Result<Instantiated, Vec<SeatProblem>> {
    let filled = fill_seats(doc, cx, source)?;
    let mut changed = filled > 0;

    // Step 0: drop the blueprint's own `::kind(blueprint)`.
    //
    // It says what the *blueprint* is, and the blueprint is what stops
    // existing here. Leaving it produces a document with two kinds --
    // `::kind(blueprint)` from the source and `::kind(daily-note)` from the
    // promotion below -- which is what running `tomet new` for the first
    // time after the shape was decided actually produced.
    let mut dropped_own_kind = false;
    doc.header.retain(|declaration| {
        if classify_declaration_lenient(declaration) != ElementKind::Declaration(DeclKind::Kind) {
            return true;
        }
        let is_blueprint_kind = normalized_declaration_args(declaration)
            .as_ref()
            .and_then(|args| args.get("kind"))
            .and_then(|v| v.as_str())
            == Some("blueprint");
        if is_blueprint_kind {
            dropped_own_kind = true;
        }
        !is_blueprint_kind
    });
    changed |= dropped_own_kind;

    // The blueprint's `::var` are its inputs. They have been given their values
    // (by `tomet-workspace`), and a document made from it has none to ask for.
    let mut dropped_vars = false;
    doc.header.retain(|declaration| {
        let is_vars =
            classify_declaration_lenient(declaration) == ElementKind::Declaration(DeclKind::Var);
        dropped_vars |= is_vars;
        !is_vars
    });
    changed |= dropped_vars;

    // Step 1: Transform ::blueprint -> ::kind
    for_each_declaration_mut(doc, |declaration| {
        if classify_declaration_lenient(declaration)
            == ElementKind::Declaration(DeclKind::Blueprint)
        {
            // The target is `::blueprint`'s positional argument, which
            // `tomet-semantics::positional` normalizes to `target`.
            // That is the one spelling: a spelled-out `target:` and a
            // `kind:` would be three spellings for one thing.
            let target_kind = normalized_declaration_args(declaration)
                .as_ref()
                .and_then(|args| args.get("target"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();

            declaration.name = Name::bare("kind");
            declaration.data.args = Some(Value::String(target_kind));
            // `{version, description}` describes the blueprint, not the
            // document it produces, so it does not survive promotion.
            declaration.data.value = None;
            changed = true;
        }
    });

    let promoted = doc.clone();
    let unresolved = resolve_expressions(doc, cx);
    changed |= *doc != promoted;

    // The seats are all filled by now. One that is still here -- brought in by an expression, say
    // -- would be a seat in a document, which is no document of a blueprint's making.
    let left = seats_left(doc);
    if !left.is_empty() {
        return Err(left);
    }

    Ok(Instantiated {
        changed,
        unresolved,
    })
}

/// The outcome of applying a blueprint to an existing document.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ApplyOutcome {
    /// Whether the document was modified.
    pub changed: bool,
    /// Expressions in the applied blueprint elements that could not be resolved.
    pub unresolved: Vec<Unresolved>,
}

/// Checks whether a document value satisfies a required type.
pub fn value_satisfies_type(val: &Value, ty: &Type) -> bool {
    if let Some(spec) = IdSpec::new(ty) {
        val.scalar_text().is_some_and(|text| spec.accepts(&text))
    } else {
        admits(ty, val).is_ok()
    }
}

fn section_id(sec: &Section) -> Option<String> {
    sec.id.as_ref().map(|id| id.0.clone())
}

fn extract_section_title(title_inlines: &[Inline]) -> String {
    let mut title = String::new();
    for inline in title_inlines {
        if let Inline::Text(t) = inline {
            title.push_str(&t.value);
        }
    }
    title
}

fn collect_doc_sections<'a>(blocks: &'a [Block], doc_sections: &mut Vec<&'a Section>) {
    for block in blocks {
        if let Block::Section(sec) = block {
            doc_sections.push(sec);
            collect_doc_sections(&sec.blocks, doc_sections);
        }
    }
}

/// Applies a blueprint to an existing document `doc` in-place.
///
/// 1. `::kind`: If `doc` has no `::kind` declaration, inserts `::kind(target_kind)` at index 0.
/// 2. `::id`: If `blueprint` declares `::id`:
///    - If `doc` has no `::id`: inserts `::id(...)` placed after `::kind` / `::version`.
///    - If `doc` has `::id`: keeps existing ID UNLESS blueprint's `::id` is a `TypeSeat` with
///      `overwrite: true` AND existing ID does not satisfy the seat's `Type`.
/// 3. `::meta`: If `blueprint` declares `::meta`:
///    - For each `(key, bp_val)`:
///      - If missing in `doc`: adds it.
///      - If present in `doc`: keeps existing value UNLESS `bp_val` is a `TypeSeat` with
///        `overwrite: true` AND existing value does not satisfy the seat's `Type`.
/// 4. Top-level sections: For each top-level section in `blueprint.body`:
///    - If missing in `doc` (matched by id or title): appends to `doc.body`.
///
/// Returns `Err(problems)` if any seat in the elements being added or overwritten cannot be filled,
/// leaving `doc` untouched.
pub fn apply_blueprint(
    doc: &mut File,
    blueprint: &File,
    cx: &EvalContext,
    source: &dyn Source,
) -> Result<ApplyOutcome, Vec<SeatProblem>> {
    let info = extract_blueprint_info(blueprint);

    // 1. Check ::kind
    let has_kind = doc.header.iter().any(|d| {
        classify_declaration_lenient(d) == ElementKind::Declaration(DeclKind::Kind)
    });
    let add_kind = if !has_kind {
        info.as_ref().map(|i| i.target_kind.clone())
    } else {
        None
    };

    // 2. Check ::id
    let bp_id_decl = blueprint.header.iter().find(|d| {
        classify_declaration_lenient(d) == ElementKind::Declaration(DeclKind::Id)
    });
    let doc_id_decl = doc.header.iter().find(|d| {
        classify_declaration_lenient(d) == ElementKind::Declaration(DeclKind::Id)
    });

    let mut add_id = false;
    let mut overwrite_id = false;
    if let Some(bp_id) = bp_id_decl {
        match doc_id_decl {
            None => {
                add_id = true;
            }
            Some(doc_id) => {
                let bp_id_val = normalized_declaration_args(bp_id)
                    .as_ref()
                    .and_then(|args| args.get("id"))
                    .or(bp_id.data.args.as_ref())
                    .cloned();
                if let Some(Value::TypeSeat(seat)) = &bp_id_val {
                    if let Ok(ty) = evaluate_type(&seat.expr, cx) {
                        if ty.overwrite() {
                            let doc_val = normalized_declaration_args(doc_id)
                                .as_ref()
                                .and_then(|args| args.get("id"))
                                .or(doc_id.data.args.as_ref())
                                .cloned()
                                .unwrap_or(Value::Null);
                            if !value_satisfies_type(&doc_val, &ty) {
                                overwrite_id = true;
                            }
                        }
                    }
                }
            }
        }
    }

    // 3. Check ::meta
    let bp_meta_decl = blueprint.header.iter().find(|d| {
        classify_declaration_lenient(d) == ElementKind::Declaration(DeclKind::Meta)
    });
    let doc_meta_decl = doc.header.iter().find(|d| {
        classify_declaration_lenient(d) == ElementKind::Declaration(DeclKind::Meta)
    });

    let mut meta_patch_entries = Vec::new();
    if let Some(bp_meta) = bp_meta_decl {
        let bp_entries = tomet_semantics::data::declaration_entries(bp_meta)
            .and_then(|v| match v {
                Value::Map(entries) => Some(entries),
                _ => None,
            })
            .unwrap_or_default();

        for (key, bp_val) in bp_entries {
            let doc_val = doc_meta_decl.and_then(|d| d.data.get_attr(&key));
            match doc_val {
                None => {
                    meta_patch_entries.push((key, bp_val));
                }
                Some(doc_val) => {
                    if let Value::TypeSeat(seat) = &bp_val {
                        if let Ok(ty) = evaluate_type(&seat.expr, cx) {
                            if ty.overwrite() && !value_satisfies_type(doc_val, &ty) {
                                meta_patch_entries.push((key, bp_val));
                            }
                        }
                    }
                }
            }
        }
    }

    // 4. Check sections
    let mut doc_sections = Vec::new();
    collect_doc_sections(&doc.body, &mut doc_sections);

    let mut missing_sections = Vec::new();
    for block in &blueprint.body {
        if let Block::Section(bp_sec) = block {
            let bp_id = section_id(bp_sec);
            let bp_title = extract_section_title(&bp_sec.title);
            let exists = doc_sections.iter().any(|d_sec| {
                let d_id = section_id(d_sec);
                let d_title = extract_section_title(&d_sec.title);
                if let (Some(bp_id), Some(d_id)) = (&bp_id, d_id) {
                    bp_id == &d_id
                } else if bp_id.is_none() {
                    d_title.trim() == bp_title.trim()
                } else {
                    false
                }
            });
            if !exists {
                missing_sections.push(Block::Section(bp_sec.clone()));
            }
        }
    }

    // If nothing to add or overwrite, return early unchanged
    if add_kind.is_none()
        && !add_id
        && !overwrite_id
        && meta_patch_entries.is_empty()
        && missing_sections.is_empty()
    {
        return Ok(ApplyOutcome {
            changed: false,
            unresolved: Vec::new(),
        });
    }

    // Construct patch AST
    let mut patch = File::default();
    if let Some(target_kind) = &add_kind {
        let mut kind_decl = declaration_new("kind");
        kind_decl.data.args = Some(Value::String(target_kind.clone()));
        patch.header.push(kind_decl);
    }
    if add_id || overwrite_id {
        let mut id_decl = declaration_new("id");
        id_decl.data.args = bp_id_decl.and_then(|d| d.data.args.clone());
        patch.header.push(id_decl);
    }
    if !meta_patch_entries.is_empty() {
        let mut meta_decl = declaration_new("meta");
        meta_decl.data.args = Some(Value::Map(meta_patch_entries));
        patch.header.push(meta_decl);
    }
    patch.body = missing_sections;

    // Fill seats on patch. If any cannot be filled, returns Err without modifying doc.
    fill_seats(&mut patch, cx, source)?;

    // Resolve expressions on patch
    let unresolved = resolve_expressions(&mut patch, cx);

    let left = seats_left(&patch);
    if !left.is_empty() {
        return Err(left);
    }

    // Apply patch to doc
    let mut changed = false;

    if add_kind.is_some() {
        if let Some(patch_kind) = patch.header.iter().find(|d| {
            classify_declaration_lenient(d) == ElementKind::Declaration(DeclKind::Kind)
        }) {
            doc.header.insert(0, patch_kind.clone());
            changed = true;
        }
    }

    if add_id {
        if let Some(patch_id) = patch.header.iter().find(|d| {
            classify_declaration_lenient(d) == ElementKind::Declaration(DeclKind::Id)
        }) {
            let insert_idx = doc
                .header
                .iter()
                .enumerate()
                .filter(|(_, d)| {
                    let kind = classify_declaration_lenient(d);
                    kind == ElementKind::Declaration(DeclKind::Kind)
                        || kind == ElementKind::Declaration(DeclKind::Version)
                })
                .map(|(i, _)| i + 1)
                .last()
                .unwrap_or(0);
            doc.header.insert(insert_idx, patch_id.clone());
            changed = true;
        }
    } else if overwrite_id {
        if let Some(patch_id) = patch.header.iter().find(|d| {
            classify_declaration_lenient(d) == ElementKind::Declaration(DeclKind::Id)
        }) {
            if let Some(doc_id) = doc.header.iter_mut().find(|d| {
                classify_declaration_lenient(d) == ElementKind::Declaration(DeclKind::Id)
            }) {
                doc_id.data.args = patch_id.data.args.clone();
                changed = true;
            }
        }
    }

    if let Some(patch_meta) = patch.header.iter().find(|d| {
        classify_declaration_lenient(d) == ElementKind::Declaration(DeclKind::Meta)
    }) {
        let filled_entries = tomet_semantics::data::declaration_entries(patch_meta)
            .and_then(|v| match v {
                Value::Map(entries) => Some(entries),
                _ => None,
            })
            .unwrap_or_default();

        if !filled_entries.is_empty() {
            if let Some(doc_meta) = doc.header.iter_mut().find(|d| {
                classify_declaration_lenient(d) == ElementKind::Declaration(DeclKind::Meta)
            }) {
                for (k, v) in filled_entries {
                    doc_meta.data.set_prop(&k, v);
                }
            } else {
                let mut new_meta = declaration_new("meta");
                new_meta.data.args = Some(Value::Map(filled_entries));
                doc.header.push(new_meta);
            }
            changed = true;
        }
    }

    if !patch.body.is_empty() {
        doc.body.extend(patch.body);
        changed = true;
    }

    Ok(ApplyOutcome {
        changed,
        unresolved,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FixedSource;
    use std::collections::HashMap;
    use tomet_expr::{DocRef, EvalValue, NoResolver};

    fn instantiate(src: &str, vars: EvalValue) -> (File, Instantiated) {
        let mut doc = tomet_ast_parser::parse_file(src).unwrap();
        let macros = HashMap::new();
        let cx = EvalContext {
            resolver: &NoResolver,
            self_doc: DocRef("notes/plan.tmt".into()),
            macros: &macros,
            var: &vars,
            var_unset: &[],
            includes: &[],
            heading_level: 0,
            notes: Default::default(),
        };
        let source = FixedSource::at("2026-11-11T09:30:00+09:00").expect("a datetime");
        let report =
            instantiate_blueprint(&mut doc, &cx, &source).expect("no type seat to stop it");
        (doc, report)
    }

    /// Instantiating leaves exactly one kind behind.
    ///
    /// A blueprint carries two: `::kind(blueprint)`, saying what the file
    /// is, and `::blueprint(target)`, saying what it produces. Only the
    /// second survives -- the first describes a thing that stops existing.
    /// The first run of `tomet new` after the shape was decided emitted
    /// both, which is what this pins.
    #[test]
    fn instantiating_leaves_one_kind() {
        let (doc, _) = instantiate(
            "::kind(blueprint)\n::blueprint(daily-note){\n  version: \"1.0\"\n}\n\n=[ Plan ]\n",
            EvalValue::Null,
        );

        let mut kinds = Vec::new();
        for_each_declaration(&doc, |declaration| {
            if classify_declaration_lenient(declaration) == ElementKind::Declaration(DeclKind::Kind)
                && let Some(s) = declaration.data.args.as_ref().and_then(|v| v.as_str())
            {
                kinds.push(s.to_string());
            }
        });
        assert_eq!(kinds, vec!["daily-note".to_string()]);
    }

    #[test]
    fn extracts_blueprint_info() {
        let src =
            "::blueprint(daily-note){\n  description: \"Daily note\"\n}\n\n=[ Plan ]#(plan)\n";
        let doc = tomet_ast_parser::parse_file(src).unwrap();
        let info = extract_blueprint_info(&doc).unwrap();
        assert_eq!(info.target_kind, "daily-note");
        assert_eq!(info.description.as_deref(), Some("Daily note"));
    }

    #[test]
    fn instantiates_blueprint_to_kind_and_resolves_expressions() {
        let src = "::blueprint(daily-note)\n::meta(\n  file: $(self::filename)\n  title: $(var.title)\n)\n\n=[ Plan for $(var.title) ]#(plan)\n";
        let vars = EvalValue::Map(vec![(
            "title".to_string(),
            EvalValue::String("Sprint 42".into()),
        )]);
        let (doc, report) = instantiate(src, vars);
        assert!(report.changed);
        assert!(report.unresolved.is_empty(), "{:?}", report.unresolved);

        let mut top_level = Vec::new();
        for_each_declaration(&doc, |declaration| top_level.push(declaration.clone()));

        let kind = top_level
            .iter()
            .find(|d| classify_declaration_lenient(d) == ElementKind::Declaration(DeclKind::Kind))
            .expect("expected a ::kind declaration");
        assert_eq!(kind.data.args, Some(Value::String("daily-note".into())));

        let meta = top_level
            .iter()
            .find(|d| classify_declaration_lenient(d) == ElementKind::Declaration(DeclKind::Meta))
            .expect("expected a ::meta declaration");
        let Some(Value::Map(entries)) = &meta.data.args else {
            panic!("expected meta map");
        };
        let get = |key: &str| entries.iter().find(|(k, _)| k == key).unwrap().1.clone();
        assert_eq!(get("file"), Value::String("plan.tmt".into()));
        assert_eq!(get("title"), Value::String("Sprint 42".into()));
    }

    #[test]
    fn an_expression_with_no_value_is_left_and_reported() {
        let (doc, report) = instantiate(
            "::blueprint(note)\n::meta(title: $(var.title))\n",
            EvalValue::Map(vec![]),
        );
        assert_eq!(report.unresolved.len(), 1);
        assert!(
            report.unresolved[0].reason.contains("title"),
            "{:?}",
            report.unresolved
        );
        assert!(format!("{doc:?}").contains("Interp"));
    }

    const UUID: &str = "00000000-0000-0000-0000-000000000000";

    fn apply_test(
        doc_src: &str,
        bp_src: &str,
        ids: &[&str],
    ) -> Result<(File, ApplyOutcome), Vec<SeatProblem>> {
        let mut doc = tomet_ast_parser::parse_file(doc_src).unwrap();
        let bp = tomet_ast_parser::parse_file(bp_src).unwrap();
        let macros = HashMap::new();
        let cx = EvalContext {
            resolver: &NoResolver,
            self_doc: DocRef("test.tmt".into()),
            macros: &macros,
            var: &EvalValue::Null,
            var_unset: &[],
            includes: &[],
            heading_level: 0,
            notes: Default::default(),
        };
        let source = FixedSource::at("2026-11-11T09:30:00+09:00")
            .unwrap()
            .with_ids(ids.iter().copied());
        let outcome = apply_blueprint(&mut doc, &bp, &cx, &source)?;
        Ok((doc, outcome))
    }

    #[test]
    fn apply_adds_missing_kind_id_meta_and_sections() {
        let doc_src = "Existing paragraph.\n";
        let bp_src = "::kind(blueprint)\n::blueprint(task)\n::id(<$uuid(default: auto)>)\n::meta(\n  title: <$string(default: \"Untitled\")>\n)\n\n=[ Details ]#(details)\n";
        let (doc, outcome) = apply_test(doc_src, bp_src, &[UUID]).unwrap();
        assert!(outcome.changed);

        // Header has ::kind(task), ::id(UUID), ::meta(title: "Untitled")
        let kind = doc
            .header
            .iter()
            .find(|d| classify_declaration_lenient(d) == ElementKind::Declaration(DeclKind::Kind))
            .expect("::kind");
        assert_eq!(kind.data.args, Some(Value::String("task".into())));

        let id = doc
            .header
            .iter()
            .find(|d| classify_declaration_lenient(d) == ElementKind::Declaration(DeclKind::Id))
            .expect("::id");
        assert_eq!(id.data.args, Some(Value::String(UUID.into())));

        let meta = doc
            .header
            .iter()
            .find(|d| classify_declaration_lenient(d) == ElementKind::Declaration(DeclKind::Meta))
            .expect("::meta");
        assert_eq!(
            meta.data.get_attr("title"),
            Some(&Value::String("Untitled".into()))
        );

        // Body has existing paragraph and added section
        assert_eq!(doc.body.len(), 2);
        assert!(matches!(&doc.body[0], Block::Paragraph(_)));
        assert!(matches!(&doc.body[1], Block::Section(s) if s.id.as_ref().map(|i| i.0.as_str()) == Some("details")));
    }

    #[test]
    fn apply_already_conforming_document_is_untouched() {
        let doc_src = "::kind(task)\n::id(123e4567-e89b-12d3-a456-426614174000)\n::meta(\n  title: \"Existing\"\n)\n\n=[ Details ]#(details)\n\nBody content.\n";
        let bp_src = "::kind(blueprint)\n::blueprint(task)\n::id(<$uuid(default: auto)>)\n::meta(\n  title: <$string(default: \"Untitled\")>\n)\n\n=[ Details ]#(details)\n";
        let before = tomet_ast_parser::parse_file(doc_src).unwrap();
        let (doc, outcome) = apply_test(doc_src, bp_src, &[UUID]).unwrap();
        assert!(!outcome.changed);
        assert_eq!(doc, before);
    }

    #[test]
    fn apply_preserves_existing_valid_id_and_meta_with_overwrite_true() {
        let doc_src = "::kind(task)\n::id(123e4567-e89b-12d3-a456-426614174000)\n::meta(\n  count: 42\n)\n";
        let bp_src = "::kind(blueprint)\n::blueprint(task)\n::id(<$uuid(default: auto, overwrite: true)>)\n::meta(\n  count: <$int(default: 0, overwrite: true)>\n)\n";
        let (doc, outcome) = apply_test(doc_src, bp_src, &[UUID]).unwrap();
        assert!(!outcome.changed);

        let id = doc
            .header
            .iter()
            .find(|d| classify_declaration_lenient(d) == ElementKind::Declaration(DeclKind::Id))
            .expect("::id");
        assert_eq!(
            id.data.args,
            Some(Value::String("123e4567-e89b-12d3-a456-426614174000".into()))
        );

        let meta = doc
            .header
            .iter()
            .find(|d| classify_declaration_lenient(d) == ElementKind::Declaration(DeclKind::Meta))
            .expect("::meta");
        assert_eq!(meta.data.get_attr("count"), Some(&Value::Int(42)));
    }

    #[test]
    fn apply_replaces_existing_invalid_id_and_meta_with_overwrite_true() {
        let doc_src = "::kind(task)\n::id(not-a-uuid)\n::meta(\n  count: \"not-an-int\"\n)\n";
        let bp_src = "::kind(blueprint)\n::blueprint(task)\n::id(<$uuid(default: auto, overwrite: true)>)\n::meta(\n  count: <$int(default: 99, overwrite: true)>\n)\n";
        let (doc, outcome) = apply_test(doc_src, bp_src, &[UUID]).unwrap();
        assert!(outcome.changed);

        let id = doc
            .header
            .iter()
            .find(|d| classify_declaration_lenient(d) == ElementKind::Declaration(DeclKind::Id))
            .expect("::id");
        assert_eq!(id.data.args, Some(Value::String(UUID.into())));

        let meta = doc
            .header
            .iter()
            .find(|d| classify_declaration_lenient(d) == ElementKind::Declaration(DeclKind::Meta))
            .expect("::meta");
        assert_eq!(meta.data.get_attr("count"), Some(&Value::Int(99)));
    }

    #[test]
    fn apply_preserves_existing_invalid_id_and_meta_with_overwrite_false() {
        let doc_src = "::kind(task)\n::id(not-a-uuid)\n::meta(\n  count: \"not-an-int\"\n)\n";
        let bp_src = "::kind(blueprint)\n::blueprint(task)\n::id(<$uuid(default: auto)>)\n::meta(\n  count: <$int(default: 99)>\n)\n";
        let (doc, outcome) = apply_test(doc_src, bp_src, &[UUID]).unwrap();
        assert!(!outcome.changed);

        let id = doc
            .header
            .iter()
            .find(|d| classify_declaration_lenient(d) == ElementKind::Declaration(DeclKind::Id))
            .expect("::id");
        assert_eq!(id.data.args, Some(Value::String("not-a-uuid".into())));

        let meta = doc
            .header
            .iter()
            .find(|d| classify_declaration_lenient(d) == ElementKind::Declaration(DeclKind::Meta))
            .expect("::meta");
        assert_eq!(
            meta.data.get_attr("count"),
            Some(&Value::String("not-an-int".into()))
        );
    }

    #[test]
    fn apply_unfillable_seat_in_missing_section_or_field_returns_err_without_modifying_doc() {
        let doc_src = "::kind(task)\nExisting paragraph.\n";
        let before = tomet_ast_parser::parse_file(doc_src).unwrap();
        let bp_src = "::kind(blueprint)\n::blueprint(task)\n::meta(\n  unfillable: <$string>\n)\n";
        let mut doc = tomet_ast_parser::parse_file(doc_src).unwrap();
        let bp = tomet_ast_parser::parse_file(bp_src).unwrap();
        let macros = HashMap::new();
        let cx = EvalContext {
            resolver: &NoResolver,
            self_doc: DocRef("test.tmt".into()),
            macros: &macros,
            var: &EvalValue::Null,
            var_unset: &[],
            includes: &[],
            heading_level: 0,
            notes: Default::default(),
        };
        let source = FixedSource::at("2026-11-11T09:30:00+09:00").unwrap();
        let res = apply_blueprint(&mut doc, &bp, &cx, &source);
        assert!(res.is_err());
        assert_eq!(doc, before);
    }
}
