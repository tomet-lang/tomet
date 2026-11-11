use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use tomet_ast::Value;
use tomet_config::PrinterConfig;
use tomet_load_config::find_config_file;
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

    let created =
        create_file_from_blueprint(&root, &blueprint_name, path, &var_map, &config, force)?;

    println!("Created {}", created.path.display());

    // The expressions with no value are in the new file as written. Said here
    // in the form `tomet check` uses, pointing at the blueprint line they
    // came from.
    for u in &created.unfilled {
        eprintln!(
            "{}:{}: warning: {}",
            created.blueprint.display(),
            u.line,
            u.message
        );
    }
    if !created.unfilled.is_empty() {
        eprintln!(
            "{} left as written in {}",
            match created.unfilled.len() {
                1 => "1 expression".to_string(),
                n => format!("{n} expressions"),
            },
            created.path.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tomet_config::RegistryConfig;

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
            "::kind(blueprint)\n::blueprint(rfc)\n::var(\n  title: { type: string }\n  author: { type: string }\n)\n::meta(\n  title: $(var.title)\n  author: $(var.author)\n)\n\n=[ Motivation for $(var.title) ]#(motivation)\n",
        )
        .unwrap();

        let cfg = PrinterConfig {
            registry: RegistryConfig {
                blueprints: vec![".tomet/blueprints/rfc.blueprint.tmt".to_string()],
                ..Default::default()
            },
            ..PrinterConfig::default()
        };

        let mut var_map = HashMap::new();
        var_map.insert(
            "title".to_string(),
            Value::String("New Auth Protocol".into()),
        );
        var_map.insert("author".to_string(), Value::String("Bob".into()));

        let created = create_file_from_blueprint(
            &temp_dir,
            "rfc",
            Path::new("rfcs/001.tmt"),
            &var_map,
            &cfg,
            false,
        )
        .unwrap();

        assert!(created.path.is_file());
        let content = fs::read_to_string(&created.path).unwrap();
        assert!(content.contains("::kind(rfc)"));
        assert!(content.contains("New Auth Protocol"));
        assert!(content.contains("Bob"));
    }

    /// The vault `name` under the temp dir, declaring one blueprint, `note`, with the text `src`.
    fn vault_with_note(name: &str, src: &str) -> (PathBuf, PrinterConfig) {
        let root =
            std::env::temp_dir().join(format!("tm_test_cli_new_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join(".tomet/blueprints")).unwrap();
        fs::write(root.join(".tomet/blueprints/note.blueprint.tmt"), src).unwrap();
        let config = PrinterConfig {
            registry: RegistryConfig {
                blueprints: vec![".tomet/blueprints/note.blueprint.tmt".to_string()],
                ..Default::default()
            },
            ..PrinterConfig::default()
        };
        (root, config)
    }

    #[test]
    fn a_blueprint_with_type_seats_makes_a_document_with_none() {
        let (root, config) = vault_with_note(
            "seats",
            "::kind(blueprint)\n::blueprint(note)\n::id(<$uuid(default: auto)>)\n::meta(\n  created: <$datetime(default: auto)>\n  count: <$int(default: 3)>\n)\n\n=[ Notes of <$date(default: auto)> ]#(notes)\n",
        );

        let created = create_file_from_blueprint(
            &root,
            "note",
            Path::new("notes/a.tmt"),
            &HashMap::new(),
            &config,
            false,
        )
        .unwrap();

        let content = fs::read_to_string(&created.path).unwrap();
        assert!(!content.contains("<$"), "{content}");
        assert!(content.contains("::kind(note)"), "{content}");
        assert!(content.contains("count: 3"), "{content}");
        let id = content
            .split("::id(")
            .nth(1)
            .and_then(|after| after.split(')').next())
            .expect("an id");
        assert!(uuid::Uuid::parse_str(id).is_ok(), "{id}");
        let _ = fs::remove_dir_all(&root);
    }

    /// A seat that cannot be filled stops `tomet new`: it fails with every one of them, and no file
    /// and no directory is left.
    #[test]
    fn a_type_seat_with_no_value_source_stops_the_command_and_writes_nothing() {
        let (root, config) = vault_with_note(
            "seat-stop",
            "::blueprint(note)\n::meta(\n  title: <$string>\n  kind: <$person>\n  id: <$uuid(default: auto)>\n)\n",
        );

        let err = create_file_from_blueprint(
            &root,
            "note",
            Path::new("fresh/notes/a.tmt"),
            &HashMap::new(),
            &config,
            false,
        )
        .unwrap_err();

        let stopped = err
            .downcast_ref::<tomet_workspace::SeatsNotFilled>()
            .expect("the seats are the cause");
        assert_eq!(stopped.problems.len(), 2);
        let said = err.to_string();
        assert!(
            said.contains(":3:10: error: type seat `<$string>`"),
            "{said}"
        );
        assert!(
            said.contains(":4:9: error: type seat `<$person>`"),
            "{said}"
        );
        assert!(said.contains("2 type seats of "), "{said}");
        assert!(!root.join("fresh").exists(), "nothing was written");
        let _ = fs::remove_dir_all(&root);
    }
}
