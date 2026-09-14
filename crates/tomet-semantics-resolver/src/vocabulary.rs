//! Loading the vocabularies a vault declares, and working out which ones
//! a document has in scope.
//!
//! This is the half of `tomet_semantics::vocabulary` that touches the
//! filesystem. Extraction stays there, pure; finding and reading the
//! files is here, because the semantics layer may not do I/O and
//! `validate_document` promises it does none.
//!
//! Nothing is discovered. A vault lists its vocabularies by path and each
//! file names itself with `@vocabulary(ns)`, the same shape blueprints
//! use -- so a path that rotted is reported where it is written instead
//! of at every document that wanted it.

use std::collections::BTreeMap;
use std::path::Path;

use tomet_ast::Document;
use tomet_semantics::{Bindings, Vocabulary};

/// Every vocabulary `config` declares, with whatever went wrong.
#[derive(Debug, Default)]
pub struct LoadedVocabularies {
    /// By the namespace each file names for itself.
    pub by_namespace: BTreeMap<String, Vocabulary>,
    /// Declared paths that are missing, unreadable, unparseable, headerless,
    /// claim a namespace twice, claim a reserved namespace, or shadow a
    /// builtin name.
    pub errors: Vec<String>,
}

/// Reads every vocabulary in `declared`, as paths relative to `root`.
///
/// Takes the paths rather than a `PrinterConfig`, because a config lives
/// a layer above this one -- and because this does not need to know what
/// a config is, only where the files are.
pub fn load_vocabularies(root: &Path, declared: &[String]) -> LoadedVocabularies {
    let mut loaded = LoadedVocabularies::default();
    let mut source: BTreeMap<String, String> = BTreeMap::new();

    // Hardcoded, not read off disk -- the same reason `std`'s elements are
    // a Rust table rather than a `@vocabulary(std)` document. Seeded first
    // so the loop below reports a vault vocabulary that collides with one
    // of these the same way it reports two vault vocabularies colliding
    // with each other.
    for builtin in tomet_semantics::builtin_doc_vocabularies() {
        source.insert(builtin.namespace.clone(), "<builtin>".to_string());
        loaded.by_namespace.insert(builtin.namespace.clone(), builtin);
    }

    for declared in declared {
        let path = root.join(declared);
        let src = match std::fs::read_to_string(&path) {
            Ok(src) => src,
            Err(e) => {
                loaded
                    .errors
                    .push(format!("declared vocabulary {declared}: {e}"));
                continue;
            }
        };
        let doc = match tomet_parser::parse_document(&src) {
            Ok(doc) => doc,
            Err(e) => {
                loaded
                    .errors
                    .push(format!("declared vocabulary {declared}: {e}"));
                continue;
            }
        };
        let Some(vocab) = Vocabulary::from_document(&doc) else {
            loaded.errors.push(format!(
                "declared vocabulary names no namespace -- it needs `@vocabulary(<ns>)`: {declared}"
            ));
            continue;
        };

        if let Some(reserved) = tomet_semantics::RESERVED_NAMESPACES
            .iter()
            .find(|ns| vocab.namespace == **ns || vocab.namespace.starts_with(&format!("{ns}.")))
        {
            loaded.errors.push(format!(
                "{declared} declares itself `{}`, which is under the reserved `{reserved}` \
                 namespace tomet already gives a hardcoded meaning",
                vocab.namespace
            ));
            continue;
        }

        // Reported here, once, rather than at every document that writes
        // the shadowed name.
        for shadowed in vocab.shadowed_builtins() {
            loaded.errors.push(format!(
                "{declared} declares `{shadowed}`, which is already a built-in name; \
                 std wins, so a vocabulary may not take one"
            ));
        }

        if let Some(first) = source.get(&vocab.namespace) {
            loaded.errors.push(format!(
                "two vocabularies both call themselves `{}`: {first} and {declared}",
                vocab.namespace
            ));
            continue;
        }
        source.insert(vocab.namespace.clone(), declared.clone());
        loaded.by_namespace.insert(vocab.namespace.clone(), vocab);
    }

    loaded
}

/// The namespaces `doc` has in scope, from what was loaded off disk.
///
/// The decision itself is `Bindings::for_document`, which is pure. This
/// only supplies what reading the filesystem found, so a caller that
/// cannot read one -- a wasm host, say -- can call that directly with
/// vocabularies it obtained some other way.
pub fn bindings_for(doc: &Document, loaded: &LoadedVocabularies) -> Bindings {
    Bindings::for_document(doc, loaded.by_namespace.values().cloned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch_dir(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(name);
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("temp dir");
        root
    }

    #[test]
    fn doc_index_resolves_with_no_declared_vocabularies_at_all() {
        let root = scratch_dir("tomet_test_resolver_doc_index");
        let loaded = load_vocabularies(&root, &[]);
        assert!(loaded.errors.is_empty(), "{:?}", loaded.errors);
        assert!(loaded.by_namespace.contains_key("doc.index"));

        let doc = tomet_parser::parse_document("@kind(doc.index)\n\n#[ An index ]\n")
            .expect("document parses");
        let bindings = bindings_for(&doc, &loaded);
        assert!(bindings.kind.is_some());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn doc_icon_resolves_with_no_declared_vocabularies_at_all() {
        let root = scratch_dir("tomet_test_resolver_doc_icon");
        let loaded = load_vocabularies(&root, &[]);
        assert!(loaded.errors.is_empty(), "{:?}", loaded.errors);
        assert!(loaded.by_namespace.contains_key("doc"));

        let doc = tomet_parser::parse_document("a @doc.icon(\"star\", pkg:\"lucide\") b\n")
            .expect("document parses");
        let bindings = bindings_for(&doc, &loaded);
        assert_eq!(
            bindings.classify(&tomet_ast::Name::namespaced("doc", "icon")),
            Ok(tomet_semantics::ElementKind::Custom("doc.icon".to_string()))
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_vault_vocabulary_cannot_claim_the_reserved_doc_namespace() {
        let root = scratch_dir("tomet_test_resolver_reserved_doc");
        fs::write(
            root.join("doc.vocabulary.tmt"),
            "@kind(vocabulary)\n@vocabulary(doc){ version: \"1.0.0\" }\n",
        )
        .expect("vocabulary file");

        let loaded = load_vocabularies(&root, &["doc.vocabulary.tmt".to_string()]);
        assert!(
            loaded
                .errors
                .iter()
                .any(|e| e.contains("reserved") && e.contains("doc")),
            "{:?}",
            loaded.errors
        );
        // The hardcoded one is still there, untouched by the rejected file.
        assert!(loaded.by_namespace.contains_key("doc.index"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_vault_vocabulary_cannot_claim_doc_index_itself() {
        let root = scratch_dir("tomet_test_resolver_reserved_doc_index");
        fs::write(
            root.join("index.vocabulary.tmt"),
            "@kind(vocabulary)\n@vocabulary(doc.index){ version: \"1.0.0\" }\n",
        )
        .expect("vocabulary file");

        let loaded = load_vocabularies(&root, &["index.vocabulary.tmt".to_string()]);
        assert!(
            loaded.errors.iter().any(|e| e.contains("reserved")),
            "{:?}",
            loaded.errors
        );

        let _ = fs::remove_dir_all(&root);
    }
}
