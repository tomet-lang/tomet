//! `${filter(...)}` -- the one query node an `@kind(index)` document
//! writes, and the pass that replaces it with the `@file` entries it
//! selects.
//!
//! # Why this is not a builtin function
//!
//! Every other `${...}` evaluates to a single `Value` and renders as
//! text. `filter` expands into *several* AST nodes, so it can never go
//! through `tomet_compute::call`'s one-value path. This pass therefore
//! pattern-matches `filter` as an **unevaluated** `InterpExpr::Call` and
//! reads its argument list as a small query language. Its arguments are
//! ordinary expressions and are evaluated the ordinary way, once per
//! candidate file.
//!
//! `by(...)` is read here for the same reason, and a smaller one: it
//! names a *field* rather than a value, so evaluating it would resolve
//! the very path the sort needs to keep.
//!
//! # Why the rows arrive as a parameter
//!
//! Building the table means reading every `.tmt` in the vault, and this
//! crate sits below the one allowed to do that (`crate-layering` in the
//! root `.writ.tmt`: `tomet-transform` is layer 4, `tomet-indexer` is
//! layer 5). So the I/O happens there, the table arrives here as plain
//! data, and this pass stays pure and testable without a directory.
//!
//! # Absent, versus nobody has it
//!
//! Every row's evaluation context is given *every* field path any row in
//! the table has, with [`Value::Null`] for the ones this row lacks. That
//! is what makes `exists(meta.draft)` answerable -- an unfilled context
//! would fail to resolve the name instead of reporting it absent.
//!
//! A path no file in the vault has is deliberately left out, so it still
//! fails to resolve. That is the typo case: `meta.tgs` should say so
//! rather than quietly match nothing.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use tomet_ast::{
    Block, Document, Element, ElementValue, InterpExpr, InterpExprKind, Literal, Placement, Sigil,
    Span, Value,
};
use tomet_compute::EvaluationContext;
use tomet_tree::element_new;

/// One candidate file, as `tomet-indexer::collect_metadata_table`
/// produces it: the path a generated `@file(...)` will carry, and the
/// dotted field paths a predicate can ask about.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexRow {
    /// Measured from the project root, forward slashes -- the spelling
    /// `tomet-indexer::resolve_document_relative` reads back.
    pub path: String,
    pub fields: BTreeMap<String, Value>,
}

/// A query that could not be read, or could not be run against a row.
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

fn err(message: impl Into<String>) -> IndexQueryError {
    IndexQueryError {
        message: message.into(),
    }
}

/// Replaces every block-level `${filter(...)}` in `doc` with the
/// `@file(...)` elements it selects, in the order `by(...)` asks for.
///
/// Returns how many query nodes were expanded, so a caller can tell an
/// index document from an ordinary one without inspecting `@kind`.
///
/// Only block-level queries are expanded. A `${filter(...)}` sitting
/// inside running text is left exactly as it is: it would have to expand
/// into block elements in the middle of a sentence, and there is no
/// sensible answer to what that means.
pub fn expand_index_queries(
    doc: &mut Document,
    rows: &[IndexRow],
) -> Result<usize, IndexQueryError> {
    // Evaluation reads the document the query sits in (that is how
    // `${some_id}` resolves), so it needs a copy taken before the
    // rewrite -- the same shape `expand_document_macros` uses.
    let source = doc.clone();
    let config = tomet_semantics::document_config(&source);
    let known = known_paths(rows);

    let mut expanded = 0;
    let mut rewritten = Vec::with_capacity(doc.blocks.len());
    for block in std::mem::take(&mut doc.blocks) {
        let query = match filter_args(&block) {
            Some(args) => Some(Query::parse(args)?),
            None => None,
        };
        match query {
            Some(query) => {
                for path in query.run(&source, &config, rows, &known)? {
                    rewritten.push(Block::Element(file_element(&path)));
                }
                expanded += 1;
            }
            None => rewritten.push(block),
        }
    }
    doc.blocks = rewritten;
    Ok(expanded)
}

