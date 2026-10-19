//! Command-line interface and developer tools for the Tomet toolchain.
//!
//! =[ Overview ]
//!
//! Provides the `tomet` command-line executable, orchestrating parser, resolver,
//! validator, formatter, and converter components for `.tmt` document processing.
//!
//! =[ Subcommands ]
//!
//! - (Inspection & Validation) `check`, `ast`, `roundtrip`, and `stats`.
//! - (Transformation & Formatting) `format` and `refactor`.
//! - (Converters) `html`, `to-md`, `from-md`, `to-typst`, `to-pandoc`, `from-pandoc`, and `export`.
//! - (Authoring) `new`.
//! - (Documentation) `cli-docs`.
//!
//! The project-level commands -- `check-links`, `serve` and `tui` -- are
//! `sisho`'s: they follow a vault's declarations, caches and screens rather
//! than a document alone, as `cargo` does for the language `rustc` compiles.
//! `sisho tomet <command>` reaches the commands above from there.

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod cli;
mod commands;
mod gate;
mod util;

use std::process::ExitCode;

use clap::Parser;

use cli::{Cli, Command};
use commands::ast::ast;
use commands::check::check;
use commands::cli_docs::cli_docs_cmd;
use commands::export::export_cmd;
use commands::format::format_cmd;
use commands::from_md::from_md_cmd;
use commands::from_pandoc::from_pandoc;
use commands::html::html;
use commands::refactor::refactor_cmd;
use commands::roundtrip::roundtrip;
use commands::stats::stats_cmd;
use commands::to_md::to_md;
use commands::to_pandoc::to_pandoc;
use commands::to_typst::to_typst;

fn main() -> ExitCode {
    gate::init_from_env();

    let cli = Cli::parse();

    let result = match &cli.command {
        Command::Check {
            path,
            data,
            quiet,
            json,
        } => check(path, *data, *quiet, *json),
        Command::Ast { file, data } => ast(file, *data),
        Command::Roundtrip { file } => roundtrip(file),
        Command::Html {
            file,
            out,
            advanced,
            lang,
            body,
        } => html(file, out, *advanced, lang.clone(), *body),
        Command::ToMd { file, out } => to_md(file, out),
        Command::ToTypst { file, out } => to_typst(file, out),
        Command::ToPandoc { file, out } => to_pandoc(file, out),
        Command::FromPandoc { file, out } => from_pandoc(file, out),
        Command::FromMd {
            path,
            out,
            in_place,
            remove_original,
            dry_run,
        } => from_md_cmd(path, out.as_deref(), *in_place, *remove_original, *dry_run),
        Command::Format {
            paths,
            in_place,
            check,
            force,
        } => format_cmd(paths, *in_place, *check, *force),
        Command::CliDocs { out, check } => cli_docs_cmd(out.as_deref(), *check),
        Command::Export {
            path,
            r#type,
            out,
            advanced,
            check,
        } => export_cmd(path, r#type.as_deref(), out.as_deref(), *advanced, *check),
        Command::Refactor {
            path,
            in_place,
            url_macros,
            meta_kind,
            check,
            force,
        } => refactor_cmd(path, *in_place, *url_macros, *meta_kind, *check, *force),
        Command::New {
            path,
            blueprint,
            list,
            force,
            vars,
        } => commands::new::new_cmd(path, blueprint.as_deref(), *list, *force, vars),
        Command::Stats { path, json } => stats_cmd(path, *json),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
