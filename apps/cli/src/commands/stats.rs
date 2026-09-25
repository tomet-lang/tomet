use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use tomet_ast::{Block, Document, Inline, Sigil};
use tomet_links::LinkKind;

use super::check::Findings;

/// What one document, or a run of them, holds. Every field is a plain count
/// so that a sweep can add documents together with [`Stats::merge`].
#[derive(Debug, Default, Clone, PartialEq)]
struct Stats {
    /// Documents measured. One for a single document.
    files: usize,
    /// Prose a reader sees: `Text` inside `[...]`, headings included,
    /// whitespace left out. `(args)`, `{value}` and code block bodies
    /// (`Inline::Raw`) are data, not prose, and are not counted.
    characters: usize,
    /// `@name`.
    named: usize,
    /// `${...}`.
    dollar: usize,
    /// `^` and `^name`.
    caret: usize,
    /// The bare entries of a list, `- ...`. Kept apart so `named` stays the
    /// count of things somebody wrote a name for.
    bare: usize,
    /// `named`, by name -- `deck.card` and `card` are different rows.
    by_name: BTreeMap<String, usize>,
    /// In `LinkKind::ALL` order.
    links: [usize; LinkKind::ALL.len()],
    headings: usize,
    max_heading_level: usize,
}

impl Stats {
    fn elements(&self) -> usize {
        self.named + self.dollar + self.caret + self.bare
    }

    fn total_links(&self) -> usize {
        self.links.iter().sum()
    }

    fn merge(&mut self, other: Stats) {
        self.files += other.files;
        self.characters += other.characters;
        self.named += other.named;
        self.dollar += other.dollar;
        self.caret += other.caret;
        self.bare += other.bare;
        for (name, n) in other.by_name {
            *self.by_name.entry(name).or_default() += n;
        }
        for (mine, theirs) in self.links.iter_mut().zip(other.links) {
            *mine += theirs;
        }
        self.headings += other.headings;
        self.max_heading_level = self.max_heading_level.max(other.max_heading_level);
    }

    /// `by_name` as rows, most used first and by name among equals.
    fn names_by_use(&self) -> Vec<(&str, usize)> {
        let mut rows: Vec<(&str, usize)> =
            self.by_name.iter().map(|(k, v)| (k.as_str(), *v)).collect();
        rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        rows
    }
}

/// Measures one parsed document.
///
/// Elements and text are walked by the same traversal (`tomet_tree`), and
/// links are `collect_links` as `check-links` uses it, so the three agree
/// about which part of the tree they are describing.
fn measure(doc: &Document) -> Stats {
    let mut stats = Stats {
        files: 1,
        ..Stats::default()
    };

    tomet_tree::for_each_element(doc, |el| match &el.sigil {
        Sigil::Named(name) => {
            stats.named += 1;
            *stats.by_name.entry(name.to_string()).or_default() += 1;
        }
        Sigil::Dollar => stats.dollar += 1,
        Sigil::Caret(_) => stats.caret += 1,
        Sigil::Bare => stats.bare += 1,
    });

    tomet_tree::for_each_inline(doc, |inline| {
        if let Inline::Text(text) = inline {
            stats.characters += text.value.chars().filter(|c| !c.is_whitespace()).count();
        }
    });

    for link in tomet_links::collect_links(doc) {
        if let Some(i) = LinkKind::ALL.iter().position(|k| *k == link.kind) {
            stats.links[i] += 1;
        }
    }

    count_headings(&doc.blocks, &mut stats);
    stats
}

fn count_headings(blocks: &[Block], stats: &mut Stats) {
    for block in blocks {
        if let Block::Section(section) = block {
            stats.headings += 1;
            stats.max_heading_level = stats.max_heading_level.max(section.level);
            count_headings(&section.blocks, stats);
        }
    }
}

/// A file that could not be measured, and why.
///
/// The text report only counts these. Which file and where is in the JSON,
/// and `check` is the place to see what is wrong with a file.
#[derive(Debug)]
struct Failure {
    file: PathBuf,
    /// `(line, column)`, when the parser said where.
    position: Option<(usize, usize)>,
    message: String,
}

