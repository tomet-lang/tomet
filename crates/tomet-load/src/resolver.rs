//! The evaluator's view of a vault on disk.

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use tomet_ast::{Block, File};
use tomet_expr::{DirRef, DocRef, EvalValue, PreparedBlocks, Resolver};

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

    fn vault_relative_path(&self, raw: &str) -> Result<PathBuf, String> {
        if raw.starts_with('/') || raw.starts_with('\\') || Path::new(raw).is_absolute() {
            return Err(format!("`{raw}` is not inside the vault"));
        }
        let mut normalized = PathBuf::new();
        for comp in Path::new(raw).components() {
            match comp {
                Component::Normal(c) => normalized.push(c),
                Component::CurDir => {}
                Component::ParentDir => {
                    if !normalized.pop() {
                        return Err(format!("`{raw}` is not inside the vault"));
                    }
                }
                Component::RootDir | Component::Prefix(_) => {
                    return Err(format!("`{raw}` is not inside the vault"));
                }
            }
        }
        let path = self.vault.root.join(&normalized);
        if path.exists() {
            let canonical_root = self.vault.root.canonicalize().map_err(|e| e.to_string())?;
            let canonical_target = path.canonicalize().map_err(|e| e.to_string())?;
            if !canonical_target.starts_with(&canonical_root) {
                return Err(format!("`{raw}` is not inside the vault"));
            }
        }
        Ok(path)
    }

    fn document_path(&self, doc: &DocRef) -> Result<PathBuf, String> {
        let path = self
            .vault_relative_path(&doc.0)
            .map_err(|_| format!("`{}` is not a document of this vault", doc.0))?;
        if !tomet_indexer::is_tm_file(&path) {
            return Err(format!("`{}` is not a document of this vault", doc.0));
        }
        Ok(path)
    }
}

