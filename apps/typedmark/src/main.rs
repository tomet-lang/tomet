//! A small CLI for manually checking that `typedmark-parser`/
//! `serde_typedmark` actually work against a real `.tm` file, instead of
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
use typedmark_semantics::{ExportType, document_config};

#[derive(Parser)]
#[command(name = "typedmark", about = "Parse and inspect TypedMark (.tm) files")]
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
    /// through `serde_typedmark` and reparse that -- confirms the
    /// save/load round trip is lossless.
    Roundtrip { file: PathBuf },
    /// Convert a `.tm` file to a standalone HTML page.
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
    /// Convert a `.tm` file to CommonMark. Lossy for constructs with no
    /// Markdown equivalent (`@links{}`, generic `<T>` elements) -- see
    /// `docs/commonmark-support.md`.
    ToMd {
        file: PathBuf,
        /// Write to this path instead of stdout.
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Serve a `.tm` file as HTML over HTTP on 127.0.0.1, re-rendering it
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

    /// Normalize a `.tm` file's whitespace (line endings, trailing
    /// whitespace, blank lines, final newline). Prints to stdout by
    /// default; see `--in-place`/`--check`.
    Format {
        file: PathBuf,
        /// Overwrite the file in place instead of printing to stdout.
        #[arg(short = 'i', long, alias = "write")]
        in_place: bool,
        /// Exit with a nonzero status if the file isn't already
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
        /// Optional path to custom formatting configuration file (.tm).
        #[arg(short, long)]
        config: Option<PathBuf>,
    },

    /// Export a `.tm` document or directory of documents based on `@config` settings or CLI overrides.
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
}

fn main() -> ExitCode {
    let cli = Cli::parse();
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
        Command::Serve {
            file,
            port,
            advanced,
            lang,
        } => serve(file, *port, *advanced, lang.clone()),
        Command::Format {
            file,
            in_place,
            check,
        } => format_cmd(file, *in_place, *check),
        Command::Tui { path, config } => typedmark_tui::run_tui(path.clone(), config.clone()),
        Command::Export {
            path,
            r#type,
            out,
            advanced,
        } => export_cmd(path, r#type.as_deref(), out.as_deref(), *advanced),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn format_parse_error(path: &Path, src: &str, err: &typedmark_parser::Error) -> String {
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
        typedmark_parser::parse_value(&src).map(|_| ())
    } else {
        typedmark_parser::parse_document(&src).map(|_| ())
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
        let value = typedmark_parser::parse_value(&src)
            .map_err(|e| anyhow::anyhow!("{}", format_parse_error(file, &src, &e)))?;
        println!("{value:#?}");
    } else {
        let doc = typedmark_parser::parse_document(&src)
            .map_err(|e| anyhow::anyhow!("{}", format_parse_error(file, &src, &e)))?;
        println!("{doc:#?}");
    }
    Ok(())
}

fn render_file(file: &Path, advanced: bool, lang: Option<String>) -> anyhow::Result<String> {
    let src = fs::read_to_string(file)?;
    let doc = typedmark_parser::parse_document(&src)
        .map_err(|e| anyhow::anyhow!("{}", format_parse_error(file, &src, &e)))?;
    let filename_title = file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("TypedMark");
    let title = meta_title(&doc).unwrap_or_else(|| filename_title.to_string());
    let options = typedmark_html::RenderOptions {
        number_headings: advanced,
        auto_slug_headings: advanced,
        lang,
    };
    Ok(typedmark_html::render_page_with(&doc, &title, &options))
}

/// Pulls a `title` string out of the document's `@meta` block, if it has
/// one -- `<title>`/`html`/`serve` prefer this over the filename when
/// present (see `.agents/tasks/ssg-readiness.md` step 1).
fn meta_title(doc: &typedmark_ast::Document) -> Option<String> {
    let meta = typedmark_semantics::document_meta(doc)?;
    let typedmark_ast::Value::Map(map) = meta else {
        return None;
    };
    map.iter().find_map(|(k, v)| {
        if k != "title" {
            return None;
        }
        match v {
            typedmark_ast::Value::String(s) => Some(s.clone()),
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
    let doc = typedmark_parser::parse_document(&src)
        .map_err(|e| anyhow::anyhow!("{}", format_parse_error(file, &src, &e)))?;
    let markdown = typedmark_markdown::to_markdown(&doc);
    match out {
        Some(path) => fs::write(path, markdown)?,
        None => println!("{markdown}"),
    }
    Ok(())
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

fn format_cmd(file: &PathBuf, write: bool, check: bool) -> anyhow::Result<()> {
    let src = read(file)?;
    let formatted = typedmark_formatter::format_source(&src);
    if check {
        if formatted == src {
            Ok(())
        } else {
            Err(anyhow::anyhow!("{} is not formatted", file.display()))
        }
    } else if write {
        if formatted != src {
            fs::write(file, formatted)?;
        }
        Ok(())
    } else {
        print!("{formatted}");
        Ok(())
    }
}

fn roundtrip(file: &PathBuf) -> anyhow::Result<()> {
    let src = read(file)?;

    // `serde_json::Value` stands in for "some arbitrary Serialize/
    // Deserialize type" here, since it can hold any shape without a
    // fixed struct -- a real caller (e.g. cettila) would use its own
    // `#[derive(Serialize, Deserialize)]` struct instead.
    let value: serde_json::Value = serde_typedmark::from_str(&src)?;
    println!("parsed:\n{}", serde_json::to_string_pretty(&value)?);

    let rendered = serde_typedmark::to_string(&value)?;
    println!("\nrendered back:\n{rendered}");

    let reparsed: serde_json::Value = serde_typedmark::from_str(&rendered)?;
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
    let doc = typedmark_parser::parse_document(&src)
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
            ExportType::CommonMark => typedmark_markdown::to_markdown(&doc),
            ExportType::Html => {
                let filename_title = file_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("TypedMark");
                let title = meta_title(&doc).unwrap_or_else(|| filename_title.to_string());
                let options = typedmark_html::RenderOptions {
                    number_headings: advanced,
                    auto_slug_headings: advanced,
                    lang: None,
                };
                typedmark_html::render_page_with(&doc, &title, &options)
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
    let files = typedmark_indexer::collect_tm_files(dir_path);
    if files.is_empty() {
        println!("No .tm or .tmt files found in {}", dir_path.display());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_parse_error_with_source_context() {
        let err = typedmark_parser::Error {
            message: "expected ']'".into(),
            line: 1,
            column: 11,
            offset: 10,
        };
        let src = "<caution>[ unterminated";
        let formatted = format_parse_error(Path::new("test.tm"), src, &err);
        assert!(formatted.contains("error: expected ']'"));
        assert!(formatted.contains("--> test.tm:1:11"));
        assert!(formatted.contains("<caution>[ unterminated"));
        assert!(formatted.contains("^ expected ']'"));
    }

    #[test]
    fn export_single_file_with_config_writes_output_file() {
        let temp_dir = std::env::temp_dir().join("typedmark_test_export");
        let _ = fs::create_dir_all(&temp_dir);
        let src_file = temp_dir.join("test_doc.tm");
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
}