impl Failure {
    fn unreadable(file: &Path) -> Self {
        Failure {
            file: file.to_path_buf(),
            position: None,
            message: "could not read file".to_string(),
        }
    }

    fn parse(file: &Path, err: &tomet_parser::Error) -> Self {
        Failure {
            file: file.to_path_buf(),
            position: Some((err.line, err.column)),
            message: err.message.clone(),
        }
    }
}

/// Everything a run found: the sum, the same numbers split by the `@kind`
/// each document declares (`None` for a document that declares none), the
/// files that could not be measured, and what `check` would say.
struct Report {
    total: Stats,
    /// Most files first, then by name; the kindless row last.
    kinds: Vec<(Option<String>, Stats)>,
    failures: Vec<Failure>,
    /// Documents that parsed, and so are in the numbers, but that `check`
    /// fails for what they say (an element that does not exist, two
    /// `@meta`).
    invalid: usize,
    /// The warnings `check` reports, over the documents that parsed.
    warnings: usize,
}

impl Report {
    /// The files `check` would fail: those that could not be measured, and
    /// those that were measured but do not validate. No file is in both.
    fn failed(&self) -> usize {
        self.failures.len() + self.invalid
    }

    fn new(
        by_kind: BTreeMap<Option<String>, Stats>,
        failures: Vec<Failure>,
        invalid: usize,
        warnings: usize,
    ) -> Self {
        let mut total = Stats::default();
        let mut kinds: Vec<(Option<String>, Stats)> = Vec::new();
        for (kind, stats) in by_kind {
            total.merge(stats.clone());
            kinds.push((kind, stats));
        }
        kinds.sort_by(|(ka, a), (kb, b)| {
            ka.is_none()
                .cmp(&kb.is_none())
                .then(b.files.cmp(&a.files))
                .then(b.characters.cmp(&a.characters))
                .then(ka.cmp(kb))
        });
        Report {
            total,
            kinds,
            failures,
            invalid,
            warnings,
        }
    }
}

/// Measures one file, or every `.tmt` under a directory, and checks each
/// the way `check` does. Returns the report and how many files were looked
/// at.
///
/// The files are found the way `check` finds them, through the vault, so
/// `workspace.ignore` applies. A file that cannot be read or parsed is left
/// out of the numbers, and the sweep goes on.
fn collect_report(path: &Path) -> anyhow::Result<(Report, usize)> {
    let vault = tomet_load::Vault::discover(path);

    let files = if path.is_file() {
        vec![path.to_path_buf()]
    } else if path.is_dir() {
        tomet_indexer::collect_tm_files_with_config(path, &vault.config, &vault.root)
    } else {
        return Err(anyhow::anyhow!("path '{}' does not exist", path.display()));
    };

    let mut by_kind: BTreeMap<Option<String>, Stats> = BTreeMap::new();
    let mut failures: Vec<Failure> = Vec::new();
    let mut invalid = 0usize;
    let mut warnings = 0usize;

    for file in &files {
        let Ok(src) = std::fs::read_to_string(file) else {
            failures.push(Failure::unreadable(file));
            continue;
        };
        let doc = match tomet_parser::parse_document(&src) {
            Ok(doc) => doc,
            Err(err) => {
                failures.push(Failure::parse(file, &err));
                continue;
            }
        };

        let findings = Findings::of(&vault, file, &doc);
        warnings += findings.warnings();
        if findings.has_errors() {
            invalid += 1;
        }
        by_kind
            .entry(tomet_semantics::document_kind(&doc))
            .or_default()
            .merge(measure(&doc));
    }

    Ok((
        Report::new(by_kind, failures, invalid, warnings),
        files.len(),
    ))
}

/// Prints statistics for one file, or every `.tmt` under a directory.
///
/// The run exits non-zero when `check` would fail a file, once the report
/// is out -- the convention `check` already follows.
pub(crate) fn stats_cmd(path: &Path, json: bool) -> anyhow::Result<()> {
    let (report, files) = collect_report(path)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&to_json(&report))?);
    } else {
        print!("{}", render(&report));
    }

    if report.failed() > 0 {
        return Err(anyhow::anyhow!(
            "{} of {files} file(s) failed",
            report.failed()
        ));
    }
    Ok(())
}

