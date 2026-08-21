//! Unit tests for Migration, Batch Meta, and Structural Refactoring engines.

use super::batch_meta::{BatchMetaEngine, MetaFileEntry};
use super::migration::MigrationEngine;
use super::structural::{StructuralAction, StructuralEngine, StructuralMatch};
use std::fs;

#[test]
fn test_markdown_migration_conversion() {
    let dir_path = std::env::temp_dir().join(format!("tm_test_{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir_path).unwrap();
    let md_path = dir_path.join("doc.md");
    fs::write(&md_path, "# Migration Test\n\n- item 1\n- item 2\n").unwrap();

    let mut items = MigrationEngine::scan(&dir_path);
    assert_eq!(items.len(), 1);
    items[0].ensure_loaded();
    assert!(items[0].typedmark_src.contains("#[Migration Test]"));

    let mut items_to_exec = items;
    items_to_exec[0].selected = true;
    let count = MigrationEngine::execute(&mut items_to_exec, false).unwrap();
    assert_eq!(count, 1);
    assert!(dir_path.join("doc.tm").exists());
    let _ = fs::remove_dir_all(&dir_path);
}

#[test]
fn test_batch_meta_update() {
    let src = "@meta{author: Bob}\n\n#[Document]\n";
    let entry = MetaFileEntry {
        path: "test.tm".into(),
        original_src: src.to_string(),
        modified_src: src.to_string(),
        metadata: typedmark_indexer::extract_metadata(src),
        selected: true,
    };

    let mut entries = vec![entry];
    BatchMetaEngine::update_meta_key(&mut entries, "meta", "author", "Alice");
    assert!(entries[0].modified_src.contains("author: Alice"));
}

#[test]
fn test_structural_search_and_rename_key() {
    let src = "@meta{author: Charlie}\n\n<note>[Check this]\n";
    let item = StructuralMatch {
        path: "test.tm".into(),
        original_src: src.to_string(),
        modified_src: src.to_string(),
        match_count: 1,
        selected: true,
    };

    let mut matches = vec![item];
    StructuralEngine::apply_action(
        &mut matches,
        &StructuralAction::RenameKey {
            old_key: "author".into(),
            new_key: "creator".into(),
        },
    );

    assert!(matches[0].modified_src.contains("creator: Charlie"));
}

#[test]
fn test_structural_rename_tag() {
    let src = "<note>[Pay attention]\n";
    let item = StructuralMatch {
        path: "test.tm".into(),
        original_src: src.to_string(),
        modified_src: src.to_string(),
        match_count: 1,
        selected: true,
    };

    let mut matches = vec![item];
    StructuralEngine::apply_action(
        &mut matches,
        &StructuralAction::RenameTag {
            from: "note".into(),
            to: "caution".into(),
        },
    );

    assert!(matches[0].modified_src.contains("<caution>[Pay attention]"));
}

#[test]
fn test_structural_replace_value() {
    let src = "@meta{author: Charlie}\n\n<note>[Check this]\n";
    let item = StructuralMatch {
        path: "test.tm".into(),
        original_src: src.to_string(),
        modified_src: src.to_string(),
        match_count: 1,
        selected: true,
    };

    let mut matches = vec![item];
    StructuralEngine::apply_action(
        &mut matches,
        &StructuralAction::ReplaceValue {
            key: "author".into(),
            new_value: "Alice".into(),
        },
    );

    assert!(matches[0].modified_src.contains("author: Alice"));
}
