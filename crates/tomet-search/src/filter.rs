//! In-memory metadata query filtering and sorting engine.
//!
//! Evaluates predicate expressions (e.g. `contains(meta.tags, "rust")`, `exists(meta.date)`)
//! and sort keys (e.g. `by(meta.date, "desc")`) across collections of metadata rows.
//!
//! This engine performs no filesystem I/O: candidate rows are provided as in-memory
//! [`IndexRow`] structures.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use tomet_ast::{Document, InterpExpr, InterpExprKind, Literal, Span, Value};
use tomet_compute::EvaluationContext;
use tomet_semantics::DocumentConfig;

/// One candidate document's queryable facts.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexRow {
    /// Document path (typically relative to the project root, with forward slashes).
    pub path: String,
    /// Dotted field paths mapped to their AST values (e.g. "kind", "meta.title", "meta.tags").
    pub fields: BTreeMap<String, Value>,
}

/// An error encountered when parsing or executing an index query.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexQueryError {
    pub message: String,
}

impl fmt::Display for IndexQueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for IndexQueryError {}

pub fn err(message: impl Into<String>) -> IndexQueryError {
    IndexQueryError {
        message: message.into(),
    }
}

/// A sort key with its direction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortKey {
    pub path: String,
    pub descending: bool,
}

/// Parsed filter query consisting of an optional combined predicate and sort keys.
#[derive(Debug, Clone)]
pub struct Query {
    /// Every non-`by` argument, wrapped in a synthetic `and(...)`.
    pub predicate: Option<InterpExpr>,
    pub sort: Vec<SortKey>,
}

impl Query {
    /// Parses a query from an AST expression argument list (as passed to `${filter(...)}`).
    pub fn parse(args: &[InterpExpr]) -> Result<Self, IndexQueryError> {
        let mut predicates = Vec::new();
        let mut sort = Vec::new();
        for arg in args {
            match sort_key(arg)? {
                Some(key) => sort.push(key),
                None => predicates.push(arg.clone()),
            }
        }
        let predicate = (!predicates.is_empty()).then(|| InterpExpr {
            kind: InterpExprKind::Call {
                callee: Box::new(identifier("and")),
                args: predicates,
            },
            span: Span::default(),
        });
        Ok(Query { predicate, sort })
    }

    /// Runs the query against `rows`, returning the paths of matching rows in sorted order.
    pub fn run(
        &self,
        doc: &Document,
        config: &DocumentConfig,
        rows: &[IndexRow],
        known: &BTreeSet<String>,
    ) -> Result<Vec<String>, IndexQueryError> {
        let mut matched = Vec::new();
        for row in rows {
            if self.matches(doc, config, row, known)? {
                matched.push(row);
            }
        }
        matched.sort_by(|a, b| self.order(a, b));
        Ok(matched.into_iter().map(|row| row.path.clone()).collect())
    }

    /// Evaluates whether a single row matches the query predicate.
    pub fn matches(
        &self,
        doc: &Document,
        config: &DocumentConfig,
        row: &IndexRow,
        known: &BTreeSet<String>,
    ) -> Result<bool, IndexQueryError> {
        let Some(predicate) = &self.predicate else {
            return Ok(true);
        };
        let ctx = context_for(row, known);
        match tomet_compute::evaluate_with_context(doc, predicate, config, &ctx) {
            Ok(Value::Bool(matched)) => Ok(matched),
            Ok(other) => Err(err(format!(
                "a filter predicate answered with {other:?} rather than yes or no, evaluating {}",
                row.path
            ))),
            Err(e) => {
                let unknown = unknown_fields(predicate, known);
                if unknown.is_empty() {
                    Err(err(format!("{}: {e}", row.path)))
                } else {
                    Err(err(format!(
                        "no file in the vault has {} -- check the spelling",
                        unknown.join(", ")
                    )))
                }
            }
        }
    }

    /// Determines ordering between two rows based on sort keys, falling back to path.
    pub fn order(&self, a: &IndexRow, b: &IndexRow) -> Ordering {
        for key in &self.sort {
            let left = sort_value(a, &key.path);
            let right = sort_value(b, &key.path);
            let ordering = match (left, right) {
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (Some(x), Some(y)) => {
                    let ordering = value_order(x, y);
                    if key.descending {
                        ordering.reverse()
                    } else {
                        ordering
                    }
                }
            };
            if ordering != Ordering::Equal {
                return ordering;
            }
        }
        a.path.cmp(&b.path)
    }
}