/// One set of numbers as JSON. `by_name` is in the same order the text
/// report lists it.
fn stats_json(stats: &Stats) -> serde_json::Map<String, serde_json::Value> {
    let mut by_name = serde_json::Map::new();
    for (name, n) in stats.names_by_use() {
        by_name.insert(name.to_string(), n.into());
    }
    let mut by_kind = serde_json::Map::new();
    for (kind, n) in LinkKind::ALL.iter().zip(stats.links) {
        by_kind.insert(kind.as_str().to_string(), n.into());
    }
    let value = serde_json::json!({
        "files": stats.files,
        "characters": stats.characters,
        "elements": {
            "total": stats.elements(),
            "named": stats.named,
            "dollar": stats.dollar,
            "caret": stats.caret,
            "bare": stats.bare,
            "by_name": by_name,
        },
        "links": {
            "total": stats.total_links(),
            "by_kind": by_kind,
        },
        "headings": {
            "count": stats.headings,
            "max_level": stats.max_heading_level,
        },
    });
    match value {
        serde_json::Value::Object(map) => map,
        _ => unreachable!("json! of an object literal is an object"),
    }
}

/// The whole run as JSON: the totals, then the same shape once per `@kind`
/// (`"kind": null` for documents that declare none), what `check` would
/// say (`check`), and the files that could not be measured (`errors`, the
/// only place the text report leaves out).
/// `files` counts the documents that were measured; a file that could not
/// be is in `errors` and in no count.
fn to_json(report: &Report) -> serde_json::Value {
    let mut out = stats_json(&report.total);
    out.insert(
        "check".to_string(),
        serde_json::json!({
            "failed": report.failed(),
            "warnings": report.warnings,
        }),
    );
    let kinds: Vec<serde_json::Value> = report
        .kinds
        .iter()
        .map(|(kind, stats)| {
            let mut row = serde_json::Map::new();
            row.insert("kind".to_string(), serde_json::json!(kind));
            row.extend(stats_json(stats));
            serde_json::Value::Object(row)
        })
        .collect();
    out.insert("kinds".to_string(), kinds.into());
    let errors: Vec<serde_json::Value> = report
        .failures
        .iter()
        .map(|f| {
            serde_json::json!({
                "file": f.file.display().to_string(),
                "line": f.position.map(|(line, _)| line),
                "column": f.position.map(|(_, column)| column),
                "message": f.message,
            })
        })
        .collect();
    out.insert("errors".to_string(), errors.into());
    serde_json::Value::Object(out)
}

const KIND_HEADER: &str = "Kind";
const KIND_NONE: &str = "(none)";
const KIND_TOTAL: &str = "Total";
const COLUMNS: [&str; 5] = ["Files", "Characters", "Elements", "Links", "Headings"];
/// The notes under the table are labelled in a column this wide.
const NOTE_LABEL: usize = 10;
/// Where a long note wraps.
const NOTE_WIDTH: usize = 79;

fn columns_of(stats: &Stats) -> [usize; 5] {
    [
        stats.files,
        stats.characters,
        stats.elements(),
        stats.total_links(),
        stats.headings,
    ]
}

