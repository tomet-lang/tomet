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

/// One file's queryable facts, keyed by the dotted paths a
/// `${filter(...)}` predicate writes: `path`, `filename`, `kind`, `meta`
/// and everything under it.
///
/// The distinction from [`extract_metadata`] is the value type.
/// `extract_metadata` collapses to `String` because its consumer is the
/// TUI's batch metadata editor, where every field is about to be shown
/// and typed into a text box. A predicate needs the value itself: a list
/// of tags has to stay a list for `contains` to look inside it, and a
/// number has to stay a number for `gt` to order it.
///
/// `@config` is deliberately absent. It configures export and printing,
/// not what the document *is*, so filtering an index on it would be
/// filtering on machinery.
#[derive(Debug, Clone, PartialEq)]
pub struct FileMetadata {
    /// As found on disk.
    pub path: PathBuf,
    pub fields: BTreeMap<String, Value>,
}

/// Reads every `.tmt` under `path` and returns what each one can be
/// queried on.
///
/// The walk is [`collect_tm_files_with_config`]'s, so `ignore`/`unswept`
/// mean here exactly what they mean everywhere else. A file that does not
/// parse contributes no row: `tomet check` is where a parse error is
/// reported, and failing the whole index for one bad file elsewhere in
/// the vault would be the wrong trade.
///
/// This is the only part of `${filter(...)}` that touches the disk. The
/// pass that consumes the table is pure and lives a layer below, in
/// `tomet-transform`.
pub fn collect_metadata_table(
    path: &Path,
    config: &PrinterConfig,
    config_root: &Path,
) -> Vec<FileMetadata> {
    collect_tm_files_with_config(path, config, config_root)
        .into_iter()
        .filter_map(|file| {
            let src = std::fs::read_to_string(&file).ok()?;
            let doc = parse_document(&src).ok()?;
            let fields = document_fields(&doc, &file, config_root);
            Some(FileMetadata { path: file, fields })
        })
        .collect()
}

/// The queryable facts of one already-parsed document. Pure -- split out
/// from [`collect_metadata_table`] so the shape of a row can be tested
/// without a directory on disk.
///
/// `path` is measured from the project root and written with forward
/// slashes, which is the spelling [`resolve_document_relative`] reads back
/// and the spelling a generated `@file(...)` has to carry.
pub fn document_fields(
    doc: &tomet_ast::Document,
    file: &Path,
    project_root: &Path,
) -> BTreeMap<String, Value> {
    let mut fields = BTreeMap::new();

    let rel = file
        .strip_prefix(project_root)
        .unwrap_or(file)
        .to_string_lossy()
        .replace('\\', "/");
    fields.insert("path".to_string(), Value::String(rel));
    if let Some(name) = file.file_name().and_then(|n| n.to_str()) {
        fields.insert("filename".to_string(), Value::String(name.to_string()));
    }
    if let Some(kind) = tomet_semantics::document_kind(doc) {
        fields.insert("kind".to_string(), Value::String(kind));
    }
    if let Some(meta) = tomet_semantics::document_meta(doc) {
        flatten_into("meta", &meta, &mut fields);
    }

    fields
}

/// Records `value` at `prefix`, then every path beneath it.
///
/// A `Map` is recorded *and* descended into, so both `meta` and
/// `meta.title` are present -- the first is what `exists(meta)` asks
/// about, the second what `eq(meta.title, "...")` does. A `Seq` is
/// recorded and not descended into: `meta.tags` is the list itself,
/// because `contains` is how a list is asked about and an index into one
/// is not something a predicate can write.
fn flatten_into(prefix: &str, value: &Value, out: &mut BTreeMap<String, Value>) {
    out.insert(prefix.to_string(), value.clone());
    if let Value::Map(entries) = value {
        for (key, child) in entries {
            flatten_into(&format!("{prefix}.{key}"), child, out);
        }
    }
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
        Value::Call(name, args) => {
            let s: Vec<_> = args.iter().map(value_to_string).collect();
            format!("{name}({})", s.join(", "))
        }
        Value::Element(el) => {
            let name = el.sigil.name().map(|n| n.to_string()).unwrap_or_default();
            match &el.args {
                Some(args) => format!("@{name}({})", value_to_string(args)),
                None => format!("@{name}"),
            }
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

#[cfg(test)]
mod metadata_table_tests {
    use super::*;

    fn fields_of(src: &str) -> BTreeMap<String, Value> {
        let doc = parse_document(src).expect("valid source");
        document_fields(
            &doc,
            Path::new("/vault/docs/guide/cheatsheet.tmt"),
            Path::new("/vault"),
        )
    }

    #[test]
    fn path_is_project_relative_with_forward_slashes() {
        let fields = fields_of("#[ Title ]\n");
        assert_eq!(
            fields.get("path"),
            Some(&Value::String("docs/guide/cheatsheet.tmt".to_string()))
        );
        assert_eq!(
            fields.get("filename"),
            Some(&Value::String("cheatsheet.tmt".to_string()))
        );
    }

    #[test]
    fn kind_comes_from_the_kind_element() {
        let fields = fields_of("@kind(writ)\n\n#[ Title ]\n");
        assert_eq!(fields.get("kind"), Some(&Value::String("writ".to_string())));
        // No `@kind` means no row entry at all, which is what lets the
        // query pass tell "absent here" from "nobody has this field".
        assert_eq!(fields_of("#[ Title ]\n").get("kind"), None);
    }

    #[test]
    fn a_list_stays_a_list() {
        let fields = fields_of("@meta{ tags: [rust, cli] }\n\n#[ Title ]\n");
        // Not the string "[rust, cli]" that `extract_metadata` produces --
        // `contains` has to be able to look inside it.
        assert_eq!(
            fields.get("meta.tags"),
            Some(&Value::Seq(vec![
                Value::String("rust".to_string()),
                Value::String("cli".to_string()),
            ]))
        );
    }

    #[test]
    fn a_yaml_fenced_meta_is_read() {
        // The spelling this repository's own documents use. It is invisible
        // to `extract_metadata`, which reads `{...}` pairs only and sees an
        // opaque `Raw` body here.
        let fields = fields_of("@meta(format:yaml)+++\ntitle: Cheatsheet\ntags:\n  - rust\n+++\n");
        assert_eq!(
            fields.get("meta.title"),
            Some(&Value::String("Cheatsheet".to_string()))
        );
        assert_eq!(
            fields.get("meta.tags"),
            Some(&Value::Seq(vec![Value::String("rust".to_string())]))
        );
    }

    #[test]
    fn a_nested_map_is_recorded_at_every_depth() {
        let fields = fields_of("@meta{ url: { wiki: \"https://example.test\" } }\n\n#[ T ]\n");
        assert!(matches!(fields.get("meta"), Some(Value::Map(_))));
        assert!(matches!(fields.get("meta.url"), Some(Value::Map(_))));
        assert_eq!(
            fields.get("meta.url.wiki"),
            Some(&Value::String("https://example.test".to_string()))
        );
    }
}
