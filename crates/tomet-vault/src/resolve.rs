//! What the vault adds to the evaluator's view of a document: the settings'
//! `type:` declarations applied to its `::meta`, and the document being
//! prepared standing in for its own path.

use std::sync::Arc;

use tomet_ast::{Block, Document, Value};
use tomet_config::{FieldConfig, PrinterConfig};
use tomet_expr::{DirRef, DocRef, EvalValue, PreparedBlocks, Resolver, typed_value};
use tomet_type::Type;

/// The document's `::meta`, typed by the settings' `meta` declarations.
///
/// - a `list` field is a list: a lone value is wrapped, and the list is
///   empty when the document lacks the field;
/// - a `datetime` field is a datetime when its text reads as one, and stays
///   text when it does not (`tomet check` is what reports the bad one, by
///   the same `parse_datetime`);
/// - a field whose type is text (`string`, `version`, `uuid`, `nanoid`) is the
///   text it was written as, so `rev: 1.10` is `"1.10"` and not `1.1`;
/// - every other declared field is `Null` when the document lacks it;
/// - a key that is neither declared nor written is absent from the map, so
///   reading it is an error and a typo cannot turn into an answer of "no".
///
/// Keys the document writes without declaring them are kept as written.
pub fn typed_meta(config: &PrinterConfig, doc: &Document, doc_ref: &DocRef) -> EvalValue {
    let written = match tomet_declaration::document_meta(doc) {
        Some(Value::Map(entries)) => entries,
        _ => Vec::new(),
    };
    let mut entries: Vec<(String, EvalValue)> = written
        .iter()
        .map(|(key, value)| {
            let value = match config.meta.fields.get(key) {
                Some(field) => typed(field, Some((value, doc_ref))),
                None => EvalValue::from_ast(value, doc_ref),
            };
            (key.clone(), value)
        })
        .collect();
    for (key, field) in &config.meta.fields {
        if !entries.iter().any(|(k, _)| k == key) {
            entries.push((key.clone(), typed(field, None)));
        }
    }
    EvalValue::Map(entries)
}

fn typed(field: &FieldConfig, value: Option<(&Value, &DocRef)>) -> EvalValue {
    let ty = field.field_type.as_deref().and_then(Type::from_name);
    match (ty, value) {
        (Some(Type::List), None) => EvalValue::Seq(Vec::new()),
        (Some(ty), Some((value, doc))) => typed_value(ty, value, doc),
        (None, Some((value, doc))) => EvalValue::from_ast(value, doc),
        (_, None) => EvalValue::Null,
    }
}

/// A resolver in which one document -- the one being prepared -- is what its
/// own path says it is, and everything else is asked of `inner`.
///
/// This is how a reference is answered from the document **as written**:
/// `prepare` rewrites the document in place, and the evaluator must not see
/// a half-rewritten one when `${#intro}` is resolved.
pub struct Overlay<'a> {
    pub path: DocRef,
    pub snapshot: Arc<Document>,
    pub meta: EvalValue,
    pub inner: &'a dyn Resolver,
}

