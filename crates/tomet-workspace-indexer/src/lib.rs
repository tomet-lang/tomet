//! Directory scanning and read-only metadata cataloging for `.tmt`/`.tmt`
//! files. Split out of `tomet-tui`'s `engine::batch_meta` module so
//! it's reusable outside the TUI (a future search/browse feature, e.g.)
//! without pulling in ratatui/crossterm.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use tomet_ast::Value;
use tomet_config::PrinterConfig;
use tomet_parser::parse_document;
use tomet_semantics::classify_std_lenient;

pub mod workspace_scan;

/// What a relative path written *inside* a document means, resolved
/// against the document and the project it belongs to.
///
/// Three cases, decided by this project's author for links and applied
/// wherever a document names a path:
///
/// - absolute -- an OS-absolute path, taken as given;
/// - starting with `./` or `../` -- relative to the referencing file's
///   own directory;
/// - anything else -- relative to the *project root*, not to the
///   process's working directory.
///
/// The last is the one that is easy to get wrong by doing nothing.
/// `@config(export: path)` did exactly that: it handed the string
/// straight to `fs::write`, so `tmtroot/readme.tmt` declaring
/// `README.ja.md` wrote to the repository root or to `docs/` depending
/// on where you happened to be standing. Running the export from the
/// right directory made it look correct.
pub fn resolve_document_relative(source: &Path, target: &str, project_root: &Path) -> PathBuf {
    let target_path = Path::new(target);

    if target_path.is_absolute() {
        return target_path.to_path_buf();
    }

    if target.starts_with("./") || target.starts_with("../") {
        return match source.parent() {
            Some(parent) => normalize_join(parent, target_path),
            None => target_path.to_path_buf(),
        };
    }

    normalize_join(project_root, target_path)
}

/// `base.join(target)` with `.` dropped and `..` popped, so the result
/// has no traversal components left in it.
pub fn normalize_join(base: &Path, target: &Path) -> PathBuf {
    let mut result = base.to_path_buf();
    for component in target.components() {
        match component {
            std::path::Component::ParentDir => {
                result.pop();
            }
            std::path::Component::CurDir => {}
            other => result.push(other.as_os_str()),
        }
    }
    result
}

/// Whether `path` matches one of `ignore_patterns` (each pattern
/// matched against the path both as given and relative to `root`, with
/// a directory-prefix or exact-segment match).
pub fn is_path_ignored(path: &Path, root: Option<&Path>, ignore_patterns: &[String]) -> bool {
    if ignore_patterns.is_empty() {
        return false;
    }
    let path_str = path.to_string_lossy().replace('\\', "/");
    let rel_str = if let Some(r) = root {
        path.strip_prefix(r)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| path_str.clone())
    } else {
        path_str.clone()
    };
    let rel_clean = rel_str.trim_start_matches('/');

    for pat in ignore_patterns {
        let pat_clean = pat.trim().replace('\\', "/");
        let pat_clean = pat_clean.trim_matches('/');
        if pat_clean.is_empty() {
            continue;
        }
        if rel_clean == pat_clean
            || rel_clean.starts_with(&format!("{pat_clean}/"))
            || path_str.ends_with(&format!("/{pat_clean}"))
            || path_str.ends_with(pat_clean)
            || path_str.contains(&format!("/{pat_clean}/"))
        {
            return true;
        }
    }
    false
}

/// Whether the sweep skips `p`: not part of the vault, or in it and
/// left alone.
///
/// The two lists answer different questions and only this one takes
/// both. `ignore` says a path is not the vault's; `unswept` says it is,
/// and is not processed -- the frozen corpus is skipped because a stale
/// export there is the coverage, not because the repository lacks it.
/// Reference resolution asks only the first, which is why a document can
/// name `tests/fixtures` and cannot name a path outside the vault.
pub fn is_path_unswept(p: &Path, config_root: Option<&Path>, config: &PrinterConfig) -> bool {
    is_path_ignored(p, config_root, &config.ignore_files)
        || is_path_ignored(p, config_root, &config.unswept_files)
}

