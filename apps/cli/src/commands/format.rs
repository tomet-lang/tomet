use std::fs;
use std::path::Path;

pub(crate) fn format_cmd(path: &Path, write: bool, check: bool) -> anyhow::Result<()> {
    if path.is_file() {
        let src = fs::read_to_string(path)?;
        let formatted = tomet_formatter::format_source(&src);
        return if check {
            if formatted == src {
                Ok(())
            } else {
                Err(anyhow::anyhow!("{} is not formatted", path.display()))
            }
        } else if write {
            if formatted != src {
                fs::write(path, formatted)?;
            }
            Ok(())
        } else {
            print!("{formatted}");
            Ok(())
        };
    }
    if !path.is_dir() {
        return Err(anyhow::anyhow!("path '{}' does not exist", path.display()));
    }

    let files = tomet_indexer::collect_tm_files(path);
    if files.is_empty() {
        println!("No .tmt or .tmt files found in {}", path.display());
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
                            println!("{}", file.display());
                            changed += 1;
                        }
                        Err(e) => eprintln!("Error writing {}: {e}", file.display()),
                    }
                }
                Err(e) => eprintln!("Error reading {}: {e}", file.display()),
            }
        }
        println!("Formatted {changed} of {} file(s).", files.len());
        return Ok(());
    }

    // Default (no --in-place/--check): print every file's formatted
    // content to stdout, headed like `head`/`tail` do for multiple
    // files -- keeps a directory's worth of output distinguishable,
    // same as single-file mode (no header) staying exactly as before.
    for file in &files {
        let src = fs::read_to_string(file)?;
        let formatted = tomet_formatter::format_source(&src);
        println!("==> {} <==", file.display());
        print!("{formatted}");
    }
    Ok(())
}
