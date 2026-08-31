//! Directory scanning and read-only metadata cataloging for `.tmt`/`.tmt`
//! files. Split out of `tomet-tui`'s `engine::batch_meta` module so
//! it's reusable outside the TUI (a future search/browse feature, e.g.)
//! without pulling in ratatui/crossterm.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use tomet_ast::{ElementValue, Value};
use tomet_config::PrinterConfig;
use tomet_parser::parse_document;
use tomet_semantics::classify;

pub mod workspace_scan;

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
            .hidden(true)
            .git_ignore(true)
            .build()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map_or(false, |ft| ft.is_file()))
        {
            let p = entry.path();
            if is_path_ignored(p, Some(config_root), &config.ignore_files) {
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
/// `.hidden(true)`/`.git_ignore(true)`/`is_path_ignored` semantics can't
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
            .hidden(true)
            .git_ignore(true)
            .build()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map_or(false, |ft| ft.is_file()))
        {
            let p = entry.path();
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
            let kind = classify(el);
            let kind = kind.as_str();
            if kind == "meta" || kind == "config" {
                if let Some(ElementValue::Data(Value::Map(entries))) = &el.value {
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
        let settings_src = r#"@settings(format:json){
  {
    "ignore": {
      "files": [
        "00-09 System/01 Apps/obsidian"
      ]
    }
  }
}
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
