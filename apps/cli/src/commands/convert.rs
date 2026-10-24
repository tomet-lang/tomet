//! The engine behind the six conversion commands: `html`, `to-md`, `to-typst`,
//! `to-pandoc`, `from-md` and `from-pandoc`.
//!
//! A command only says which way it converts ([`Direction`]); how the input is
//! read and where the result goes is the same for all of them. The input is a
//! file, standard input (`-`), or a directory. The result goes to standard
//! output, to `--out` (a file, or a directory that gets one named file, or, for
//! a directory input, the directory to write into with its sub-directories
//! kept), or next to the source with the new extension (`--in-place`).
//! `--dry-run` converts and checks, and writes nothing.
//!
//! What goes to standard output is exactly what a file would hold: no newline
//! is added, so the two never differ.
//!
//! Nothing here follows a document's own `::config(export: ...)`; that is
//! `export`. A conversion here is one you ask for by name.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use tomet_load::Vault;

use crate::util::{format_parse_error, is_stdin, meta_title, read, vault_for};

/// A format Tomet can be written as.
#[derive(Debug, Clone)]
pub(crate) enum Target {
    CommonMark,
    Html(HtmlOptions),
    Typst,
    /// Pandoc's own AST, JSON-encoded: `pandoc -f json` turns it into every
    /// format Pandoc writes.
    Pandoc,
}

impl Target {
    fn extension(&self) -> &'static str {
        match self {
            Target::CommonMark => "md",
            Target::Html(_) => "html",
            Target::Typst => "typ",
            Target::Pandoc => "json",
        }
    }
}

/// What `html` can be asked for beyond the format itself.
#[derive(Debug, Clone, Default)]
pub(crate) struct HtmlOptions {
    /// Number headings by nesting level (1, 1.1, 1.2, 2, ...).
    pub advanced: bool,
    /// `<html lang="...">` of the page shell.
    pub lang: Option<String>,
    /// The rendered body alone, with no `<html>`/`<head>` shell.
    pub body: bool,
}

/// A format that can be read as Tomet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Source {
    CommonMark,
    /// Pandoc's JSON AST (`pandoc -t json <file>`): the bridge from every
    /// format Pandoc reads.
    Pandoc,
}

/// Which way a conversion goes.
#[derive(Debug, Clone)]
pub(crate) enum Direction {
    /// From Tomet.
    To(Target),
    /// To Tomet.
    From(Source),
}

impl Direction {
    /// The extension of what is written.
    fn extension(&self) -> &'static str {
        match self {
            Direction::To(target) => target.extension(),
            Direction::From(_) => "tmt",
        }
    }
}

/// Where the result goes, and what else to do about it.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Plan<'a> {
    /// Write here. For a file, this is a file or an existing directory (which
    /// then gets one named after the source). For a directory, it is the
    /// directory to write into.
    pub out: Option<&'a Path>,
    /// Write next to each source, with the new extension.
    pub in_place: bool,
    /// Convert and check, and write nothing.
    pub dry_run: bool,
    /// Delete each source once its result is written. Only for a conversion
    /// to Tomet: the point is to migrate, and a Tomet source is not something
    /// to throw away.
    pub remove_original: bool,
}

/// What converting one file came to.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Converted {
    /// The converted text, for standard output: nowhere was named to write it.
    Text(String),
    /// Written to this path.
    Written(PathBuf),
    /// A dry run: it converts, and nothing was written.
    Checked,
}

/// Converts what `path` names -- a file, `-`, or a directory -- and shows what
/// came of it on standard output.
pub(crate) fn run(path: &Path, direction: &Direction, plan: &Plan<'_>) -> Result<()> {
    if plan.remove_original && plan.out.is_none() && !plan.in_place {
        return Err(anyhow!(
            "--remove-original needs --out or --in-place: there is nothing to replace \
             the original with otherwise"
        ));
    }
    let mut stdout = std::io::stdout().lock();

    if is_stdin(path) || path.is_file() {
        match convert_file(path, direction, plan)? {
            // Exactly the text, as it would be written to a file.
            Converted::Text(text) => write!(stdout, "{text}")?,
            Converted::Written(dest) => {
                writeln!(stdout, "Converted {} -> {}", path.display(), dest.display())?;
            }
            Converted::Checked => writeln!(stdout, "OK (dry-run): {}", path.display())?,
        }
        return Ok(());
    }
    if !path.is_dir() {
        return Err(anyhow!("path '{}' does not exist", path.display()));
    }

    let tree = convert_tree(path, direction, plan)?;
    if tree.items.is_empty() {
        let found = match direction {
            Direction::From(_) => ".md",
            Direction::To(_) => ".tmt",
        };
        writeln!(stdout, "No {found} files found in {}", path.display())?;
        return Ok(());
    }

    let (total, succeeded, failed) = (tree.items.len(), tree.succeeded(), tree.failed());
    let finished = if plan.dry_run {
        "Dry-run finished"
    } else {
        "Conversion finished"
    };
    writeln!(
        stdout,
        "{finished}: {total} total, {succeeded} succeeded, {failed} failed"
    )?;
    if failed > 0 {
        let mut stderr = std::io::stderr().lock();
        writeln!(stderr, "\nErrors encountered ({failed}):")?;
        for item in &tree.items {
            if let Err(error) = &item.result {
                writeln!(stderr, "  {}: {error}", item.source.display())?;
            }
        }
        return Err(anyhow!("{failed} file(s) failed conversion"));
    }
    Ok(())
}

