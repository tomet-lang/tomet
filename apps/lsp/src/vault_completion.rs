//! Completions that need the vault: what a vocabulary declares, the ids of a
//! document, and the documents there are.
//!
//! The text alone knows the built-in elements and the address schemes
//! ([`completions_for_with_uri`]). It cannot know that `::kind(deck)` brings a
//! `card`, which ids a document has, or which documents a vault holds -- those
//! belong to the vault the document is in, the same one diagnostics and hover
//! answer from.

use lsp_types::{CompletionItem, CompletionItemKind, Position, Uri};
use tomet_ast::Document;
use tomet_expr::{DirRef, DocRef, Resolver};
use tomet_load::Vault;

use crate::completion::completions_for_with_uri;
use crate::position::get_line_prefix;

/// The completions at `pos` in `text`, a document of `vault`.
///
/// What is being typed decides what is offered, from the line up to the cursor:
///
/// - after `tmt("` or `tmt:`, the documents of the vault;
/// - after `#` where a reference is written (in `${...}`, after `@link(`), the
///   ids of this document, or of the one named on the left of the `#`
///   (`tmt("p")#`, `tmt:p#`);
/// - after `@`, the built-in elements and those of the vocabularies in scope;
/// - anything else is the text alone's to say.
///
/// A `#` that starts a declaration (`#(id)`, `#[`) is not a reference, and gets
/// none of the ids.
pub(crate) fn completions(
    vault: &Vault,
    text: &str,
    pos: Position,
    uri: Option<&Uri>,
) -> Vec<CompletionItem> {
    let prefix = get_line_prefix(text, pos);

    if let Some(target) = id_reference(prefix) {
        return ids(vault, text, pos, target.as_deref());
    }
    if is_document_path(prefix) {
        return documents(vault);
    }

    let mut items = completions_for_with_uri(text, pos, uri);
    if prefix.ends_with('@') {
        items.extend(vocabulary_elements(vault, text, pos));
    }
    items
}

/// Whether the cursor is inside a `${ ... }` that has not been closed.
fn in_expression(prefix: &str) -> bool {
    match (prefix.rfind("${"), prefix.rfind('}')) {
        (Some(open), Some(close)) => open > close,
        (Some(_), None) => true,
        _ => false,
    }
}

/// If the cursor is on the id after a `#` that writes a reference: `Some(None)`
/// for this document, `Some(Some(path))` for the document the `#` follows.
fn id_reference(prefix: &str) -> Option<Option<String>> {
    let hash = prefix.rfind('#')?;
    let (left, typed) = (&prefix[..hash], &prefix[hash + 1..]);
    if !typed
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        return None;
    }
    if let Some(path) = document_before(left) {
        return Some(Some(path));
    }
    if in_expression(left) || left.trim_end().ends_with("@link(") {
        return Some(None);
    }
    None
}

/// The path of the document a `#` follows: `tmt("p")#` and `tmt:p#`.
fn document_before(left: &str) -> Option<String> {
    if let Some(open) = left.rfind("tmt(\"") {
        let after = &left[open + 5..];
        if let Some(end) = after.find("\")")
            && end + 2 == after.len()
        {
            return Some(after[..end].to_string());
        }
    }
    let scheme = left.rfind("tmt:")?;
    let path = &left[scheme + 4..];
    let ends_the_path = |c: char| c.is_whitespace() || matches!(c, ')' | ',' | ']' | '}' | '"');
    if path.is_empty() || path.contains(ends_the_path) {
        return None;
    }
    Some(path.to_string())
}

/// Whether the cursor is on the path of a document: `tmt("p` and `tmt:p`.
fn is_document_path(prefix: &str) -> bool {
    if let Some(open) = prefix.rfind("tmt(\"") {
        return !prefix[open + 5..].contains('"');
    }
    if let Some(scheme) = prefix.rfind("tmt:") {
        let path = &prefix[scheme + 4..];
        let ends_the_path =
            |c: char| c.is_whitespace() || matches!(c, ')' | ',' | ']' | '}' | '"' | '#');
        return !path.contains(ends_the_path);
    }
    false
}

/// The documents of the vault, by the path the language measures them from.
fn documents(vault: &Vault) -> Vec<CompletionItem> {
    let mut paths: Vec<String> = vault
        .resolver()
        .dir_docs(&DirRef(String::new()))
        .unwrap_or_default()
        .into_iter()
        .map(|doc| doc.0)
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path| CompletionItem {
            label: path.clone(),
            insert_text: Some(path),
            kind: Some(CompletionItemKind::FILE),
            detail: Some("A document of this vault".to_string()),
            ..CompletionItem::default()
        })
        .collect()
}

