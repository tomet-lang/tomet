//! A small CLI for manually checking that `tomet-parser`/
//! `serde_tomet` actually work against a real `.tmt` file, instead of
//! only trusting `cargo test`'s unit tests.

use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use axum::Router;
use axum::extract::State;
use axum::response::Html;
use axum::routing::get;

use clap::{Parser, Subcommand};
use tomet_semantics::{ExportType, document_config};

#[derive(Parser)]
#[command(name = "tomet", about = "Parse and inspect Tomet (.tmt) files")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Parse a file and report whether it's valid (prints "OK" or a detailed
    /// snippet error with its line:column), without dumping the AST.
    Check {
        file: PathBuf,
        /// Parse as a data-only document (key: value / seq / scalar)
        /// instead of the full document grammar.
        #[arg(long)]
        data: bool,
        /// Do not print "OK" when parse succeeds.
        #[arg(short, long)]
        quiet: bool,
        /// Output parse diagnostic error as JSON format.
        #[arg(long)]
        json: bool,
    },
    /// Parse a file and pretty-print the resulting AST.
    Ast {
        file: PathBuf,
        #[arg(long)]
        data: bool,
    },
    /// Parse a data file into a generic value, then render it back out
    /// through `serde_tomet` and reparse that -- confirms the
    /// save/load round trip is lossless.
    Roundtrip { file: PathBuf },
    /// Convert a `.tmt` file to a standalone HTML page.
    Html {
        file: PathBuf,
        /// Write to this path instead of stdout.
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Number headings sequentially by nesting level (1, 1.1, 1.2, 2,
        /// ...) instead of the plain, unnumbered default.
        #[arg(long)]
        advanced: bool,
        /// `<html lang="...">` for the page shell. Defaults to "ja".
        #[arg(long)]
        lang: Option<String>,
    },
    /// Convert a `.tmt` file to CommonMark. Lossy for constructs with no
    /// Markdown equivalent (`@links{}`, generic `<T>` elements) -- see
    /// `docs/commonmark-support.md`.
    ToMd {
        file: PathBuf,
        /// Write to this path instead of stdout.
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Convert CommonMark Markdown (.md) to Tomet (.tmt).
    FromMd {
        /// Target .md file or directory containing .md files.
        path: PathBuf,
        /// Write to this path instead of stdout (for single file) or target output directory (for directory).
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Write converted `.tmt` file(s) in place.
        #[arg(short = 'i', long, alias = "write")]
        in_place: bool,
        /// Remove original `.md` file(s) after conversion (only with --in-place or --out).
        #[arg(long)]
        remove_original: bool,
        /// Dry-run mode: convert and validate in memory without writing files.
        #[arg(long)]
        dry_run: bool,
    },
    /// Serve a `.tmt` file as HTML over HTTP on 127.0.0.1, re-rendering it
    /// fresh on every request (just reload the page after editing).
    Serve {
        file: PathBuf,
        #[arg(short, long, default_value_t = 8787)]
        port: u16,
        /// Number headings sequentially by nesting level (1, 1.1, 1.2, 2,
        /// ...) instead of the plain, unnumbered default.
        #[arg(long)]
        advanced: bool,
        /// `<html lang="...">` for the page shell. Defaults to "ja".
        #[arg(long)]
        lang: Option<String>,
    },

    /// Normalize a `.tmt`/`.tmt` file's whitespace (line endings, trailing
    /// whitespace, blank lines, final newline). A directory formats every
    /// `.tmt`/`.tmt` file found under it (same file discovery as `export`/
    /// `check-links`: honors `.gitignore` and the project config's
    /// `ignore_files`). Prints to stdout by default; see
    /// `--in-place`/`--check`.
    Format {
        path: PathBuf,
        /// Overwrite the file(s) in place instead of printing to stdout.
        #[arg(short = 'i', long, alias = "write")]
        in_place: bool,
        /// Exit with a nonzero status if any file isn't already
        /// formatted, without writing or printing anything.
        #[arg(long, conflicts_with = "in_place")]
        check: bool,
    },

    /// Launch the interactive TUI workbench for Markdown migration,
    /// batch metadata editing, and structural AST refactoring.
    Tui {
        /// Target directory or file path (defaults to current directory ".").
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Optional path to custom formatting configuration file (.tmt).
        #[arg(short, long)]
        config: Option<PathBuf>,
    },

    /// Export a `.tmt` document or directory of documents based on `@config` settings or CLI overrides.
    Export {
        /// Target file or directory path (defaults to current directory ".").
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Target format (`commonmark` | `html` | `all`). Overrides @config.
        #[arg(short = 't', long)]
        r#type: Option<String>,
        /// Output destination file or directory path. Overrides @config.
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Number headings sequentially by nesting level for HTML export.
        #[arg(long)]
        advanced: bool,
    },

    /// Check every `@file`/`<embed>` link in a `.tmt`/`.tmt` document or
    /// directory for broken (non-existent) local-file targets. Uses an
    /// SQLite cache (keyed by source-file mtime) under
    /// `<config_root>/.tomet/` so re-checking a large vault doesn't
    /// re-parse unchanged files. Exits non-zero if any broken link is
    /// found.
    CheckLinks {
        /// Target file or directory path (defaults to current directory ".").
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Print the report as JSON instead of human-readable text.
        #[arg(long)]
        json: bool,
    },

    /// Refactor .tmt file(s) across a workspace (URL macros, @meta.type -> @kind, Value DSL normalization).
    Refactor {
        /// Target file or directory path (defaults to current directory ".").
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Write modified files in place.
        #[arg(short = 'i', long, alias = "write")]
        in_place: bool,
        /// Only rewrite URLs matching macros.
        #[arg(long)]
        url_macros: bool,
        /// Only promote @meta.type to @kind.
        #[arg(long)]
        meta_kind: bool,
        /// Only normalize @meta(format:yaml) to Value DSL.
        #[arg(long)]
        value_dsl: bool,
        /// Check/dry-run mode without modifying files (exits non-zero if changes are needed).
        #[arg(long, conflicts_with = "in_place")]
        check: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    if let Command::CheckLinks { path, json } = &cli.command {
        return match check_links_cmd(path, *json) {
            Ok(true) => ExitCode::FAILURE, // ran fine, but found broken links
            Ok(false) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        };
    }

    let result = match &cli.command {
        Command::Check {
            file,
            data,
            quiet,
            json,
        } => check(file, *data, *quiet, *json),
        Command::Ast { file, data } => ast(file, *data),
        Command::Roundtrip { file } => roundtrip(file),
        Command::Html {
            file,
            out,
            advanced,
            lang,
        } => html(file, out, *advanced, lang.clone()),
        Command::ToMd { file, out } => to_md(file, out),
        Command::FromMd {
            path,
            out,
            in_place,
            remove_original,
            dry_run,
        } => from_md_cmd(
            path,
            out.as_deref(),
            *in_place,
            *remove_original,
            *dry_run,
        ),
        Command::Serve {
            file,
            port,
            advanced,
            lang,
        } => serve(file, *port, *advanced, lang.clone()),
        Command::Format {
            path,
            in_place,
            check,
        } => format_cmd(path, *in_place, *check),
        Command::Tui { path, config } => tomet_tui::run_tui(path.clone(), config.clone()),
        Command::Export {
            path,
            r#type,
            out,
            advanced,
        } => export_cmd(path, r#type.as_deref(), out.as_deref(), *advanced),
        Command::Refactor {
            path,
            in_place,
            url_macros,
            meta_kind,
            value_dsl,
            check,
        } => refactor_cmd(
            path,
            *in_place,
            *url_macros,
            *meta_kind,
            *value_dsl,
            *check,
        ),
        Command::CheckLinks { .. } => unreachable!("handled above, before this match"),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn format_parse_error(path: &Path, src: &str, err: &tomet_parser::Error) -> String {
    let line_num = err.line;
    let col_num = err.column;

    let line_text = src.lines().nth(line_num.saturating_sub(1)).unwrap_or("");

    let indent = " ".repeat(col_num.saturating_sub(1));
    let line_str = line_num.to_string();
    let padding = " ".repeat(line_str.len());

    format!(
        "error: {}\n  --> {}:{}:{}\n   {}\n {} | {}\n   {}| {}^ {}",
        err.message,
        path.display(),
        line_num,
        col_num,
        "|",
        line_str,
        line_text,
        padding,
        indent,
        err.message
    )
}

fn read(file: &PathBuf) -> anyhow::Result<String> {
    Ok(fs::read_to_string(file)?)
}

fn check(file: &PathBuf, data: bool, quiet: bool, json: bool) -> anyhow::Result<()> {
    let src = read(file)?;
    let res = if data {
        tomet_parser::parse_value(&src).map(|_| ())
    } else {
        tomet_parser::parse_document(&src).map(|_| ())
    };

    match res {
        Ok(()) => {
            if !quiet {
                println!("OK");
            }
            Ok(())
        }
        Err(err) => {
            if json {
                let json_err = serde_json::json!({
                    "file": file.display().to_string(),
                    "error": {
                        "message": err.message,
                        "line": err.line,
                        "column": err.column,
                        "offset": err.offset,
                    }
                });
                println!("{}", serde_json::to_string_pretty(&json_err)?);
            } else {
                eprintln!("{}", format_parse_error(file, &src, &err));
            }
            Err(anyhow::anyhow!("check failed"))
        }
    }
}

fn ast(file: &PathBuf, data: bool) -> anyhow::Result<()> {
    let src = read(file)?;
    if data {
        let value = tomet_parser::parse_value(&src)
            .map_err(|e| anyhow::anyhow!("{}", format_parse_error(file, &src, &e)))?;
        println!("{value:#?}");
    } else {
        let doc = tomet_parser::parse_document(&src)
            .map_err(|e| anyhow::anyhow!("{}", format_parse_error(file, &src, &e)))?;
        println!("{doc:#?}");
    }
    Ok(())
}

fn render_file(file: &Path, advanced: bool, lang: Option<String>) -> anyhow::Result<String> {
    let src = fs::read_to_string(file)?;
    let doc = tomet_parser::parse_document(&src)
        .map_err(|e| anyhow::anyhow!("{}", format_parse_error(file, &src, &e)))?;
    let filename_title = file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Tomet");
    let title = meta_title(&doc).unwrap_or_else(|| filename_title.to_string());
    let options = tomet_html::RenderOptions {
        number_headings: advanced,
        auto_slug_headings: advanced,
        lang,
    };
    Ok(tomet_html::render_page_with(&doc, &title, &options))
}

/// Pulls a `title` string out of the document's `@meta` block, if it has
/// one -- `<title>`/`html`/`serve` prefer this over the filename when
/// present (see `.agents/tasks/ssg-readiness.md` step 1).
fn meta_title(doc: &tomet_ast::Document) -> Option<String> {
    let meta = tomet_semantics::document_meta(doc)?;
    let tomet_ast::Value::Map(map) = meta else {
        return None;
    };
    map.iter().find_map(|(k, v)| {
        if k != "title" {
            return None;
        }
        match v {
            tomet_ast::Value::String(s) => Some(s.clone()),
            _ => None,
        }
    })
}

fn html(
    file: &PathBuf,
    out: &Option<PathBuf>,
    advanced: bool,
    lang: Option<String>,
) -> anyhow::Result<()> {
    let page = render_file(file, advanced, lang)?;
    match out {
        Some(path) => fs::write(path, page)?,
        None => println!("{page}"),
    }
    Ok(())
}

fn to_md(file: &PathBuf, out: &Option<PathBuf>) -> anyhow::Result<()> {
    let src = read(file)?;
    let doc = tomet_parser::parse_document(&src)
        .map_err(|e| anyhow::anyhow!("{}", format_parse_error(file, &src, &e)))?;
    let markdown = tomet_markdown::to_markdown(&doc);
    match out {
        Some(path) => fs::write(path, markdown)?,
        None => println!("{markdown}"),
    }
    Ok(())
}

fn from_md_cmd(
    path: &Path,
    out: Option<&Path>,
    in_place: bool,
    remove_original: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    if path.is_file() {
        from_md_single_file(path, out, in_place, remove_original, dry_run)
    } else if path.is_dir() {
        from_md_directory(path, out, in_place, remove_original, dry_run)
    } else {
        Err(anyhow::anyhow!("path '{}' does not exist", path.display()))
    }
}

fn convert_md_source(path: &Path, src: &str) -> anyhow::Result<String> {
    let (config, _, _) = tomet_config::find_config_file(path).unwrap_or((
        tomet_config::PrinterConfig::default(),
        path.to_path_buf(),
        path.to_path_buf(),
    ));
    let import_opts = tomet_markdown::ImportOptions {
        adjust_table_width: config.table_adjust_width.as_deref() == Some("true")
            || config.table_adjust_width.as_deref() == Some("auto"),
        table_adjust_width_mode: config.table_adjust_width.clone(),
        table_max_col_width: config.table_max_col_width,
        table_align: config.table_align.clone(),
    };
    let mut doc = tomet_markdown::from_markdown_with_options(src, &import_opts);
    tomet_printer::ensure_document_id_with_config(&mut doc, &config);
    let tmt = tomet_printer::document_to_tm_with_config(&doc, &config);
    if let Err(e) = tomet_parser::parse_document(&tmt) {
        return Err(anyhow::anyhow!(
            "generated .tmt has parse error: {}",
            format_parse_error(path, &tmt, &e)
        ));
    }
    Ok(tmt)
}

fn from_md_single_file(
    file_path: &Path,
    out: Option<&Path>,
    in_place: bool,
    remove_original: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let src = fs::read_to_string(file_path)?;
    let tmt = convert_md_source(file_path, &src)?;

    if dry_run {
        println!("OK (dry-run): {}", file_path.display());
        return Ok(());
    }

    if let Some(out_path) = out {
        let dest = if out_path.is_dir() {
            let stem = file_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("output");
            out_path.join(format!("{stem}.tmt"))
        } else {
            out_path.to_path_buf()
        };
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&dest, &tmt)?;
        if remove_original && file_path != dest {
            let _ = fs::remove_file(file_path);
        }
        println!("Converted {} -> {}", file_path.display(), dest.display());
    } else if in_place {
        let dest = file_path.with_extension("tmt");
        fs::write(&dest, &tmt)?;
        if remove_original && file_path != dest {
            let _ = fs::remove_file(file_path);
        }
        println!("Converted {} -> {}", file_path.display(), dest.display());
    } else {
        print!("{tmt}");
    }
    Ok(())
}

fn from_md_directory(
    dir_path: &Path,
    out: Option<&Path>,
    in_place: bool,
    remove_original: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let (config, _, config_root) = tomet_config::find_config_file(dir_path).unwrap_or((
        tomet_config::PrinterConfig::default(),
        dir_path.to_path_buf(),
        dir_path.to_path_buf(),
    ));

    let index = tomet_indexer::workspace_scan::WorkspaceIndex::build(dir_path, &config, &config_root);
    let candidates = index.migration_candidates();

    if candidates.is_empty() {
        println!("No .md files found in {}", dir_path.display());
        return Ok(());
    }

    let mut succeeded = 0;
    let mut failed = 0;
    let mut error_reports = Vec::new();

    for candidate in &candidates {
        let src = match fs::read_to_string(&candidate.source_path) {
            Ok(s) => s,
            Err(e) => {
                failed += 1;
                error_reports.push(format!("{}: read error: {e}", candidate.source_path.display()));
                continue;
            }
        };

        match convert_md_source(&candidate.source_path, &src) {
            Ok(tmt) => {
                succeeded += 1;
                if !dry_run {
                    if let Some(out_dir) = out {
                        let rel = candidate
                            .source_path
                            .strip_prefix(dir_path)
                            .unwrap_or(&candidate.source_path);
                        let dest = out_dir.join(rel).with_extension("tmt");
                        if let Some(parent) = dest.parent() {
                            let _ = fs::create_dir_all(parent);
                        }
                        if let Err(e) = fs::write(&dest, &tmt) {
                            eprintln!("Error writing {}: {e}", dest.display());
                        } else if remove_original && candidate.source_path != dest {
                            let _ = fs::remove_file(&candidate.source_path);
                        }
                    } else if in_place {
                        let dest = &candidate.target_path;
                        if let Err(e) = fs::write(dest, &tmt) {
                            eprintln!("Error writing {}: {e}", dest.display());
                        } else if remove_original && candidate.source_path != *dest {
                            let _ = fs::remove_file(&candidate.source_path);
                        }
                    }
                }
            }
            Err(e) => {
                failed += 1;
                error_reports.push(format!("{}: {e}", candidate.source_path.display()));
            }
        }
    }

    if dry_run {
        println!(
            "Dry-run finished: {} total, {} succeeded, {} failed",
            candidates.len(),
            succeeded,
            failed
        );
    } else {
        println!(
            "Conversion finished: {} total, {} succeeded, {} failed",
            candidates.len(),
            succeeded,
            failed
        );
    }

    if !error_reports.is_empty() {
        eprintln!("\nErrors encountered ({}):", error_reports.len());
        for err in &error_reports {
            eprintln!("  {err}");
        }
    }

    if failed > 0 {
        Err(anyhow::anyhow!("{failed} file(s) failed conversion"))
    } else {
        Ok(())
    }
}

#[derive(Clone)]
struct ServeState {
    file: PathBuf,
    advanced: bool,
    lang: Option<String>,
}

fn serve(file: &PathBuf, port: u16, advanced: bool, lang: Option<String>) -> anyhow::Result<()> {
    let state = ServeState {
        file: file.clone(),
        advanced,
        lang,
    };
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let app = Router::new()
            .route("/", get(render_handler))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind(addr).await?;
        println!("Serving at http://{addr} (Ctrl+C to stop)");
        axum::serve(listener, app).await?;
        Ok(())
    })
}

async fn render_handler(State(state): State<ServeState>) -> Html<String> {
    Html(
        render_file(&state.file, state.advanced, state.lang.clone())
            .unwrap_or_else(|e| error_page(&state.file, &e)),
    )
}

fn error_page(file: &Path, err: &anyhow::Error) -> String {
    format!(
        "<!DOCTYPE html>\n<html lang=\"ja\">\n<head><meta charset=\"utf-8\"><title>error</title></head>\n<body>\n<h1>Parse error</h1>\n<p>{}</p>\n<pre>{}</pre>\n</body>\n</html>\n",
        escape_html(&file.display().to_string()),
        escape_html(&err.to_string()),
    )
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn format_cmd(path: &Path, write: bool, check: bool) -> anyhow::Result<()> {
    if path.is_file() {
        let src = fs::read_to_string(path)?;
        let formatted = tomet_formatter::format_source(&src);
        return if check {
            if formatted == src {
                Ok(())
            } else {
                Err(anyhow::anyhow!("{} is not formatted", path.display()))
            }
        } else if write {
            if formatted != src {
                fs::write(path, formatted)?;
            }
            Ok(())
        } else {
            print!("{formatted}");
            Ok(())
        };
    }
    if !path.is_dir() {
        return Err(anyhow::anyhow!("path '{}' does not exist", path.display()));
    }

    let files = tomet_indexer::collect_tm_files(path);
    if files.is_empty() {
        println!("No .tmt or .tmt files found in {}", path.display());
        return Ok(());
    }

    if check {
        let mut unformatted = Vec::new();
        for file in &files {
            let src = fs::read_to_string(file)?;
            if tomet_formatter::format_source(&src) != src {
                unformatted.push(file);
            }
        }
        if unformatted.is_empty() {
            return Ok(());
        }
        for file in &unformatted {
            println!("{}", file.display());
        }
        return Err(anyhow::anyhow!(
            "{} of {} file(s) not formatted",
            unformatted.len(),
            files.len()
        ));
    }

    if write {
        let mut changed = 0;
        for file in &files {
            match fs::read_to_string(file) {
                Ok(src) => {
                    let formatted = tomet_formatter::format_source(&src);
                    if formatted == src {
                        continue;
                    }
                    match fs::write(file, formatted) {
                        Ok(()) => {
                            println!("{}", file.display());
                            changed += 1;
                        }
                        Err(e) => eprintln!("Error writing {}: {e}", file.display()),
                    }
                }
                Err(e) => eprintln!("Error reading {}: {e}", file.display()),
            }
        }
        println!("Formatted {changed} of {} file(s).", files.len());
        return Ok(());
    }

    // Default (no --in-place/--check): print every file's formatted
    // content to stdout, headed like `head`/`tail` do for multiple
    // files -- keeps a directory's worth of output distinguishable,
    // same as single-file mode (no header) staying exactly as before.
    for file in &files {
        let src = fs::read_to_string(file)?;
        let formatted = tomet_formatter::format_source(&src);
        println!("==> {} <==", file.display());
        print!("{formatted}");
    }
    Ok(())
}

fn roundtrip(file: &PathBuf) -> anyhow::Result<()> {
    let src = read(file)?;

    // `serde_json::Value` stands in for "some arbitrary Serialize/
    // Deserialize type" here, since it can hold any shape without a
    // fixed struct -- a real caller (e.g. cettila) would use its own
    // `#[derive(Serialize, Deserialize)]` struct instead.
    let value: serde_json::Value = serde_tomet::from_str(&src)?;
    println!("parsed:\n{}", serde_json::to_string_pretty(&value)?);

    let rendered = serde_tomet::to_string(&value)?;
    println!("\nrendered back:\n{rendered}");

    let reparsed: serde_json::Value = serde_tomet::from_str(&rendered)?;
    if reparsed == value {
        println!("\nround-trip OK");
        Ok(())
    } else {
        println!("\nround-trip MISMATCH");
        Err(anyhow::anyhow!(
            "re-parsing the rendered output produced a different value"
        ))
    }
}

fn export_cmd(
    target_path: &Path,
    override_type: Option<&str>,
    override_out: Option<&Path>,
    advanced: bool,
) -> anyhow::Result<()> {
    if target_path.is_file() {
        export_single_file(target_path, override_type, override_out, advanced)
    } else if target_path.is_dir() {
        export_directory(target_path, override_type, override_out, advanced)
    } else {
        Err(anyhow::anyhow!(
            "path '{}' does not exist",
            target_path.display()
        ))
    }
}

fn export_single_file(
    file_path: &Path,
    override_type: Option<&str>,
    override_out: Option<&Path>,
    advanced: bool,
) -> anyhow::Result<()> {
    let src = fs::read_to_string(file_path)?;
    let doc = tomet_parser::parse_document(&src)
        .map_err(|e| anyhow::anyhow!("{}", format_parse_error(file_path, &src, &e)))?;

    let config = document_config(&doc);

    // Determine target formats to export
    let targets = if let Some(t_str) = override_type {
        if t_str.eq_ignore_ascii_case("all") {
            if !config.export_type.is_empty() {
                config.export_type.clone()
            } else {
                vec![ExportType::CommonMark, ExportType::Html]
            }
        } else {
            vec![ExportType::parse(t_str)]
        }
    } else if !config.export_type.is_empty() {
        config.export_type.clone()
    } else {
        vec![ExportType::CommonMark]
    };

    for target in &targets {
        let ext = match target {
            ExportType::CommonMark => "md",
            ExportType::Html => "html",
            ExportType::Custom(s) => s.as_str(),
        };

        let out_path = if let Some(out) = override_out {
            if out.is_dir() || (override_out.is_some() && (targets.len() > 1 || file_path.is_dir()))
            {
                let stem = file_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("output");
                Some(out.join(format!("{stem}.{ext}")))
            } else {
                Some(out.to_path_buf())
            }
        } else if let Some(cfg_path) = config.export_path_for(target) {
            Some(PathBuf::from(cfg_path))
        } else if targets.len() == 1 {
            None
        } else {
            let stem = file_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("output");
            Some(PathBuf::from(format!("{stem}.{ext}")))
        };

        let rendered = match target {
            ExportType::CommonMark => tomet_markdown::to_markdown(&doc),
            ExportType::Html => {
                let filename_title = file_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Tomet");
                let title = meta_title(&doc).unwrap_or_else(|| filename_title.to_string());
                let options = tomet_html::RenderOptions {
                    number_headings: advanced,
                    auto_slug_headings: advanced,
                    lang: None,
                };
                tomet_html::render_page_with(&doc, &title, &options)
            }
            ExportType::Custom(fmt) => {
                return Err(anyhow::anyhow!("unsupported export type: {fmt}"));
            }
        };

        if let Some(dest) = out_path {
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&dest, &rendered)?;
            println!("Exported {} -> {}", file_path.display(), dest.display());
        } else {
            print!("{rendered}");
        }
    }

    Ok(())
}

fn export_directory(
    dir_path: &Path,
    override_type: Option<&str>,
    override_out: Option<&Path>,
    advanced: bool,
) -> anyhow::Result<()> {
    let files = tomet_indexer::collect_tm_files(dir_path);
    if files.is_empty() {
        println!("No .tmt or .tmt files found in {}", dir_path.display());
        return Ok(());
    }

    let mut exported_count = 0;
    for file in &files {
        let relative_out = if let Some(out_dir) = override_out {
            if let Ok(rel) = file.strip_prefix(dir_path) {
                let parent = rel.parent().unwrap_or_else(|| Path::new(""));
                Some(out_dir.join(parent))
            } else {
                Some(out_dir.to_path_buf())
            }
        } else {
            None
        };

        match export_single_file(file, override_type, relative_out.as_deref(), advanced) {
            Ok(()) => exported_count += 1,
            Err(e) => eprintln!("Error exporting {}: {e}", file.display()),
        }
    }

    println!("Batch export finished: {exported_count} file(s) processed.");
    Ok(())
}

/// Runs the broken-link check and prints its report. Returns `Ok(true)`
/// if any broken link was found (distinct from `Err`, which means the
/// check itself couldn't run) so `main` can exit non-zero without
/// treating "found breakage" the same as "something went wrong".
fn check_links_cmd(target_path: &Path, json: bool) -> anyhow::Result<bool> {
    if !target_path.exists() {
        return Err(anyhow::anyhow!(
            "path '{}' does not exist",
            target_path.display()
        ));
    }

    let (config, _, config_root) = tomet_config::find_config_file(target_path).unwrap_or((
        tomet_config::PrinterConfig::default(),
        target_path.to_path_buf(),
        target_path.to_path_buf(),
    ));

    let cache_path = tomet_links::default_cache_path(&config_root);
    let mut cache = tomet_links::LinkCache::open(&cache_path)?;

    let report =
        tomet_links::check_vault(target_path, &config, &config_root, &config_root, &mut cache);

    if json {
        println!("{}", check_links_report_to_json(&report));
    } else {
        println!(
            "{} file(s) scanned, {} link(s) checked, {} broken",
            report.files_scanned,
            report.links_checked,
            report.broken.len(),
        );
        for link in &report.broken {
            println!(
                "  {}:{}:{}: broken link -> {}",
                link.source.display(),
                link.span.start.line,
                link.span.start.column,
                link.target
            );
        }
        for (file, err) in &report.errors {
            eprintln!("  {}: {err}", file.display());
        }
    }

    Ok(!report.broken.is_empty())
}

fn check_links_report_to_json(report: &tomet_links::CheckReport) -> String {
    let broken: Vec<serde_json::Value> = report
        .broken
        .iter()
        .map(|l| {
            serde_json::json!({
                "source": l.source.display().to_string(),
                "target": l.target,
                "line": l.span.start.line,
                "column": l.span.start.column,
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

fn refactor_cmd(
    path: &Path,
    in_place: bool,
    url_macros: bool,
    meta_kind: bool,
    value_dsl: bool,
    check: bool,
) -> anyhow::Result<()> {
    let options = if !url_macros && !meta_kind && !value_dsl {
        tomet_workspace::RefactorOptions::default()
    } else {
        tomet_workspace::RefactorOptions {
            url_to_macros: url_macros,
            meta_type_to_kind: meta_kind,
            meta_to_value_dsl: value_dsl,
        }
    };

    let mut results = tomet_workspace::refactor_workspace(path, &options)?;
    let changed_files: Vec<_> = results.iter().filter(|r| r.is_changed()).collect();

    if check {
        if changed_files.is_empty() {
            println!("All {} file(s) are up to date.", results.len());
            return Ok(());
        }
        for f in &changed_files {
            println!(
                "Needs refactor ({} change(s)): {}",
                f.changes_count,
                f.path.display()
            );
        }
        return Err(anyhow::anyhow!(
            "{} of {} file(s) need refactoring",
            changed_files.len(),
            results.len()
        ));
    }

    if in_place {
        let saved = tomet_workspace::save_file_diffs(&mut results)?;
        println!("Refactored {} of {} file(s).", saved, results.len());
        return Ok(());
    }

    if path.is_file() {
        if let Some(res) = results.first() {
            print!("{}", res.modified_src);
        }
    } else {
        for res in &changed_files {
            println!(
                "==> {} ({} changes) <==",
                res.path.display(),
                res.changes_count
            );
            print!("{}", res.modified_src);
        }
        println!(
            "\nDry-run: {} of {} file(s) would be modified.",
            changed_files.len(),
            results.len()
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_parse_error_with_source_context() {
        let err = tomet_parser::Error {
            message: "expected ']'".into(),
            line: 1,
            column: 11,
            offset: 10,
        };
        let src = "<caution>[ unterminated";
        let formatted = format_parse_error(Path::new("test.tmt"), src, &err);
        assert!(formatted.contains("error: expected ']'"));
        assert!(formatted.contains("--> test.tmt:1:11"));
        assert!(formatted.contains("<caution>[ unterminated"));
        assert!(formatted.contains("^ expected ']'"));
    }

    #[test]
    fn export_single_file_with_config_writes_output_file() {
        let temp_dir = std::env::temp_dir().join("tomet_test_export");
        let _ = fs::create_dir_all(&temp_dir);
        let src_file = temp_dir.join("test_doc.tmt");
        let out_file = temp_dir.join("test_out.md");

        let src_content = format!(
            "@config(\n  export: {{\n    type: commonmark\n    path: \"{}\"\n  }}\n)\n#[ Hello Export ]\n",
            out_file.display()
        );
        fs::write(&src_file, src_content).unwrap();

        let res = export_cmd(&src_file, None, None, false);
        assert!(res.is_ok());
        assert!(out_file.exists());
        let exported_text = fs::read_to_string(&out_file).unwrap();
        assert!(exported_text.contains("# Hello Export"));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn from_md_single_file_converts_markdown() {
        let temp_dir = std::env::temp_dir().join("tomet_test_from_md");
        let _ = fs::create_dir_all(&temp_dir);
        let src_file = temp_dir.join("test_doc.md");
        let out_file = temp_dir.join("test_doc.tmt");

        let md_content = "---\naliases:\n  - Hello\n---\n\n# Title\n\n- [[Other Note]]\n- bare text\n";
        fs::write(&src_file, md_content).unwrap();

        let res = from_md_cmd(&src_file, Some(&out_file), false, false, false);
        assert!(res.is_ok());
        assert!(out_file.exists());
        let tmt_text = fs::read_to_string(&out_file).unwrap();
        assert!(tmt_text.contains("#[Title]"));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn refactor_cmd_in_place_transforms_document() {
        let temp_dir = std::env::temp_dir().join("tomet_test_refactor_cmd");
        let _ = fs::create_dir_all(&temp_dir);
        let src_file = temp_dir.join("doc.tmt");

        let src_content = r#"@version(1.0)
@meta(format:yaml){
  type: note
  title: My Title
}
@config{
  macros: {
    gh: "https://github.com/${1}"
  }
}

- @link("https://github.com/cettila-projects/tomet")[Repo]
"#;
        fs::write(&src_file, src_content).unwrap();

        let res = refactor_cmd(&src_file, true, false, false, false, false);
        assert!(res.is_ok());

        let refactored = fs::read_to_string(&src_file).unwrap();
        assert!(refactored.contains("@kind(note)"));
        assert!(refactored.contains("$gh(\"cettila-projects/tomet\")"));
        assert!(!refactored.contains("format:yaml"));

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