/// Collects every `.tmt`/`.tmt` file under `path` (or just `path` itself
/// if it's a single file), auto-discovering the nearest
/// `default.config.tmt`/`tomet.config.tmt` for `ignore_files` rules.
pub fn collect_tm_files(path: &Path) -> Vec<PathBuf> {
    let (config, _, config_root) = tomet_config::find_config_file(path).unwrap_or_else(|| {
        (
            PrinterConfig::default(),
            path.to_path_buf(),
            path.to_path_buf(),
        )
    });
    collect_tm_files_with_config(path, &config, &config_root)
}

/// Same as [`collect_tm_files`] but with an explicit `config`/
/// `config_root` instead of auto-discovering one.
pub fn collect_tm_files_with_config(
    path: &Path,
    config: &PrinterConfig,
    config_root: &Path,
) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if path.is_file() {
        if is_tm_file(path) {
            files.push(path.to_path_buf());
        }
    } else if path.is_dir() {
        for entry in WalkBuilder::new(path)
            .hidden(false)
            .git_ignore(true)
            .build()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map_or(false, |ft| ft.is_file()))
        {
            let p = entry.path();
            if is_path_unswept(p, Some(config_root), config) {
                continue;
            }
            if is_tm_file(p) {
                files.push(p.to_path_buf());
            }
        }
    }
    files.sort();
    files
}

/// Every non-ignored path under `path` (or just `path` itself if it's a
/// single file) -- no extension filtering at all, unlike
/// `collect_tm_files`/`WorkspaceIndex`. The shared, lowest-level walk
/// primitive other extension-filtered scans (this crate's own
/// `collect_tm_files_with_config`, `workspace_scan`'s `WorkspaceIndex`,
/// and `tomet-links`'s existence checks) can build on, so
/// `.hidden(false)`/`.git_ignore(true)`/`is_path_ignored` semantics can't
/// drift between independently-configured `WalkBuilder`s.
pub fn collect_all_paths_with_config(
    path: &Path,
    config: &PrinterConfig,
    config_root: &Path,
) -> std::collections::HashSet<PathBuf> {
    let mut paths = std::collections::HashSet::new();
    if path.is_file() {
        paths.insert(path.to_path_buf());
    } else if path.is_dir() {
        for entry in WalkBuilder::new(path)
            .hidden(false)
            .git_ignore(true)
            .build()
            .filter_map(|e| e.ok())
            // Directories are in the set too. Its one consumer is the
            // link checker, and `@dir(docs/spec)` names one -- a perfectly
            // ordinary thing for an index page to write, and one that
            // reported broken for as long as this collected only files.
            .filter(|e| e.file_type().is_some_and(|ft| ft.is_file() || ft.is_dir()))
        {
            let p = entry.path();
            // `ignore` only, deliberately: `unswept` says a path is in
            // the vault and not processed, and a reference to it is
            // therefore fine. Asking both here is what made
            // `@dir(tests/fixtures)` report broken while the directory
            // sat committed in the repository.
            if is_path_ignored(p, Some(config_root), &config.ignore_files) {
                continue;
            }
            paths.insert(p.to_path_buf());
        }
    }
    paths
}

fn is_tm_file(p: &Path) -> bool {
    p.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("tm") || ext.eq_ignore_ascii_case("tmt"))
        .unwrap_or(false)
}

