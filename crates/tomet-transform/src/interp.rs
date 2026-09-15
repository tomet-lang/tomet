//! Resolving `${...}` before anything renders.
//!
//! # Why this is a pass and not a renderer's job
//!
//! It was a renderer's job, four times over, and the four disagreed:
//! markdown evaluated with an [`EvaluationContext`], HTML evaluated
//! without one (so `${self.path}` resolved in one and not the other, and
//! the vault's macros expanded in neither), Typst and Pandoc did not
//! evaluate at all. The same document produced three different answers
//! depending on what it was being turned into.
//!
//! A `${...}` means one thing. Deciding what it means is the language's
//! job, so it is settled here, once, before any converter sees the tree --
//! and a converter that never meets an `InterpExpr` has nothing left to
//! disagree about.
//!
//! # What a failure does
//!
//! Nothing is dropped and nothing is fatal. An expression that cannot be
//! resolved is replaced by its own source spelling, so the output shows
//! `${some_id}` exactly as written, and is reported in the returned
//! [`Unresolved`] list so `tomet check` can warn about it. That keeps a
//! document that shows `${...}` as an example working, while still giving
//! a typo somewhere to surface.
//!
//! # `filter` is not ours
//!
//! `${filter(...)}` is an index query: it expands into several elements
//! rather than into text, and [`crate::index_query`] owns it. It is
//! skipped here rather than reported, because "unknown function `filter`"
//! would be a lie.

use tomet_ast::{
    Block, Document, Element, ElementValue, Inline, InterpExpr, InterpExprKind, Paragraph, Sigil,
    Span, Text, Value,
};
use tomet_compute::EvaluationContext;
use tomet_semantics::DocumentConfig;

/// A `${...}` that could not be resolved, left in the output as written.
#[derive(Debug, Clone, PartialEq)]
pub struct Unresolved {
    /// The expression as written, without the surrounding `${` and `}`.
    pub source: String,
    /// Where it sits in the document that was passed in.
    pub span: Span,
    /// What the evaluator said.
    pub reason: String,
}

/// Replaces every `${...}` in `doc` with the text it stands for.
///
/// `config` carries the macros in scope (the document's own and the
/// vault's, already merged by the caller) and `ctx` the variables --
/// `self.path` and the rest. Both are the caller's to supply because this
/// crate cannot read a config file or know where a document sits.
pub fn resolve_interpolations(
    doc: &mut Document,
    config: &DocumentConfig,
    ctx: &EvaluationContext,
) -> Vec<Unresolved> {
    // `${some_id}` resolves against the document it sits in, so evaluation
    // reads a copy taken before the rewrite -- the shape `index_query` and
    // `expand_document_macros` both use.
    let source = doc.clone();
    let mut unresolved = Vec::new();
    doc.blocks = std::mem::take(&mut doc.blocks)
        .into_iter()
        .map(|block| resolve_block(block, &source, config, ctx, &mut unresolved))
        .collect();
    unresolved
}

fn resolve_block(
    block: Block,
    source: &Document,
    config: &DocumentConfig,
    ctx: &EvaluationContext,
    unresolved: &mut Vec<Unresolved>,
) -> Block {
    match block {
        Block::Paragraph(mut p) => {
            p.content = resolve_inlines(p.content, source, config, ctx, unresolved);
            Block::Paragraph(p)
        }
        Block::Element(el) => match interp_of(&el) {
            // A block-level `${...}` stood alone on its line, so what
            // replaces it stands alone too.
            Some(expr) => {
                let span = el.span;
                let text = resolve_one(expr, span, source, config, ctx, unresolved);
                Block::Paragraph(Paragraph::new(
                    vec![Inline::Text(Text { value: text, span })],
                    span,
                ))
            }
            None => Block::Element(resolve_element(el, source, config, ctx, unresolved)),
        },
    }
}

fn resolve_element(
    mut el: Element,
    source: &Document,
    config: &DocumentConfig,
    ctx: &EvaluationContext,
    unresolved: &mut Vec<Unresolved>,
) -> Element {
    if let Some(content) = el.content.take() {
        el.content = Some(resolve_inlines(content, source, config, ctx, unresolved));
    }
    if let Some(children) = el.children.take() {
        el.children = Some(
            children
                .into_iter()
                .map(|block| resolve_block(block, source, config, ctx, unresolved))
                .collect(),
        );
    }
    el
}

