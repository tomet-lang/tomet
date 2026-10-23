#![cfg(feature = "cache")]

use std::path::PathBuf;

use tomet_config::PrinterConfig;
use tomet_links::{LinkCache, check_vault};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn temp_cache_path() -> PathBuf {
    std::env::temp_dir().join(format!("tm_links_test_{}.sqlite3", uuid::Uuid::new_v4()))
}

#[test]
fn finds_broken_links_and_ignores_external_urls() {
    let root = fixtures_dir();
    let config = PrinterConfig::default();
    let cache_path = temp_cache_path();
    let mut cache = LinkCache::open(&cache_path).unwrap();

    let report = check_vault(&root, &config, &root, &root, &mut cache);

    assert_eq!(
        report.files_scanned, 3,
        "expected good.tmt + bad.tmt + sub/nested.tmt"
    );
    let mut broken_targets: Vec<&str> = report.broken.iter().map(|b| b.target.as_str()).collect();
    broken_targets.sort();
    assert_eq!(
        broken_targets,
        vec!["missing.tmt#anchor", "missing.txt", "no-such-note"],
        "full report: {:?}",
        report.broken
    );

    let _ = std::fs::remove_file(&cache_path);
}

#[test]
fn resolves_source_relative_project_root_relative_and_ref_targets() {
    let root = fixtures_dir();
    let config = PrinterConfig::default();
    let cache_path = temp_cache_path();
    let mut cache = LinkCache::open(&cache_path).unwrap();

    let report = check_vault(&root, &config, &root, &root, &mut cache);

    let mut broken_from_nested: Vec<&str> = report
        .broken
        .iter()
        .filter(|b| b.source.ends_with("nested.tmt"))
        .map(|b| b.target.as_str())
        .collect();
    broken_from_nested.sort();

    // `./sibling.txt` (source-relative) and bare `real.txt`
    // (project-root-relative) both resolve; `ref:real` resolves via
    // stem search against `real.txt`; `tmt:good.tmt#anchor` resolves
    // (anchor verified against good.tmt AST); only
    // `ref:no-such-note` and `tmt:missing.tmt#anchor` are actually broken.
    assert_eq!(
        broken_from_nested,
        vec!["missing.tmt#anchor", "no-such-note"]
    );

    let _ = std::fs::remove_file(&cache_path);
}

#[test]
fn second_run_reuses_the_cache_without_reparsing() {
    let root = fixtures_dir();
    let config = PrinterConfig::default();
    let cache_path = temp_cache_path();
    let mut cache = LinkCache::open(&cache_path).unwrap();

    let first = check_vault(&root, &config, &root, &root, &mut cache);

    // Confirm every file is a cache Miss on first run, then a Hit on
    // the second run against the same cache instance/file.
    for file in tomet_indexer::collect_tm_files(&root) {
        let outcome = cache.links_for(&file).unwrap();
        assert!(
            outcome.is_hit(),
            "{file:?} should be cached after check_vault's first pass"
        );
    }

    let second = check_vault(&root, &config, &root, &root, &mut cache);
    assert_eq!(first.broken.len(), second.broken.len());
    assert_eq!(first.files_scanned, second.files_scanned);

    let _ = std::fs::remove_file(&cache_path);
}

#[test]
fn reports_broken_link_when_anchor_does_not_exist_in_target_doc() {
    let temp_dir = std::env::temp_dir().join(format!("tm_anchor_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let target_doc = temp_dir.join("target.tmt");
    std::fs::write(&target_doc, "::meta{}\n\n=[ Heading ] #(real_anchor)\n").unwrap();

    let referencing_doc = temp_dir.join("referencing.tmt");
    std::fs::write(
        &referencing_doc,
        "::meta{}\n\n@link(target:tmt:target.tmt#real_anchor)\n@link(target:tmt:target.tmt#no_such_anchor)\n",
    )
    .unwrap();

    let config = PrinterConfig::default();
    let cache_path = temp_cache_path();
    let mut cache = LinkCache::open(&cache_path).unwrap();

    let report = check_vault(&temp_dir, &config, &temp_dir, &temp_dir, &mut cache);

    let broken_targets: Vec<&str> = report.broken.iter().map(|b| b.target.as_str()).collect();
    assert_eq!(broken_targets, vec!["target.tmt#no_such_anchor"]);

    let _ = std::fs::remove_file(&cache_path);
    let _ = std::fs::remove_dir_all(&temp_dir);
}