pub fn extract_metadata(src: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    if let Ok(doc) = parse_document(src) {
        tomet_tree::for_each_element(&doc, |el| {
            let kind = classify_std_lenient(el);
            let kind = kind.as_str();
            if kind == "meta" || kind == "config" {
                if let Some(entries) = el.value.as_ref().map(|v| v.pairs().collect::<Vec<_>>()) {
                    for (k, v) in entries {
                        map.insert(format!("{kind}.{k}"), value_to_string(v));
                    }
                }
                if let Some(Value::Map(entries)) = &el.args {
                    for (k, v) in entries {
                        map.insert(format!("{kind}.(args).{k}"), value_to_string(v));
                    }
                }
            }
        });
    }
    map
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        Value::Seq(items) => {
            let s: Vec<_> = items.iter().map(value_to_string).collect();
            format!("[{}]", s.join(", "))
        }
        Value::Map(entries) => {
            let s: Vec<_> = entries
                .iter()
                .map(|(k, v)| format!("{k}: {}", value_to_string(v)))
                .collect();
            format!("{{{}}}", s.join(", "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_path_ignored_matches_directory_prefix() {
        let root = Path::new("/workspace");
        let ignore_files = vec!["00-09 System/01 Apps/obsidian".to_string()];
        let ignored_file = root.join("00-09 System/01 Apps/obsidian/note.md");
        let normal_file = root.join("00-09 System/01 Apps/other/note.md");

        assert!(is_path_ignored(&ignored_file, Some(root), &ignore_files));
        assert!(!is_path_ignored(&normal_file, Some(root), &ignore_files));
    }

    #[test]
    fn test_is_path_ignored_with_config_from_settings() {
        let settings_src = r#"@settings(format:json)+++
{
  "ignore": {
    "files": [
      "00-09 System/01 Apps/obsidian"
    ]
  }
}
+++
"#;
        let cfg =
            tomet_config::load_config_from_str(settings_src).expect("failed to parse settings");
        let root = Path::new("/workspace");
        let ignored_file = root.join("00-09 System/01 Apps/obsidian/note.md");
        assert!(is_path_ignored(
            &ignored_file,
            Some(root),
            &cfg.ignore_files
        ));
    }

    #[test]
    fn test_extract_metadata_collects_meta_and_config_fields() {
        let src = "@meta{author: Alice}\n\n@config{lang: en}\n\n#[Doc]\n";
        let map = extract_metadata(src);
        assert_eq!(map.get("meta.author"), Some(&"Alice".to_string()));
        assert_eq!(map.get("config.lang"), Some(&"en".to_string()));
    }

    #[test]
    fn test_collect_tm_files_finds_tm_and_tmt_only() {
        let dir = std::env::temp_dir().join(format!("tm_indexer_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.tmt"), "#[A]\n").unwrap();
        std::fs::write(dir.join("b.tmt"), "#[B]\n").unwrap();
        std::fs::write(dir.join("c.md"), "# C\n").unwrap();

        let files = collect_tm_files(&dir);
        let names: Vec<_> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();
        assert_eq!(names, vec!["a.tmt", "b.tmt"]);

        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod unswept_tests {
    use super::*;

    fn config(ignore: &[&str], unswept: &[&str]) -> PrinterConfig {
        PrinterConfig {
            ignore_files: ignore.iter().map(|s| s.to_string()).collect(),
            unswept_files: unswept.iter().map(|s| s.to_string()).collect(),
            ..PrinterConfig::default()
        }
    }

    #[test]
    fn the_sweep_skips_both_lists_and_references_skip_only_ignore() {
        // The whole point of the split. `unswept` says a path is in the
        // vault and left alone, so a document may name it; `ignore` says
        // it is not the vault's, so a reference to it is broken.
        //
        // While one list served both, `@dir(tests/fixtures)` reported
        // broken against a directory sitting committed in the repository.
        let root = Path::new("/vault");
        let cfg = config(&["vendor"], &["tests/fixtures"]);

        let frozen = Path::new("/vault/tests/fixtures/x.tmt");
        let foreign = Path::new("/vault/vendor/y.tmt");

        assert!(is_path_unswept(frozen, Some(root), &cfg), "frozen is not swept");
        assert!(is_path_unswept(foreign, Some(root), &cfg), "foreign is not swept");

        assert!(
            !is_path_ignored(frozen, Some(root), &cfg.ignore_files),
            "frozen is still referable"
        );
        assert!(
            is_path_ignored(foreign, Some(root), &cfg.ignore_files),
            "foreign is not referable"
        );
    }
}