/// Converts one file, or standard input (`-`), and puts the result where
/// `plan` says.
fn convert_file(path: &Path, direction: &Direction, plan: &Plan<'_>) -> Result<Converted> {
    // Said before anything is read: waiting for input that can then only be
    // refused would leave the person at a terminal with nothing to type.
    if is_stdin(path) && plan.in_place && plan.out.is_none() && !plan.dry_run {
        return Err(anyhow!(
            "cannot convert in place: standard input is not a file"
        ));
    }
    let src = read(path).map_err(|e| anyhow!("cannot read {}: {e}", path.display()))?;
    let text = convert_text(&vault_for(path), &src, path, direction)?;

    if plan.dry_run {
        return Ok(Converted::Checked);
    }
    let ext = direction.extension();
    let dest = if let Some(out) = plan.out {
        if out.is_dir() {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .filter(|_| !is_stdin(path))
                .unwrap_or("output");
            out.join(format!("{stem}.{ext}"))
        } else {
            out.to_path_buf()
        }
    } else if plan.in_place {
        path.with_extension(ext)
    } else {
        return Ok(Converted::Text(text));
    };

    write(&dest, &text)?;
    if plan.remove_original && path != dest {
        let _ = fs::remove_file(path);
    }
    Ok(Converted::Written(dest))
}

/// What converting one file of a directory came to.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Item {
    source: PathBuf,
    /// Where it went (or would go, in a dry run), or why it did not.
    result: std::result::Result<PathBuf, String>,
}

/// Every file of a directory that was looked at.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Tree {
    items: Vec<Item>,
}

impl Tree {
    fn failed(&self) -> usize {
        self.items.iter().filter(|i| i.result.is_err()).count()
    }

    fn succeeded(&self) -> usize {
        self.items.len() - self.failed()
    }
}

/// Converts every file under `dir` that is in the format being converted from:
/// the Markdown files for [`Source::CommonMark`], the Tomet documents for
/// [`Direction::To`].
///
/// A directory is converted into files, so the plan has to name where they go.
/// One file that fails does not stop the others; it is an [`Item`] with an
/// error, and a file only counts once it is where it was asked to go.
fn convert_tree(dir: &Path, direction: &Direction, plan: &Plan<'_>) -> Result<Tree> {
    if !plan.dry_run && plan.out.is_none() && !plan.in_place {
        return Err(anyhow!(
            "a directory is converted into files: pass --out <DIR> or --in-place (or --dry-run to only check)"
        ));
    }
    let files = match direction {
        Direction::From(Source::CommonMark) => markdown_files(dir),
        Direction::From(Source::Pandoc) => {
            return Err(anyhow!(
                "a directory can be converted from md, not from pandoc: \
                 Pandoc's JSON is one document, usually piped in (`-`)"
            ));
        }
        Direction::To(_) => tomet_indexer::collect_tm_files(dir),
    };

    // Discovered once for the whole run, as `export` does.
    let vault = vault_for(dir);
    let ext = direction.extension();
    let items = files
        .into_iter()
        .map(|source| {
            let result = convert_tree_file(&vault, dir, &source, direction, plan, ext);
            Item { source, result }
        })
        .collect();
    Ok(Tree { items })
}

