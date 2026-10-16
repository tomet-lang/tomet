use std::collections::BTreeMap;
use std::path::Path;

/// Runs the broken-link check and prints its report. Returns `Ok(true)`
/// if any broken link was found (distinct from `Err`, which means the
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

    Ok(!report.broken.is_empty())
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