/// Whether `doc` is an index document -- `@kind(doc.index)`.
pub fn is_index_document(doc: &Document) -> bool {
    tomet_semantics::document_kind(doc).as_deref() == Some("doc.index")
}

/// Collects every field path any row carries in `rows`.
pub fn known_paths(rows: &[IndexRow]) -> BTreeSet<String> {
    rows.iter()
        .flat_map(|row| row.fields.keys().cloned())
        .collect()
}

/// Reads one `by(field)` / `by(field, "asc"|"desc")` argument.
pub fn sort_key(expr: &InterpExpr) -> Result<Option<SortKey>, IndexQueryError> {
    let InterpExprKind::Call { callee, args } = &expr.kind else {
        return Ok(None);
    };
    let InterpExprKind::Identifier(name) = &callee.kind else {
        return Ok(None);
    };
    if name != "by" {
        return Ok(None);
    }

    let (field, order) = match args.as_slice() {
        [field] => (field, None),
        [field, order] => (field, Some(order)),
        other => {
            return Err(err(format!(
                "`by` takes a field and an optional \"asc\"/\"desc\", got {} argument(s)",
                other.len()
            )));
        }
    };

    let path = dotted_path(field)
        .ok_or_else(|| err("`by`'s first argument has to name a field, like `by(meta.created)`"))?;

    let descending = match order {
        None => false,
        Some(order) => match &order.kind {
            InterpExprKind::Literal(Literal::String(s)) if s == "asc" => false,
            InterpExprKind::Literal(Literal::String(s)) if s == "desc" => true,
            _ => {
                return Err(err(format!(
                    "`by({path}, ...)`'s direction has to be \"asc\" or \"desc\""
                )));
            }
        },
    };

    Ok(Some(SortKey { path, descending }))
}

/// Converts an AST identifier or member chain into a dotted string path.
pub fn dotted_path(expr: &InterpExpr) -> Option<String> {
    match &expr.kind {
        InterpExprKind::Identifier(name) => Some(name.clone()),
        InterpExprKind::Member { object, member } => {
            Some(format!("{}.{member}", dotted_path(object)?))
        }
        _ => None,
    }
}

/// Finds field paths referenced in `predicate` that no row in `known` possesses.
pub fn unknown_fields(predicate: &InterpExpr, known: &BTreeSet<String>) -> Vec<String> {
    let mut referenced = BTreeSet::new();
    collect_field_paths(predicate, &mut referenced);
    referenced
        .into_iter()
        .filter(|path| !known.contains(path))
        .collect()
}

fn collect_field_paths(expr: &InterpExpr, out: &mut BTreeSet<String>) {
    match &expr.kind {
        InterpExprKind::Call { args, .. } => {
            for arg in args {
                collect_field_paths(arg, out);
            }
        }
        InterpExprKind::NamedArg { value, .. } => collect_field_paths(value, out),
        InterpExprKind::Identifier(_) | InterpExprKind::Member { .. } => {
            if let Some(path) = dotted_path(expr) {
                out.insert(path);
            }
        }
        InterpExprKind::Literal(_) => {}
    }
}

/// Builds an evaluation context with every known path bound, or `Null` if absent.
pub fn context_for(row: &IndexRow, known: &BTreeSet<String>) -> EvaluationContext {
    let mut ctx = EvaluationContext::new();
    for path in known {
        let value = row.fields.get(path).cloned().unwrap_or(Value::Null);
        ctx.vars.insert(path.clone(), value);
    }
    ctx
}

/// Reads the value for a sort key, treating `Null` as absent.
pub fn sort_value<'a>(row: &'a IndexRow, path: &str) -> Option<&'a Value> {
    row.fields
        .get(path)
        .filter(|value| !matches!(value, Value::Null))
}

fn value_order(a: &Value, b: &Value) -> Ordering {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x.cmp(y),
        (Value::String(x), Value::String(y)) => x.cmp(y),
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        _ => match (as_f64(a), as_f64(b)) {
            (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
            _ => Ordering::Equal,
        },
    }
}

fn as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Int(i) => Some(*i as f64),
        Value::Float(f) => Some(*f),
        _ => None,
    }
}

