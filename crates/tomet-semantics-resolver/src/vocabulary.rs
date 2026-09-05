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
use tomet_semantics::{Bindings, Vocabulary, document_kind};

/// Every vocabulary `config` declares, with whatever went wrong.
#[derive(Debug, Default)]
pub struct LoadedVocabularies {
    /// By the namespace each file names for itself.
    pub by_namespace: BTreeMap<String, Vocabulary>,
    /// Declared paths that are missing, unreadable, unparseable, headerless,
    /// claim a namespace twice, or shadow a builtin name.
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

/// The namespaces `doc` has in scope.
///
/// Its `@kind(X)` binds the vocabulary calling itself `X`, with no
/// declaration in the document -- the kind is already on the first line,
/// which is what earns its names the right to be written bare. Everything
/// else has to be asked for with `@use`, and is always written out.
pub fn bindings_for(doc: &Document, loaded: &LoadedVocabularies) -> Bindings {
    let kind = document_kind(doc)
        .and_then(|kind| loaded.by_namespace.get(&kind))
        .cloned();

    let mut used = BTreeMap::new();
    for namespace in used_namespaces(doc) {
        if let Some(vocab) = loaded.by_namespace.get(&namespace) {
            used.insert(namespace, vocab.clone());
        }
    }

    Bindings { kind, used }
}

/// The namespaces a document asks for with `@use`.
///
/// The argument is a path, and the namespace is whatever that file calls
/// itself -- so this resolves the path back through what was declared,
/// rather than guessing a name from the filename.
fn used_namespaces(doc: &Document) -> Vec<String> {
    use tomet_ast::Block;
    use tomet_semantics::{ElementKind, classify_lenient, normalized_element_args};
    use tomet_tree::ValueExt;

    let mut names = Vec::new();
    for block in &doc.blocks {
        let Block::Element(el) = block else { continue };
        if classify_lenient(el) != ElementKind::Use {
            continue;
        }
        let Some(args) = normalized_element_args(el) else {
            continue;
        };
        let target = args
            .as_str()
            .map(str::to_string)
            .or_else(|| {
                args.get("target")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            })
            .or_else(|| {
                args.get("file")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            });
        if let Some(target) = target {
            // The declared list is keyed by namespace, and a `@use` names
            // a file. The file's stem is the best link available until
            // `@use` resolution reads the file itself.
            let stem = Path::new(&target)
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.trim_end_matches(".tmt").trim_end_matches(".vocabulary"))
                .unwrap_or(&target);
            names.push(stem.to_string());
        }
    }
    names
}
