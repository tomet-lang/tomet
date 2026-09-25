//! Workspace and document refactoring pipeline.

use std::fs;
use std::path::{Path, PathBuf};

use tomet_ast::Document;
use tomet_config::{PrinterConfig, find_config_file};
use tomet_formatter::format_source;
use tomet_indexer::collect_tm_files_with_config;
use tomet_parser::parse_document;
use tomet_printer::document_to_tm_with_config;
use tomet_transform::{
    MacroSet, normalize_embedded_to_value_dsl, promote_meta_type_to_kind,
    transform_link_targets_with_macros,
};

use crate::diff::FileDiff;

/// Configuration options for workspace and document refactoring passes.
#[derive(Debug, Clone)]
pub struct RefactorOptions {
    /// Rewrite URLs matching `@config` macro patterns to `$macro(...)` invocations.
    pub url_to_macros: bool,
    /// Promote `@meta` `type:` field to top-level `@kind(...)` element.
    pub meta_type_to_kind: bool,
    /// Normalize `@meta(format:yaml)` to native Value DSL `@meta`.
    pub meta_to_value_dsl: bool,
}

impl Default for RefactorOptions {
    fn default() -> Self {
        Self {
            url_to_macros: true,
            meta_type_to_kind: true,
            meta_to_value_dsl: true,
        }
    }
}

/// Applies configured refactoring transformations to a parsed [`Document`].
pub fn refactor_document(
    doc: &mut Document,
    config: &PrinterConfig,
    options: &RefactorOptions,
) -> usize {
    let mut total_changes = 0;

    if options.url_to_macros {
        let doc_config = tomet_semantics::document_config(doc);
        let mut all_macros = config.macros.clone();
        for (k, v) in doc_config.macros {
            all_macros.insert(k, v);
        }
        let macro_set = MacroSet::from_map(&all_macros);
        total_changes += transform_link_targets_with_macros(doc, &macro_set);
    }

    if options.meta_type_to_kind && promote_meta_type_to_kind(doc) {
        total_changes += 1;
    }

    if options.meta_to_value_dsl && normalize_embedded_to_value_dsl(doc) {
        total_changes += 1;
    }

    total_changes
}

/// Refactors source text, returning the transformed and formatted text alongside the changes count.
///
/// **Lossy: this drops comments.** It parses to a `Document` and prints
/// the whole document back, and `tomet_ast` has no comment node at all --
/// `//` and `/* */` exist in the source text and nowhere in the tree, so
/// anything that round-trips through the AST loses them. Measured, not
/// assumed: a file carrying `@meta{ type: note }` and a `//` line comes
/// back with the comment gone.
///
/// The counting half is sound, because it counts rule hits rather than a
/// text diff -- `tomet refactor --check` is safe to run on anything, and
/// `FileDiff::is_changed` is `changes_count > 0` precisely so a file no
/// rule touched is never rewritten. It is `-i` on a file a rule *does*
/// touch that costs you the comments.
///
/// The `+++` fence used to be collapsed onto one line here too. That half
/// is fixed: a fence is delimited by lines, so the printer reproduces it.
pub fn refactor_source(
    src: &str,
    config: &PrinterConfig,
    options: &RefactorOptions,
) -> anyhow::Result<(String, usize)> {
    let mut doc = parse_document(src).map_err(|e| anyhow::anyhow!("parse error: {e}"))?;
    let changes = refactor_document(&mut doc, config, options);

    let mut effective_config = config.clone();
    if options.meta_to_value_dsl {
        effective_config.meta_format = None;
    }

    let rendered = document_to_tm_with_config(&doc, &effective_config);
    let formatted = format_source(&rendered);

    Ok((formatted, changes))
}

/// Discovers and refactors all `.tmt` files in `dir_or_file`.
/// What a sweep found: the per-file diffs, and the files it could not
/// look at.
///
/// The errors are carried rather than dropped. One unreadable or
/// unparseable file should not abort the sweep -- that is
/// `check_vault`'s convention too -- but it must not vanish either.
/// `--check` is a guard, and a guard that stays quiet about the files it
/// skipped has its hole exactly where it matters most: a file broken
/// enough not to parse is the likeliest one to still carry the spelling
/// being migrated away from.
#[derive(Debug, Default)]
pub struct RefactorReport {
    pub diffs: Vec<FileDiff>,
    /// Path and error message, one per file that could not be read or
    /// parsed.
    pub errors: Vec<(PathBuf, String)>,
}

pub fn refactor_workspace(
    target_path: &Path,
    options: &RefactorOptions,
) -> anyhow::Result<RefactorReport> {
    let (config, _, config_root) = find_config_file(target_path).unwrap_or_else(|| {
        (
            PrinterConfig::default(),
            target_path.to_path_buf(),
            target_path.to_path_buf(),
        )
    });

    let files = if target_path.is_file() {
        vec![target_path.to_path_buf()]
    } else {
        collect_tm_files_with_config(target_path, &config, &config_root)
    };

    let mut report = RefactorReport::default();

    for path in files {
        let original_src = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                report.errors.push((path, e.to_string()));
                continue;
            }
        };

        match refactor_source(&original_src, &config, options) {
            Ok((modified_src, count)) => {
                report
                    .diffs
                    .push(FileDiff::new(path, original_src, modified_src, count));
            }
            Err(e) => {
                report.errors.push((path, e.to_string()));
            }
        }
    }

    Ok(report)
}
