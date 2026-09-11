//! `${filter(...)}` -- the one query node an `@kind(doc.index)` document
//! may write, and the pass that replaces it with the `@link(ref:...)`
//! entries it selects, in place among whatever else the author wrote by
//! hand.
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
    Block, Document, Element, ElementValue, Entry, Inline, InterpExpr, InterpExprKind, Literal,
    Placement, Sigil, Span, Value,
};
use tomet_compute::EvaluationContext;
use tomet_tree::{element_list_item, element_new};

/// One candidate file, as `tomet-indexer::collect_metadata_table`
/// produces it: the path a generated `@link(ref:...)` will carry, and the
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

/// Replaces every `${filter(...)}` in `doc` with the `@link(ref:...)`
/// entries it selects, in the order `by(...)` asks for -- in place,
/// whether the query sat at the top level or as a list item nested
/// arbitrarily deep under hand-written entries (see [`rewrite_blocks`]/
/// [`rewrite_list_value`]).
///
/// Returns how many query nodes were expanded. This is a count for a
/// caller that wants one (`Prepared::expanded_queries`); whether to run
/// this pass at all is [`is_index_document`]'s question, not this
/// function's -- it rewrites whatever queries it finds regardless of
/// `@kind`.
///
/// Only a query that is a whole block, or a whole list item, is expanded.
/// One sitting inside running text (`索引は ${filter(...)} で書く`) is left
/// exactly as it is: it would have to expand into several elements in the
/// middle of a sentence, or several list items in the middle of one, and
/// there is no sensible answer to what that means.
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
    let blocks = std::mem::take(&mut doc.blocks);
    doc.blocks = rewrite_blocks(blocks, &source, &config, rows, &known, &mut expanded)?;
    Ok(expanded)
}

/// Rewrites a run of blocks -- the document's own top level, or a list
/// item's nested children (a sub-list, or anything else an author put
/// there) -- replacing every `${filter(...)}` with what it selects.
///
/// A list found here has its own items walked by [`rewrite_list_value`],
/// since a query nested *inside* a list item is invisible at this level
/// (it lives in the item's `content`, not as a `Block` of its own). A bare
/// block-level query -- one not inside any list -- expands into
/// block-level `@link` elements directly, preserving what this always did
/// before entries were allowed to nest.
fn rewrite_blocks(
    blocks: Vec<Block>,
    source: &Document,
    config: &tomet_semantics::DocumentConfig,
    rows: &[IndexRow],
    known: &BTreeSet<String>,
    expanded: &mut usize,
) -> Result<Vec<Block>, IndexQueryError> {
    let mut rewritten = Vec::with_capacity(blocks.len());
    for block in blocks {
        let Block::Element(mut el) = block else {
            rewritten.push(block);
            continue;
        };

        if tomet_semantics::list_ordered(&el).is_some() {
            let value = el.value.take().unwrap_or_else(ElementValue::empty_group);
            el.value = Some(rewrite_list_value(
                value, source, config, rows, known, expanded,
            )?);
            rewritten.push(Block::Element(el));
            continue;
        }

        match filter_args_of_element(&el) {
            Some(args) => {
                let query = Query::parse(args)?;
                for path in query.run(source, config, rows, known)? {
                    rewritten.push(Block::Element(link_block_element(&path)));
                }
                *expanded += 1;
            }
            None => rewritten.push(Block::Element(el)),
        }
    }
    Ok(rewritten)
}

/// Rewrites one list's items in place: an item whose entire content is
/// `${filter(...)}` is replaced by the sibling list items it selects
/// (zero or more, at that same position); every surviving item's own
/// nested children are walked by [`rewrite_blocks`] the same way, so a
/// query nested arbitrarily deep under hand-written entries still expands.
fn rewrite_list_value(
    value: ElementValue,
    source: &Document,
    config: &tomet_semantics::DocumentConfig,
    rows: &[IndexRow],
    known: &BTreeSet<String>,
    expanded: &mut usize,
) -> Result<ElementValue, IndexQueryError> {
    let ElementValue::Group(entries) = value else {
        return Ok(value);
    };

    let mut rewritten = Vec::with_capacity(entries.len());
    for entry in entries {
        let Entry::Element(mut item) = entry else {
            rewritten.push(entry);
            continue;
        };

        let query_args = item
            .content
            .as_deref()
            .and_then(filter_args_of_content)
            .map(<[InterpExpr]>::to_vec);

        if let Some(args) = query_args {
            let query = Query::parse(&args)?;
            for path in query.run(source, config, rows, known)? {
                rewritten.push(Entry::Element(link_list_item(&path, item.span)));
            }
            *expanded += 1;
            continue;
        }

        if let Some(children) = item.children.take() {
            item.children = Some(rewrite_blocks(
                children, source, config, rows, known, expanded,
            )?);
        }
        rewritten.push(Entry::Element(item));
    }
    Ok(ElementValue::Group(rewritten))
}