/// The ids of this document, or of `other` -- one of the vault's.
fn ids(vault: &Vault, text: &str, pos: Position, other: Option<&str>) -> Vec<CompletionItem> {
    let (collected, whose) = match other {
        Some(path) => {
            let doc = vault.resolver().document(&DocRef(path.to_string()));
            match doc {
                Ok(doc) => (tomet_validator::collect_ids(&doc), path.to_string()),
                Err(_) => return Vec::new(),
            }
        }
        None => match parse_while_typing(vault, text, pos) {
            Some(doc) => (
                tomet_validator::collect_ids(&doc),
                "this document".to_string(),
            ),
            None => return Vec::new(),
        },
    };
    let mut seen = std::collections::HashSet::new();
    collected
        .into_iter()
        .filter(|(id, _)| seen.insert(id.clone()))
        .map(|(id, _)| CompletionItem {
            label: id.clone(),
            insert_text: Some(id),
            kind: Some(CompletionItemKind::REFERENCE),
            detail: Some(format!("An id in {whose}")),
            ..CompletionItem::default()
        })
        .collect()
}

/// The elements of the vocabularies `text` has in scope: the bare names of the
/// vocabulary its `::kind` binds, and `ns.name` for each one it `::use`s.
fn vocabulary_elements(vault: &Vault, text: &str, pos: Position) -> Vec<CompletionItem> {
    let Some(doc) = parse_while_typing(vault, text, pos) else {
        return Vec::new();
    };
    let bindings = vault.bindings(&doc);
    let mut items = Vec::new();
    let mut add = |vocabulary: &tomet_semantics::vocabulary::Vocabulary, qualified: bool| {
        for (name, decl) in &vocabulary.elements {
            let label = if qualified {
                format!("{}.{name}", vocabulary.namespace)
            } else {
                name.clone()
            };
            let params: Vec<&str> = decl.params.iter().map(|p| p.name.as_str()).collect();
            let detail = if params.is_empty() {
                format!("{} element", vocabulary.namespace)
            } else {
                format!("{} element ({})", vocabulary.namespace, params.join(", "))
            };
            items.push(CompletionItem {
                label: label.clone(),
                insert_text: Some(label),
                kind: Some(CompletionItemKind::CLASS),
                detail: Some(detail),
                ..CompletionItem::default()
            });
        }
    };
    if let Some(kind) = &bindings.kind {
        add(kind, false);
    }
    for vocabulary in bindings.used.values() {
        add(vocabulary, true);
    }
    items
}

/// `text` as a document, even while a line of it is being typed.
///
/// What is being typed is rarely valid on its own -- `@link(#` has no closing
/// bracket -- and the rest of the document is what the completion is about. So
/// when `text` does not read, the line with the cursor on it is left out and
/// the rest is read.
fn parse_while_typing(vault: &Vault, text: &str, pos: Position) -> Option<Document> {
    if let Ok((doc, _)) = vault.parse(text) {
        return Some(doc);
    }
    let without_line: String = text
        .split_inclusive('\n')
        .enumerate()
        .map(|(i, line)| if i as u32 == pos.line { "\n" } else { line })
        .collect();
    vault.parse(&without_line).ok().map(|(doc, _)| doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hash_that_writes_a_reference_is_told_from_one_that_declares() {
        // A reference: in an expression, or the target of a link.
        assert_eq!(id_reference("${#"), Some(None));
        assert_eq!(id_reference("see ${add(#"), Some(None));
        assert_eq!(id_reference("@link(#in"), Some(None));
        // Another document's: the one named on the left.
        assert_eq!(
            id_reference("${tmt(\"docs/a.tmt\")#"),
            Some(Some("docs/a.tmt".to_string()))
        );
        assert_eq!(
            id_reference("@link(tmt:docs/a.tmt#in"),
            Some(Some("docs/a.tmt".to_string()))
        );
        // A declaration or a heading is not a reference.
        assert_eq!(id_reference("=[ Title ]#("), None);
        assert_eq!(id_reference("#["), None);
        assert_eq!(id_reference("plain prose #tag"), None);
        // An expression that has been closed is over.
        assert_eq!(id_reference("${x} then #"), None);
    }

    #[test]
    fn the_cursor_is_on_a_document_path_only_inside_one() {
        assert!(is_document_path("${tmt(\""));
        assert!(is_document_path("${tmt(\"docs/"));
        assert!(is_document_path("@link(tmt:"));
        assert!(is_document_path("@link(tmt:docs/a"));
        assert!(!is_document_path("${tmt(\"docs/a.tmt\""));
        assert!(!is_document_path("@link(tmt:docs/a.tmt#"));
        assert!(!is_document_path("@link(file:"));
    }
}