fn resolve_inlines(
    inlines: Vec<Inline>,
    source: &Document,
    config: &DocumentConfig,
    ctx: &EvaluationContext,
    unresolved: &mut Vec<Unresolved>,
) -> Vec<Inline> {
    inlines
        .into_iter()
        .map(|inline| match inline {
            Inline::Text(t) => Inline::Text(t),
            Inline::Raw(t) => Inline::Raw(t),
            Inline::SoftBreak(b) => Inline::SoftBreak(b),
            Inline::LineBreak(b) => Inline::LineBreak(b),
            Inline::Element(el) => match interp_of(&el) {
                Some(expr) => {
                    let span = el.span;
                    let value = resolve_one(expr, span, source, config, ctx, unresolved);
                    Inline::Text(Text { value, span })
                }
                None => Inline::Element(resolve_element(el, source, config, ctx, unresolved)),
            },
        })
        .collect()
}

/// The `${...}` expression this element carries, unless it is an index
/// query -- those belong to [`crate::index_query`].
fn interp_of(el: &Element) -> Option<&InterpExpr> {
    if !matches!(el.sigil, Sigil::Dollar) {
        return None;
    }
    let ElementValue::Interp(expr) = el.value.as_ref()? else {
        return None;
    };
    if is_index_query(expr) {
        return None;
    }
    Some(expr)
}

fn is_index_query(expr: &InterpExpr) -> bool {
    let InterpExprKind::Call { callee, .. } = &expr.kind else {
        return false;
    };
    matches!(&callee.kind, InterpExprKind::Identifier(name) if name == "filter")
}

/// The text one expression stands for -- its value, or its own spelling
/// when it has none.
fn resolve_one(
    expr: &InterpExpr,
    span: Span,
    source: &Document,
    config: &DocumentConfig,
    ctx: &EvaluationContext,
    unresolved: &mut Vec<Unresolved>,
) -> String {
    match tomet_compute::evaluate_with_context(source, expr, config, ctx) {
        Ok(value) => value_text(&value),
        Err(reason) => {
            unresolved.push(Unresolved {
                source: expr.to_string(),
                span,
                reason: reason.to_string(),
            });
            // Written back in the `${...}` form even when the source said
            // `$name(args)`: the two parse to the same tree, and the tree
            // is all that is left by here.
            format!("${{{expr}}}")
        }
    }
}