/// Whether `doc` contains a block-level `${filter(...)}` at all.
///
/// The cheap question to ask before the expensive one. Building the table
/// means parsing every `.tmt` in the vault, and a caller exporting a
/// whole directory would otherwise pay that for documents that ask no
/// question.
///
/// It is also what decides whether a document is treated as an index, in
/// place of reading `@kind(index)` or the filename: a document with a
/// query wants it answered, and one without is unaffected either way.
pub fn has_index_query(doc: &Document) -> bool {
    doc.blocks.iter().any(|block| filter_args(block).is_some())
}

/// The argument list of a block-level `${filter(...)}`, if that is what
/// this block is.
fn filter_args(block: &Block) -> Option<&[InterpExpr]> {
    let Block::Element(el) = block else {
        return None;
    };
    if !matches!(el.sigil, Sigil::Dollar) {
        return None;
    }
    let Some(ElementValue::Interp(expr)) = &el.value else {
        return None;
    };
    let InterpExprKind::Call { callee, args } = &expr.kind else {
        return None;
    };
    match &callee.kind {
        InterpExprKind::Identifier(name) if name == "filter" => Some(args),
        _ => None,
    }
}

struct SortKey {
    path: String,
    descending: bool,
}

struct Query {
    /// Every non-`by` argument, wrapped in a synthetic `and(...)`.
    ///
    /// Wrapping rather than looping and testing each result keeps one
    /// definition of what counts as a match: `and`'s, in
    /// `tomet-compute`'s `functions.rs`. A second truthiness rule written
    /// here is a rule that can drift from the one authors actually see.
    predicate: Option<InterpExpr>,
    sort: Vec<SortKey>,
}

