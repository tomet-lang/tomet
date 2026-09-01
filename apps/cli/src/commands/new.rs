use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use tomet_ast::Value;
use tomet_config::{PrinterConfig, find_config_file};
use tomet_workspace::{create_file_from_template, list_templates};

pub fn new_cmd(
    path: &Path,
    template: Option<&str>,
    list: bool,
    force: bool,
    vars: &[(String, String)],
) -> Result<()> {
    let current_dir = std::env::current_dir()?;
    let (config, root) = if let Some((cfg, _, proj_root)) = find_config_file(&current_dir) {
        (cfg, proj_root)
    } else {
        (PrinterConfig::default(), current_dir.clone())
    };

    if list {
        let templates = list_templates(&root, &config);
        if templates.is_empty() {
            println!(
                "No templates found in workspace (checked templates/, .tomet/templates/, and @config)"
            );
        } else {
            println!("Available templates:");
            for (name, path) in templates {
                println!("  - {:<15} ({})", name, path.display());
            }
        }
        return Ok(());
    }

    let template_name = match template {
        Some(t) => t.to_string(),
        None => {
            if let Some(parent_dir_name) = path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
            {
                parent_dir_name.to_string()
            } else {
                "default".to_string()
            }
        }
    };

    let mut var_map = HashMap::new();
    for (k, v) in vars {
        var_map.insert(k.clone(), Value::String(v.clone()));
    }

    let created_path =
        create_file_from_template(&current_dir, &template_name, path, &var_map, &config, force)?;

    println!("Created {}", created_path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_new_cmd_execution() {
        let temp_dir = std::env::temp_dir().join(format!("tm_test_cli_new_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        let _ = fs::create_dir_all(&temp_dir);

        let tmpl_dir = temp_dir.join("templates");
        fs::create_dir_all(&tmpl_dir).unwrap();
        let tmpl_file = tmpl_dir.join("rfc.tmt");
        fs::write(
            &tmpl_file,
            "@blueprint(rfc)\n@meta{\n  id: ${uuid(\"nil\")}\n  title: ${title}\n  author: ${vars.author}\n}\n\n#[ Motivation for ${title} ] {id: motivation}\n",
        )
        .unwrap();

        let cfg = PrinterConfig::default();

        let mut var_map = HashMap::new();
        var_map.insert(
            "title".to_string(),
            Value::String("New Auth Protocol".into()),
        );
        var_map.insert(
            "vars".to_string(),
            Value::Map(vec![("author".to_string(), Value::String("Bob".into()))]),
        );

        let created = create_file_from_template(
            &temp_dir,
            "rfc",
            Path::new("rfcs/001.tmt"),
            &var_map,
            &cfg,
            false,
        )
        .unwrap();

        assert!(created.is_file());
        let content = fs::read_to_string(&created).unwrap();
        assert!(content.contains("@kind(rfc)"));
        assert!(content.contains("New Auth Protocol"));
        assert!(content.contains("Bob"));
    }
}
