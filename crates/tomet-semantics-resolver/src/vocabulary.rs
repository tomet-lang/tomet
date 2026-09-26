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

use std::path::Path;

use tomet_ast::Document;
use tomet_semantics::Bindings;

pub use tomet_semantics::LoadedVocabularies;

/// Reads every vocabulary in `declared`, as paths relative to `root`.
///
/// Takes the paths rather than a `PrinterConfig`, because a config lives
/// a layer above this one -- and because this does not need to know what
/// a config is, only where the files are. Reading and parsing are all it
/// does; whether a vocabulary is acceptable is
/// [`LoadedVocabularies::add`], in `tomet-semantics`.
pub fn load_vocabularies(root: &Path, declared: &[String]) -> LoadedVocabularies {
    let mut loaded = LoadedVocabularies::default();

    for declared in declared {
        let path = root.join(declared);
        match std::fs::read_to_string(&path) {
            Ok(src) => add_source(&mut loaded, declared, &src),
            Err(e) => loaded
                .errors
                .push(format!("declared vocabulary {declared}: {e}")),
        }
    }

    loaded
}

/// The same as [`load_vocabularies`] for sources the caller already holds,
/// as `(label, source text)` pairs. Touches nothing outside memory, so a
/// host with no filesystem gets the same verdicts as one with a vault.
pub fn load_vocabulary_sources(sources: &[(&str, &str)]) -> LoadedVocabularies {
    let mut loaded = LoadedVocabularies::default();
    for (label, src) in sources {
        add_source(&mut loaded, label, src);
    }
    loaded
}

fn add_source(loaded: &mut LoadedVocabularies, label: &str, src: &str) {
    match tomet_parser::parse_document(src) {
        Ok(doc) => loaded.add(label, &doc),
        Err(e) => loaded
            .errors
            .push(format!("declared vocabulary {label}: {e}")),
    }
}

/// The namespaces `doc` has in scope, from what was loaded off disk.
///
/// The decision itself is `Bindings::for_document`, which is pure. This
/// only supplies what reading the filesystem found, so a caller that
/// cannot read one -- a wasm host, say -- can call that directly with
/// vocabularies it obtained some other way.
pub fn bindings_for(doc: &Document, loaded: &LoadedVocabularies) -> Bindings {
    loaded.bindings_for(doc)
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