fn convert_tree_file(
    vault: &Vault,
    dir: &Path,
    source: &Path,
    direction: &Direction,
    plan: &Plan<'_>,
    ext: &str,
) -> std::result::Result<PathBuf, String> {
    let src = fs::read_to_string(source).map_err(|e| format!("read error: {e}"))?;
    let text = convert_text(vault, &src, source, direction).map_err(|e| e.to_string())?;

    let dest = if let Some(out) = plan.out {
        let rel = source.strip_prefix(dir).unwrap_or(source);
        out.join(rel).with_extension(ext)
    } else {
        source.with_extension(ext)
    };
    if plan.dry_run {
        return Ok(dest);
    }
    write(&dest, &text).map_err(|e| e.to_string())?;
    if plan.remove_original && source != dest {
        let _ = fs::remove_file(source);
    }
    Ok(dest)
}

fn markdown_files(dir: &Path) -> Vec<PathBuf> {
    let (config, _, config_root) = tomet_load_config::find_config_file(dir).unwrap_or((
        tomet_config::PrinterConfig::default(),
        dir.to_path_buf(),
        dir.to_path_buf(),
    ));
    tomet_indexer::workspace_scan::WorkspaceIndex::build(dir, &config, &config_root)
        .migration_candidates()
        .into_iter()
        .map(|candidate| candidate.source_path)
        .collect()
}

/// The conversion itself, with no destination in it. `path` names the source
/// (`-` for standard input) for the sake of messages, the title and the config.
fn convert_text(vault: &Vault, src: &str, path: &Path, direction: &Direction) -> Result<String> {
    match direction {
        Direction::To(target) => to_text(vault, src, path, target),
        Direction::From(Source::CommonMark) => from_markdown(src, path),
        Direction::From(Source::Pandoc) => from_pandoc(src),
    }
}

fn to_text(vault: &Vault, src: &str, path: &Path, target: &Target) -> Result<String> {
    let (mut doc, _bindings) = vault
        .parse(src)
        .map_err(|e| anyhow!("{}", format_parse_error(path, src, &e)))?;
    // The vault settles what a `${macro.…}` or a `${self.…}` means, so this and
    // `export` cannot disagree about it.
    vault.prepare(&mut doc, path);
    Ok(match target {
        Target::CommonMark => tomet_markdown::to_markdown(&doc),
        Target::Typst => tomet_typst::to_typst(&doc),
        Target::Pandoc => serde_json::to_string(&tomet_pandoc::to_pandoc(&doc))?,
        Target::Html(options) => html(&doc, path, options),
    })
}

fn html(doc: &tomet_ast::Document, path: &Path, options: &HtmlOptions) -> String {
    let render = tomet_html::RenderOptions {
        number_headings: options.advanced,
        auto_slug_headings: options.advanced,
        lang: options.lang.clone(),
        ..Default::default()
    };
    if options.body {
        return tomet_html::render_body_with(doc, &render);
    }
    // The shell carries the title: the document's own, else its file's name.
    let fallback = path
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|_| !is_stdin(path))
        .unwrap_or("Tomet");
    let title = meta_title(doc).unwrap_or_else(|| fallback.to_string());
    tomet_html::render_page_with(doc, &title, &render)
}

/// Markdown to Tomet, laid out as the vault's config says (table widths) and
/// given an id if the config asks for one. The result is parsed again, so a
/// conversion never hands back text that is not Tomet.
fn from_markdown(src: &str, path: &Path) -> Result<String> {
    // Standard input has no place of its own; it borrows the working
    // directory's config.
    let at = if is_stdin(path) { Path::new(".") } else { path };
    let (config, _, _) = tomet_load_config::find_config_file(at).unwrap_or((
        tomet_config::PrinterConfig::default(),
        at.to_path_buf(),
        at.to_path_buf(),
    ));
    let import = tomet_markdown::ImportOptions {
        adjust_table_width: config.format.table_adjust_width.as_deref() == Some("true")
            || config.format.table_adjust_width.as_deref() == Some("auto"),
        table_adjust_width_mode: config.format.table_adjust_width.clone(),
        table_max_col_width: config.format.table_max_col_width,
        table_align: config.format.table_align.clone(),
    };
    let mut doc = tomet_markdown::from_markdown_with_options(src, &import);
    tomet_printer::ensure_document_id_with_config(&mut doc, &config);
    let tmt = tomet_printer::document_to_tm_with_config(&doc, &config);
    if let Err(e) = tomet_parser::parse_document(&tmt) {
        return Err(anyhow!(
            "generated .tmt has parse error: {}",
            format_parse_error(path, &tmt, &e)
        ));
    }
    Ok(tmt)
}

