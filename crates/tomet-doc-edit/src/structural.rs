//! Structural Search & Replace Engine (AST-Aware Grep / Refactoring).

use std::fs;
use std::path::{Path, PathBuf};

use tomet_ast::{Document, Element, ElementValue, Sigil, Value};
use tomet_parser::parse_document;
use tomet_printer::document_to_tm;
use tomet_semantics::classify;

#[derive(Debug, Clone, Default)]
pub struct StructuralQuery {
    pub tag: Option<String>,
    pub key: Option<String>,
    pub value_contains: Option<String>,
}

#[derive(Debug, Clone)]
pub enum StructuralAction {
    RenameTag { from: String, to: String },
    RenameKey { old_key: String, new_key: String },
    ReplaceValue { key: String, new_value: String },
}

#[derive(Debug, Clone)]
pub struct StructuralMatch {
    pub path: PathBuf,
    pub original_src: String,
    pub modified_src: String,
    pub match_count: usize,
    pub selected: bool,
}

pub struct StructuralEngine;

impl StructuralEngine {
    /// Perform structural query search across `.tmt` files.
    pub fn search(dir: &Path, query: &StructuralQuery) -> Vec<StructuralMatch> {
        let (config, _, config_root) =
            tomet_config::find_config_file(dir).unwrap_or_else(|| {
                (
                    tomet_config::PrinterConfig::default(),
                    dir.to_path_buf(),
                    dir.to_path_buf(),
                )
            });
        Self::search_with_config(dir, query, &config, &config_root)
    }

    pub fn search_with_config(
        dir: &Path,
        query: &StructuralQuery,
        config: &tomet_config::PrinterConfig,
        config_root: &Path,
    ) -> Vec<StructuralMatch> {
        let mut matches = Vec::new();
        let paths = tomet_indexer::collect_tm_files_with_config(dir, config, config_root);

        for path in paths {
            if let Ok(src) = fs::read_to_string(&path) {
                if let Ok(doc) = parse_document(&src) {
                    let count = count_matches(&doc, query);
                    if count > 0 {
                        matches.push(StructuralMatch {
                            path,
                            original_src: src.clone(),
                            modified_src: src,
                            match_count: count,
                            selected: true,
                        });
                    }
                }
            }
        }
        matches
    }

    /// Apply structural refactoring action to matched items.
    pub fn apply_action(matches: &mut [StructuralMatch], action: &StructuralAction) {
        for m in matches.iter_mut() {
            if !m.selected {
                continue;
            }
            if let Ok(mut doc) = parse_document(&m.original_src) {
                let mut count = 0;
                transform_doc(&mut doc, action, &mut count);
                if count > 0 {
                    m.match_count = count;
                    m.modified_src = document_to_tm(&doc);
                }
            }
        }
    }

    /// Save refactored matches to disk.
    pub fn save(matches: &mut [StructuralMatch]) -> anyhow::Result<usize> {
        let mut count = 0;
        for m in matches.iter_mut() {
            if m.selected && m.original_src != m.modified_src {
                fs::write(&m.path, &m.modified_src)?;
                m.original_src = m.modified_src.clone();
                count += 1;
            }
        }
        Ok(count)
    }
}

fn count_matches(doc: &Document, query: &StructuralQuery) -> usize {
    let mut count = 0;
    tomet_walker::for_each_element(doc, |el| {
        if matches_query(el, query) {
            count += 1;
        }
    });
    count
}

fn matches_query(el: &Element, query: &StructuralQuery) -> bool {
    let kind = classify(el);
    if let Some(target_tag) = &query.tag {
        if !target_tag.is_empty() && !kind.as_str().eq_ignore_ascii_case(target_tag) {
            return false;
        }
    }

    if let Some(target_key) = &query.key {
        if !target_key.is_empty() && !tomet_walker::element_has_prop_key(el, target_key) {
            return false;
        }
    }

    if let Some(sub) = &query.value_contains {
        if !sub.is_empty() {
            let in_args = el
                .args
                .as_ref()
                .map_or(false, |v| value_contains_str(v, sub));
            let in_val = el.value.as_ref().map_or(false, |v| match v {
                ElementValue::Data(data_val) => value_contains_str(data_val, sub),
                ElementValue::Children(_) | ElementValue::Interp(_) => false,
            });
            if !in_args && !in_val {
                return false;
            }
        }
    }

    true
}

fn transform_doc(doc: &mut Document, action: &StructuralAction, count: &mut usize) {
    tomet_walker::for_each_element_mut(doc, |el| match action {
        StructuralAction::RenameTag { from, to } => {
            let kind = classify(el);
            if kind.as_str().eq_ignore_ascii_case(from) {
                match &mut el.sigil {
                    Sigil::Type(name) => *name = to.clone(),
                    Sigil::At(Some(name)) => *name = to.clone(),
                    _ => {}
                }
                *count += 1;
            }
        }
        StructuralAction::RenameKey { old_key, new_key } => {
            if tomet_walker::element_rename_prop_key(el, old_key, new_key) {
                *count += 1;
            }
        }
        StructuralAction::ReplaceValue { key, new_value } => {
            if tomet_walker::element_replace_prop_value(
                el,
                key,
                Value::String(new_value.clone()),
            ) {
                *count += 1;
            }
        }
    });
}

fn value_contains_str(v: &Value, sub: &str) -> bool {
    match v {
        Value::String(s) => s.contains(sub),
        Value::Map(entries) => entries
            .iter()
            .any(|(k, val)| k.contains(sub) || value_contains_str(val, sub)),
        Value::Seq(items) => items.iter().any(|item| value_contains_str(item, sub)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_structural_search_and_rename_key() {
        let src = "@meta{author: Charlie}\n\n<note>[Check this]\n";
        let item = StructuralMatch {
            path: "test.tmt".into(),
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
            path: "test.tmt".into(),
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
            path: "test.tmt".into(),
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
}
