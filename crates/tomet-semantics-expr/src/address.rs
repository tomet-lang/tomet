//! Addresses written as text: `addr(tmt:docs/a.tmt#intro)` and a string's
//! `.addr()`.
//!
//! The text has the shape `<document>#<address inside it>`. The left side is
//! `tmt:path`, `ref:name` or `dir:path`, or nothing for the current document.
//! The right side is read as the steps that follow a `#` in an expression, so
//! `intro.tags.item(2)` means what it means after `#` anywhere else.

use super::context::EvalContext;
use super::error::EvalError;
use super::parts::normalize_path;
use super::value::{DirRef, DocRef, Env, EvalValue};

/// The name under which the document to resolve a leading `#id` against is
/// bound in an [`Env`]. A real name can never contain `#`, so no lambda
/// parameter can collide with it.
pub(crate) const ROOT: &str = "#doc";

/// The scheme of `text` when it points at something outside the document it is written in, which is
/// every address with a document on the left of `#`. The schemes `address_value` reads, and no more.
pub(crate) fn outside_scheme(text: &str) -> Option<&'static str> {
    let text = text.trim();
    let left = text.split_once('#').map_or(text, |(left, _)| left);
    let (scheme, _) = left.split_once(':')?;
    ["tmt", "ref", "dir"].into_iter().find(|s| *s == scheme)
}

/// Reads `text` as an address and returns what it points at.
pub(crate) fn address_value(text: &str, cx: &EvalContext) -> Result<EvalValue, EvalError> {
    let text = text.trim();
    let (left, inner) = match text.split_once('#') {
        Some((left, inner)) => (left, Some(inner)),
        None => (text, None),
    };

    let base = if left.is_empty() {
        EvalValue::Document(cx.self_doc.clone())
    } else {
        let (scheme, rest) = left
            .split_once(':')
            .ok_or_else(|| EvalError::BadAddress(text.to_string()))?;
        match scheme {
            "tmt" => EvalValue::Document(DocRef(normalize_path(rest))),
            "ref" => EvalValue::Document(
                cx.resolver
                    .note(rest.trim())
                    .map_err(|why| EvalError::Unresolved(format!("note `{rest}`: {why}")))?,
            ),
            "dir" => EvalValue::Directory(DirRef(normalize_path(rest))),
            "file" | "https" | "http" => {
                return Err(EvalError::NotYetSupported("a `file:` or `https:` address"));
            }
            _ => return Err(EvalError::BadAddress(text.to_string())),
        }
    };

    let Some(inner) = inner.filter(|i| !i.is_empty()) else {
        return Ok(base);
    };
    let EvalValue::Document(_) = base else {
        return Err(EvalError::message(format!(
            "`{text}`: a directory has nothing inside it to address"
        )));
    };
    let interp = tomet_ast_parser::parse_interpolation_str(&format!("{{$ #{inner} }}"))
        .map_err(|_| EvalError::BadAddress(text.to_string()))?;
    super::eval(&interp.expr, cx, &Env::new().bind(ROOT, base))
}