/// Whether `doc` is an index document -- `@kind(doc.index)`, nothing else.
///
/// Not "does it contain a `${filter(...)}}`": an index document may be
/// entirely hand-written `@link(ref:"...")` entries with no query in it at
/// all, and that is still an index document. `@kind` is the one source of
/// truth; the filename is not consulted here either (a caller that wants a
/// cheap pre-parse filter, e.g. tomet-book's `.index.tmt` suffix, applies
/// it before ever calling this).
///
/// This is also the cheap question to ask before the expensive one: a
/// caller exporting a whole directory only pays for building the vault-wide
/// metadata table (`VaultIndex::build`) for documents this returns `true`
/// for.
pub fn is_index_document(doc: &Document) -> bool {
    tomet_semantics::document_kind(doc).as_deref() == Some("doc.index")
}

/// The argument list of a `${filter(...)}`, if `el` is exactly that --
/// whether `el` sits as a block of its own or as a list item's entire
/// inline content (see [`filter_args_of_content`]), the shape checked is
/// the same: a `$`-sigil element whose value is an unevaluated call to
/// `filter`.
fn filter_args_of_element(el: &Element) -> Option<&[InterpExpr]> {
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

/// The argument list of a `${filter(...)}`, if a list item's whole inline
/// content is exactly that and nothing else -- `- ${filter(...)}}`, not
/// `- some text ${filter(...)}}` (which has no sensible expansion: it
/// would have to splice several list items into the middle of one, so it
/// is left as source, same as an inline query is at the top level).
fn filter_args_of_content(content: &[Inline]) -> Option<&[InterpExpr]> {
    let [Inline::Element(el)] = content else {
        return None;
    };
    filter_args_of_element(el)
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

/// `@link(ref:path)`. The bare positional argument is `"ref:{path}"` --
/// `link`'s single positional slot normalizes to `target`
/// (`builtin_positional_arg_keys`), and `ref:` is one of `target`'s own
/// recognized scheme prefixes (`TargetScheme::Ref`), so this reads back
/// identically to a hand-written `@link(ref:"path")`.
fn link_element(path: &str) -> Element {
    let mut el = element_new(Sigil::named("link"));
    el.args = Some(Value::String(format!("ref:{path}")));
    el
}

/// `@link(ref:path)` as a block of its own -- for a `${filter(...)}` that
/// sits at the top level, outside any list (the shape this pass has
/// always supported; entries nested under a hand-written list use
/// [`link_list_item`] instead).
fn link_block_element(path: &str) -> Element {
    let mut el = link_element(path);
    el.placement = Placement::Block;
    el
}

/// One list item wrapping a single `@link(ref:path)` -- indistinguishable
/// from `- @link(ref:"path")` typed by hand. `span` is the query's own
/// span, reused since a generated item has no source position of its own.
fn link_list_item(path: &str, span: Span) -> Element {
    element_list_item(
        vec![Inline::Element(link_element(path))],
        None,
        None,
        Vec::new(),
        span,
    )
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
        Ok(link_paths(&doc))
    }

    /// The `ref:` path of every block-level `@link` in the document, in
    /// order -- the shape a top-level (not inside any list) query expands
    /// into.
    fn link_paths(doc: &Document) -> Vec<String> {
        doc.blocks
            .iter()
            .filter_map(|block| match block {
                Block::Element(el) => link_ref(el),
                _ => None,
            })
            .collect()
    }

    /// `el`'s `ref:` path, with the scheme prefix stripped -- `None` for
    /// anything that isn't a `@link(ref:...)`.
    fn link_ref(el: &Element) -> Option<String> {
        let (kind, target) = tomet_semantics::link_target_of(el)?;
        if kind != tomet_semantics::ElementKind::Link {
            return None;
        }
        let (scheme, rest) = tomet_semantics::target_scheme(&target);
        (scheme == tomet_semantics::TargetScheme::Ref).then(|| rest.to_string())
    }

    /// The `ref:` path of every list item directly under `list_el`, in
    /// source order -- `None` where an item isn't a lone `@link`, so a
    /// caller can tell a query-generated item from a hand-written label.
    fn item_refs(list_el: &Element) -> Vec<Option<String>> {
        tomet_semantics::list_items(list_el)
            .into_iter()
            .map(|item| {
                let [Inline::Element(el)] = item.content.as_deref()? else {
                    return None;
                };
                link_ref(el)
            })
            .collect()
    }

    /// The first top-level list in `doc` -- the `@kind` header sits before
    /// it, so it is never simply `doc.blocks[0]`.
    fn top_list(doc: &Document) -> &Element {
        doc.blocks
            .iter()
            .find_map(|b| match b {
                Block::Element(el) if tomet_semantics::list_ordered(el).is_some() => Some(el),
                _ => None,
            })
            .expect("expected a top-level list")
    }

    #[test]
    fn a_predicate_selects_and_paths_break_the_tie() {
        let paths = expand("@kind(doc.index)\n\n${filter(contains(meta.tags, \"rust\"))}\n").unwrap();
        assert_eq!(paths, ["docs/a.tmt", "docs/c.tmt"]);
    }

    #[test]
    fn several_arguments_are_anded_together() {
        let src = "@kind(doc.index)\n\n${filter(contains(meta.tags, \"rust\"), exists(meta.created))}\n";
        assert_eq!(expand(src).unwrap(), ["docs/a.tmt"]);
    }

    #[test]
    fn by_sorts_and_desc_reverses() {
        let asc = expand("@kind(doc.index)\n\n${filter(by(meta.created, \"asc\"))}\n").unwrap();
        assert_eq!(asc, ["docs/a.tmt", "docs/b.tmt", "docs/c.tmt"]);
        let desc = expand("@kind(doc.index)\n\n${filter(by(meta.created, \"desc\"))}\n").unwrap();
        // `c` has no `created` at all, so it stays last under both --
        // an index of recent notes should not open with the undated one.
        assert_eq!(desc, ["docs/b.tmt", "docs/a.tmt", "docs/c.tmt"]);
    }

    #[test]
    fn a_query_with_no_predicate_takes_everything() {
        let paths = expand("@kind(doc.index)\n\n${filter(by(meta.created))}\n").unwrap();
        assert_eq!(paths.len(), 3);
    }

    #[test]
    fn a_field_this_row_lacks_reads_as_absent_rather_than_failing() {
        // Only `docs/b.tmt` carries `meta.draft`; the other two have to
        // answer "no" instead of failing to resolve the name.
        let src = "@kind(doc.index)\n\n${filter(not(exists(meta.draft)))}\n";
        assert_eq!(expand(src).unwrap(), ["docs/a.tmt", "docs/c.tmt"]);
        let src = "@kind(doc.index)\n\n${filter(exists(meta.draft))}\n";
        assert_eq!(expand(src).unwrap(), ["docs/b.tmt"]);
    }

    #[test]
    fn a_field_no_file_has_is_an_error() {
        // The typo case. Filling it in as `Null` too would turn
        // `meta.tgs` into a query that quietly matches nothing.
        let err = expand("@kind(doc.index)\n\n${filter(exists(meta.tgs))}\n").unwrap_err();
        assert!(err.message.contains("meta.tgs"), "{}", err.message);
        // And it names the field, not whatever the resolver last tried to
        // look up -- the raw failure here is about an element id `meta`.
        assert!(!err.message.contains("element with id"), "{}", err.message);
        // `exists` is a function, not a misspelt field.
        assert!(!err.message.contains("exists"), "{}", err.message);
    }

    #[test]
    fn an_unorderable_pair_is_reported_by_the_predicate() {
        let err = expand("@kind(doc.index)\n\n${filter(gt(meta.tags, 1))}\n").unwrap_err();
        assert!(err.message.contains("cannot order"), "{}", err.message);
    }

    #[test]
    fn by_rejects_a_direction_it_does_not_know() {
        let err = expand("@kind(doc.index)\n\n${filter(by(meta.created, \"newest\"))}\n").unwrap_err();
        assert!(err.message.contains("asc"), "{}", err.message);
    }

    #[test]
    fn an_inline_query_is_left_alone() {
        // It would have to expand into block elements mid-sentence.
        let src = "@kind(doc.index)\n\n索引は ${filter(exists(meta.created))} で書く。\n";
        let mut doc = tomet_parser::parse_document(src).expect("valid source");
        let expanded = expand_index_queries(&mut doc, &table()).unwrap();
        assert_eq!(expanded, 0);
        assert!(link_paths(&doc).is_empty());
    }

    #[test]
    fn surrounding_blocks_keep_their_positions() {
        let src = "@kind(doc.index)\n\n#[ Rust ]\n\n${filter(contains(meta.tags, \"rust\"))}\n\n#[ Go ]\n\n${filter(contains(meta.tags, \"go\"))}\n";
        let mut doc = tomet_parser::parse_document(src).expect("valid source");
        assert_eq!(expand_index_queries(&mut doc, &table()).unwrap(), 2);
        assert_eq!(
            link_paths(&doc),
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
    fn a_generated_link_element_carries_a_ref_target() {
        // The shape a hand-written `@link(ref:"docs/b.tmt")` parses to, so
        // the printer has nothing special to do with it. That it prints
        // and re-parses is checked in `tomet-tests`, which is where a test
        // spanning the printer belongs.
        let mut doc = tomet_parser::parse_document(
            "@kind(doc.index)\n\n${filter(contains(meta.tags, \"go\"))}\n",
        )
        .expect("valid source");
        expand_index_queries(&mut doc, &table()).unwrap();
        let Some(Block::Element(el)) = doc.blocks.last() else {
            panic!("expected a generated element");
        };
        assert!(el.sigil.is_bare_named("link"));
        assert_eq!(el.placement, Placement::Block);
        assert_eq!(
            tomet_semantics::normalized_element_args(el)
                .as_ref()
                .and_then(|args| args.get("target")),
            Some(&Value::String("ref:docs/b.tmt".to_string()))
        );
    }

    #[test]
    fn a_query_nested_in_a_manual_entry_expands_as_children() {
        // `- @link(ref:"a")` with a query as its own sub-list: the parent
        // stays hand-written, its children come from the query.
        let src = "@kind(doc.index)\n\n- @link(ref:\"docs/a.tmt\")\n  - ${filter(contains(meta.tags, \"go\"))}\n";
        let mut doc = tomet_parser::parse_document(src).expect("valid source");
        assert_eq!(expand_index_queries(&mut doc, &table()).unwrap(), 1);

        let list = top_list(&doc);
        let top = tomet_semantics::list_items(list);
        assert_eq!(top.len(), 1);
        assert_eq!(
            top[0].content.as_deref().and_then(|c| match c {
                [Inline::Element(el)] => link_ref(el),
                _ => None,
            }),
            Some("docs/a.tmt".to_string())
        );

        let Some(Block::Element(nested)) = top[0].children.as_deref().and_then(|c| c.first())
        else {
            panic!("expected the query to have expanded into a nested list");
        };
        assert_eq!(item_refs(nested), [Some("docs/b.tmt".to_string())]);
    }

    #[test]
    fn a_query_interleaves_with_manual_entries_at_the_same_level() {
        let src = "@kind(doc.index)\n\n- @link(ref:\"docs/a.tmt\")\n- ${filter(contains(meta.tags, \"go\"))}\n- @link(ref:\"docs/c.tmt\")\n";
        let mut doc = tomet_parser::parse_document(src).expect("valid source");
        assert_eq!(expand_index_queries(&mut doc, &table()).unwrap(), 1);

        assert_eq!(
            item_refs(top_list(&doc)),
            [
                Some("docs/a.tmt".to_string()),
                Some("docs/b.tmt".to_string()),
                Some("docs/c.tmt".to_string()),
            ]
        );
    }

    #[test]
    fn a_query_matching_nothing_leaves_no_artifact() {
        let src =
            "@kind(doc.index)\n\n- @link(ref:\"docs/a.tmt\")\n- ${filter(contains(meta.tags, \"nope\"))}\n";
        let mut doc = tomet_parser::parse_document(src).expect("valid source");
        assert_eq!(expand_index_queries(&mut doc, &table()).unwrap(), 1);

        assert_eq!(item_refs(top_list(&doc)), [Some("docs/a.tmt".to_string())]);
    }
}
