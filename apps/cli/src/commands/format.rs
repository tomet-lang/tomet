use std::fs;
use std::path::PathBuf;

pub(crate) fn format_cmd(paths: &[PathBuf], write: bool, check: bool) -> anyhow::Result<()> {
    if paths.is_empty() {
        return Ok(());
    }

    if paths.len() == 1 && paths[0].is_file() && !write && !check {
        let src = fs::read_to_string(&paths[0])?;
        let formatted = tomet_formatter::format_source(&src);
        print!("{formatted}");
        return Ok(());
    }

    let mut files = Vec::new();
    for path in paths {
        if path.is_file() {
            files.push(path.clone());
        } else if path.is_dir() {
            files.extend(tomet_indexer::collect_tm_files(path));
        } else {
            return Err(anyhow::anyhow!("path '{}' does not exist", path.display()));
        }
    }

    if files.is_empty() {
        println!("No .tmt files found");
        return Ok(());
    }

    if check {
        let mut unformatted = Vec::new();
        for file in &files {
            let src = fs::read_to_string(file)?;
            if tomet_formatter::format_source(&src) != src {
                unformatted.push(file);
            }
        }
        if unformatted.is_empty() {
            return Ok(());
        }
        for file in &unformatted {
            println!("{}", file.display());
        }
        return Err(anyhow::anyhow!(
            "{} of {} file(s) not formatted",
            unformatted.len(),
            files.len()
        ));
    }

    if write {
        let mut changed = 0;
        for file in &files {
            match fs::read_to_string(file) {
                Ok(src) => {
                    let formatted = tomet_formatter::format_source(&src);
                    if formatted == src {
                        continue;
                    }
                    match fs::write(file, formatted) {
                        Ok(()) => {
                            if files.len() > 1 || paths[0].is_dir() {
                                println!("{}", file.display());
                            }
                            changed += 1;
                        }
                        Err(e) => eprintln!("Error writing {}: {e}", file.display()),
                    }
                }
                Err(e) => eprintln!("Error reading {}: {e}", file.display()),
            }
        }
        if files.len() > 1 || paths[0].is_dir() {
            println!("Formatted {changed} of {} file(s).", files.len());
        }
        return Ok(());
    }

    for file in &files {
        let src = fs::read_to_string(file)?;
        let formatted = tomet_formatter::format_source(&src);
        println!("==> {} <==", file.display());
        print!("{formatted}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_format_multiple_files_in_place() {
        let temp_dir =
            std::env::temp_dir().join(format!("tomet_test_fmt_multi_{}", nanoid::nanoid!()));
        let _ = fs::create_dir_all(&temp_dir);
        let file1 = temp_dir.join("a.tmt");
        let file2 = temp_dir.join("b.tmt");

        fs::write(&file1, "# Heading   \n\n\nBody   \n").unwrap();
        fs::write(&file2, "# Heading 2   \n").unwrap();

        let paths = vec![file1.clone(), file2.clone()];
        format_cmd(&paths, true, false).unwrap();

        let res1 = fs::read_to_string(&file1).unwrap();
        let res2 = fs::read_to_string(&file2).unwrap();

        assert_eq!(res1, "# Heading\n\nBody\n");
        assert_eq!(res2, "# Heading 2\n");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_format_check_mode() {
        let temp_dir =
            std::env::temp_dir().join(format!("tomet_test_fmt_check_{}", nanoid::nanoid!()));
        let _ = fs::create_dir_all(&temp_dir);
        let file = temp_dir.join("c.tmt");

        fs::write(&file, "# Heading   \n").unwrap();
        let paths = vec![file.clone()];
        assert!(format_cmd(&paths, false, true).is_err());

        format_cmd(&paths, true, false).unwrap();
        assert!(format_cmd(&paths, false, true).is_ok());

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
