//! The evaluator's view of a vault on disk.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tomet_ast::{Block, File};
use tomet_config::CacheConfig;
use tomet_expr::{CacheRequest, DirRef, DocRef, EvalError, EvalValue, PreparedBlocks, Resolver};

use crate::Vault;

/// Resolves the on-disk cache base directory for `local` (`false` = repo-shared,
/// `true` = machine-local) against `vault_root` and `cache_config`.
///
/// - When `!local`: requires `cache_config.dir` (`::cache(dir: ...)`).
/// - When `local`: requires `cache_config.local == true` (`::cache(local: true)`),
///   using `TOMET_CACHE_DIR` when set and non-empty, otherwise
///   `$XDG_CACHE_HOME/tomet/cache/<vault_id>` (falling back to `~/.cache/tomet/cache/<vault_id>`).
pub fn resolve_cache_dir(
    vault_root: &Path,
    cache_config: &CacheConfig,
    local: bool,
) -> Result<PathBuf, String> {
    let env_dir = std::env::var_os("TOMET_CACHE_DIR");
    let project_cache_dir =
        directories::ProjectDirs::from("", "", "tomet").map(|dirs| dirs.cache_dir().to_path_buf());
    resolve_cache_dir_with(
        vault_root,
        cache_config,
        local,
        env_dir.as_deref(),
        project_cache_dir.as_deref(),
    )
}

/// Resolves the on-disk `.cache.ctmt` path for `req` against `vault_root` and `cache_config`.
pub fn resolve_cache_entry_path(
    vault_root: &Path,
    cache_config: &CacheConfig,
    req: &CacheRequest,
) -> Result<PathBuf, String> {
    let base = resolve_cache_dir(vault_root, cache_config, req.local)?;
    Ok(base.join(tomet_cache::entry_rel_path(&req.kind, &req.key)))
}

fn resolve_cache_dir_with(
    vault_root: &Path,
    cache_config: &CacheConfig,
    local: bool,
    tomet_cache_dir: Option<&OsStr>,
    project_cache_dir: Option<&Path>,
) -> Result<PathBuf, String> {
    if !local {
        let dir = cache_config
            .dir
            .as_deref()
            .filter(|d| !d.is_empty())
            .ok_or_else(|| "`::cache(dir: ...)` is not declared in vault.config.tmt".to_string())?;
        return Ok(vault_root.join(dir));
    }

    if !cache_config.local {
        return Err("`::cache(local: true)` is not enabled in vault.config.tmt".to_string());
    }

    let base_dir = match tomet_cache_dir {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => {
            let canonical_root = vault_root
                .canonicalize()
                .unwrap_or_else(|_| vault_root.to_path_buf());
            let vault_id = tomet_cache::vault_id(&canonical_root);
            project_cache_dir
                .map(Path::to_path_buf)
                .unwrap_or_else(|| std::env::temp_dir().join("tomet"))
                .join("cache")
                .join(vault_id)
        }
    };
    Ok(base_dir)
}