fn value_text(value: &Value) -> String {
    match value {
        // Bare, not quoted: `${gh(12)}` is a URL in running prose, not a
        // string literal being written back to source.
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        // A list or a map has no plain-prose spelling. The `.tmt` one is
        // at least a spelling a reader of this format recognises, which
        // `format!("{other:?}")` -- Rust's debug output, what the four
        // converters each emitted -- was not.
        container => tomet_style::render_value_inner(container),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_ast::Block;

    fn config(macros: &[(&str, &str)]) -> DocumentConfig {
        let mut config = DocumentConfig::default();
        for (name, template) in macros {
            config.macros.insert(name.to_string(), template.to_string());
        }
        config
    }

    fn vars(path: &str) -> EvaluationContext {
        EvaluationContext::new().with_var(
            "self",
            Value::Map(vec![("path".to_string(), Value::String(path.to_string()))]),
        )
    }

    /// Every text node in the document, joined. Asserting on this rather
    /// than on rendered CommonMark keeps these tests about what the pass
    /// produced -- a converter's own escaping (`_` becomes `\_` in
    /// CommonMark, correctly) is that converter's business.
    fn text_of(doc: &Document) -> String {
        fn walk(out: &mut String, items: &[Inline]) {
            for inline in items {
                match inline {
                    Inline::Text(t) => out.push_str(&t.value),
                    Inline::Raw(t) => out.push_str(&t.value),
                    Inline::SoftBreak(_) | Inline::LineBreak(_) => {}
                    Inline::Element(el) => {
                        if let Some(content) = &el.content {
                            walk(out, content);
                        }
                    }
                }
            }
        }
        let mut out = String::new();
        for block in &doc.blocks {
            match block {
                Block::Paragraph(p) => walk(&mut out, &p.content),
                Block::Element(el) => {
                    if let Some(content) = &el.content {
                        walk(&mut out, content);
                    }
                }
            }
        }
        out
    }

    fn resolve(
        src: &str,
        config: &DocumentConfig,
        ctx: &EvaluationContext,
    ) -> (Document, Vec<Unresolved>) {
        let mut doc = tomet_parser::parse_document(src).expect("valid source");
        let unresolved = resolve_interpolations(&mut doc, config, ctx);
        (doc, unresolved)
    }

    /// The expansion the four converters each used to decide for
    /// themselves, settled once and checked once.
    #[test]
    fn macros_and_builtins_and_self_all_resolve() {
        let config = config(&[
            ("gh", "https://github.com/tomet/tomet/issues/${1}"),
            ("copyright", "(C) 2026 Tomet"),
        ]);
        let (doc, unresolved) = resolve(
            "Issue: $gh(42) Footer: ${copyright} Math: ${add(10, 5)} Here: ${self.path}\n",
            &config,
            &vars("docs/guide/cheatsheet.tmt"),
        );
        assert_eq!(
            text_of(&doc),
            "Issue: https://github.com/tomet/tomet/issues/42 Footer: (C) 2026 Tomet \
             Math: 15 Here: docs/guide/cheatsheet.tmt"
        );
        assert!(unresolved.is_empty(), "{unresolved:?}");
    }

    #[test]
    fn an_unresolvable_expression_is_left_as_written_and_reported() {
        let (doc, unresolved) = resolve(
            "例: ${some_id}\n",
            &DocumentConfig::default(),
            &EvaluationContext::new(),
        );
        // Left in the output, so a document showing `${...}` as an example
        // still renders as one.
        assert_eq!(text_of(&doc), "例: ${some_id}");
        // ...and reported, so a typo has somewhere to surface.
        assert_eq!(unresolved.len(), 1);
        assert_eq!(unresolved[0].source, "some_id");
        assert_eq!(unresolved[0].span.start.line, 1);
    }

    #[test]
    fn a_block_level_interpolation_becomes_a_paragraph() {
        let config = config(&[("copyright", "(C) 2026 Tomet")]);
        let (doc, _) = resolve(
            "#[ T ]\n\n${copyright}\n",
            &config,
            &EvaluationContext::new(),
        );
        assert!(
            matches!(doc.blocks.last(), Some(Block::Paragraph(p))
                if matches!(p.content.as_slice(), [Inline::Text(t)] if t.value == "(C) 2026 Tomet")),
            "{:?}",
            doc.blocks.last()
        );
    }

    /// `${filter(...)}` is an index query, not text. Reporting it as an
    /// unresolved interpolation would be a lie -- `index_query` owns it,
    /// and on a document that never went through that pass it has to stay
    /// exactly as it is.
    #[test]
    fn an_index_query_is_not_touched() {
        let (doc, unresolved) = resolve(
            "@kind(index)\n\n${filter(contains(meta.tags, \"rust\"))}\n",
            &DocumentConfig::default(),
            &EvaluationContext::new(),
        );
        assert!(unresolved.is_empty(), "{unresolved:?}");
        assert!(
            matches!(doc.blocks.last(), Some(Block::Element(el)) if matches!(el.sigil, Sigil::Dollar)),
            "the query element was rewritten"
        );
    }

    /// A string is written back with the escapes `parse_quoted` reads,
    /// which is what the five hand-written copies of this disagreed about:
    /// the printer wrapped it in bare quotes, so an embedded `"` produced
    /// source that will not parse.
    #[test]
    fn an_unresolvable_string_literal_keeps_its_quoting() {
        let (doc, _) = resolve(
            "${unknown(\"a \\\"quoted\\\" word\")}\n",
            &DocumentConfig::default(),
            &EvaluationContext::new(),
        );
        let written = text_of(&doc);
        assert_eq!(written, "${unknown(\"a \\\"quoted\\\" word\")}");
        // And what it wrote parses back to the same expression.
        tomet_parser::parse_document(&written).expect("re-parses");
    }
}
