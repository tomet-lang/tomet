use std::path::Path;

pub(crate) fn refactor_cmd(
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

    let mut report = tomet_workspace::refactor_workspace(path, &options)?;
    let results = &report.diffs;
    let changed_files: Vec<_> = results.iter().filter(|r| r.is_changed()).collect();

    // A file the sweep could not read or parse is not a file it checked.
    // Saying so is the whole point: it is the likeliest one to still
    // carry the spelling being migrated away from.
    for (file, err) in &report.errors {
        eprintln!("Could not check {}: {err}", file.display());
    }

    if check {
        if changed_files.is_empty() && report.errors.is_empty() {
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
        if changed_files.is_empty() {
            return Err(anyhow::anyhow!(
                "{} file(s) could not be checked",
                report.errors.len()
            ));
        }
        return Err(anyhow::anyhow!(
            "{} of {} file(s) need refactoring{}",
            changed_files.len(),
            results.len(),
            if report.errors.is_empty() {
                String::new()
            } else {
                format!(", and {} could not be checked", report.errors.len())
            }
        ));
    }

    if in_place {
        // Lossy -- `refactor_source` re-prints from the AST, which carries
        // no comments. See its doc comment before widening what `-i` runs on.
        let total = report.diffs.len();
        let saved = tomet_workspace::save_file_diffs(&mut report.diffs)?;
        println!("Refactored {saved} of {total} file(s).");
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
    use std::fs;

    /// A file that does not parse is a file the sweep did not check, and
    /// `--check` has to say so by failing. It used to warn on stderr and
    /// exit 0, which reported a clean tree containing a file nobody had
    /// looked at.
    #[test]
    fn check_fails_on_a_file_it_could_not_parse() {
        let temp_dir = std::env::temp_dir().join("tomet_test_refactor_unparseable");
        let _ = fs::remove_dir_all(&temp_dir);
        let _ = fs::create_dir_all(&temp_dir);

        fs::write(temp_dir.join("fine.tmt"), "@kind(note)\n\n#[ Title ]\n").unwrap();
        fs::write(temp_dir.join("broken.tmt"), "#[ unterminated\n").unwrap();

        let res = refactor_cmd(&temp_dir, false, false, false, false, true);
        assert!(
            res.is_err(),
            "--check must not pass while a file could not be parsed"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn refactor_cmd_in_place_transforms_document() {
        let temp_dir = std::env::temp_dir().join("tomet_test_refactor_cmd");
        let _ = fs::create_dir_all(&temp_dir);
        let src_file = temp_dir.join("doc.tmt");

        let src_content = r#"@version(1.0)
@meta(format:yaml)+++
type: note
title: My Title
+++
@config{
  macros: {
    gh: "https://github.com/${1}"
  }
}

- @link("https://github.com/tomet/tomet")[Repo]
"#;
        fs::write(&src_file, src_content).unwrap();

        let res = refactor_cmd(&src_file, true, false, false, false, false);
        assert!(res.is_ok());

        let refactored = fs::read_to_string(&src_file).unwrap();
        assert!(refactored.contains("@kind(note)"));
        assert!(refactored.contains("$gh(\"tomet/tomet\")"));
        assert!(!refactored.contains("format:yaml"));

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