fn identifier(name: &str) -> InterpExpr {
    InterpExpr {
        kind: InterpExprKind::Identifier(name.to_string()),
        span: Span::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_parser::parse_document;

    fn row(path: &str, fields: &[(&str, Value)]) -> IndexRow {
        IndexRow {
            path: path.to_string(),
            fields: fields
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        }
    }

    fn tags(items: &[&str]) -> Value {
        Value::Seq(items.iter().map(|s| Value::String(s.to_string())).collect())
    }

    fn text(s: &str) -> Value {
        Value::String(s.to_string())
    }

    fn sample_table() -> Vec<IndexRow> {
        vec![
            row(
                "docs/a.tmt",
                &[
                    ("meta.tags", tags(&["rust", "cli"])),
                    ("meta.created", text("2026-01-01")),
                ],
            ),
            row(
                "docs/b.tmt",
                &[
                    ("meta.tags", tags(&["go"])),
                    ("meta.created", text("2026-09-01")),
                    ("meta.draft", Value::Bool(true)),
                ],
            ),
            row("docs/c.tmt", &[("meta.tags", tags(&["rust"]))]),
        ]
    }

    fn parse_filter_from_src(src: &str) -> (Document, DocumentConfig, Query) {
        let doc = parse_document(src).expect("valid source");
        let config = tomet_semantics::document_config(&doc);
        let mut filter_args = None;
        for block in &doc.blocks {
            if let tomet_ast::Block::Element(el) = block {
                if let Some(tomet_ast::ElementValue::Interp(expr)) = &el.value {
                    if let InterpExprKind::Call { callee, args } = &expr.kind {
                        if let InterpExprKind::Identifier(name) = &callee.kind {
                            if name == "filter" {
                                filter_args = Some(args.clone());
                                break;
                            }
                        }
                    }
                }
            }
        }
        let args = filter_args.expect("filter query in source");
        let query = Query::parse(&args).expect("valid query");
        (doc, config, query)
    }

    #[test]
    fn a_predicate_selects_and_paths_break_the_tie() {
        let (doc, config, query) =
            parse_filter_from_src("${filter(contains(meta.tags, \"rust\"))}\n");
        let rows = sample_table();
        let known = known_paths(&rows);
        let matches = query.run(&doc, &config, &rows, &known).unwrap();
        assert_eq!(matches, vec!["docs/a.tmt", "docs/c.tmt"]);
    }

    #[test]
    fn several_arguments_are_anded_together() {
        let (doc, config, query) = parse_filter_from_src(
            "${filter(contains(meta.tags, \"rust\"), exists(meta.created))}\n",
        );
        let rows = sample_table();
        let known = known_paths(&rows);
        let matches = query.run(&doc, &config, &rows, &known).unwrap();
        assert_eq!(matches, vec!["docs/a.tmt"]);
    }

    #[test]
    fn by_sorts_and_desc_reverses() {
        let rows = sample_table();
        let known = known_paths(&rows);

        let (doc, config, query_asc) =
            parse_filter_from_src("${filter(by(meta.created, \"asc\"))}\n");
        let asc = query_asc.run(&doc, &config, &rows, &known).unwrap();
        assert_eq!(asc, vec!["docs/a.tmt", "docs/b.tmt", "docs/c.tmt"]);

        let (doc, config, query_desc) =
            parse_filter_from_src("${filter(by(meta.created, \"desc\"))}\n");
        let desc = query_desc.run(&doc, &config, &rows, &known).unwrap();
        assert_eq!(desc, vec!["docs/b.tmt", "docs/a.tmt", "docs/c.tmt"]);
    }

    #[test]
    fn a_field_this_row_lacks_reads_as_absent() {
        let rows = sample_table();
        let known = known_paths(&rows);

        let (doc, config, query) = parse_filter_from_src("${filter(not(exists(meta.draft)))}\n");
        let matches = query.run(&doc, &config, &rows, &known).unwrap();
        assert_eq!(matches, vec!["docs/a.tmt", "docs/c.tmt"]);
    }

    #[test]
    fn a_field_no_file_has_is_an_error() {
        let (doc, config, query) = parse_filter_from_src("${filter(exists(meta.tgs))}\n");
        let rows = sample_table();
        let known = known_paths(&rows);
        let err = query.run(&doc, &config, &rows, &known).unwrap_err();
        assert!(err.message.contains("meta.tgs"));
    }

    #[test]
    fn by_rejects_unknown_direction() {
        let doc = parse_document("${filter(by(meta.created, \"newest\"))}\n").unwrap();
        let mut filter_args = None;
        for block in &doc.blocks {
            if let tomet_ast::Block::Element(el) = block {
                if let Some(tomet_ast::ElementValue::Interp(expr)) = &el.value {
                    if let InterpExprKind::Call { args, .. } = &expr.kind {
                        filter_args = Some(args.clone());
                    }
                }
            }
        }
        let err = Query::parse(&filter_args.unwrap()).unwrap_err();
        assert!(err.message.contains("asc"));
    }
}
