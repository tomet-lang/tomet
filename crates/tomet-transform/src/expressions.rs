//! Resolving every `${...}` in a document, once, before anything renders it.
//!
//! This is the single place where an expression's value is put back into a
//! document. It replaces three passes that each handled some positions: the
//! body (`interp`), the arguments of elements (`macro_expand`, called by one
//! binding only) and the index queries (`index_query`). What a value does at
//! a position is one table, in `tmtroot/spec/expressions/overview.expr.tmt`;
//! in short:
//!
//! - data (arguments, properties): any value that can be data is kept;
//! - inside a paragraph: a scalar is text, a fragment of one paragraph is
//!   spliced in, anything else is a mismatch;
//! - on a line of its own: a scalar is a paragraph, a fragment is its blocks;
//! - as a list item: a list gives one item per element, a scalar gives one;
//! - a document, an element, a directory, a function or a datetime has no
//!   place in output at all.
//!
//! =[ A value that does not fit ]
//!
//! Nothing is dropped and nothing is fatal. An expression that cannot be
//! evaluated, or whose value has no place at its position, is written back
//! as its own source (`${...}`) and reported in the returned [`Unresolved`]
//! list, so `tomet check` can warn about it. That keeps a document that
//! shows `${...}` as an example working, and still gives a typo somewhere
//! to surface.
//!
//! =[ What an expression sees ]
//!
//! The caller's [`EvalContext`] decides: which phase, which documents, which
//! macros. References resolve through its resolver, so a caller that wants
//! them answered from the document **as written** -- and it should -- hands
//! in a resolver that returns the document from before this pass rewrote it.

use tomet_ast::{
    Block, Document, Element, ElementValue, Entry, Inline, InterpExpr, List, ListItem, Paragraph,
    Sigil, Span, Text, Value,
};
use tomet_expr::{EvalContext, EvalValue, evaluate};
use tomet_tree::list_item;

/// A `${...}` that could not be resolved, left in the output as written.
#[derive(Debug, Clone, PartialEq)]
pub struct Unresolved {
    /// The expression as written, without the surrounding `${` and `}`.
    pub source: String,
    /// Where it sits in the document that was passed in.
    pub span: Span,
    /// Why: what the evaluator said, or what the value could not be placed as.
    pub reason: String,
}

/// Replaces every `${...}` in `doc` with what it stands for.
pub fn resolve_expressions(doc: &mut Document, cx: &EvalContext) -> Vec<Unresolved> {
    let mut pass = Pass {
        cx,
        unresolved: Vec::new(),
    };
    doc.blocks = std::mem::take(&mut doc.blocks)
        .into_iter()
        .flat_map(|block| pass.block(block))
        .collect();
    pass.unresolved
}

struct Pass<'a> {
    cx: &'a EvalContext<'a>,
    unresolved: Vec<Unresolved>,
}

/// The expression an element carries, when the element is a `${...}`.
fn dollar_expr(el: &Element) -> Option<&InterpExpr> {
    if !matches!(el.sigil, Sigil::Dollar) {
        return None;
    }
    match el.value.as_ref()? {
        ElementValue::Interp(expr) => Some(expr),
        ElementValue::Group(_) => None,
    }
}