/// The report as text: a table with a row per `@kind` and a total, then
/// the breakdowns that do not fit a column.
///
/// The table follows `scc`'s shape -- a row per category, a column per
/// measure, rules between the header, the rows and the total.
fn render(report: &Report) -> String {
    let rows: Vec<(String, [usize; 5])> = report
        .kinds
        .iter()
        .map(|(kind, stats)| {
            let label = kind.clone().unwrap_or_else(|| KIND_NONE.to_string());
            (label, columns_of(stats))
        })
        .collect();
    let total = columns_of(&report.total);

    let label_width = rows
        .iter()
        .map(|(label, _)| label.chars().count())
        .chain([KIND_HEADER.len(), KIND_TOTAL.len()])
        .max()
        .unwrap_or(KIND_HEADER.len());
    let widths: Vec<usize> = (0..COLUMNS.len())
        .map(|i| {
            rows.iter()
                .map(|(_, cells)| cells)
                .chain([&total])
                .map(|cells| group(cells[i]).len())
                .chain([COLUMNS[i].len()])
                .max()
                .unwrap_or(0)
        })
        .collect();
    let line_width = label_width + widths.iter().map(|w| w + 2).sum::<usize>();
    let rule = "─".repeat(line_width);

    let row = |label: &str, cells: Option<&[usize; 5]>| {
        let pad = label_width - label.chars().count();
        let mut line = format!("{label}{}", " ".repeat(pad));
        for (i, width) in widths.iter().enumerate() {
            let cell = match cells {
                Some(cells) => group(cells[i]),
                None => COLUMNS[i].to_string(),
            };
            line.push_str(&format!("  {cell:>width$}"));
        }
        line.push('\n');
        line
    };

    let mut out = String::new();
    out.push_str(&rule);
    out.push('\n');
    out.push_str(&row(KIND_HEADER, None));
    out.push_str(&rule);
    out.push('\n');
    for (label, cells) in &rows {
        out.push_str(&row(label, Some(cells)));
    }
    out.push_str(&rule);
    out.push('\n');
    out.push_str(&row(KIND_TOTAL, Some(&total)));
    out.push_str(&rule);
    out.push('\n');

    let stats = &report.total;
    let mut notes: Vec<(&str, String)> = Vec::new();
    notes.push((
        "Elements",
        format!(
            "@ {} / ${{}} {} / ^ {} / - {}",
            group(stats.named),
            group(stats.dollar),
            group(stats.caret),
            group(stats.bare)
        ),
    ));
    notes.push((
        "Links",
        LinkKind::ALL
            .iter()
            .zip(stats.links)
            .map(|(kind, n)| format!("{} {}", kind.as_str(), group(n)))
            .collect::<Vec<_>>()
            .join(" / "),
    ));
    notes.push((
        "Headings",
        format!("deepest level {}", stats.max_heading_level),
    ));
    notes.push((
        "Check",
        format!(
            "failed {} / warnings {}",
            group(report.failed()),
            group(report.warnings)
        ),
    ));
    for (label, text) in &notes {
        out.push_str(&format!("{label:<NOTE_LABEL$}{text}\n"));
    }

    let names: Vec<String> = stats
        .names_by_use()
        .iter()
        .map(|(name, n)| format!("@{name} {}", group(*n)))
        .collect();
    if !names.is_empty() {
        out.push_str(&wrap_note("By name", &names));
    }
    out.push_str(&rule);
    out.push('\n');
    out
}

/// `label` and then `items` joined by `, `, wrapped at [`NOTE_WIDTH`] with
/// the continuation lines lined up under the first item.
fn wrap_note(label: &str, items: &[String]) -> String {
    let indent = " ".repeat(NOTE_LABEL);
    let mut out = format!("{label:<NOTE_LABEL$}");
    let mut line_len = NOTE_LABEL;
    for (i, item) in items.iter().enumerate() {
        let last = i + 1 == items.len();
        let piece = if last {
            item.clone()
        } else {
            format!("{item},")
        };
        let needs_room = if line_len == NOTE_LABEL { 0 } else { 1 };
        if line_len > NOTE_LABEL && line_len + needs_room + piece.chars().count() > NOTE_WIDTH {
            out.push('\n');
            out.push_str(&indent);
            line_len = NOTE_LABEL;
        } else if line_len > NOTE_LABEL {
            out.push(' ');
            line_len += 1;
        }
        out.push_str(&piece);
        line_len += piece.chars().count();
    }
    out.push('\n');
    out
}

