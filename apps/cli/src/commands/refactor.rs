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

    let mut results = tomet_workspace::refactor_workspace(path, &options)?;
    let changed_files: Vec<_> = results.iter().filter(|r| r.is_changed()).collect();

    if check {
        if changed_files.is_empty() {
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
        return Err(anyhow::anyhow!(
            "{} of {} file(s) need refactoring",
            changed_files.len(),
            results.len()
        ));
    }

    if in_place {
        let saved = tomet_workspace::save_file_diffs(&mut results)?;
        println!("Refactored {} of {} file(s).", saved, results.len());
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

    #[test]
    fn refactor_cmd_in_place_transforms_document() {
        let temp_dir = std::env::temp_dir().join("tomet_test_refactor_cmd");
        let _ = fs::create_dir_all(&temp_dir);
        let src_file = temp_dir.join("doc.tmt");

        let src_content = r#"@version(1.0)
@meta(format:yaml){
  type: note
  title: My Title
}
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