impl Query {
    fn parse(args: &[InterpExpr]) -> Result<Self, IndexQueryError> {
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

    fn run(
        &self,
        doc: &Document,
        config: &tomet_semantics::DocumentConfig,
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

    fn matches(
        &self,
        doc: &Document,
        config: &tomet_semantics::DocumentConfig,
        row: &IndexRow,
        known: &BTreeSet<String>,
    ) -> Result<bool, IndexQueryError> {
        // No predicate at all -- `${filter(by(meta.created))}` is a
        // legitimate "everything, in this order".
        let Some(predicate) = &self.predicate else {
            return Ok(true);
        };
        let ctx = context_for(row, known);
        match tomet_compute::evaluate_with_context(doc, predicate, config, &ctx) {
            Ok(Value::Bool(matched)) => Ok(matched),
            // `and` always answers with a `Bool`, so this is only
            // reachable if that ever stops being true.
            Ok(other) => Err(err(format!(
                "a filter predicate answered with {other:?} rather than yes or no, \
                 evaluating {}",
                row.path
            ))),
            // A field nobody has is left out of the context deliberately
            // (see this module's header), so it arrives here as whatever
            // the resolver says about a name it could not find -- which
            // talks about document ids and names the wrong half of the
            // path. Say what actually went wrong instead.
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

    fn order(&self, a: &IndexRow, b: &IndexRow) -> Ordering {
        for key in &self.sort {
            let left = sort_value(a, &key.path);
            let right = sort_value(b, &key.path);
            let ordering = match (left, right) {
                // A file the sort key says nothing about goes last, and
                // stays last under `desc` -- the direction is applied to
                // the comparison, not to this. An index of recent notes
                // should not open with the ones carrying no date.
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
        // Path last, so the output of a query never depends on the order
        // the table happened to be walked in.
        a.path.cmp(&b.path)
    }
}

/// Reads one `by(field)` / `by(field, "asc"|"desc")` argument. `Ok(None)`
/// means this argument is a predicate, not a sort key.
fn sort_key(expr: &InterpExpr) -> Result<Option<SortKey>, IndexQueryError> {
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

/// `meta.created` -> `"meta.created"`. `None` for anything that is not a
/// plain identifier or member chain.
fn dotted_path(expr: &InterpExpr) -> Option<String> {
    match &expr.kind {
        InterpExprKind::Identifier(name) => Some(name.clone()),
        InterpExprKind::Member { object, member } => Some(format!("{}.{member}", dotted_path(object)?)),
        _ => None,
    }
}

/// The field paths a predicate reads that no file in the table has.
///
/// A function name is not a field: only the *arguments* of a call are
/// walked, never its callee, or `exists` and `and` would themselves be
/// reported as misspelt fields.
fn unknown_fields(predicate: &InterpExpr, known: &BTreeSet<String>) -> Vec<String> {
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

/// Every field path any row carries.
fn known_paths(rows: &[IndexRow]) -> BTreeSet<String> {
    rows.iter()
        .flat_map(|row| row.fields.keys().cloned())
        .collect()
}

/// One row as an evaluation context: every known path bound, `Null` for
/// the ones this row does not have. See this module's header.
fn context_for(row: &IndexRow, known: &BTreeSet<String>) -> EvaluationContext {
    let mut ctx = EvaluationContext::new();
    for path in known {
        let value = row.fields.get(path).cloned().unwrap_or(Value::Null);
        ctx.vars.insert(path.clone(), value);
    }
    ctx
}

/// The value a sort key reads, with `Null` treated as absent -- a field
/// explicitly set to nothing sorts with the files that never had it.
fn sort_value<'a>(row: &'a IndexRow, path: &str) -> Option<&'a Value> {
    row.fields
        .get(path)
        .filter(|value| !matches!(value, Value::Null))
}

/// Ordering for a sort key: two numbers numerically, two strings
/// bytewise (so an ISO-8601 date sorts correctly as text).
///
/// Anything else compares equal rather than erroring. Sorting is a
/// pairwise question asked across a whole table, and one odd pair should
/// not fail a query that is otherwise answerable; the path tiebreak in
/// [`Query::order`] keeps the result deterministic either way. This is
/// deliberately laxer than the `gt`/`lt` predicates, which do report a
/// mixed pair -- there, the comparison *is* the answer.
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

/// `@file(path)` as a block. The path is a bare positional argument,
/// which `builtin_positional_arg_keys` normalizes to `target` -- the same
/// shape a hand-written `@file(docs/README.tmt)` parses to.
fn file_element(path: &str) -> Element {
    let mut el = element_new(Sigil::named("file"));
    el.placement = Placement::Block;
    el.args = Some(Value::String(path.to_string()));
    el
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_semantics::ValueExt;

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

    /// Three notes: two tagged `rust`, one of those undated, one tagged
    /// `go`, and one carrying an explicit `draft`.
    fn table() -> Vec<IndexRow> {
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

    fn expand(src: &str) -> Result<Vec<String>, IndexQueryError> {
        let mut doc = tomet_parser::parse_document(src).expect("valid source");
        expand_index_queries(&mut doc, &table())?;
        Ok(file_paths(&doc))
    }

    /// The `target` of every block-level `@file` in the document, in order.
    fn file_paths(doc: &Document) -> Vec<String> {
        doc.blocks
            .iter()
            .filter_map(|block| match block {
                Block::Element(el) if el.sigil.is_bare_named("file") => match &el.args {
                    Some(Value::String(path)) => Some(path.clone()),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_predicate_selects_and_paths_break_the_tie() {
        let paths = expand("@kind(index)\n\n${filter(contains(meta.tags, \"rust\"))}\n").unwrap();
        assert_eq!(paths, ["docs/a.tmt", "docs/c.tmt"]);
    }

    #[test]
    fn several_arguments_are_anded_together() {
        let src = "@kind(index)\n\n${filter(contains(meta.tags, \"rust\"), exists(meta.created))}\n";
        assert_eq!(expand(src).unwrap(), ["docs/a.tmt"]);
    }

    #[test]
    fn by_sorts_and_desc_reverses() {
        let asc = expand("@kind(index)\n\n${filter(by(meta.created, \"asc\"))}\n").unwrap();
        assert_eq!(asc, ["docs/a.tmt", "docs/b.tmt", "docs/c.tmt"]);
        let desc = expand("@kind(index)\n\n${filter(by(meta.created, \"desc\"))}\n").unwrap();
        // `c` has no `created` at all, so it stays last under both --
        // an index of recent notes should not open with the undated one.
        assert_eq!(desc, ["docs/b.tmt", "docs/a.tmt", "docs/c.tmt"]);
    }

    #[test]
    fn a_query_with_no_predicate_takes_everything() {
        let paths = expand("@kind(index)\n\n${filter(by(meta.created))}\n").unwrap();
        assert_eq!(paths.len(), 3);
    }

    #[test]
    fn a_field_this_row_lacks_reads_as_absent_rather_than_failing() {
        // Only `docs/b.tmt` carries `meta.draft`; the other two have to
        // answer "no" instead of failing to resolve the name.
        let src = "@kind(index)\n\n${filter(not(exists(meta.draft)))}\n";
        assert_eq!(expand(src).unwrap(), ["docs/a.tmt", "docs/c.tmt"]);
        let src = "@kind(index)\n\n${filter(exists(meta.draft))}\n";
        assert_eq!(expand(src).unwrap(), ["docs/b.tmt"]);
    }

    #[test]
    fn a_field_no_file_has_is_an_error() {
        // The typo case. Filling it in as `Null` too would turn
        // `meta.tgs` into a query that quietly matches nothing.
        let err = expand("@kind(index)\n\n${filter(exists(meta.tgs))}\n").unwrap_err();
        assert!(err.message.contains("meta.tgs"), "{}", err.message);
        // And it names the field, not whatever the resolver last tried to
        // look up -- the raw failure here is about an element id `meta`.
        assert!(!err.message.contains("element with id"), "{}", err.message);
        // `exists` is a function, not a misspelt field.
        assert!(!err.message.contains("exists"), "{}", err.message);
    }

    #[test]
    fn an_unorderable_pair_is_reported_by_the_predicate() {
        let err = expand("@kind(index)\n\n${filter(gt(meta.tags, 1))}\n").unwrap_err();
        assert!(err.message.contains("cannot order"), "{}", err.message);
    }

    #[test]
    fn by_rejects_a_direction_it_does_not_know() {
        let err = expand("@kind(index)\n\n${filter(by(meta.created, \"newest\"))}\n").unwrap_err();
        assert!(err.message.contains("asc"), "{}", err.message);
    }

    #[test]
    fn an_inline_query_is_left_alone() {
        // It would have to expand into block elements mid-sentence.
        let src = "@kind(index)\n\n索引は ${filter(exists(meta.created))} で書く。\n";
        let mut doc = tomet_parser::parse_document(src).expect("valid source");
        let expanded = expand_index_queries(&mut doc, &table()).unwrap();
        assert_eq!(expanded, 0);
        assert!(file_paths(&doc).is_empty());
    }

    #[test]
    fn surrounding_blocks_keep_their_positions() {
        let src = "@kind(index)\n\n#[ Rust ]\n\n${filter(contains(meta.tags, \"rust\"))}\n\n#[ Go ]\n\n${filter(contains(meta.tags, \"go\"))}\n";
        let mut doc = tomet_parser::parse_document(src).expect("valid source");
        assert_eq!(expand_index_queries(&mut doc, &table()).unwrap(), 2);
        assert_eq!(
            file_paths(&doc),
            ["docs/a.tmt", "docs/c.tmt", "docs/b.tmt"]
        );
        // Two headings, still one on each side of the first expansion.
        let headings = doc
            .blocks
            .iter()
            .filter(|b| matches!(b, Block::Element(el) if el.sigil.is_bare_named("heading")))
            .count();
        assert_eq!(headings, 2);
    }

    #[test]
    fn a_generated_file_element_carries_a_bare_positional_path() {
        // The shape a hand-written `@file(docs/b.tmt)` parses to, so the
        // printer has nothing special to do with it. That it prints and
        // re-parses is checked in `tomet-tests`, which is where a test
        // spanning the printer belongs.
        let mut doc =
            tomet_parser::parse_document("@kind(index)\n\n${filter(contains(meta.tags, \"go\"))}\n")
                .expect("valid source");
        expand_index_queries(&mut doc, &table()).unwrap();
        let Some(Block::Element(el)) = doc.blocks.last() else {
            panic!("expected a generated element");
        };
        assert!(el.sigil.is_bare_named("file"));
        assert_eq!(el.placement, Placement::Block);
        assert_eq!(
            tomet_semantics::normalized_element_args(el)
                .as_ref()
                .and_then(|args| args.get("target")),
            Some(&Value::String("docs/b.tmt".to_string()))
        );
    }
}
