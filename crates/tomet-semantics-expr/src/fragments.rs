//! Values that are trees: what `link` and `raw` build.

use tomet_ast::{Block, Inline, Name, Paragraph, Placement, RawText, Text, Tuple, Value};
use tomet_scalar::Scalar;
use tomet_span::Span;
use tomet_tree::{ElementExt, element_new};

use super::args::Args;
use super::error::EvalError;
use super::value::EvalValue;

/// Extracts the label text from an evaluated argument, if one was passed.
///
/// Only a scalar converts to text implicitly. Sequences, maps, references,
/// functions, fragments, and `none` are rejected with a type mismatch.
pub(crate) fn link_label(label: Option<&EvalValue>) -> Result<Option<String>, EvalError> {
    let Some(label) = label else {
        return Ok(None);
    };
    super::methods::scalar_text(label)
        .map(Some)
        .map_err(|err| match err {
            EvalError::TypeMismatch { expected, got, .. } => EvalError::TypeMismatch {
                what: "the label of a link".to_string(),
                expected,
                got,
            },
            other => other,
        })
}

/// A link to a document or to an element of one: `@link(tmt:path)` or
/// `@link(tmt:path#id)`, optionally labeled, written into a paragraph of its own
/// so the result can go where output goes. In a list it becomes one item, in a
/// paragraph it is the link, and on its own line it is a paragraph holding the
/// link.
pub(crate) fn link_fragment(
    target: &EvalValue,
    label: Option<&EvalValue>,
) -> Result<EvalValue, EvalError> {
    let address = match target {
        EvalValue::Document(doc) => format!("tmt:{}", doc.0),
        EvalValue::Part { doc, part } => match part.id() {
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
    let label = link_label(label)?;
    let mut link = element_new(Name::bare("link"));
    link.data.args = Some(Tuple::from_unnamed([Value::Scalar(Scalar::Text(address))]));
    let span = Span::default();
    if let Some(text) = label {
        link.content = Some(vec![Block::Paragraph(Paragraph::new(
            vec![Inline::Text(Text::new(text, span))],
            span,
        ))]);
    }
    Ok(EvalValue::Fragment(vec![Block::Paragraph(Paragraph::new(
        vec![Inline::Element(link)],
        span,
    ))]))
}

/// Builds a raw code block fragment: `raw(text, lang: "...")`.
pub(crate) fn raw_fragment(args: Args) -> Result<EvalValue, EvalError> {
    let mut lang = None;
    for (name, value) in &args.named {
        match (name.as_str(), value) {
            ("lang", EvalValue::Scalar(Scalar::Text(text))) => {
                lang = Some(text.clone());
            }
            ("lang", other) => {
                return Err(EvalError::mismatch(
                    "`raw`'s `lang:`",
                    "a text scalar",
                    other.type_name(),
                ));
            }
            (other, _) => {
                return Err(EvalError::message(format!(
                    "`raw` takes `lang:`, not `{other}:`"
                )));
            }
        }
    }
    let mut positional = args.positional;
    if positional.is_empty() {
        return Err(EvalError::WrongArgCount {
            function: "raw".to_string(),
            expected: "1".to_string(),
            got: 0,
        });
    }
    if positional.len() > 1 {
        return Err(EvalError::WrongArgCount {
            function: "raw".to_string(),
            expected: "1".to_string(),
            got: positional.len(),
        });
    }
    let code = match positional.remove(0) {
        EvalValue::Scalar(Scalar::Text(text)) => text,
        other => {
            return Err(EvalError::mismatch(
                "`raw`",
                "a text scalar",
                other.type_name(),
            ));
        }
    };

    let span = Span::default();
    let mut element = element_new(Name::bare("raw"))
        .with_placement(Placement::Block)
        .with_span(span)
        .with_content(vec![Block::Paragraph(Paragraph::new(
            vec![Inline::Raw(RawText::new(code, span))],
            span,
        ))]);
    if let Some(l) = lang {
        element.data.args = Some(Tuple::from_named(vec![(
            "lang".to_string(),
            Value::Scalar(Scalar::Text(l)),
        )]));
    }
    Ok(EvalValue::Fragment(vec![Block::Element(element)]))
}
