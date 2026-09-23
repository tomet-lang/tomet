use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "tomet", about = "Parse and inspect Tomet (.tmt) files")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Check a `.tmt` file, or every one under a directory, and report
    /// whether it is valid: it parses, and every element it writes exists
    /// in a namespace the document has in scope.
    ///
    /// A directory is swept the way `format --check` and `check-links`
    /// sweep one -- through the workspace index, so `workspace.ignore`
    /// applies and hidden files like `.writ.tmt` are included. One
    /// unreadable file does not stop the sweep; the run exits non-zero at
    /// the end.
    Check {
        /// File or directory to check (defaults to ".").
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Parse as a data-only document (key: value / seq / scalar)
        /// instead of the full document grammar. One file only -- a
        /// directory of data-only documents is not a thing this has met.
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
    /// through `tove` and reparse that -- confirms the
    /// save/load round trip is lossless.
    Roundtrip { file: PathBuf },
    /// Convert a `.tmt` file to HTML: a standalone page, or just the
    /// rendered body with `--body`.
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
        /// Emit just the rendered body, with no `<html>`/`<head>` shell,
        /// for templating into a page of your own. `--lang` has no effect
        /// alongside this, since the shell is what carries it.
        #[arg(long)]
        body: bool,
    },
    /// Convert a `.tmt` file to CommonMark. Lossy for constructs with no
    /// Markdown equivalent (`@links{}`, and any `@T` element the importer
    /// never produces); `tomet-markdown`'s module docs list what falls
    /// back to HTML passthrough.
    ToMd {
        file: PathBuf,
        /// Write to this path instead of stdout.
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Convert a `.tmt` file to Typst markup source. Lossy for constructs
    /// with no Typst equivalent -- see `tomet-convert-typst`'s crate doc.
    ToTypst {
        file: PathBuf,
        /// Write to this path instead of stdout.
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Convert a `.tmt` file to Pandoc's JSON AST, which `pandoc -f json`
    /// reads -- the bridge to every format Pandoc writes (docx, LaTeX,
    /// EPUB, org, ...). Pass `-` to read the `.tmt` from stdin.
    ToPandoc {
        file: PathBuf,
        /// Write to this path instead of stdout.
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Convert Pandoc's JSON AST (`pandoc -t json <file>`) to Tomet
    /// (.tmt) -- the bridge from every format Pandoc reads. Pass `-` to
    /// read the JSON from stdin, which is the usual way:
    /// `pandoc -t json x.docx | tomet from-pandoc -`.
    FromPandoc {
        /// Target `.json` file, or `-` for stdin.
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
        /// Target file(s) or directory/directories to format.
        #[arg(required = true)]
        paths: Vec<PathBuf>,
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
        /// Compare each declared output against what the source renders to
        /// now, without writing anything. Exits non-zero if any differ or
        /// are missing -- a generated file that disagrees with its source
        /// is a failure with a mechanical fix.
        #[arg(long)]
        check: bool,
    },

    /// Check every `@file`/`<embed>` link in a `.tmt`/`.tmt` document or
    /// directory for broken (non-existent) local-file targets. Uses an
    /// SQLite cache (keyed by source-file mtime) in the user's cache
    /// directory -- `~/.cache/tomet/` on Unix, never inside the vault --
    /// so re-checking a large vault doesn't re-parse unchanged files.
    /// Exits non-zero if any broken link is found.
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
    /// Create a new .tmt document from a blueprint.
    New {
        /// Target file path to create (defaults to "document.tmt" if omitted when --list is not used).
        #[arg(default_value = "document.tmt")]
        path: PathBuf,
        /// Blueprint name, as the blueprint gives it in `@blueprint(...)`
        /// (e.g. "daily-note", "rfc"). Resolved among the blueprints the
        /// vault declares in `blueprints`, never searched for on disk.
        #[arg(short, long)]
        blueprint: Option<String>,
        /// List the blueprints this vault declares instead of creating a file.
        #[arg(short, long)]
        list: bool,
        /// Overwrite target file if it already exists.
        #[arg(short = 'f', long)]
        force: bool,
        /// Pass custom variable as `key=value`. Can be specified multiple times.
        #[arg(long = "var", value_parser = parse_key_val)]
        vars: Vec<(String, String)>,
    },
}

fn parse_key_val(s: &str) -> Result<(String, String), String> {
    let pos = s
        .find('=')
        .ok_or_else(|| format!("invalid KEY=value: no `=` found in `{s}`"))?;
    Ok((s[..pos].to_string(), s[pos + 1..].to_string()))
}
