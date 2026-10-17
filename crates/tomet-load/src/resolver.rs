//! The evaluator's view of a vault on disk.

use std::path::Path;
use std::sync::Arc;

use tomet_ast::Document;
use tomet_expr::{DirRef, DocRef, EvalValue, Resolver};

use crate::Vault;

/// Answers the evaluator's questions from the vault's files.
///
/// Documents are read and parsed once and kept by the [`Vault`], so
/// `vault.docs` in two documents of one export run reads each file once.
/// A path is the vault-relative one with `/` separators, whatever the
/// platform.
pub struct DiskResolver<'v> {
    vault: &'v Vault,
}

impl<'v> DiskResolver<'v> {
    pub(crate) fn new(vault: &'v Vault) -> Self {
        Self { vault }
    }

    fn relative(&self, path: &Path) -> Option<DocRef> {
        let rel = path.strip_prefix(&self.vault.root).unwrap_or(path);
        Some(DocRef(rel.to_str()?.replace('\\', "/")))
    }
}

impl Resolver for DiskResolver<'_> {
    fn document(&self, doc: &DocRef) -> Result<Arc<Document>, String> {
        if let Some(cached) = self.vault.cached_document(&doc.0) {
            return Ok(cached);
        }
        let path = self.vault.root.join(&doc.0);
        let src = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let parsed = tomet_parser::parse_document(&src).map_err(|e| e.to_string())?;
        Ok(self.vault.cache_document(&doc.0, parsed))
    }

    fn note(&self, name: &str) -> Result<DocRef, String> {
        let index = self.vault.link_index();
        let found = index
            .resolve_ref(name, None)
            .filter(|path| path.extension().is_some_and(|ext| ext == "tmt"))
            .ok_or_else(|| "no such note".to_string())?;
        self.relative(found)
            .ok_or_else(|| "the note's path is not text".to_string())
    }

    fn dir_docs(&self, dir: &DirRef) -> Result<Vec<DocRef>, String> {
        let abs = if dir.0.is_empty() {
            self.vault.root.clone()
        } else {
            self.vault.root.join(&dir.0)
        };
        if !abs.is_dir() {
            return Err("not a directory".to_string());
        }
        Ok(
            tomet_indexer::collect_tm_files_with_config(&abs, &self.vault.config, &self.vault.root)
                .iter()
                .filter_map(|path| self.relative(path))
                .collect(),
        )
    }

    fn meta(&self, doc: &DocRef) -> Result<EvalValue, String> {
        let document = self.document(doc)?;
        Ok(tomet_vault::typed_meta(&self.vault.config, &document, doc))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    const SETTINGS: &str = "@kind(config)
@settings{
  meta: {
    tags: { type: list }
    created: { type: datetime }
  }
}
";

    fn fixture(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(name);
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("notes/deep")).expect("temp dirs");
        fs::write(root.join("default.config.tmt"), SETTINGS).expect("config");
        fs::write(
            root.join("notes/rust.tmt"),
            "@meta{ tags: list(rust), created: \"2026-01-01\" }\n\n=[ Rust ]#(top)\n",
        )
        .expect("rust");
        fs::write(root.join("notes/deep/go.tmt"), "@meta{ title: Go }\n").expect("go");
        fs::write(root.join("index.tmt"), "Index\n").expect("index");
        root
    }

    #[test]
    fn a_directory_lists_its_documents_sorted_and_relative() {
        let root = fixture("tomet_test_disk_resolver_dir");
        let vault = Vault::discover(&root);
        let resolver = vault.resolver();
        let docs = resolver.dir_docs(&DirRef("notes".into())).unwrap();
        assert_eq!(
            docs,
            [
                DocRef("notes/deep/go.tmt".into()),
                DocRef("notes/rust.tmt".into())
            ]
        );
        // The root directory is every document, the config file excepted
        // only if the settings say so.
        let all = resolver.dir_docs(&DirRef(String::new())).unwrap();
        assert!(all.contains(&DocRef("index.tmt".into())));
        assert!(all.contains(&DocRef("notes/rust.tmt".into())));
        assert!(resolver.dir_docs(&DirRef("nope".into())).is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_document_is_read_once_and_shared() {
        let root = fixture("tomet_test_disk_resolver_cache");
        let vault = Vault::discover(&root);
        let resolver = vault.resolver();
        let first = resolver.document(&DocRef("notes/rust.tmt".into())).unwrap();
        let second = resolver.document(&DocRef("notes/rust.tmt".into())).unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        assert!(resolver.document(&DocRef("missing.tmt".into())).is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn meta_is_typed_by_the_vaults_settings() {
        let root = fixture("tomet_test_disk_resolver_meta");
        let vault = Vault::discover(&root);
        let resolver = vault.resolver();

        let EvalValue::Map(rust) = resolver.meta(&DocRef("notes/rust.tmt".into())).unwrap() else {
            panic!("meta is a map");
        };
        let get = |entries: &[(String, EvalValue)], key: &str| {
            entries
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
        };
        assert!(matches!(
            get(&rust, "created"),
            Some(EvalValue::DateTime(_))
        ));
        assert_eq!(
            get(&rust, "tags"),
            Some(EvalValue::Seq(vec![EvalValue::String("rust".into())]))
        );

        // A document that has neither field still has both, typed.
        let EvalValue::Map(go) = resolver.meta(&DocRef("notes/deep/go.tmt".into())).unwrap() else {
            panic!("meta is a map");
        };
        assert_eq!(get(&go, "tags"), Some(EvalValue::Seq(vec![])));
        assert_eq!(get(&go, "created"), Some(EvalValue::Null));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_note_is_found_by_its_name() {
        let root = fixture("tomet_test_disk_resolver_note");
        let vault = Vault::discover(&root);
        let resolver = vault.resolver();
        assert_eq!(
            resolver.note("go").unwrap(),
            DocRef("notes/deep/go.tmt".into())
        );
        assert_eq!(
            resolver.note("rust").unwrap(),
            DocRef("notes/rust.tmt".into())
        );
        assert!(resolver.note("nope").is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_evaluator_runs_over_the_disk_through_this_resolver() {
        use std::collections::HashMap;
        use tomet_expr::{EvalContext, Phase, SystemGenerate, evaluate};

        let root = fixture("tomet_test_disk_resolver_eval");
        let vault = Vault::discover(&root);
        let resolver = vault.resolver();
        let macros = HashMap::new();
        let vars = EvalValue::Null;
        let cx = EvalContext {
            phase: Phase::Output,
            resolver: &resolver,
            generate: &SystemGenerate,
            self_doc: DocRef("index.tmt".into()),
            macros: &macros,
            vars: &vars,
        };
        let element = tomet_parser::parse_dollar_element_str(
            "${vault.docs.filter(d => d.meta.tags.contains(\"rust\")).map(d => d.path)}",
        )
        .unwrap();
        let Some(tomet_ast::ElementValue::Interp(expr)) = element.value else {
            panic!("an expression");
        };
        assert_eq!(
            evaluate(&expr, &cx).unwrap(),
            EvalValue::Seq(vec![EvalValue::String("notes/rust.tmt".into())])
        );
        let _ = fs::remove_dir_all(&root);
    }
}