/// `18304` as `18,304`.
fn group(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::check::check;
    use std::fs;

    fn measure_src(src: &str) -> Stats {
        measure(&tomet_parser::parse_document(src).expect("test source parses"))
    }

    #[test]
    fn groups_digits_by_three() {
        assert_eq!(group(0), "0");
        assert_eq!(group(999), "999");
        assert_eq!(group(1000), "1,000");
        assert_eq!(group(18304), "18,304");
        assert_eq!(group(1234567), "1,234,567");
    }

    #[test]
    fn whitespace_is_not_counted() {
        // 3 characters once the spaces and the line break are gone.
        let stats = measure_src("あ い\nう\n");
        assert_eq!(stats.characters, 3);
    }

    #[test]
    fn headings_count_as_characters() {
        let stats = measure_src("= 見出し\n\n本文\n");
        assert_eq!(stats.characters, 5);
        assert_eq!(stats.headings, 1);
        assert_eq!(stats.max_heading_level, 1);
    }

    #[test]
    fn args_and_value_are_not_characters() {
        // Only the two characters inside `[...]` are prose.
        let stats = measure_src("@note(id: abcdef){ key: value }[ 文字 ]\n");
        assert_eq!(stats.characters, 2);
    }

    #[test]
    fn a_code_block_body_is_not_characters() {
        let stats = measure_src("```rust\nlet x = 1;\n```\n");
        assert_eq!(stats.characters, 0);
    }

    #[test]
    fn heading_depth_is_the_deepest_level() {
        let stats = measure_src("= a\n\n== b\n\n=== c\n\n== d\n");
        assert_eq!(stats.headings, 4);
        assert_eq!(stats.max_heading_level, 3);
    }

    #[test]
    fn each_sigil_is_counted_on_its_own() {
        let stats = measure_src("@note[ ${meta.title} ^(n1) ]\n\n- あ\n- い\n");
        assert_eq!(stats.dollar, 1);
        assert_eq!(stats.caret, 1);
        assert_eq!(stats.bare, 2);
        // `@note` and the `@ul` the list is.
        assert_eq!(stats.named, 2);
        assert_eq!(stats.elements(), 6);
        assert_eq!(stats.by_name.get("note"), Some(&1));
        assert_eq!(stats.by_name.get("ul"), Some(&1));
    }

    #[test]
    fn a_namespaced_name_is_its_own_row() {
        let stats = measure_src("@card[ a ]\n\n@deck.card[ b ]\n");
        assert_eq!(stats.by_name.get("card"), Some(&1));
        assert_eq!(stats.by_name.get("deck.card"), Some(&1));
    }

    #[test]
    fn links_are_the_ones_check_links_looks_at() {
        let stats = measure_src(
            "@link(https://example.com)[ a ]\n\
             @link(target: id:n1)[ b ]\n\
             @link(target: readme.md)[ c ]\n\
             @link(target: tm:foo/bar)[ d ]\n\
             @link(target: ref:x)[ e ]\n\
             @embed(target: pic.png)\n\
             @file(a.txt)\n\
             @dir(src)\n",
        );
        // Url and Id are left out, as `collect_links` leaves them out.
        let by_kind: Vec<(&str, usize)> = LinkKind::ALL
            .iter()
            .zip(stats.links)
            .map(|(k, n)| (k.as_str(), n))
            .collect();
        assert_eq!(
            by_kind,
            [("file", 2), ("dir", 1), ("embed", 1), ("tm", 1), ("ref", 1)]
        );
        assert_eq!(stats.total_links(), 6);
    }

    #[test]
    fn merging_adds_counts_and_keeps_the_deepest_heading() {
        let mut a = measure_src("= a\n\n@note[ x ]\n");
        let b = measure_src("= a\n\n=== b\n\n@note[ yy ]\n");
        a.merge(b);
        // `a` + `x` from the first, `a` + `b` + `yy` from the second.
        assert_eq!(a.characters, 2 + 4);
        assert_eq!(a.by_name.get("note"), Some(&2));
        assert_eq!(a.headings, 3);
        assert_eq!(a.max_heading_level, 3);
    }

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("tomet_test_stats_{name}_{}", nanoid::nanoid!()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A file that does not parse fails the run, but it does not hide the
    /// numbers of the files that do.
    #[test]
    fn a_broken_file_fails_the_run_after_the_sweep() {
        let root = scratch("sweep");
        fs::write(root.join("good.tmt"), "@callout[ 文字 ]\n").unwrap();
        fs::write(root.join("bad.tmt"), "@callout[ unterminated\n").unwrap();

        let err = stats_cmd(&root, true).expect_err("one file is bad");
        assert!(err.to_string().contains("1 of 2"), "{err}");

        let (report, files) = collect_report(&root).unwrap();
        assert_eq!(files, 2);
        assert_eq!(report.total.files, 1);
        assert_eq!(report.total.characters, 2);
        assert_eq!(report.failures.len(), 1);

        // The good file alone is a clean run.
        stats_cmd(&root.join("good.tmt"), true).expect("the good file passes");
        let _ = fs::remove_dir_all(&root);
    }

    /// What the report calls failed and warned is what `check` says of the
    /// same directory: a document that parses but does not validate is
    /// measured and failed, and a `@draft` or an unresolved `${...}` is a
    /// warning that fails nothing.
    #[test]
    fn failed_and_warnings_are_what_check_reports() {
        let root = scratch("check");
        fs::write(root.join("ok.tmt"), "@callout[ a ]\n").unwrap();
        fs::write(root.join("draft.tmt"), "@draft[ later ]\n").unwrap();
        fs::write(root.join("unresolved.tmt"), "@callout[ ${missing_id} ]\n").unwrap();
        fs::write(root.join("invalid.tmt"), "@nonesuch[ x ]\n").unwrap();
        fs::write(root.join("bad.tmt"), "@callout[ unterminated\n").unwrap();

        let (report, files) = collect_report(&root).unwrap();
        assert_eq!(files, 5);
        // Four parse, so four are measured -- the invalid one included.
        assert_eq!(report.total.files, 4);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.invalid, 1);
        assert_eq!(report.failed(), 2);
        assert_eq!(report.warnings, 2);

        let from_check = check(&root, false, true, false).expect_err("two files fail");
        assert!(from_check.to_string().contains("2 of 5"), "{from_check}");
        let from_stats = stats_cmd(&root, false).expect_err("two files fail");
        assert!(from_stats.to_string().contains("2 of 5"), "{from_stats}");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_path_is_an_error() {
        let err =
            stats_cmd(Path::new("/nonexistent/tomet-stats"), false).expect_err("no such path");
        assert!(err.to_string().contains("does not exist"));
    }

    fn report_of(docs: &[(&str, &str)]) -> Report {
        let mut by_kind: BTreeMap<Option<String>, Stats> = BTreeMap::new();
        for (name, src) in docs {
            let doc = tomet_parser::parse_document(src).expect("test source parses");
            assert_eq!(
                tomet_semantics::document_kind(&doc).is_some(),
                !name.is_empty()
            );
            by_kind
                .entry(tomet_semantics::document_kind(&doc))
                .or_default()
                .merge(measure(&doc));
        }
        Report::new(by_kind, Vec::new(), 0, 0)
    }

    #[test]
    fn documents_are_grouped_by_their_declared_kind() {
        let report = report_of(&[
            ("deck", "@kind(deck)\n\n@note[ ab ]\n"),
            ("deck", "@kind(deck)\n\n@note[ c ]\n"),
            ("writ", "@kind(writ)\n\n@note[ dddd ]\n"),
            ("", "@note[ e ]\n"),
        ]);
        let rows: Vec<(Option<&str>, usize, usize)> = report
            .kinds
            .iter()
            .map(|(k, s)| (k.as_deref(), s.files, s.characters))
            .collect();
        // Most files first, the kindless row last however small it is.
        assert_eq!(
            rows,
            [(Some("deck"), 2, 3), (Some("writ"), 1, 4), (None, 1, 1)]
        );
        assert_eq!(report.total.files, 4);
        assert_eq!(report.total.characters, 8);
    }

    #[test]
    fn the_table_has_a_row_per_kind_and_a_total() {
        let report = report_of(&[
            ("deck", "@kind(deck)\n\n@note[ ab ]\n"),
            ("", "@note[ e ]\n"),
        ]);
        let text = render(&report);
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines[0].chars().all(|c| c == '─'), "{text}");
        assert!(lines[1].starts_with("Kind"), "{text}");
        for header in COLUMNS {
            assert!(lines[1].contains(header), "{text}");
        }
        assert!(lines.iter().any(|l| l.starts_with("deck")), "{text}");
        assert!(lines.iter().any(|l| l.starts_with("(none)")), "{text}");
        assert!(lines.iter().any(|l| l.starts_with("Total")), "{text}");
        // The rule closes the report, as it does in `scc`.
        assert!(lines.last().unwrap().chars().all(|c| c == '─'), "{text}");
        // Every table line is as wide as its rule.
        let width = lines[0].chars().count();
        for line in &lines[1..6] {
            if !line.starts_with('─') {
                assert_eq!(line.chars().count(), width, "{line:?}");
            }
        }
    }

    #[test]
    fn the_breakdowns_sit_under_the_total() {
        let mut report = report_of(&[("", "@note[ a ${x} ^(n) ]\n\n@note[ b ]\n")]);
        report.warnings = 12;
        report.invalid = 3;
        let text = render(&report);
        let after_total = text.split("Total").nth(1).expect("has a total row");
        // A symbol for each kind of element: the sigil it is written with.
        assert!(after_total.contains("@ 2 / ${} 1 / ^ 1 / - 0"), "{text}");
        assert!(after_total.contains("file 0 / dir 0 / embed 0 / tm 0 / ref 0"));
        assert!(after_total.contains("deepest level 0"));
        assert!(
            after_total.contains(&format!("{:<NOTE_LABEL$}failed 3 / warnings 12", "Check")),
            "{text}"
        );
        assert!(
            after_total.contains(&format!("{:<NOTE_LABEL$}@note 2", "By name")),
            "{text}"
        );
    }

    #[test]
    fn the_text_report_names_no_file() {
        let mut report = report_of(&[("", "@note[ a ]\n")]);
        report
            .failures
            .push(Failure::unreadable(Path::new("unreadable-file.tmt")));
        let text = render(&report);
        assert!(!text.contains("unreadable-file.tmt"), "{text}");
        assert!(text.contains("failed 1 / warnings 0"), "{text}");
    }

    #[test]
    fn a_long_name_list_wraps_under_its_first_item() {
        let items: Vec<String> = (0..30).map(|i| format!("@element-{i} {i}")).collect();
        let text = wrap_note("By name", &items);
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines.len() > 1, "{text}");
        for line in &lines {
            assert!(line.chars().count() <= NOTE_WIDTH, "{line:?}");
        }
        for line in &lines[1..] {
            assert!(line.starts_with(&" ".repeat(NOTE_LABEL)), "{line:?}");
            assert!(!line[NOTE_LABEL..].starts_with(' '), "{line:?}");
        }
        assert!(text.trim_end().ends_with("@element-29 29"));
    }

    #[test]
    fn json_carries_the_totals_the_kinds_the_check_and_the_failures() {
        let mut report = report_of(&[
            ("deck", "@kind(deck)\n\n@note[ ab ]\n"),
            ("", "@note[ e ]\n"),
        ]);
        report.warnings = 5;
        report.invalid = 1;
        report
            .failures
            .push(Failure::unreadable(Path::new("b.tmt")));
        report.failures.push(Failure::parse(
            Path::new("c.tmt"),
            &tomet_parser::Error {
                message: "expected ']'".into(),
                line: 3,
                column: 7,
                offset: 20,
            },
        ));
        let json = to_json(&report);
        assert_eq!(json["files"], 2);
        assert_eq!(json["characters"], 3);
        assert_eq!(json["kinds"][0]["kind"], "deck");
        assert_eq!(json["kinds"][0]["characters"], 2);
        assert!(json["kinds"][1]["kind"].is_null());
        // Two files that could not be measured and one that did not validate.
        assert_eq!(json["check"]["failed"], 3);
        assert_eq!(json["check"]["warnings"], 5);
        // The JSON is where a failure is named, and where it is.
        assert_eq!(json["errors"][0]["file"], "b.tmt");
        assert!(json["errors"][0]["line"].is_null());
        assert_eq!(json["errors"][1]["file"], "c.tmt");
        assert_eq!(json["errors"][1]["line"], 3);
        assert_eq!(json["errors"][1]["column"], 7);
        assert_eq!(json["errors"][1]["message"], "expected ']'");
    }
}
