//! A small CLI for manually checking that `typedmark-parser`/
//! `serde_typedmark` actually work against a real `.tm` file, instead of
//! only trusting `cargo test`'s unit tests.

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

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
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match &cli.command {
        Command::Check { file, data } => check(file, *data),
        Command::Ast { file, data } => ast(file, *data),
        Command::Roundtrip { file } => roundtrip(file),
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
        Err(anyhow::anyhow!("re-parsing the rendered output produced a different value"))
    }
}