fn from_pandoc(json: &str) -> Result<String> {
    let pandoc: tomet_pandoc::PandocDoc = serde_json::from_str(json).map_err(|e| {
        anyhow!("not a Pandoc JSON document ({e}). Produce one with `pandoc -t json <file>`.")
    })?;
    Ok(tomet_printer::document_to_tm(&tomet_pandoc::from_pandoc(
        &pandoc,
    )))
}

/// Writes `text` to `dest`, making its directory if it has none.
fn write(dest: &Path, text: &str) -> Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| anyhow!("cannot create {}: {e}", parent.display()))?;
    }
    fs::write(dest, text).map_err(|e| anyhow!("cannot write {}: {e}", dest.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory in a fresh temporary place, removed when dropped.
    struct Dir(PathBuf);

    impl Dir {
        fn new(name: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir = std::env::temp_dir().join(format!("tomet-convert-{nanos}-{name}"));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn write(&self, rel: &str, text: &str) -> PathBuf {
            let path = self.0.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, text).unwrap();
            path
        }

        fn path(&self, rel: &str) -> PathBuf {
            self.0.join(rel)
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const TMT: &str = "=[ Hello Convert ]\n\nA paragraph.\n";
    const MD: &str = "# Hello Convert\n\nA paragraph.\n";

    fn to(target: Target) -> Direction {
        Direction::To(target)
    }

    fn from_md() -> Direction {
        Direction::From(Source::CommonMark)
    }

    fn stdout() -> Plan<'static> {
        Plan::default()
    }

    fn text_of(converted: Converted) -> String {
        match converted {
            Converted::Text(text) => text,
            other => panic!("expected text, got {other:?}"),
        }
    }

    #[test]
    fn each_target_writes_its_format() {
        let dir = Dir::new("targets");
        let src = dir.write("doc.tmt", TMT);

        let md = text_of(convert_file(&src, &to(Target::CommonMark), &stdout()).unwrap());
        assert!(md.contains("# Hello Convert"), "{md}");

        let html = text_of(
            convert_file(&src, &to(Target::Html(HtmlOptions::default())), &stdout()).unwrap(),
        );
        assert!(html.contains("<html"), "{html}");
        assert!(
            html.contains("<title>doc</title>"),
            "the file stem titles it: {html}"
        );

        let typst = text_of(convert_file(&src, &to(Target::Typst), &stdout()).unwrap());
        assert_eq!(typst, "= Hello Convert\n\nA paragraph.\n\n");

        let pandoc = text_of(convert_file(&src, &to(Target::Pandoc), &stdout()).unwrap());
        assert!(
            serde_json::from_str::<serde_json::Value>(&pandoc).is_ok(),
            "{pandoc}"
        );
    }

    #[test]
    fn html_can_be_a_body_alone() {
        let dir = Dir::new("html-body");
        let src = dir.write("doc.tmt", TMT);
        let options = HtmlOptions {
            body: true,
            ..HtmlOptions::default()
        };

        let body = text_of(convert_file(&src, &to(Target::Html(options)), &stdout()).unwrap());
        assert!(!body.contains("<html"), "{body}");
        assert!(body.contains("Hello Convert"), "{body}");
    }

    #[test]
    fn what_goes_to_standard_output_is_what_would_be_written() {
        let dir = Dir::new("same-bytes");
        let src = dir.write("doc.tmt", TMT);
        let out = dir.path("doc.md");

        let printed = text_of(convert_file(&src, &to(Target::CommonMark), &stdout()).unwrap());
        convert_file(
            &src,
            &to(Target::CommonMark),
            &Plan {
                out: Some(&out),
                ..Plan::default()
            },
        )
        .unwrap();
        assert_eq!(fs::read_to_string(&out).unwrap(), printed);
    }

    #[test]
    fn markdown_becomes_tomet() {
        let dir = Dir::new("from-md");
        let src = dir.write("note.md", MD);

        let tmt = text_of(convert_file(&src, &from_md(), &stdout()).unwrap());
        assert!(tmt.contains("=[Hello Convert]"), "{tmt}");
        assert!(tomet_parser::parse_document(&tmt).is_ok(), "{tmt}");
    }

    #[test]
    fn pandoc_json_becomes_tomet_and_a_round_trip_keeps_the_text() {
        let dir = Dir::new("pandoc");
        let src = dir.write("doc.tmt", TMT);
        let json = text_of(convert_file(&src, &to(Target::Pandoc), &stdout()).unwrap());
        let json_file = dir.write("doc.json", &json);

        let back =
            text_of(convert_file(&json_file, &Direction::From(Source::Pandoc), &stdout()).unwrap());
        assert!(back.contains("Hello Convert"), "{back}");
        assert!(back.contains("A paragraph."), "{back}");
    }

    #[test]
    fn something_that_is_not_pandoc_json_says_how_to_make_it() {
        let dir = Dir::new("not-pandoc");
        let src = dir.write("x.json", "{ not pandoc");
        let error = convert_file(&src, &Direction::From(Source::Pandoc), &stdout()).unwrap_err();
        assert!(
            error.to_string().contains("not a Pandoc JSON document"),
            "{error}"
        );
        assert!(error.to_string().contains("pandoc -t json"), "{error}");
    }

    #[test]
    fn a_parse_error_is_shown_with_its_line_and_column() {
        let dir = Dir::new("parse-error");
        let src = dir.write("broken.tmt", "=[ unterminated\n");
        let error = convert_file(&src, &to(Target::CommonMark), &stdout()).unwrap_err();
        assert!(error.to_string().contains("--> "), "{error}");
        assert!(error.to_string().contains("broken.tmt"), "{error}");
    }

    #[test]
    fn a_missing_file_is_named() {
        let dir = Dir::new("missing");
        let missing = dir.path("nope.tmt");
        let error = convert_file(&missing, &to(Target::CommonMark), &stdout()).unwrap_err();
        assert!(error.to_string().contains("nope.tmt"), "{error}");
    }

    #[test]
    fn out_is_a_file_or_a_directory_that_gets_a_named_file() {
        let dir = Dir::new("out");
        let src = dir.write("doc.tmt", TMT);
        let target = to(Target::CommonMark);

        let file = dir.path("elsewhere/renamed.md");
        let converted = convert_file(
            &src,
            &target,
            &Plan {
                out: Some(&file),
                ..Plan::default()
            },
        )
        .unwrap();
        assert_eq!(converted, Converted::Written(file.clone()));
        assert!(file.is_file());

        let folder = dir.path("folder");
        fs::create_dir_all(&folder).unwrap();
        let converted = convert_file(
            &src,
            &target,
            &Plan {
                out: Some(&folder),
                ..Plan::default()
            },
        )
        .unwrap();
        assert_eq!(converted, Converted::Written(folder.join("doc.md")));
    }

    #[test]
    fn in_place_writes_next_to_the_source_with_the_new_extension() {
        let dir = Dir::new("in-place");
        let src = dir.write("note.md", MD);

        let converted = convert_file(
            &src,
            &from_md(),
            &Plan {
                in_place: true,
                ..Plan::default()
            },
        )
        .unwrap();
        assert_eq!(converted, Converted::Written(dir.path("note.tmt")));
        assert!(dir.path("note.md").exists(), "the original stays");
    }

    #[test]
    fn remove_original_deletes_the_source_once_it_is_written() {
        let dir = Dir::new("remove");
        let src = dir.write("note.md", MD);

        convert_file(
            &src,
            &from_md(),
            &Plan {
                in_place: true,
                remove_original: true,
                ..Plan::default()
            },
        )
        .unwrap();
        assert!(dir.path("note.tmt").is_file());
        assert!(!dir.path("note.md").exists());
    }

    #[test]
    fn remove_original_needs_somewhere_to_write() {
        let dir = Dir::new("remove-nowhere");
        let src = dir.write("note.md", MD);

        let error = run(
            &src,
            &from_md(),
            &Plan {
                remove_original: true,
                ..Plan::default()
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("--remove-original"), "{error}");
        assert!(src.exists());
    }

    #[test]
    fn a_dry_run_converts_and_writes_nothing() {
        let dir = Dir::new("dry");
        let src = dir.write("note.md", MD);

        let converted = convert_file(
            &src,
            &from_md(),
            &Plan {
                in_place: true,
                dry_run: true,
                ..Plan::default()
            },
        )
        .unwrap();
        assert_eq!(converted, Converted::Checked);
        assert!(!dir.path("note.tmt").exists());

        // And it still fails on what would not convert.
        let bad = dir.write("bad.json", "{ not pandoc");
        assert!(
            convert_file(
                &bad,
                &Direction::From(Source::Pandoc),
                &Plan {
                    dry_run: true,
                    ..Plan::default()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn standard_input_cannot_be_converted_in_place() {
        let error = convert_file(
            Path::new("-"),
            &from_md(),
            &Plan {
                in_place: true,
                ..Plan::default()
            },
        );
        // Refused before anything is read. If it read first, this test would
        // wait for input at a terminal and never finish.
        assert!(
            error
                .unwrap_err()
                .to_string()
                .contains("standard input is not a file")
        );
    }

    #[test]
    fn a_directory_needs_somewhere_to_be_converted_into() {
        let dir = Dir::new("tree-needs-dest");
        dir.write("a.md", MD);

        let error = convert_tree(&dir.0, &from_md(), &stdout()).unwrap_err();
        assert!(
            error.to_string().contains("--out <DIR> or --in-place"),
            "{error}"
        );
        assert!(!dir.path("a.tmt").exists());
    }

    #[test]
    fn a_directory_of_markdown_is_converted_with_its_structure_kept() {
        let dir = Dir::new("tree-from");
        dir.write("a.md", MD);
        dir.write("sub/b.md", MD);
        let out = dir.path("out");

        let tree = convert_tree(
            &dir.0,
            &from_md(),
            &Plan {
                out: Some(&out),
                ..Plan::default()
            },
        )
        .unwrap();

        assert_eq!(tree.items.len(), 2, "{tree:?}");
        assert_eq!(tree.failed(), 0, "{tree:?}");
        assert!(out.join("a.tmt").is_file());
        assert!(out.join("sub/b.tmt").is_file());
        assert!(dir.path("a.md").exists());
    }

    #[test]
    fn a_directory_of_tomet_is_converted_to_a_format_in_place() {
        let dir = Dir::new("tree-to");
        dir.write("a.tmt", TMT);
        dir.write("sub/b.tmt", TMT);

        let tree = convert_tree(
            &dir.0,
            &to(Target::CommonMark),
            &Plan {
                in_place: true,
                ..Plan::default()
            },
        )
        .unwrap();

        assert_eq!(tree.succeeded(), 2, "{tree:?}");
        assert!(dir.path("a.md").is_file());
        assert!(dir.path("sub/b.md").is_file());
        assert!(dir.path("a.tmt").exists(), "the source stays");
    }

    #[test]
    fn one_file_that_fails_is_counted_and_the_others_are_still_converted() {
        let dir = Dir::new("tree-failure");
        dir.write("good.tmt", TMT);
        dir.write("broken.tmt", "=[ unterminated\n");
        let out = dir.path("out");

        let tree = convert_tree(
            &dir.0,
            &to(Target::CommonMark),
            &Plan {
                out: Some(&out),
                ..Plan::default()
            },
        )
        .unwrap();

        assert_eq!(tree.failed(), 1, "{tree:?}");
        assert!(out.join("good.md").is_file());
        let broken = tree
            .items
            .iter()
            .find(|i| i.source.ends_with("broken.tmt"))
            .unwrap();
        assert!(broken.result.as_ref().unwrap_err().contains("error:"));
    }

    /// A file that was converted but could not be written was not converted:
    /// it stays counted as a failure, and its original is not removed.
    #[test]
    fn a_file_that_could_not_be_written_is_a_failure_and_keeps_its_original() {
        let dir = Dir::new("tree-write");
        dir.write("a.md", MD);
        dir.write("b.md", MD);
        // `a.tmt` is a directory, so writing the file there fails.
        fs::create_dir_all(dir.path("a.tmt")).unwrap();

        let tree = convert_tree(
            &dir.0,
            &from_md(),
            &Plan {
                in_place: true,
                remove_original: true,
                ..Plan::default()
            },
        )
        .unwrap();

        assert_eq!(tree.failed(), 1, "{tree:?}");
        assert!(dir.path("b.tmt").is_file(), "the others are still done");
        assert!(dir.path("a.md").is_file(), "not converted, so not removed");
        assert!(!dir.path("b.md").exists(), "converted, so removed");
    }

    #[test]
    fn a_directory_cannot_be_converted_from_pandoc() {
        let dir = Dir::new("tree-pandoc");
        let error = convert_tree(
            &dir.0,
            &Direction::From(Source::Pandoc),
            &Plan {
                dry_run: true,
                ..Plan::default()
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("not from pandoc"), "{error}");
    }

    #[test]
    fn a_dry_run_over_a_directory_writes_nothing() {
        let dir = Dir::new("tree-dry");
        dir.write("a.md", MD);

        let tree = convert_tree(
            &dir.0,
            &from_md(),
            &Plan {
                dry_run: true,
                ..Plan::default()
            },
        )
        .unwrap();
        assert_eq!(tree.succeeded(), 1);
        assert!(!dir.path("a.tmt").exists());
    }
}
