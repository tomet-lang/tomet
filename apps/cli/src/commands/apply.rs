use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Result, bail};
use tomet_ast::Value;
use tomet_config::PrinterConfig;
use tomet_load_config::find_config_file;
use tomet_workspace::{SeatsNotFilled, apply_blueprint_to_file};

pub fn apply_cmd(
    paths: &[PathBuf],
    blueprint: Option<&str>,
    dry_run: bool,
    vars: &[(String, String)],
) -> Result<()> {
    let current_dir = std::env::current_dir()?;
    let (config, root) = if let Some((cfg, _, proj_root)) = find_config_file(&current_dir) {
        (cfg, proj_root)
    } else {
        (PrinterConfig::default(), current_dir.clone())
    };

    let mut files = Vec::new();
    for path in paths {
        let abs = if path.is_absolute() {
            path.clone()
        } else {
            current_dir.join(path)
        };
        if abs.is_file() {
            files.push(abs);
        } else if abs.is_dir() {
            let dir_files = tomet_indexer::collect_tm_files_with_config(&abs, &config, &root);
            files.extend(dir_files);
        } else {
            bail!("path '{}' does not exist", path.display());
        }
    }

    if files.is_empty() {
        println!("No .tmt files found to apply blueprint to.");
        return Ok(());
    }

    let mut var_map = HashMap::new();
    for (k, v) in vars {
        var_map.insert(k.clone(), Value::String(v.clone()));
    }

    let mut changed_count = 0;
    let mut error_count = 0;

    for file in &files {
        let rel_path = file.strip_prefix(&root).unwrap_or(file);
        match apply_blueprint_to_file(&root, blueprint, rel_path, &var_map, &config, dry_run) {
            Ok(applied) => {
                if applied.changed {
                    changed_count += 1;
                    if dry_run {
                        println!(
                            "Would apply {} to {}",
                            applied.blueprint.display(),
                            rel_path.display()
                        );
                    } else {
                        println!(
                            "Applied {} to {}",
                            applied.blueprint.display(),
                            rel_path.display()
                        );
                    }
                } else {
                    println!("Unchanged {}", rel_path.display());
                }

                for u in &applied.unfilled {
                    eprintln!(
                        "{}:{}: warning: {}",
                        applied.blueprint.display(),
                        u.line,
                        u.message
                    );
                }
                if !applied.unfilled.is_empty() {
                    eprintln!(
                        "{} left as written in {}",
                        match applied.unfilled.len() {
                            1 => "1 expression".to_string(),
                            n => format!("{n} expressions"),
                        },
                        rel_path.display()
                    );
                }
            }
            Err(err) => {
                error_count += 1;
                if let Some(seats) = err.downcast_ref::<SeatsNotFilled>() {
                    eprintln!("{seats}");
                } else {
                    eprintln!("error: {}: {err}", rel_path.display());
                }
            }
        }
    }

    if files.len() > 1 {
        if dry_run {
            println!(
                "Would apply blueprint to {changed_count} of {} files.",
                files.len()
            );
        } else {
            println!(
                "Applied blueprint to {changed_count} of {} files.",
                files.len()
            );
        }
    }

    if error_count > 0 {
        bail!("{error_count} file(s) failed to apply blueprint");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use tomet_config::RegistryConfig;

    fn setup_test_vault(name: &str) -> (PathBuf, PrinterConfig) {
        let temp_dir =
            std::env::temp_dir().join(format!("tm_test_cli_apply_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let tmpl_dir = temp_dir.join(".tomet/blueprints");
        fs::create_dir_all(&tmpl_dir).unwrap();
        let tmpl_file = tmpl_dir.join("task.blueprint.tmt");
        fs::write(
            &tmpl_file,
            "::kind(blueprint)\n::blueprint(task)\n::id(<$uuid(default: auto)>)\n::var(\n  title: { default: \"Untitled\" }\n)\n::meta(\n  title: $(var.title)\n)\n\n=[ Description ]#(description)\n",
        )
        .unwrap();

        let cfg = PrinterConfig {
            registry: RegistryConfig {
                blueprints: vec![".tomet/blueprints/task.blueprint.tmt".to_string()],
                ..Default::default()
            },
            ..PrinterConfig::default()
        };

        (temp_dir, cfg)
    }

    #[test]
    fn test_apply_cmd_file() {
        let (temp_dir, cfg) = setup_test_vault("file");
        let doc_path = temp_dir.join("my-task.tmt");
        fs::write(&doc_path, "::kind(task)\n\nInitial notes.\n").unwrap();

        let mut vars = HashMap::new();
        vars.insert("title".to_string(), Value::String("Task 1".into()));

        let applied = apply_blueprint_to_file(
            &temp_dir,
            None,
            Path::new("my-task.tmt"),
            &vars,
            &cfg,
            false,
        )
        .unwrap();

        assert!(applied.changed);
        let content = fs::read_to_string(&doc_path).unwrap();
        assert!(content.contains("::kind(task)"));
        assert!(content.contains("::id("));
        assert!(content.contains("title: \"Task 1\""));
        assert!(content.contains("=[Description] #(description)"));
        assert!(content.contains("Initial notes."));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_apply_cmd_dry_run() {
        let (temp_dir, cfg) = setup_test_vault("dry_run");
        let doc_path = temp_dir.join("my-task.tmt");
        let original = "::kind(task)\n\nInitial notes.\n";
        fs::write(&doc_path, original).unwrap();

        let applied = apply_blueprint_to_file(
            &temp_dir,
            None,
            Path::new("my-task.tmt"),
            &HashMap::new(),
            &cfg,
            true, // dry_run
        )
        .unwrap();

        assert!(applied.changed);
        let content = fs::read_to_string(&doc_path).unwrap();
        assert_eq!(content, original);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_apply_cmd_explicit_blueprint() {
        let (temp_dir, cfg) = setup_test_vault("explicit");
        let doc_path = temp_dir.join("raw-doc.tmt");
        fs::write(&doc_path, "Just some thoughts.\n").unwrap();

        let applied = apply_blueprint_to_file(
            &temp_dir,
            Some("task"),
            Path::new("raw-doc.tmt"),
            &HashMap::new(),
            &cfg,
            false,
        )
        .unwrap();

        assert!(applied.changed);
        let content = fs::read_to_string(&doc_path).unwrap();
        assert!(content.contains("::kind(task)"));
        assert!(content.contains("title: Untitled"));

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