impl Pass<'_> {
    /// Evaluates, or records why it could not and returns `None`.
    fn evaluate(&mut self, expr: &InterpExpr, span: Span) -> Option<EvalValue> {
        match evaluate(expr, self.cx) {
            Ok(value) => Some(value),
            Err(reason) => {
                self.fail(expr, span, reason.to_string());
                None
            }
        }
    }

    fn fail(&mut self, expr: &InterpExpr, span: Span, reason: String) {
        self.unresolved.push(Unresolved {
            source: expr.to_string(),
            span,
            reason,
        });
    }

    /// The expression as it was written, for when it has no value.
    fn source_text(expr: &InterpExpr, span: Span) -> Text {
        Text {
            value: format!("${{{expr}}}"),
            span,
        }
    }

    // ---- blocks -------------------------------------------------------------

    fn block(&mut self, block: Block) -> Vec<Block> {
        match block {
            Block::Paragraph(mut p) => {
                p.content = self.inlines(p.content);
                vec![Block::Paragraph(p)]
            }
            Block::Element(el) => match dollar_expr(&el) {
                Some(expr) => self.block_expression(expr, el.span),
                None => vec![Block::Element(self.element(el))],
            },
            Block::Section(mut sec) => {
                sec.title = self.inlines(sec.title);
                sec.args = sec.args.map(|v| self.value(v));
                sec.value = sec.value.map(|v| self.element_value(v));
                sec.connects = sec.connects.into_iter().map(|c| self.element(c)).collect();
                sec.blocks = sec.blocks.into_iter().flat_map(|b| self.block(b)).collect();
                vec![Block::Section(sec)]
            }
            Block::List(list) => vec![Block::List(self.list(list))],
        }
    }

    /// A `${...}` standing alone on its line.
    fn block_expression(&mut self, expr: &InterpExpr, span: Span) -> Vec<Block> {
        let fallback = || {
            vec![Block::Paragraph(Paragraph::new(
                vec![Inline::Text(Self::source_text(expr, span))],
                span,
            ))]
        };
        let Some(value) = self.evaluate(expr, span) else {
            return fallback();
        };
        match to_blocks(value, span) {
            Ok(blocks) => blocks,
            Err(reason) => {
                self.fail(expr, span, reason);
                fallback()
            }
        }
    }

    // ---- inlines ------------------------------------------------------------

    fn inlines(&mut self, inlines: Vec<Inline>) -> Vec<Inline> {
        inlines
            .into_iter()
            .flat_map(|inline| match inline {
                Inline::Element(el) => match dollar_expr(&el) {
                    Some(expr) => self.inline_expression(expr, el.span),
                    None => vec![Inline::Element(self.element(el))],
                },
                other => vec![other],
            })
            .collect()
    }

    fn inline_expression(&mut self, expr: &InterpExpr, span: Span) -> Vec<Inline> {
        let fallback = || vec![Inline::Text(Self::source_text(expr, span))];
        let Some(value) = self.evaluate(expr, span) else {
            return fallback();
        };
        match to_inlines(value, span) {
            Ok(inlines) => inlines,
            Err(reason) => {
                self.fail(expr, span, reason);
                fallback()
            }
        }
    }

    // ---- elements and data --------------------------------------------------

    fn element(&mut self, mut el: Element) -> Element {
        el.args = el.args.map(|v| self.value(v));
        el.value = el.value.map(|v| self.element_value(v));
        el.content = el
            .content
            .map(|blocks| blocks.into_iter().flat_map(|b| self.block(b)).collect());
        el.connects = el.connects.into_iter().map(|c| self.element(c)).collect();
        el
    }

    fn element_value(&mut self, value: ElementValue) -> ElementValue {
        match value {
            // The element *is* the expression; its owner decided what to do.
            ElementValue::Interp(expr) => ElementValue::Interp(expr),
            ElementValue::Group(entries) => ElementValue::Group(
                entries
                    .into_iter()
                    .map(|entry| match entry {
                        Entry::Pair(key, value) => Entry::Pair(key, self.value(value)),
                        Entry::Element(el) => Entry::Element(self.element(el)),
                    })
                    .collect(),
            ),
        }
    }

    /// A `${...}` where data goes: it keeps its value, whatever the type
    /// allows to be kept as data.
    fn value(&mut self, value: Value) -> Value {
        match value {
            Value::Element(el) => match dollar_expr(&el) {
                Some(expr) => {
                    let (expr, span) = (expr.clone(), el.span);
                    self.data_expression(&expr, span)
                        .unwrap_or(Value::Element(el))
                }
                None => Value::Element(Box::new(self.element(*el))),
            },
            Value::Seq(items) => Value::Seq(items.into_iter().map(|v| self.value(v)).collect()),
            Value::Map(entries) => Value::Map(
                entries
                    .into_iter()
                    .map(|(key, v)| (key, self.value(v)))
                    .collect(),
            ),
            Value::Call(name, args) => {
                Value::Call(name, args.into_iter().map(|v| self.value(v)).collect())
            }
            Value::Blocks(blocks) => {
                Value::Blocks(blocks.into_iter().flat_map(|b| self.block(b)).collect())
            }
            other => other,
        }
    }

    fn data_expression(&mut self, expr: &InterpExpr, span: Span) -> Option<Value> {
        let value = self.evaluate(expr, span)?;
        match value.to_ast() {
            Some(data) => Some(data),
            None => {
                self.fail(
                    expr,
                    span,
                    format!("a {} cannot be stored as data", value.type_name()),
                );
                None
            }
        }
    }

    // ---- lists --------------------------------------------------------------

    fn list(&mut self, mut list: List) -> List {
        list.connects = list.connects.into_iter().map(|c| self.element(c)).collect();
        list.items = std::mem::take(&mut list.items)
            .into_iter()
            .flat_map(|item| self.item(item))
            .collect();
        list
    }

    fn item(&mut self, mut item: ListItem) -> Vec<ListItem> {
        let sublist = item.sublist.take().map(|sub| self.list(sub));
        let Some((expr, span)) = item_expression(&item).map(|(e, s)| (e.clone(), s)) else {
            item.element = self.element(item.element);
            item.sublist = sublist;
            return vec![item];
        };

        // The item is nothing but one `${...}`: a list of values becomes a
        // list of items. What was nested under the item goes with the last
        // new one, and an empty result removes the item and its nest.
        let fallback = |this: &mut Self, item: ListItem| {
            let mut item = item;
            item.element = this.element(item.element);
            item.sublist = sublist.clone();
            vec![item]
        };
        let Some(value) = self.evaluate(&expr, span) else {
            return fallback(self, item);
        };
        match to_items(value, span) {
            Ok(mut items) => {
                if let Some(last) = items.last_mut() {
                    last.sublist = sublist;
                }
                items
            }
            Err(reason) => {
                self.fail(&expr, span, reason);
                fallback(self, item)
            }
        }
    }
}

