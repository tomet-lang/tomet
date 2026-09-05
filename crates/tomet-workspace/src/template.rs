//! Blueprint discovery and file instantiation across a workspace.
//!
//! A vault keeps its blueprints in `.tomet/blueprints/`. The names here
//! still say `template` because `tomet new --template` does; the
//! vocabulary migration that renamed `docs/examples/templates/` to
//! `blueprints/` has not reached the CLI's flag yet.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use tomet_ast::Value;
use tomet_compute::EvaluationContext;
use tomet_config::PrinterConfig;
use tomet_transform::instantiate_blueprint;

/// Discovers a blueprint file for `name` within `root` workspace or according to `config`.
pub fn find_template(root: &Path, name: &str, config: &PrinterConfig) -> Option<PathBuf> {
    // 1. Check explicit configuration
    if let Some(cfg_path) = config.templates.get(name) {
        let path = root.join(cfg_path);
        if path.is_file() {
            return Some(path);
        }
    }

    // 2. Direct path check (e.g. if name is already a path like ".tomet/blueprints/daily.blueprint.tmt")
    let direct_path = root.join(name);
    if direct_path.is_file() {
        return Some(direct_path);
    }
    let direct_tmt = root.join(format!("{name}.tmt"));
    if direct_tmt.is_file() {
        return Some(direct_tmt);
    }

    // 3. The convention: `.tomet/blueprints/<name>.blueprint.tmt`.
    //
    // One directory, one spelling. There used to be three of each --
    // `templates/`, `.tomet/templates/` and `docs/examples/blueprints/`,
    // times `<name>.tmt`, `template.<name>.tmt` and
    // `<name>.template.tmt` -- which is how `docs/docs.settings.tmt`
    // pointed at a filename that does not exist and still resolved.
    //
    // `templates/` at the root of a vault is gone because that name is
    // the vault owner's to spend, not Tomet's: a directory without a dot
    // belongs to whoever made the vault. `.tomet/` is the one place this
    // tool gets to define, so a convention lives there or nowhere.
    let conventional = blueprint_dir(root).join(format!("{name}.blueprint.tmt"));
    if conventional.is_file() {
        return Some(conventional);
    }

    None
}

/// `<root>/.tomet/blueprints`, where a vault keeps the blueprints
/// `tomet new` instantiates.
pub fn blueprint_dir(root: &Path) -> PathBuf {
    root.join(".tomet").join("blueprints")
}

/// Lists all available blueprints in workspace.
pub fn list_templates(root: &Path, config: &PrinterConfig) -> Vec<(String, PathBuf)> {
    let mut results = HashMap::new();

    // From config
    for (name, path_str) in &config.templates {
        let p = root.join(path_str);
        if p.is_file() {
            results.insert(name.clone(), p);
        }
    }

    // From the convention. One directory, matching `find_template` --
    // the two used to disagree about which spellings and which
    // directories existed.
    let dir = blueprint_dir(root);
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("tmt") {
                continue;
            }
            // `daily-note.blueprint.tmt` lists as `daily-note`.
            if let Some(name) = path
                .file_name()
                .and_then(|s| s.to_str())
                .and_then(|s| s.strip_suffix(".blueprint.tmt"))
            {
                results.entry(name.to_string()).or_insert(path);
            }
        }
    }

    let mut list: Vec<(String, PathBuf)> = results.into_iter().collect();
    list.sort_by(|a, b| a.0.cmp(&b.0));
    list
}

/// Instantiates a template file into formatted Tomet source text for `output_path`.
pub fn instantiate_template_file(
    template_path: &Path,
    output_path: &Path,
    vars: &HashMap<String, Value>,
    config: &PrinterConfig,
) -> Result<String> {
    let src = fs::read_to_string(template_path)
        .with_context(|| format!("failed to read template at {}", template_path.display()))?;

    let mut doc = tomet_parser::parse_document(&src)
        .map_err(|e| anyhow::anyhow!("failed to parse template: {e}"))?;

    let filename = output_path
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("document.tmt")
        .to_string();

    let default_title = output_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Document")
        .to_string();

    let mut ctx_vars = vars.clone();
    ctx_vars
        .entry("filename".to_string())
        .or_insert(Value::String(filename));
    ctx_vars
        .entry("title".to_string())
        .or_insert(Value::String(default_title));

    let ctx = EvaluationContext { vars: ctx_vars };

    instantiate_blueprint(&mut doc, &ctx);

    let rendered = tomet_printer::document_to_tm_with_config(&doc, config);
    Ok(rendered)
}

/// Creates a new document from a template and writes it to disk.
pub fn create_file_from_template(
    root: &Path,
    template_name_or_path: &str,
    target_rel_path: &Path,
    vars: &HashMap<String, Value>,
    config: &PrinterConfig,
    overwrite: bool,
) -> Result<PathBuf> {
    let template_file = find_template(root, template_name_or_path, config)
        .ok_or_else(|| anyhow::anyhow!("template '{template_name_or_path}' not found"))?;

    let target_abs_path = root.join(target_rel_path);

    if target_abs_path.exists() && !overwrite {
        bail!("target file already exists: {}", target_abs_path.display());
    }

    if let Some(parent) = target_abs_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directories for {}", parent.display()))?;
    }

    let content = instantiate_template_file(&template_file, &target_abs_path, vars, config)?;
    fs::write(&target_abs_path, content)
        .with_context(|| format!("failed to write file {}", target_abs_path.display()))?;

    Ok(target_abs_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_instantiation_and_creation() {
        let temp_dir = std::env::temp_dir().join(format!("tm_test_tmpl_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        let _ = fs::create_dir_all(&temp_dir);
        let root = &temp_dir;

        let tmpl_dir = blueprint_dir(root);
        fs::create_dir_all(&tmpl_dir).unwrap();

        let tmpl_file = tmpl_dir.join("daily-note.blueprint.tmt");
        fs::write(
            &tmpl_file,
            "@blueprint(daily-note)\n@meta{\n  id: ${uuid(\"nil\")}\n  title: ${title}\n}\n\n#[ Tasks for ${title} ] {id: tasks}\n",
        )
        .unwrap();

        let config = PrinterConfig::default();
        let target_path = Path::new("notes/2026-09-01.tmt");

        let mut vars = HashMap::new();
        vars.insert(
            "title".to_string(),
            Value::String("2026-09-01 Daily".into()),
        );

        let created_path =
            create_file_from_template(root, "daily-note", target_path, &vars, &config, false)
                .unwrap();

        assert!(created_path.is_file());
        let result_content = fs::read_to_string(&created_path).unwrap();
        assert!(result_content.contains("@kind(daily-note)"));
        assert!(result_content.contains("00000000-0000-0000-0000-00000000000"));
        assert!(result_content.contains("Tasks for 2026-09-01 Daily"));
    }
}
