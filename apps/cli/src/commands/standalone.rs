//! `tomet standalone`: take a document out of its vault.
//!
//! The document is read in its vault, made standalone by `tomet-convert-standalone`, and written
//! where `--out` says, or to standard output. What was done goes to standard error, so standard
//! output is exactly the file, as it is for the conversion commands.
//!
//! There is no `--in-place`: freezing replaces `${self.path}` and its like by their values, so
//! doing it to the original would take those expressions out of the file in the vault. The
//! command refuses to write over its own input for the same reason.
//!
//! It ends with an error when something the file takes from outside could not be dealt with, so
//! a script notices that what it was given is not yet standalone. The file is written all the
//! same.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use tomet_convert_standalone::{Change, Options, Treatment, make_standalone};
use tomet_lexer::LineIndex;

use crate::util::{format_parse_error, vault_for};

pub(crate) fn run(file: &Path, out: Option<&Path>, freeze_fields: bool) -> Result<()> {
    if !file.is_file() {
        // Not `-`: where a file sits is part of what `${self.path}` says, so there is nothing to
        // freeze it to for text that has no place.
        return Err(anyhow!(
            "'{}' is not a file: a document is taken out of its vault from where it sits",
            file.display()
        ));
    }
    let dest = out.map(|out| destination(file, out));
    if let Some(dest) = &dest
        && same_file(file, dest)
    {
        return Err(anyhow!(
            "'{}' is the file being converted: writing over it would take the expressions it \
             freezes out of the file in the vault; write somewhere else",
            dest.display()
        ));
    }

    let src =
        fs::read_to_string(file).map_err(|e| anyhow!("cannot read {}: {e}", file.display()))?;
    let vault = vault_for(file);
    let (doc, _) = vault
        .parse(&src)
        .map_err(|e| anyhow!("{}", format_parse_error(file, &src, &e)))?;

    let converted = make_standalone(
        &vault,
        &doc,
        file,
        &vault.resolver(),
        &Options { freeze_fields },
    );

    let text = tomet_printer::document_to_tm_with_config(&converted.document, &vault.config);
    // A conversion never hands back text that is not Tomet.
    if let Err(e) = tomet_parser::parse_document(&text) {
        return Err(anyhow!(
            "generated .tmt has parse error: {}",
            format_parse_error(file, &text, &e)
        ));
    }

    match &dest {
        Some(dest) => write(dest, &text)?,
        None => std::io::stdout().lock().write_all(text.as_bytes())?,
    }

    let mut stderr = std::io::stderr().lock();
    let lines = LineIndex::new(&src);
    for change in &converted.changes {
        writeln!(stderr, "{}", describe(change, &lines))?;
    }
    let count = |wanted: fn(&Treatment) -> bool| {
        converted
            .changes
            .iter()
            .filter(|change| wanted(&change.treatment))
            .count()
    };
    writeln!(
        stderr,
        "{} carried, {} frozen, {} left",
        count(|t| matches!(t, Treatment::Carried)),
        count(|t| matches!(t, Treatment::Frozen)),
        count(|t| matches!(t, Treatment::Left { .. })),
    )?;

    if converted.remaining.is_empty() {
        Ok(())
    } else {
        Err(anyhow!(
            "{} thing(s) the file takes from outside could not be dealt with, so it is not \
             standalone yet",
            converted.remaining.len()
        ))
    }
}

/// `carried  the macro `gh` ... (line 3)`
fn describe(change: &Change, lines: &LineIndex) -> String {
    let at = lines.line_col(change.span.start);
    match &change.treatment {
        Treatment::Carried => format!("carried  {} (line {})", change.dirt, at.line),
        Treatment::Frozen => format!("frozen   {} (line {})", change.dirt, at.line),
        Treatment::Left { reason } => {
            format!("left     {} (line {}): {reason}", change.dirt, at.line)
        }
    }
}

/// `out` is a file, or a directory the file goes into under its own name.
fn destination(file: &Path, out: &Path) -> PathBuf {
    if out.is_dir() {
        out.join(file.file_name().unwrap_or_default())
    } else {
        out.to_path_buf()
    }
}

/// Whether two paths are one file on disk (the destination need not exist yet).
fn same_file(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn write(dest: &Path, text: &str) -> Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| anyhow!("cannot create {}: {e}", parent.display()))?;
    }
    fs::write(dest, text).map_err(|e| anyhow!("cannot write {}: {e}", dest.display()))
}

#[cfg(test)]
mod tests;