/// The expression of an item whose whole content is exactly one `${...}`.
fn item_expression(item: &ListItem) -> Option<(&InterpExpr, Span)> {
    let [Block::Paragraph(p)] = item.element.content.as_deref()? else {
        return None;
    };
    let [Inline::Element(el)] = p.content.as_slice() else {
        return None;
    };
    dollar_expr(el).map(|expr| (expr, el.span))
}

// ---- turning a value into what a position accepts ---------------------------

/// What to do instead, for a value that has no place in output.
fn misfit(value: &EvalValue) -> String {
    let hint = match value {
        EvalValue::Seq(_) => " -- join it into text with `.join(\", \")`",
        EvalValue::Document(_) | EvalValue::Node { .. } => {
            " -- use `include(...)` for its content, or `.link()` for a link"
        }
        EvalValue::DateTime(_) => " -- write it with `.format(...)`",
        EvalValue::Directory(_) => " -- list its documents with `.docs`",
        _ => "",
    };
    format!("a {} cannot be placed in the text{hint}", value.type_name())
}

/// The text of a scalar, if that is what the value is.
fn scalar_text(value: &EvalValue) -> Option<String> {
    Some(match value {
        EvalValue::Null => String::new(),
        EvalValue::Bool(b) => b.to_string(),
        EvalValue::Int(i) => i.to_string(),
        EvalValue::Float(f) => f.to_string(),
        EvalValue::String(s) => s.clone(),
        _ => return None,
    })
}

fn to_inlines(value: EvalValue, span: Span) -> Result<Vec<Inline>, String> {
    if let Some(text) = scalar_text(&value) {
        return Ok(vec![Inline::Text(Text { value: text, span })]);
    }
    match value {
        EvalValue::Fragment(blocks) => match <[Block; 1]>::try_from(blocks) {
            Ok([Block::Paragraph(p)]) => Ok(p.content),
            _ => Err(
                "a fragment of more than one paragraph cannot go inside a paragraph -- put it on a line of its own"
                    .to_string(),
            ),
        },
        other => Err(misfit(&other)),
    }
}

fn to_blocks(value: EvalValue, span: Span) -> Result<Vec<Block>, String> {
    if let Some(text) = scalar_text(&value) {
        return Ok(vec![Block::Paragraph(Paragraph::new(
            vec![Inline::Text(Text { value: text, span })],
            span,
        ))]);
    }
    match value {
        EvalValue::Fragment(blocks) => Ok(blocks),
        other => Err(misfit(&other)),
    }
}

fn to_items(value: EvalValue, span: Span) -> Result<Vec<ListItem>, String> {
    let values = match value {
        EvalValue::Seq(values) => values,
        single => vec![single],
    };
    values
        .into_iter()
        .map(|value| {
            let blocks = to_blocks(value, span)?;
            Ok(list_item(
                Sigil::Bare,
                blocks,
                None,
                None,
                None,
                Vec::new(),
                None,
                span,
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests;
