//! Command-line interface and developer tools for the Tomet toolchain.
//!
//! =[ Overview ]
//!
//! Provides the `tomet` command-line executable, orchestrating parser, resolver,
//! validator, formatter, and converter components for `.tmt` document processing.
//!
//! =[ Subcommands ]
//!
//! - @strong[Inspection & Validation] : `check`, `check-links`, `ast`, `roundtrip`, and `stats`.
//! - @strong[Transformation & Formatting] : `format` and `refactor`.
//! - @strong[Converters] : `html`, `to-md`, `from-md`, `to-typst`, `to-pandoc`, `from-pandoc`, and `export`.
//! - @strong[Documentation & Ecosystem] : `api`, `cli-docs`, `tui`, and `serve`.

mod cli;
mod commands;
mod util;

use std::process::ExitCode;

use clap::Parser;

use cli::{Cli, Command};
use commands::api::api_cmd;
use commands::ast::ast;
use commands::check::check;
use commands::check_links::check_links_cmd;
use commands::cli_docs::cli_docs_cmd;
use commands::export::export_cmd;
use commands::format::format_cmd;
use commands::from_md::from_md_cmd;
use commands::from_pandoc::from_pandoc;
use commands::html::html;
use commands::refactor::refactor_cmd;
use commands::roundtrip::roundtrip;
use commands::serve::serve;
use commands::stats::stats_cmd;
use commands::to_md::to_md;
use commands::to_pandoc::to_pandoc;
use commands::to_typst::to_typst;

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
        Command::Serve {
            file,
            port,
            advanced,
            lang,
        } => serve(file, *port, *advanced, lang.clone()),
        Command::Format {
            paths,
            in_place,
            check,
            force,
        } => format_cmd(paths, *in_place, *check, *force),
        Command::CliDocs { out, check } => cli_docs_cmd(out.as_deref(), *check),
        Command::Tui { path, config } => tomet_tui::run_tui(path.clone(), config.clone()),
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
            value_dsl,
            check,
            force,
        } => refactor_cmd(
            path,
            *in_place,
            *url_macros,
            *meta_kind,
            *value_dsl,
            *check,
            *force,
        ),
        Command::New {
            path,
            blueprint,
            list,
            force,
            vars,
        } => commands::new::new_cmd(path, blueprint.as_deref(), *list, *force, vars),
        Command::Stats { path, json } => stats_cmd(path, *json),
        Command::Api {
            path,
            out,
            private,
            check,
        } => api_cmd(path, out.as_deref(), *private, *check),
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
