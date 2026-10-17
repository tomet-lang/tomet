//! Values that are trees: what `link` builds.

use tomet_ast::{Block, Inline, Paragraph, Sigil, Span, Value};
use tomet_tree::element_new;

use super::error::EvalError;
use super::value::EvalValue;

/// A link to a document or to an element of one: `@link(tmt:path)` or
/// `@link(tmt:path#id)`, written into a paragraph of its own so the result
/// can go where output goes. In a list it becomes one item, in a paragraph it
/// is the link, and on its own line it is a paragraph holding the link.
pub(crate) fn link_fragment(target: &EvalValue) -> Result<EvalValue, EvalError> {
    let address = match target {
        EvalValue::Document(doc) => format!("tmt:{}", doc.0),
        EvalValue::Node { doc, node } => match node.id() {
            Some(id) => format!("tmt:{}#{id}", doc.0),
            None => {
                return Err(EvalError::message(
                    "an element without an id cannot be linked to; give it one with `#(id)`",
                ));
            }
        },
        other => {
            return Err(EvalError::mismatch(
                "`link`",
                "a document or an element",
                other.type_name(),
            ));
        }
    };
    let mut link = element_new(Sigil::named("link"));
    link.args = Some(Value::String(address));
    let span = Span::default();
    Ok(EvalValue::Fragment(vec![Block::Paragraph(Paragraph::new(
        vec![Inline::Element(link)],
        span,
    ))]))
}
