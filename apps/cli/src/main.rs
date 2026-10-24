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
//! - (Documentation) `spec`.
//!
//! The project-level commands -- `check-links`, `serve` and `tui` -- are
//! `sisho`'s: they follow a vault's declarations, caches and screens rather
//! than a document alone, as `cargo` does for the language `rustc` compiles.
//! `sisho tomet <command>` reaches the commands above from there.

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod cli;
mod commands;
mod util;

use std::process::ExitCode;

use clap::Parser;

use cli::{Cli, Command, ConvertArgs};
use commands::ast::ast;
use commands::check::check;
use commands::convert::{Direction, HtmlOptions, Plan, Source, Target};
use commands::export::export_cmd;
use commands::format::format_cmd;
use commands::refactor::refactor_cmd;
use commands::roundtrip::roundtrip;
use commands::stats::stats_cmd;

/// Runs one of the six conversion commands: `direction` is all that sets them
/// apart.
fn convert(io: &ConvertArgs, direction: Direction, remove_original: bool) -> anyhow::Result<()> {
    let plan = Plan {
        out: io.out.as_deref(),
        in_place: io.in_place,
        dry_run: io.dry_run,
        remove_original,
    };
    commands::convert::run(&io.path, &direction, &plan)
}

fn main() -> ExitCode {
    jobserver_gate::init_from_env();

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
            io,
            advanced,
            lang,
            body,
        } => convert(
            io,
            Direction::To(Target::Html(HtmlOptions {
                advanced: *advanced,
                lang: lang.clone(),
                body: *body,
            })),
            false,
        ),
        Command::ToMd { io } => convert(io, Direction::To(Target::CommonMark), false),
        Command::ToTypst { io } => convert(io, Direction::To(Target::Typst), false),
        Command::ToPandoc { io } => convert(io, Direction::To(Target::Pandoc), false),
        Command::FromPandoc {
            io,
            remove_original,
        } => convert(io, Direction::From(Source::Pandoc), *remove_original),
        Command::FromMd {
            io,
            remove_original,
        } => convert(io, Direction::From(Source::CommonMark), *remove_original),
        Command::Format {
            paths,
            in_place,
            check,
            force,
        } => format_cmd(paths, *in_place, *check, *force),
        Command::Spec {
            target,
            all,
            out,
            check,
        } => commands::spec::spec_cmd(*target, *all, out.as_deref(), *check),
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
