use tomet_ast::{Document, Span, Value};
use tomet_tree::{ElementExt, ValueExt, for_each_element};

/// Walks every node in `doc` and returns `(id, span)` for each `id` key
/// found in an attached `Value::Map` (see `ElementExt::attrs_view`), in document order.
pub(crate) fn collect_ids(doc: &Document) -> Vec<(String, Span)> {
    let mut ids = Vec::new();
    for_each_element(doc, |el| {
        if let Some(id) = id_from_value(el.attrs_view()) {
            ids.push((id, el.span));
        }
    });
    ids
}

/// Looks up an `id` key in a `Value::Map` and renders it to a comparable
/// string (`Value::String` as-is, `Value::Int` via `to_string`; any other
/// shape isn't treated as an id).
fn id_from_value(value: Option<Value>) -> Option<String> {
    let value = value?;
    let id_val = value.get("id")?;
    match id_val {
        Value::String(s) => Some(s.clone()),
        Value::Int(i) => Some(i.to_string()),
        _ => None,
    }
}

use tomet_cst::{SyntaxKind, SyntaxNode, TextRange};

/// Scans `root` for all `id: <value>` property occurrences and returns their exact [`TextRange`]s.
pub(crate) fn collect_ids_cst(root: &SyntaxNode) -> Vec<(String, TextRange)> {
    let mut ids = Vec::new();
    let tokens: Vec<_> = root
        .descendants_with_tokens()
        .filter_map(|el| el.into_token())
        .collect();

    let mut i = 0;
    while i < tokens.len() {
        if tokens[i].kind() == SyntaxKind::IDENT && tokens[i].text() == "id" {
            let mut j = i + 1;
            while j < tokens.len() && tokens[j].kind().is_trivia() {
                j += 1;
            }
            if j < tokens.len() && tokens[j].kind() == SyntaxKind::COLON {
                let mut k = j + 1;
                while k < tokens.len() && tokens[k].kind().is_trivia() {
                    k += 1;
                }
                if k < tokens.len() {
                    let val_token = &tokens[k];
                    let kind = val_token.kind();
                    if kind == SyntaxKind::IDENT
                        || kind == SyntaxKind::INT_NUMBER
                        || kind == SyntaxKind::STRING_LITERAL
                    {
                        let text = val_token
                            .text()
                            .trim_matches('"')
                            .trim_matches('\'')
                            .to_string();
                        ids.push((text, val_token.text_range()));
                    }
                }
            }
        }
        i += 1;
    }
    ids
}
