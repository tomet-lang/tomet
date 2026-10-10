use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use tomet_config::PrinterConfig;
use tomet_extract_rust::{RustCrateSource, RustExtractOptions, extract_crate_doc};
use walkdir::WalkDir;

pub(crate) fn api_cmd(
    target_path: &Path,
    override_out: Option<&Path>,
    private: bool,
    check: bool,
) -> anyhow::Result<()> {
    if !target_path.exists() {
        return Err(anyhow::anyhow!(
            "path '{}' does not exist",
            target_path.display()
        ));
    }

    // 1. Locate config and resolve output directory
    let (config, _, config_root) = tomet_load_config::find_config_file(target_path).unwrap_or((
        PrinterConfig::default(),
        target_path.to_path_buf(),
        target_path.to_path_buf(),
    ));

    let out_dir = match override_out {
        Some(dir) => {
            if dir.is_absolute() {
                dir.to_path_buf()
            } else {
                config_root.join(dir)
            }
        }
        None => {
            if let Some(cfg_out) = &config.api.rust_out {
                config_root.join(cfg_out)
            } else {
                return Err(anyhow::anyhow!(
                    "no output directory specified: set `@config{{ api: {{ rust: {{ out: \"...\" }} }} }}` in your configuration file or pass `--out <DIR>`"
                ));
            }
        }
    };

    // 2. Discover crates to process
    let crates = discover_crates(target_path)?;
    if crates.is_empty() {
        return Err(anyhow::anyhow!(
            "no Rust crates found under '{}'",
            target_path.display()
        ));
    }

    let options = RustExtractOptions { private };
    let mut stale_files = Vec::new();
    let mut generated_count = 0;

    for (crate_name, crate_dir) in &crates {
        let crate_source = match load_crate_source(crate_name, crate_dir) {
            Ok(src) => src,
            Err(e) => {
                eprintln!("Warning: skipping crate '{}': {e}", crate_name);
                continue;
            }
        };

        let doc = match extract_crate_doc(&crate_source, &options) {
            Ok(doc) => doc,
            Err(e) => {
                eprintln!(
                    "Warning: failed to extract doc for crate '{}': {e}",
                    crate_name
                );
                continue;
            }
        };

        let printed = tomet_printer::document_to_tm_with_config(&doc, &config);
        let formatted = tomet_formatter::format_source_with_config(&printed, &config);

        let out_file = out_dir.join(format!("{crate_name}.tmt"));

        if check {
            match fs::read_to_string(&out_file) {
                Ok(existing) if existing == formatted => {}
                _ => {
                    stale_files.push(out_file);
                }
            }
        } else {
            if let Some(parent) = out_file.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&out_file, &formatted)?;
            println!("Wrote {}", out_file.display());
            generated_count += 1;
        }
    }

    if check {
        if stale_files.is_empty() {
            println!("All API documentation files are up to date.");
            Ok(())
        } else {
            for file in &stale_files {
                println!("Stale API documentation: {}", file.display());
            }
            Err(anyhow::anyhow!(
                "{} API documentation file(s) differ from source; re-run without --check",
                stale_files.len()
            ))
        }
    } else {
        println!(
            "Generated API documentation for {} crate(s) into '{}'",
            generated_count,
            out_dir.display()
        );
        Ok(())
    }
}

/// Represents a discovered crate: (crate_name, crate_root_directory)
type DiscoveredCrate = (String, PathBuf);