impl Resolver for Overlay<'_> {
    fn document(&self, doc: &DocRef) -> Result<Arc<Document>, String> {
        if *doc == self.path {
            Ok(self.snapshot.clone())
        } else {
            self.inner.document(doc)
        }
    }

    fn note(&self, name: &str) -> Result<DocRef, String> {
        self.inner.note(name)
    }

    fn dir_docs(&self, dir: &DirRef) -> Result<Vec<DocRef>, String> {
        self.inner.dir_docs(dir)
    }

    fn meta(&self, doc: &DocRef) -> Result<EvalValue, String> {
        if *doc == self.path {
            Ok(self.meta.clone())
        } else {
            self.inner.meta(doc)
        }
    }

    fn prepare_blocks(
        &self,
        doc: &DocRef,
        blocks: Vec<Block>,
        includes: &[DocRef],
    ) -> Result<PreparedBlocks, String> {
        self.inner.prepare_blocks(doc, blocks, includes)
    }

    fn rebase_links(&self, blocks: Vec<Block>, from: &DocRef, to: &DocRef) -> Vec<Block> {
        tomet_semantics::rebase_relative_links(blocks, &from.0, &to.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_expr::MemoryResolver;

    fn config(settings: &str) -> PrinterConfig {
        tomet_config::load_config_from_str(settings).expect("valid settings")
    }

    const SETTINGS: &str = "::settings{
  meta: {
    tags: { type: list }
    created: { type: datetime, format: rfc3339 }
    title: { type: string }
    rev: { type: version }
    note: { always_newline: true }
  }
}
";

    fn meta_of(src: &str) -> EvalValue {
        let doc = tomet_parser::parse_document(src).unwrap();
        typed_meta(&config(SETTINGS), &doc, &DocRef("a.tmt".into()))
    }

    fn field<'a>(meta: &'a EvalValue, key: &str) -> Option<&'a EvalValue> {
        match meta {
            EvalValue::Map(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    #[test]
    fn a_list_field_the_document_lacks_is_an_empty_list() {
        let meta = meta_of("::meta{ title: A }\n");
        assert_eq!(field(&meta, "tags"), Some(&EvalValue::Seq(vec![])));
    }

    #[test]
    fn a_list_field_written_as_one_value_is_a_list_of_one() {
        let meta = meta_of("::meta{ tags: rust }\n");
        assert_eq!(
            field(&meta, "tags"),
            Some(&EvalValue::Seq(vec![EvalValue::String("rust".into())]))
        );
        let meta = meta_of("::meta{ tags: list(a, b) }\n");
        assert_eq!(
            field(&meta, "tags"),
            Some(&EvalValue::Seq(vec![
                EvalValue::String("a".into()),
                EvalValue::String("b".into())
            ]))
        );
    }

    /// A field whose type is text is the text it was written as: a bare `1.10` is the
    /// version `1.10`, not the number `1.1`; and a `title` of `2024` is `"2024"`.
    #[test]
    fn a_text_field_is_the_text_it_was_written_as() {
        let meta = meta_of("::meta{ rev: 1.10, title: 2024 }\n");
        assert_eq!(field(&meta, "rev"), Some(&EvalValue::String("1.10".into())));
        assert_eq!(
            field(&meta, "title"),
            Some(&EvalValue::String("2024".into()))
        );
        // not declared: read as it is, as before
        let meta = meta_of("::meta{ other: 1.10 }\n");
        assert_eq!(field(&meta, "other"), Some(&EvalValue::Float(1.1)));
    }

    #[test]
    fn a_declared_field_the_document_lacks_is_null() {
        let meta = meta_of("::meta{ tags: a }\n");
        assert_eq!(field(&meta, "title"), Some(&EvalValue::Null));
        assert_eq!(field(&meta, "created"), Some(&EvalValue::Null));
        assert_eq!(field(&meta, "note"), Some(&EvalValue::Null));
    }

    #[test]
    fn a_datetime_field_becomes_a_datetime_and_bad_text_stays_text() {
        let meta = meta_of("::meta{ created: \"2026-10-17T09:00:00+09:00\" }\n");
        assert!(matches!(
            field(&meta, "created"),
            Some(EvalValue::DateTime(_))
        ));
        let meta = meta_of("::meta{ created: yesterday }\n");
        assert_eq!(
            field(&meta, "created"),
            Some(&EvalValue::String("yesterday".into()))
        );
    }

    /// The forms `tomet format` rewrites as RFC 3339 are datetimes here too;
    /// a missing offset is UTC.
    #[test]
    fn the_looser_iso_8601_forms_are_datetimes_too() {
        for text in ["2026-10-17", "2026-10-17T09:00", "2026-10-17 09:00:00"] {
            let meta = meta_of(&format!("::meta{{ created: \"{text}\" }}\n"));
            assert!(
                matches!(field(&meta, "created"), Some(EvalValue::DateTime(_))),
                "{text}"
            );
        }
    }

    #[test]
    fn an_undeclared_key_is_absent_unless_written() {
        let meta = meta_of("::meta{ title: A }\n");
        assert_eq!(field(&meta, "draft"), None);
        let meta = meta_of("::meta{ draft: true }\n");
        assert_eq!(field(&meta, "draft"), Some(&EvalValue::Bool(true)));
    }

    #[test]
    fn a_document_without_meta_still_has_the_declared_fields() {
        let meta = meta_of("Just text.\n");
        assert_eq!(field(&meta, "tags"), Some(&EvalValue::Seq(vec![])));
        assert_eq!(field(&meta, "title"), Some(&EvalValue::Null));
    }

    #[test]
    fn the_overlay_answers_for_its_own_path_and_defers_for_the_rest() {
        let inner = MemoryResolver::new()
            .with_source("a.tmt", "::meta{ title: from-inner }\n")
            .unwrap()
            .with_source("b.tmt", "::meta{ title: B }\n")
            .unwrap();
        let rewritten = Arc::new(tomet_parser::parse_document("::meta{ title: mine }\n").unwrap());
        let overlay = Overlay {
            path: DocRef("a.tmt".into()),
            snapshot: rewritten.clone(),
            meta: EvalValue::String("overlaid".into()),
            inner: &inner,
        };
        assert!(Arc::ptr_eq(
            &overlay.document(&DocRef("a.tmt".into())).unwrap(),
            &rewritten
        ));
        assert_eq!(
            overlay.meta(&DocRef("a.tmt".into())),
            Ok(EvalValue::String("overlaid".into()))
        );
        assert!(overlay.document(&DocRef("b.tmt".into())).is_ok());
        assert_ne!(
            overlay.meta(&DocRef("b.tmt".into())),
            Ok(EvalValue::String("overlaid".into()))
        );
    }
}
