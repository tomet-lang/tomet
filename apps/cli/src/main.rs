//! A small CLI for manually checking that `tomet-parser`/
//! `serde_tomet` actually work against a real `.tmt` file, instead of
//! only trusting `cargo test`'s unit tests.

mod cli;
mod commands;
mod util;

use std::process::ExitCode;

use clap::Parser;

use cli::{Cli, Command};
use commands::ast::ast;
use commands::check::check;
use commands::check_links::check_links_cmd;
use commands::export::export_cmd;
use commands::format::format_cmd;
use commands::from_md::from_md_cmd;
use commands::from_pandoc::from_pandoc;
use commands::html::html;
use commands::refactor::refactor_cmd;
use commands::roundtrip::roundtrip;
use commands::serve::serve;
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
        } => html(file, out, *advanced, lang.clone()),
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
        } => format_cmd(paths, *in_place, *check),
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
        } => refactor_cmd(path, *in_place, *url_macros, *meta_kind, *value_dsl, *check),
        Command::New {
            path,
            blueprint,
            list,
            force,
            vars,
        } => commands::new::new_cmd(path, blueprint.as_deref(), *list, *force, vars),
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