#[cfg(test)]
fn resolve_cache_entry_path_with(
    vault_root: &Path,
    cache_config: &CacheConfig,
    req: &CacheRequest,
    tomet_cache_dir: Option<&OsStr>,
    project_cache_dir: Option<&Path>,
) -> Result<PathBuf, String> {
    let base = resolve_cache_dir_with(
        vault_root,
        cache_config,
        req.local,
        tomet_cache_dir,
        project_cache_dir,
    )?;
    Ok(base.join(tomet_cache::entry_rel_path(&req.kind, &req.key)))
}

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
    fn document(&self, doc: &DocRef) -> Result<Arc<File>, String> {
        let path = self.vault.document_path(&doc.0)?;
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
        let abs = self.vault.directory_path(&dir.0)?;
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
        let path = self.vault.document_path(&doc.0)?;
        std::fs::read_to_string(&path).map_err(|e| e.to_string())
    }

    fn prepare_blocks(
        &self,
        doc: &DocRef,
        blocks: Vec<Block>,
        includes: &[DocRef],
    ) -> Result<PreparedBlocks, String> {
        let scope = self.vault.core.block_scope(doc, includes, self)?;
        let mut holder = File::new(vec![], blocks, tomet_span::Span::default());
        let unresolved = tomet_eval::resolve_expressions(&mut holder, &scope.context()).len();
        Ok(PreparedBlocks {
            blocks: holder.body,
            unresolved,
        })
    }

    fn rebase_links(&self, blocks: Vec<Block>, from: &DocRef, to: &DocRef) -> Vec<Block> {
        tomet_semantics::rebase_relative_links(blocks, &from.0, &to.0)
    }

    fn cache(&self, req: &CacheRequest) -> Result<EvalValue, EvalError> {
        let path = resolve_cache_entry_path(&self.vault.root, &self.vault.config.cache, req)
            .map_err(EvalError::Cache)?;
        let src = match std::fs::read_to_string(&path) {
            Ok(src) => src,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                let prefix = if req.local {
                    "local cache miss"
                } else {
                    "cache miss"
                };
                return Err(EvalError::Cache(format!(
                    "{prefix} for `cache.{}({:?})` (expected `{}`)",
                    req.kind,
                    req.key,
                    path.display()
                )));
            }
            Err(err) => {
                return Err(EvalError::Cache(format!(
                    "failed to read cache entry `{}` for `cache.{}({:?})`: {err}",
                    path.display(),
                    req.kind,
                    req.key
                )));
            }
        };
        let entry = tomet_cache::parse_cache_entry(&src, Some(&req.key)).map_err(|err| {
            EvalError::Cache(format!(
                "invalid cache entry `{}` for `cache.{}({:?})`: {err}",
                path.display(),
                req.kind,
                req.key
            ))
        })?;
        Ok(EvalValue::from_ast(&entry.value, &DocRef(String::new())))
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
        use tomet_eval::evaluate;
        use tomet_expr::EvalContext;

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
        use tomet_eval::evaluate;
        use tomet_expr::EvalContext;

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
        fs::write(
            root.join("notes/deep/UPPER.TMT"),
            "::kind(upper)\n\nUpper doc\n",
        )
        .expect("upper.TMT");

        let vault = Vault::discover(&root);
        let resolver = vault.resolver();

        // 1. Normal document, ./a.tmt, and sub/../a.tmt still work
        assert_eq!(
            vault.document_path("notes/rust.tmt").unwrap(),
            root.join("notes/rust.tmt")
        );
        assert_eq!(
            vault.document_path("./notes/rust.tmt").unwrap(),
            root.join("notes/rust.tmt")
        );
        assert_eq!(
            vault.document_path("notes/deep/../rust.tmt").unwrap(),
            root.join("notes/rust.tmt")
        );
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
        assert_eq!(
            vault.document_path("notes/deep/UPPER.TMT").unwrap(),
            root.join("notes/deep/UPPER.TMT")
        );
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
        assert_eq!(
            vault.document_path("data.txt").unwrap_err(),
            "`data.txt` is not a document of this vault"
        );
        let err_doc = resolver.document(&DocRef("data.txt".into())).unwrap_err();
        assert!(err_doc.contains("`data.txt` is not a document of this vault"));
        assert!(!err_doc.contains("SECRET"));
        let err_src = resolver.source(&DocRef("data.txt".into())).unwrap_err();
        assert!(err_src.contains("`data.txt` is not a document of this vault"));
        assert!(!err_src.contains("SECRET"));

        // 4. Leaving vault via ../x.tmt refused without leaking content
        assert_eq!(
            vault.document_path("../outside_secret.tmt").unwrap_err(),
            "`../outside_secret.tmt` is not a document of this vault"
        );
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
        assert!(
            vault
                .document_path(&abs_path_str)
                .unwrap_err()
                .contains("not a document of this vault")
        );
        let err_abs = resolver
            .document(&DocRef(abs_path_str.clone()))
            .unwrap_err();
        assert!(err_abs.contains("not a document of this vault"));
        assert!(!err_abs.contains("SECRET"));
        let err_abs_src = resolver.source(&DocRef(abs_path_str)).unwrap_err();
        assert!(err_abs_src.contains("not a document of this vault"));
        assert!(!err_abs_src.contains("SECRET"));

        // 6. Leaving vault via sub/../../x.tmt refused without leaking content
        assert!(
            vault
                .document_path("notes/../../outside_secret.tmt")
                .unwrap_err()
                .contains("not a document of this vault")
        );
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
            assert!(
                vault
                    .document_path("notes/sym_outside.tmt")
                    .unwrap_err()
                    .contains("not a document of this vault")
            );
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
            assert!(
                vault
                    .directory_path("notes/sym_dir")
                    .unwrap_err()
                    .contains("not a directory of this vault")
            );
            let err_sym_dir = resolver
                .dir_docs(&DirRef("notes/sym_dir".into()))
                .unwrap_err();
            assert!(err_sym_dir.contains("not a directory of this vault"));
            assert!(!err_sym_dir.contains("SECRET"));
            assert!(
                vault
                    .document_path("notes/sym_dir/nested.tmt")
                    .unwrap_err()
                    .contains("not a document of this vault")
            );
            let err_sym_dir_doc = resolver
                .document(&DocRef("notes/sym_dir/nested.tmt".into()))
                .unwrap_err();
            assert!(err_sym_dir_doc.contains("not a document of this vault"));
            assert!(!err_sym_dir_doc.contains("SECRET"));
        }

        // 8. Directory cases: in-vault directories, non-directory, and escaping
        assert_eq!(vault.directory_path("notes").unwrap(), root.join("notes"));
        assert_eq!(
            vault.directory_path("notes/deep").unwrap(),
            root.join("notes/deep")
        );
        assert_eq!(vault.directory_path("").unwrap(), root);
        assert_eq!(vault.directory_path("nope").unwrap_err(), "not a directory");
        assert_eq!(
            vault.directory_path("data.txt").unwrap_err(),
            "not a directory"
        );
        assert_eq!(
            vault.directory_path("notes/rust.tmt").unwrap_err(),
            "not a directory"
        );

        assert_eq!(
            vault.directory_path("..").unwrap_err(),
            "`..` is not a directory of this vault"
        );
        let err_dir_parent = resolver.dir_docs(&DirRef("..".into())).unwrap_err();
        assert!(err_dir_parent.contains("not a directory of this vault"));
        assert!(!err_dir_parent.contains("SECRET"));

        assert!(
            vault
                .directory_path("notes/../../..")
                .unwrap_err()
                .contains("not a directory of this vault")
        );
        let err_dir_deep = resolver
            .dir_docs(&DirRef("notes/../../..".into()))
            .unwrap_err();
        assert!(err_dir_deep.contains("not a directory of this vault"));
        assert!(!err_dir_deep.contains("SECRET"));

        let outside_dir_str = outside.to_str().unwrap().to_string();
        assert!(
            vault
                .directory_path(&outside_dir_str)
                .unwrap_err()
                .contains("not a directory of this vault")
        );
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

    fn write_cache_file(base_dir: &Path, kind: &str, key: &str, contents: &str) -> PathBuf {
        let path = base_dir.join(tomet_cache::entry_rel_path(kind, key));
        fs::create_dir_all(path.parent().unwrap()).expect("cache parent dir");
        fs::write(&path, contents).expect("write cache entry");
        path
    }

    fn cache_vault(name: &str, config: &str) -> PathBuf {
        let root = std::env::temp_dir().join(name);
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("vault dir");
        fs::write(root.join(tomet_load_config::VAULT_CONFIG_FILE), config).expect("config");
        root
    }

    fn eval_doc_expr(
        vault: &Vault,
        doc_path: &Path,
        expr_src: &str,
    ) -> Result<EvalValue, EvalError> {
        let (doc, _) = vault.file(doc_path).expect("read doc");
        let interp = tomet_ast_parser::parse_interpolation_str(&format!("{{$ {expr_src} }}"))
            .expect("valid expr");
        vault.evaluate(&doc, doc_path, &interp.expr)
    }

    fn findings_of(vault: &Vault, doc_path: &Path) -> crate::Findings {
        let (doc, _) = vault.file(doc_path).expect("read doc");
        vault.findings(doc_path, &doc)
    }

    #[test]
    fn resolve_cache_entry_path_handles_shared_and_local_configs() {
        let root = cache_vault("tomet_test_cache_path_helper", "::kind(config)\n");
        let canonical = root.canonicalize().unwrap();
        let rel = tomet_cache::entry_rel_path("command", "whoami");

        let shared_req = CacheRequest {
            kind: "command".into(),
            key: "whoami".into(),
            local: false,
            inputs: vec![],
        };
        let local_req = CacheRequest {
            local: true,
            ..shared_req.clone()
        };

        // Undeclared ::cache rejects both shared and local
        let none_cfg = CacheConfig::default();
        assert!(
            resolve_cache_entry_path_with(&root, &none_cfg, &shared_req, None, None)
                .unwrap_err()
                .contains("::cache(dir: ...)")
        );
        assert!(
            resolve_cache_entry_path_with(&root, &none_cfg, &local_req, None, None)
                .unwrap_err()
                .contains("::cache(local: true)")
        );

        // Declared dir resolves under vault_root/dir/<kind>/<sha256>.cache.ctmt
        let shared_cfg = CacheConfig {
            dir: Some(".tomet/cache".into()),
            local: false,
        };
        assert_eq!(
            resolve_cache_entry_path_with(&root, &shared_cfg, &shared_req, None, None).unwrap(),
            root.join(".tomet/cache").join(&rel)
        );

        // Local enabled with TOMET_CACHE_DIR override vs default XDG/vault_id path
        let local_cfg = CacheConfig {
            dir: None,
            local: true,
        };
        assert_eq!(
            resolve_cache_entry_path_with(
                &root,
                &local_cfg,
                &local_req,
                Some(OsStr::new("/tmp/custom-cache")),
                Some(Path::new("/tmp/xdg/tomet")),
            )
            .unwrap(),
            PathBuf::from("/tmp/custom-cache").join(&rel)
        );
        // Empty TOMET_CACHE_DIR falls back to XDG/vault_id
        let expected_vault_id = tomet_cache::vault_id(&canonical);
        assert_eq!(
            resolve_cache_entry_path_with(
                &root,
                &local_cfg,
                &local_req,
                Some(OsStr::new("")),
                Some(Path::new("/tmp/xdg/tomet")),
            )
            .unwrap(),
            PathBuf::from("/tmp/xdg/tomet")
                .join("cache")
                .join(expected_vault_id)
                .join(&rel)
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn repo_shared_cache_hit_resolves_scalar_and_map_values() {
        let root = cache_vault(
            "tomet_test_load_cache_shared_hit",
            "::kind(config)\n::cache(dir: \".tomet/cache\")\n",
        );
        let cache_dir = root.join(".tomet/cache");
        write_cache_file(
            &cache_dir,
            "command",
            "git rev-parse --short HEAD",
            "::kind(cache)\nkey: \"git rev-parse --short HEAD\"\nvalue: \"abc1234\"\n",
        );
        write_cache_file(
            &cache_dir,
            "command",
            "status",
            "::kind(cache)\nkey: \"status\"\nvalue: (\n  stdout: \"clean\"\n  code: 0\n)\n",
        );

        let doc_path = root.join("note.tmt");
        fs::write(
            &doc_path,
            "Rev: $cache.command(\"git rev-parse --short HEAD\")\nStatus: {$ cache.command(\"status\").stdout } ({$ cache.command(\"status\").code })\n",
        )
        .unwrap();

        let vault = Vault::discover(&root);
        assert_eq!(
            eval_doc_expr(
                &vault,
                &doc_path,
                r#"cache.command("git rev-parse --short HEAD")"#
            )
            .unwrap(),
            EvalValue::Scalar(Scalar::Text("abc1234".into()))
        );
        assert_eq!(
            eval_doc_expr(&vault, &doc_path, r#"cache.command("status").stdout"#).unwrap(),
            EvalValue::Scalar(Scalar::Text("clean".into()))
        );
        assert_eq!(
            eval_doc_expr(&vault, &doc_path, r#"cache.command("status").code"#).unwrap(),
            EvalValue::Scalar(Scalar::Int(0))
        );

        let findings = findings_of(&vault, &doc_path);
        assert!(!findings.has_errors(), "{:?}", findings.diagnostics);
        assert!(findings.unresolved.is_empty(), "{:?}", findings.unresolved);
        assert_eq!(findings.warnings(), 0, "{:?}", findings.diagnostics);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn local_cache_hit_resolves_with_tomet_cache_dir_override() {
        static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var_os("TOMET_CACHE_DIR");
        struct Restore(Option<std::ffi::OsString>);
        impl Drop for Restore {
            fn drop(&mut self) {
                // SAFETY: guarded by `ENV_LOCK` for the duration of this test.
                unsafe {
                    match &self.0 {
                        Some(v) => std::env::set_var("TOMET_CACHE_DIR", v),
                        None => std::env::remove_var("TOMET_CACHE_DIR"),
                    }
                }
            }
        }
        let _restore = Restore(prev);

        let root = cache_vault(
            "tomet_test_load_cache_local_hit",
            "::kind(config)\n::cache(local: true)\n",
        );
        let local_cache_dir = std::env::temp_dir().join("tomet_test_load_local_cache_store");
        let _ = fs::remove_dir_all(&local_cache_dir);
        write_cache_file(
            &local_cache_dir,
            "command",
            "whoami",
            "::kind(cache)\nkey: \"whoami\"\nvalue: \"alice\"\n",
        );
        // SAFETY: guarded by `ENV_LOCK` for the duration of this test.
        unsafe {
            std::env::set_var("TOMET_CACHE_DIR", &local_cache_dir);
        }

        let doc_path = root.join("note.tmt");
        fs::write(&doc_path, "User: $cache.command(\"whoami\", local: true)\n").unwrap();

        let vault = Vault::discover(&root);
        assert_eq!(
            eval_doc_expr(&vault, &doc_path, r#"cache.command("whoami", local: true)"#).unwrap(),
            EvalValue::Scalar(Scalar::Text("alice".into()))
        );
        let findings = findings_of(&vault, &doc_path);
        assert!(!findings.has_errors(), "{:?}", findings.diagnostics);
        assert!(findings.unresolved.is_empty(), "{:?}", findings.unresolved);
        assert_eq!(findings.warnings(), 0, "{:?}", findings.diagnostics);

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&local_cache_dir);
    }

    #[test]
    fn cache_failures_are_reported_as_errors_and_preserve_source_on_prepare() {
        // 1. Undeclared ::cache
        let root_undeclared = cache_vault("tomet_test_load_cache_undeclared", "::kind(config)\n");
        let doc_undeclared = root_undeclared.join("note.tmt");
        fs::write(&doc_undeclared, "Out: $cache.command(\"whoami\")\n").unwrap();
        let vault = Vault::discover(&root_undeclared);
        let findings = findings_of(&vault, &doc_undeclared);
        assert!(findings.has_errors());
        assert_eq!(findings.warnings(), 0);
        assert!(matches!(
            findings.diagnostics.as_slice(),
            [tomet_validator::Diagnostic::CacheError { reason, .. }]
                if reason.contains("::cache(dir: ...)")
        ));
        let (mut prepared_doc, _) = vault.file(&doc_undeclared).unwrap();
        let prepared = vault.prepare(&mut prepared_doc, &doc_undeclared);
        assert_eq!(prepared.unresolved.len(), 1);
        assert!(matches!(
            prepared_doc.body.as_slice(),
            [Block::Paragraph(p)] if p.content.iter().any(|inline| matches!(
                inline,
                tomet_ast::Inline::Text(t) if t.value.contains("$cache.command(\"whoami\")")
            ))
        ));
        let _ = fs::remove_dir_all(&root_undeclared);

        // 2. local: true when ::cache(local: false)
        let root_no_local = cache_vault(
            "tomet_test_load_cache_no_local",
            "::kind(config)\n::cache(dir: \".tomet/cache\", local: false)\n",
        );
        let doc_no_local = root_no_local.join("note.tmt");
        fs::write(&doc_no_local, "$cache.command(\"whoami\", local: true)\n").unwrap();
        let findings = findings_of(&Vault::discover(&root_no_local), &doc_no_local);
        assert!(findings.has_errors());
        assert_eq!(findings.warnings(), 0);
        assert!(matches!(
            findings.diagnostics.as_slice(),
            [tomet_validator::Diagnostic::CacheError { reason, .. }]
                if reason.contains("::cache(local: true)")
        ));
        let _ = fs::remove_dir_all(&root_no_local);

        // 3. Missing .cache.ctmt (cache miss)
        let root_miss = cache_vault(
            "tomet_test_load_cache_miss",
            "::kind(config)\n::cache(dir: \".tomet/cache\")\n",
        );
        let doc_miss = root_miss.join("note.tmt");
        fs::write(&doc_miss, "$cache.command(\"missing\")\n").unwrap();
        let findings = findings_of(&Vault::discover(&root_miss), &doc_miss);
        assert!(findings.has_errors());
        assert_eq!(findings.warnings(), 0);
        assert!(matches!(
            findings.diagnostics.as_slice(),
            [tomet_validator::Diagnostic::CacheError { reason, .. }]
                if reason.contains("cache miss")
        ));
        let _ = fs::remove_dir_all(&root_miss);

        // 4. Corrupt syntax and key mismatch in .cache.ctmt
        let root_bad = cache_vault(
            "tomet_test_load_cache_corrupt",
            "::kind(config)\n::cache(dir: \".tomet/cache\")\n",
        );
        write_cache_file(
            &root_bad.join(".tomet/cache"),
            "command",
            "corrupt",
            "::kind(cache)\nkey: \"unterminated\n",
        );
        write_cache_file(
            &root_bad.join(".tomet/cache"),
            "command",
            "mismatch",
            "::kind(cache)\nkey: \"other-key\"\nvalue: \"v\"\n",
        );
        let doc_corrupt = root_bad.join("corrupt.tmt");
        fs::write(&doc_corrupt, "$cache.command(\"corrupt\")\n").unwrap();
        let doc_mismatch = root_bad.join("mismatch.tmt");
        fs::write(&doc_mismatch, "$cache.command(\"mismatch\")\n").unwrap();

        let vault_bad = Vault::discover(&root_bad);
        let f_corrupt = findings_of(&vault_bad, &doc_corrupt);
        assert!(f_corrupt.has_errors());
        assert_eq!(f_corrupt.warnings(), 0);
        assert!(matches!(
            f_corrupt.diagnostics.as_slice(),
            [tomet_validator::Diagnostic::CacheError { reason, .. }]
                if reason.contains("invalid cache entry")
        ));

        let f_mismatch = findings_of(&vault_bad, &doc_mismatch);
        assert!(f_mismatch.has_errors());
        assert_eq!(f_mismatch.warnings(), 0);
        assert!(matches!(
            f_mismatch.diagnostics.as_slice(),
            [tomet_validator::Diagnostic::CacheError { reason, .. }]
                if reason.contains("key mismatch")
        ));

        let _ = fs::remove_dir_all(&root_bad);
    }
}
