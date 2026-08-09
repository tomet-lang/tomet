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

#[derive(Parser)]
#[command(name = "typedmark", about = "Parse and inspect TypedMark (.tm) files")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Parse a file and report whether it's valid (prints "OK" or an error
    /// with its line:column), without dumping the AST.
    Check {
        file: PathBuf,
        /// Parse as a data-only document (key: value / seq / scalar)
        /// instead of the full document grammar.
        #[arg(long)]
        data: bool,
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
    },
    /// Normalize a `.tm` file's whitespace (line endings, trailing
    /// whitespace, blank lines, final newline). Prints to stdout by
    /// default; see `--write`/`--check`.
    Format {
        file: PathBuf,
        /// Overwrite the file in place instead of printing to stdout.
        #[arg(short, long)]
        write: bool,
        /// Exit with a nonzero status if the file isn't already
        /// formatted, without writing or printing anything.
        #[arg(long, conflicts_with = "write")]
        check: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match &cli.command {
        Command::Check { file, data } => check(file, *data),
        Command::Ast { file, data } => ast(file, *data),
        Command::Roundtrip { file } => roundtrip(file),
        Command::Html { file, out } => html(file, out),
        Command::ToMd { file, out } => to_md(file, out),
        Command::Serve { file, port } => serve(file, *port),
        Command::Format { file, write, check } => format_cmd(file, *write, *check),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn read(file: &PathBuf) -> anyhow::Result<String> {
    Ok(fs::read_to_string(file)?)
}

fn check(file: &PathBuf, data: bool) -> anyhow::Result<()> {
    let src = read(file)?;
    if data {
        typedmark_parser::parse_value(&src)?;
    } else {
        typedmark_parser::parse_document(&src)?;
    }
    println!("OK");
    Ok(())
}

fn ast(file: &PathBuf, data: bool) -> anyhow::Result<()> {
    let src = read(file)?;
    if data {
        let value = typedmark_parser::parse_value(&src)?;
        println!("{value:#?}");
    } else {
        let doc = typedmark_parser::parse_document(&src)?;
        println!("{doc:#?}");
    }
    Ok(())
}

fn render_file(file: &Path) -> anyhow::Result<String> {
    let src = fs::read_to_string(file)?;
    let doc = typedmark_parser::parse_document(&src)?;
    let title = file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("TypedMark");
    Ok(typedmark_renderer::render_page(&doc, title))
}

fn html(file: &PathBuf, out: &Option<PathBuf>) -> anyhow::Result<()> {
    let page = render_file(file)?;
    match out {
        Some(path) => fs::write(path, page)?,
        None => println!("{page}"),
    }
    Ok(())
}

fn to_md(file: &PathBuf, out: &Option<PathBuf>) -> anyhow::Result<()> {
    let src = read(file)?;
    let doc = typedmark_parser::parse_document(&src)?;
    let markdown = typedmark_markdown::to_markdown(&doc);
    match out {
        Some(path) => fs::write(path, markdown)?,
        None => println!("{markdown}"),
    }
    Ok(())
}

fn serve(file: &PathBuf, port: u16) -> anyhow::Result<()> {
    let file = file.clone();
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let app = Router::new()
            .route("/", get(render_handler))
            .with_state(file);
        let listener = tokio::net::TcpListener::bind(addr).await?;
        println!("Serving at http://{addr} (Ctrl+C to stop)");
        axum::serve(listener, app).await?;
        Ok(())
    })
}

async fn render_handler(State(file): State<PathBuf>) -> Html<String> {
    Html(render_file(&file).unwrap_or_else(|e| error_page(&file, &e)))
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
    // fixed struct -- a real caller (e.g. tomet) would use its own
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
