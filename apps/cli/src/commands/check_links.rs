use std::collections::BTreeMap;
use std::path::Path;

/// Runs the broken-link check and prints its report. Returns `Ok(true)`
/// if there is something to deal with -- a broken link, or a document that
/// could not be read (distinct from `Err`, which means the
/// check itself couldn't run) so `main` can exit non-zero without
/// treating "found breakage" the same as "something went wrong".
pub(crate) fn check_links_cmd(target_path: &Path, json: bool) -> anyhow::Result<bool> {
    if !target_path.exists() {
        return Err(anyhow::anyhow!(
            "path '{}' does not exist",
            target_path.display()
        ));
    }

    let (config, _, config_root) = tomet_load_config::find_config_file(target_path).unwrap_or((
        tomet_config::PrinterConfig::default(),
        target_path.to_path_buf(),
        target_path.to_path_buf(),
    ));

    let cache_path = tomet_links::default_cache_path();
    let mut cache = tomet_links::LinkCache::open(&cache_path)?;

    let report =
        tomet_links::check_vault(target_path, &config, &config_root, &config_root, &mut cache);

    // One database is shared by every vault on the machine, so rows for
    // vaults that have since moved or been deleted would otherwise sit
    // there forever. Nothing reads them, but nothing removes them either.
    let _ = cache.prune_missing();

    if json {
        println!("{}", check_links_report_to_json(&report));
    } else {
        println!(
            "{} file(s) scanned, {} link(s) checked, {} broken",
            report.files_scanned,
            report.links_checked,
            report.broken.len(),
        );
        for (link, at) in report.broken.iter().zip(locate(&report.broken)) {
            let at = at.map_or("?:?".to_string(), |p| format!("{}:{}", p.line, p.column));
            println!(
                "  {}:{at}: broken link -> {}",
                link.source.display(),
                link.target
            );
        }
        for (file, err) in &report.errors {
            eprintln!("  {}: {err}", file.display());
        }
    }

    Ok(has_problems(&report))
}

/// Whether the run found something the person has to deal with: a link that
/// is broken, or a document that could not be looked at.
///
/// The second counts. A document that could not be read has links nobody
/// checked, and a run that exits 0 over it says they are fine -- the same
/// reason `check` and `refactor --check` fail on a file they could not read.
fn has_problems(report: &tomet_links::CheckReport) -> bool {
    !report.broken.is_empty() || !report.errors.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn a_clean_report_has_no_problems() {
        assert!(!has_problems(&tomet_links::CheckReport::default()));
    }

    #[test]
    fn a_broken_link_is_a_problem() {
        let report = tomet_links::CheckReport {
            broken: vec![tomet_links::BrokenLink {
                source: PathBuf::from("a.tmt"),
                target: "missing.png".to_string(),
                span: tomet_ast::Span::new(0, 1),
            }],
            ..Default::default()
        };
        assert!(has_problems(&report));
    }

    /// No broken link was found, because the document that might have held
    /// one could not be read.
    #[test]
    fn a_document_that_could_not_be_read_is_a_problem() {
        let report = tomet_links::CheckReport {
            errors: vec![(PathBuf::from("a.tmt"), "could not read".to_string())],
            ..Default::default()
        };
        assert!(has_problems(&report));
    }
}

/// The line and column of each broken link. A link carries a byte span into its
/// source file, so each file is read once and indexed once, however many of its
/// links are broken. `None` when the file can no longer be read.
fn locate(broken: &[tomet_links::BrokenLink]) -> Vec<Option<tomet_lexer::LineCol>> {
    let mut located = vec![None; broken.len()];
    let mut by_file: BTreeMap<&Path, Vec<usize>> = BTreeMap::new();
    for (i, link) in broken.iter().enumerate() {
        by_file.entry(link.source.as_path()).or_default().push(i);
    }
    for (path, links) in by_file {
        let Ok(src) = std::fs::read_to_string(path) else {
            continue;
        };
        let index = tomet_lexer::LineIndex::new(&src);
        for i in links {
            located[i] = Some(index.line_col(broken[i].span.start));
        }
    }
    located
}

fn check_links_report_to_json(report: &tomet_links::CheckReport) -> String {
    let broken: Vec<serde_json::Value> = report
        .broken
        .iter()
        .zip(locate(&report.broken))
        .map(|(l, at)| {
            serde_json::json!({
                "source": l.source.display().to_string(),
                "target": l.target,
                "line": at.map(|p| p.line),
                "column": at.map(|p| p.column),
            })
        })
        .collect();
    let errors: Vec<serde_json::Value> = report
        .errors
        .iter()
        .map(|(path, err)| {
            serde_json::json!({
                "source": path.display().to_string(),
                "error": err,
            })
        })
        .collect();
    serde_json::json!({
        "files_scanned": report.files_scanned,
        "links_checked": report.links_checked,
        "broken": broken,
        "errors": errors,
    })
    .to_string()
}
