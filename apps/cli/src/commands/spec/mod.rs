//! Unified `tomet spec` command: automated specification and reference extraction.

use std::fs;
use std::path::{Path, PathBuf};

use clap::ValueEnum;

pub mod cli;
pub mod config;
pub mod util;

use util::config_for_output;

/// Target specification to generate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SpecTarget {
    /// CLI commands, arguments, and options derived from clap definitions.
    Cli,
    /// Vault and document configuration schema derived from tomet-config domain models.
    Config,
}

pub fn spec_cmd(
    target: Option<SpecTarget>,
    all: bool,
    out: Option<&Path>,
    check: bool,
) -> anyhow::Result<()> {
    if all {
        let Some(out_dir) = out else {
            anyhow::bail!("--all requires --out <DIR>");
        };
        return generate_all(out_dir, check);
    }

    let target = target.unwrap_or(SpecTarget::Cli);
    let (rendered, filename) = match target {
        SpecTarget::Cli => (cli::render(&config_for_output(out)), "cli.tmt"),
        SpecTarget::Config => (config::render(&config_for_output(out)), "config.tmt"),
    };

    let Some(out) = out else {
        print!("{rendered}");
        return Ok(());
    };

    let out_path = resolve_target_file(out, filename);
    write_or_check(
        &out_path,
        &rendered,
        check,
        match target {
            SpecTarget::Cli => "cli",
            SpecTarget::Config => "config",
        },
    )
}

fn generate_all(out_dir: &Path, check: bool) -> anyhow::Result<()> {
    let cli_path = out_dir.join("cli.tmt");
    let config_path = out_dir.join("config.tmt");

    let cli_text = cli::render(&config_for_output(Some(&cli_path)));
    let config_text = config::render(&config_for_output(Some(&config_path)));

    write_or_check(&cli_path, &cli_text, check, "cli")?;
    write_or_check(&config_path, &config_text, check, "config")?;

    Ok(())
}

fn resolve_target_file(out: &Path, default_name: &str) -> PathBuf {
    if out.is_dir() || out.extension().is_none() {
        out.join(default_name)
    } else {
        out.to_path_buf()
    }
}

fn write_or_check(path: &Path, content: &str, check: bool, target: &str) -> anyhow::Result<()> {
    if check {
        return match fs::read_to_string(path) {
            Ok(current) if current == content => Ok(()),
            Ok(_) => Err(anyhow::anyhow!(
                "{} is out of date; regenerate it with `tomet spec {} --out {}`",
                path.display(),
                target,
                path.display()
            )),
            Err(e) => Err(anyhow::anyhow!(
                "cannot read {}: {e}; generate it with `tomet spec {} --out {}`",
                path.display(),
                target,
                path.display()
            )),
        };
    }

    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    println!("Wrote {}", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_all_generates_both_specs_and_check_passes() {
        let dir = std::env::temp_dir().join(format!("tomet_test_spec_all_{}", nanoid::nanoid!()));
        let out_dir = dir.join("generated");

        assert!(
            spec_cmd(None, true, Some(&out_dir), true).is_err(),
            "should fail before writing"
        );

        spec_cmd(None, true, Some(&out_dir), false).unwrap();
        assert!(out_dir.join("cli.tmt").exists());
        assert!(out_dir.join("config.tmt").exists());

        assert!(
            spec_cmd(None, true, Some(&out_dir), true).is_ok(),
            "check must pass after write"
        );

        // Stale detection
        fs::write(out_dir.join("config.tmt"), "stale content\n").unwrap();
        assert!(
            spec_cmd(None, true, Some(&out_dir), true).is_err(),
            "must detect drift in config.tmt"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
