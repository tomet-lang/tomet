//! Inline AST sequence operations: trimming at edges and merging adjacent
//! `Text` or `SoftBreak` items during lowering.

use tomet_ast::{Inline, SoftBreak, Text};
use tomet_span::Span;

/// The inline delimiters and the element each one makes, longest first so
/// `**` is tried before `*`.
pub const DELIMITERS: [(&str, &str); 5] = [
    ("**", "strong"),
    ("__", "strong"),
    ("~~", "strikeout"),
    ("*", "em"),
    ("_", "em"),
];

/// Trims leading/trailing whitespace from a finished inline sequence.
pub(crate) fn trim_edges(mut items: Vec<Inline>) -> Vec<Inline> {
    loop {
        match items.first() {
            Some(Inline::SoftBreak(_)) => {
                items.remove(0);
            }
            Some(Inline::Text(t)) => {
                let trimmed = t.value.trim_start();
                if trimmed.is_empty() {
                    items.remove(0);
                } else {
                    if trimmed.len() != t.value.len() {
                        let trimmed = trimmed.to_string();
                        if let Some(Inline::Text(t)) = items.first_mut() {
                            t.value = trimmed;
                        }
                    }
                    break;
                }
            }
            _ => break,
        }
    }
    loop {
        match items.last() {
            Some(Inline::SoftBreak(_)) => {
                items.pop();
            }
            Some(Inline::Text(t)) => {
                let trimmed = t.value.trim_end();
                if trimmed.is_empty() {
                    items.pop();
                } else {
                    if trimmed.len() != t.value.len() {
                        let trimmed = trimmed.to_string();
                        if let Some(Inline::Text(t)) = items.last_mut() {
                            t.value = trimmed;
                        }
                    }
                    break;
                }
            }
            _ => break,
        }
    }
    items
}

/// Pushes a `Text`, merging into the previous item if it is also a `Text`.
pub(crate) fn push_text(items: &mut Vec<Inline>, value: String, span: Span) {
    if let Some(Inline::Text(prev)) = items.last_mut() {
        prev.value.push_str(&value);
        prev.span = Span::new(prev.span.start, span.end);
        return;
    }
    items.push(Inline::Text(Text::new(value, span)));
}

/// Appends `more` to `items`, merging across the join the same way
/// [`push_text`]/[`push_soft_break`] do within a single flush.
pub(crate) fn extend_merging(items: &mut Vec<Inline>, more: Vec<Inline>) {
    let mut more = more.into_iter();
    if let Some(first) = more.next() {
        match first {
            Inline::Text(t) => push_text(items, t.value, t.span),
            Inline::SoftBreak(b) => push_soft_break(items, b.span),
            other => items.push(other),
        }
    }
    items.extend(more);
}

/// Pushes a `SoftBreak`, widening the previous one instead if it is also a
/// `SoftBreak` sitting right before it.
pub(crate) fn push_soft_break(items: &mut Vec<Inline>, span: Span) {
    if let Some(Inline::SoftBreak(prev)) = items.last_mut() {
        prev.span = Span::new(prev.span.start, span.end);
        return;
    }
    items.push(Inline::SoftBreak(SoftBreak { span }));
}