/// Discovers Rust crates starting from `path`. If `path` is a single crate, returns it.
/// If it contains a workspace Cargo.toml, returns all workspace member crates.
fn discover_crates(path: &Path) -> anyhow::Result<Vec<DiscoveredCrate>> {
    let mut crates = Vec::new();
    let cargo_toml_path =
        if path.is_file() && path.file_name().and_then(|n| n.to_str()) == Some("Cargo.toml") {
            path.to_path_buf()
        } else if path.is_dir() {
            path.join("Cargo.toml")
        } else {
            return Ok(crates);
        };

    if !cargo_toml_path.exists() {
        return Ok(crates);
    }

    let content = fs::read_to_string(&cargo_toml_path)?;
    let parsed: toml::Value = toml::from_str(&content)?;

    let base_dir = cargo_toml_path.parent().unwrap_or(Path::new("."));

    // Check if it has a [workspace]
    if let Some(workspace) = parsed.get("workspace") {
        if let Some(members) = workspace.get("members").and_then(|m| m.as_array()) {
            for member in members {
                if let Some(member_str) = member.as_str() {
                    let member_dir = base_dir.join(member_str);
                    let member_cargo = member_dir.join("Cargo.toml");
                    if member_cargo.exists() {
                        if let Ok(member_content) = fs::read_to_string(&member_cargo) {
                            if let Ok(member_parsed) =
                                toml::from_str::<toml::Value>(&member_content)
                            {
                                if let Some(name) = member_parsed
                                    .get("package")
                                    .and_then(|p| p.get("name"))
                                    .and_then(|n| n.as_str())
                                {
                                    crates.push((name.to_string(), member_dir));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Check if the root Cargo.toml itself is also a package
    if let Some(pkg) = parsed.get("package") {
        if let Some(name) = pkg.get("name").and_then(|n| n.as_str()) {
            if !crates.iter().any(|(n, _)| n == name) {
                crates.push((name.to_string(), base_dir.to_path_buf()));
            }
        }
    }

    Ok(crates)
}

/// Loads all Rust source files for a crate into an in-memory `RustCrateSource`.
fn load_crate_source(crate_name: &str, crate_dir: &Path) -> anyhow::Result<RustCrateSource> {
    let mut files = HashMap::new();

    let root_file = if crate_dir.join("src/lib.rs").exists() {
        PathBuf::from("src/lib.rs")
    } else if crate_dir.join("src/main.rs").exists() {
        PathBuf::from("src/main.rs")
    } else {
        return Err(anyhow::anyhow!("neither src/lib.rs nor src/main.rs found"));
    };

    let src_dir = crate_dir.join("src");
    if !src_dir.exists() {
        return Err(anyhow::anyhow!("src/ directory not found"));
    }

    for entry in WalkDir::new(&src_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("rs") {
            let relative = path
                .strip_prefix(crate_dir)
                .map(Path::to_path_buf)
                .unwrap_or_else(|_| path.to_path_buf());
            let content = fs::read_to_string(path)?;
            files.insert(relative, content);
        }
    }

    Ok(RustCrateSource {
        crate_name: crate_name.to_string(),
        root_file,
        files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_cmd_lifecycle_and_check() {
        let temp_dir = std::env::temp_dir().join(format!("tomet_test_api_{}", nanoid::nanoid!()));
        let crate_dir = temp_dir.join("test_crate");
        let src_dir = crate_dir.join("src");
        fs::create_dir_all(&src_dir).unwrap();

        fs::write(
            crate_dir.join("Cargo.toml"),
            r#"[package]
name = "test-dummy"
version = "0.1.0"
edition = "2024"
"#,
        )
        .unwrap();

        fs::write(
            src_dir.join("lib.rs"),
            r#"//! Test dummy crate documentation.

/// A test function.
pub fn hello() -> &'static str {
    "world"
}
"#,
        )
        .unwrap();

        let out_dir = temp_dir.join("output");

        // 1. Generation
        let gen_res = api_cmd(&crate_dir, Some(&out_dir), false, false);
        assert!(gen_res.is_ok());

        let generated_file = out_dir.join("test-dummy.tmt");
        assert!(generated_file.exists());
        let content = fs::read_to_string(&generated_file).unwrap();
        assert!(content.contains("test-dummy"));
        assert!(content.contains("Test dummy crate documentation."));
        assert!(content.contains("fn hello"));

        // 2. Check mode - should succeed when up to date
        let check_ok = api_cmd(&crate_dir, Some(&out_dir), false, true);
        assert!(check_ok.is_ok());

        // 3. Check mode - should fail when modified/stale
        fs::write(&generated_file, "stale content").unwrap();
        let check_stale = api_cmd(&crate_dir, Some(&out_dir), false, true);
        assert!(check_stale.is_err());

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
