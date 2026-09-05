use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use tomet_ast::Value;
use tomet_config::{PrinterConfig, find_config_file};
use tomet_workspace::{create_file_from_blueprint, declaration_errors, list_blueprints};

pub fn new_cmd(
    path: &Path,
    blueprint: Option<&str>,
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

    // A declared path that rotted is reported here, once, wherever the
    // command was going. Finding out at the document that wanted the
    // blueprint would name the wrong file.
    for problem in declaration_errors(&root, &config) {
        eprintln!("warning: {problem}");
    }

    if list {
        let blueprints = list_blueprints(&root, &config);
        if blueprints.is_empty() {
            println!(
                "This vault declares no blueprints. Add their paths to `blueprints` in the config."
            );
        } else {
            println!("Declared blueprints:");
            for (name, path) in blueprints {
                println!("  - {:<15} ({})", name, path.display());
            }
        }
        return Ok(());
    }

    let blueprint_name = match blueprint {
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
        create_file_from_blueprint(&root, &blueprint_name, path, &var_map, &config, force)?;

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

        let tmpl_dir = temp_dir.join(".tomet/blueprints");
        fs::create_dir_all(&tmpl_dir).unwrap();
        let tmpl_file = tmpl_dir.join("rfc.blueprint.tmt");
        fs::write(
            &tmpl_file,
            "@kind(blueprint)\n@blueprint(rfc)\n@meta{\n  id: ${uuid(\"nil\")}\n  title: ${title}\n  author: ${vars.author}\n}\n\n#[ Motivation for ${title} ] {id: motivation}\n",
        )
        .unwrap();

        let cfg = PrinterConfig {
            blueprints: vec![".tomet/blueprints/rfc.blueprint.tmt".to_string()],
            ..PrinterConfig::default()
        };

        let mut var_map = HashMap::new();
        var_map.insert(
            "title".to_string(),
            Value::String("New Auth Protocol".into()),
        );
        var_map.insert(
            "vars".to_string(),
            Value::Map(vec![("author".to_string(), Value::String("Bob".into()))]),
        );

        let created = create_file_from_blueprint(
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