impl Resolver for DiskResolver<'_> {
    fn document(&self, doc: &DocRef) -> Result<Arc<File>, String> {
        let path = self.document_path(doc)?;
        if let Some(cached) = self.vault.cached_file(&doc.0) {
            return Ok(cached);
        }
        let src = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let parsed = tomet_ast_parser::parse_file(&src).map_err(|e| e.to_string())?;
        Ok(self.vault.cache_file(&doc.0, parsed))
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
        let abs = self
            .vault_relative_path(&dir.0)
            .map_err(|_| format!("`{}` is not a directory of this vault", dir.0))?;
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

    fn source(&self, doc: &DocRef) -> Result<String, String> {
        let path = self.document_path(doc)?;
        std::fs::read_to_string(&path).map_err(|e| e.to_string())
    }

    fn prepare_blocks(
        &self,
        doc: &DocRef,
        blocks: Vec<Block>,
        includes: &[DocRef],
    ) -> Result<PreparedBlocks, String> {
        self.vault.prepare_blocks(doc, blocks, includes, self)
    }

    fn rebase_links(&self, blocks: Vec<Block>, from: &DocRef, to: &DocRef) -> Vec<Block> {
        tomet_semantics::rebase_relative_links(blocks, &from.0, &to.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tomet_ast::Tuple;
    use tomet_scalar::Scalar;

    const SETTINGS: &str = "::kind(config)
::field(
  tags: (type: list)
  created: (type: datetime)
)
";

    fn fixture(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(name);
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("notes/deep")).expect("temp dirs");
        fs::write(root.join(tomet_load_config::VAULT_CONFIG_FILE), SETTINGS).expect("config");
        fs::write(
            root.join("notes/rust.tmt"),
            "::meta(tags: (rust), created: \"2026-01-01\")\n\n=[ Rust ]#(top)\n",
        )
        .expect("rust");
        fs::write(root.join("notes/deep/go.tmt"), "::meta(title: Go)\n").expect("go");
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

        let EvalValue::Tuple(rust) = resolver.meta(&DocRef("notes/rust.tmt".into())).unwrap()
        else {
            panic!("meta is a tuple");
        };
        assert!(matches!(rust.get("created"), Some(EvalValue::DateTime(_))));
        assert_eq!(
            rust.get("tags"),
            Some(&EvalValue::Tuple(Tuple::from_unnamed(vec![
                EvalValue::Scalar(Scalar::Text("rust".into()))
            ])))
        );

        // A document that has neither field still has both, typed.
        let EvalValue::Tuple(go) = resolver.meta(&DocRef("notes/deep/go.tmt".into())).unwrap()
        else {
            panic!("meta is a tuple");
        };
        assert_eq!(go.get("tags"), Some(&EvalValue::Tuple(Tuple::new())));
        assert_eq!(go.get("created"), Some(&EvalValue::Scalar(Scalar::None)));
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
        use tomet_expr::{EvalContext, evaluate};

        let root = fixture("tomet_test_disk_resolver_eval");
        let vault = Vault::discover(&root);
        let resolver = vault.resolver();
        let macros = HashMap::new();
        let var = EvalValue::Scalar(Scalar::None);
        let cx = EvalContext {
            resolver: &resolver,
            self_doc: DocRef("index.tmt".into()),
            macros: &macros,
            var: &var,
            var_unset: &[],
            includes: &[],
            heading_level: 0,
            notes: Default::default(),
        };
        let interp = tomet_ast_parser::parse_interpolation_str(
            "{$ vault::docs.filter(d => d::meta.tags.contains(\"rust\")).map(d => d::path) }",
        )
        .unwrap();
        assert_eq!(
            evaluate(&interp.expr, &cx).unwrap(),
            EvalValue::Tuple(Tuple::from_unnamed(vec![EvalValue::Scalar(Scalar::Text(
                "notes/rust.tmt".into()
            ))]))
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn documents_and_directories_must_stay_inside_the_vault() {
        use std::collections::HashMap;
        use tomet_expr::{EvalContext, evaluate};

        let root = fixture("tomet_test_disk_resolver_security_root");
        let outside = std::env::temp_dir().join("tomet_test_disk_resolver_security_outside");
        let _ = fs::remove_dir_all(&outside);
        fs::create_dir_all(&outside).expect("outside dir");

        // Write secrets outside the vault
        let outside_secret_file = outside.join("outside_secret.tmt");
        fs::write(&outside_secret_file, "SECRET_OUTSIDE_DOC\n").expect("outside secret doc");
        let outside_dir = outside.join("outside_dir");
        fs::create_dir_all(&outside_dir).expect("outside subdir");
        fs::write(
            outside_dir.join("nested.tmt"),
            "::kind(secret)\n\nSECRET_DIR_DOC\n",
        )
        .expect("outside dir doc");

        // Write secrets / non-tmt files inside the vault
        fs::write(root.join("data.txt"), "SECRET_IN_VAULT_NON_TMT\n").expect("data.txt");
        fs::write(root.join("notes/deep/UPPER.TMT"), "::kind(upper)\n\nUpper doc\n")
            .expect("upper.TMT");

        let vault = Vault::discover(&root);
        let resolver = vault.resolver();

        // 1. Normal document, ./a.tmt, and sub/../a.tmt still work
        assert!(resolver.document(&DocRef("notes/rust.tmt".into())).is_ok());
        assert_eq!(
            resolver.source(&DocRef("notes/rust.tmt".into())).unwrap(),
            "::meta(tags: (rust), created: \"2026-01-01\")\n\n=[ Rust ]#(top)\n"
        );
        assert!(
            resolver
                .document(&DocRef("./notes/rust.tmt".into()))
                .is_ok()
        );
        assert_eq!(
            resolver.source(&DocRef("./notes/rust.tmt".into())).unwrap(),
            "::meta(tags: (rust), created: \"2026-01-01\")\n\n=[ Rust ]#(top)\n"
        );
        assert!(
            resolver
                .document(&DocRef("notes/deep/../rust.tmt".into()))
                .is_ok()
        );
        assert_eq!(
            resolver
                .source(&DocRef("notes/deep/../rust.tmt".into()))
                .unwrap(),
            "::meta(tags: (rust), created: \"2026-01-01\")\n\n=[ Rust ]#(top)\n"
        );

        // 2. Uppercase .TMT works according to the indexer's extension test (case-insensitive)
        assert!(
            resolver
                .document(&DocRef("notes/deep/UPPER.TMT".into()))
                .is_ok()
        );
        assert_eq!(
            resolver
                .source(&DocRef("notes/deep/UPPER.TMT".into()))
                .unwrap(),
            "::kind(upper)\n\nUpper doc\n"
        );

        // 3. Non-document in vault: data.txt refused without leaking content
        let err_doc = resolver
            .document(&DocRef("data.txt".into()))
            .unwrap_err();
        assert!(err_doc.contains("`data.txt` is not a document of this vault"));
        assert!(!err_doc.contains("SECRET"));
        let err_src = resolver.source(&DocRef("data.txt".into())).unwrap_err();
        assert!(err_src.contains("`data.txt` is not a document of this vault"));
        assert!(!err_src.contains("SECRET"));

        // 4. Leaving vault via ../x.tmt refused without leaking content
        let err_rel = resolver
            .document(&DocRef("../outside_secret.tmt".into()))
            .unwrap_err();
        assert!(err_rel.contains("not a document of this vault"));
        assert!(!err_rel.contains("SECRET"));
        let err_rel_src = resolver
            .source(&DocRef("../outside_secret.tmt".into()))
            .unwrap_err();
        assert!(err_rel_src.contains("not a document of this vault"));
        assert!(!err_rel_src.contains("SECRET"));

        // 5. Absolute path refused without leaking content
        let abs_path_str = outside_secret_file.to_str().unwrap().to_string();
        let err_abs = resolver
            .document(&DocRef(abs_path_str.clone()))
            .unwrap_err();
        assert!(err_abs.contains("not a document of this vault"));
        assert!(!err_abs.contains("SECRET"));
        let err_abs_src = resolver.source(&DocRef(abs_path_str)).unwrap_err();
        assert!(err_abs_src.contains("not a document of this vault"));
        assert!(!err_abs_src.contains("SECRET"));

        // 6. Leaving vault via sub/../../x.tmt refused without leaking content
        let err_sub = resolver
            .document(&DocRef("notes/../../outside_secret.tmt".into()))
            .unwrap_err();
        assert!(err_sub.contains("not a document of this vault"));
        assert!(!err_sub.contains("SECRET"));
        let err_sub_src = resolver
            .source(&DocRef("notes/../../outside_secret.tmt".into()))
            .unwrap_err();
        assert!(err_sub_src.contains("not a document of this vault"));
        assert!(!err_sub_src.contains("SECRET"));

        // 7. Symlinks pointing outside (Unix)
        #[cfg(unix)]
        {
            let sym_file = root.join("notes/sym_outside.tmt");
            std::os::unix::fs::symlink(&outside_secret_file, &sym_file).expect("symlink file");
            let err_sym = resolver
                .document(&DocRef("notes/sym_outside.tmt".into()))
                .unwrap_err();
            assert!(err_sym.contains("not a document of this vault"));
            assert!(!err_sym.contains("SECRET"));
            let err_sym_src = resolver
                .source(&DocRef("notes/sym_outside.tmt".into()))
                .unwrap_err();
            assert!(err_sym_src.contains("not a document of this vault"));
            assert!(!err_sym_src.contains("SECRET"));

            let sym_dir = root.join("notes/sym_dir");
            std::os::unix::fs::symlink(&outside_dir, &sym_dir).expect("symlink dir");
            let err_sym_dir = resolver
                .dir_docs(&DirRef("notes/sym_dir".into()))
                .unwrap_err();
            assert!(err_sym_dir.contains("not a directory of this vault"));
            assert!(!err_sym_dir.contains("SECRET"));
            let err_sym_dir_doc = resolver
                .document(&DocRef("notes/sym_dir/nested.tmt".into()))
                .unwrap_err();
            assert!(err_sym_dir_doc.contains("not a document of this vault"));
            assert!(!err_sym_dir_doc.contains("SECRET"));
        }

        // 8. Directory escaping: addr(dir:..) and absolute directory
        let err_dir_parent = resolver.dir_docs(&DirRef("..".into())).unwrap_err();
        assert!(err_dir_parent.contains("not a directory of this vault"));
        assert!(!err_dir_parent.contains("SECRET"));

        let err_dir_deep = resolver
            .dir_docs(&DirRef("notes/../../..".into()))
            .unwrap_err();
        assert!(err_dir_deep.contains("not a directory of this vault"));
        assert!(!err_dir_deep.contains("SECRET"));

        let outside_dir_str = outside.to_str().unwrap().to_string();
        let err_dir_abs = resolver.dir_docs(&DirRef(outside_dir_str)).unwrap_err();
        assert!(err_dir_abs.contains("not a directory of this vault"));
        assert!(!err_dir_abs.contains("SECRET"));

        // 9. End-to-end expression evaluation
        let macros = HashMap::new();
        let var = EvalValue::Scalar(Scalar::None);
        let cx = EvalContext {
            resolver: &resolver,
            self_doc: DocRef("index.tmt".into()),
            macros: &macros,
            var: &var,
            var_unset: &[],
            includes: &[],
            heading_level: 0,
            notes: Default::default(),
        };

        let interp =
            tomet_ast_parser::parse_interpolation_str("{$ tmt(\"data.txt\")::source }").unwrap();
        let err_eval = evaluate(&interp.expr, &cx).unwrap_err();
        let err_eval_msg = err_eval.to_string();
        assert!(err_eval_msg.contains("not a document of this vault"));
        assert!(!err_eval_msg.contains("SECRET"));

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&outside);
    }
}
