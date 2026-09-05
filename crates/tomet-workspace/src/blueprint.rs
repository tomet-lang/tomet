//! Blueprint discovery and file instantiation across a workspace.
//!
//! A vault declares its blueprints in its config, as a list of paths, and
//! each blueprint names itself with `@blueprint(X)`. Nothing is derived
//! from a filename and nothing is searched for by convention.
//!
//! That is a change from what this did, and the reason is in this
//! feature's own history. Lookup used to try `templates/`,
//! `.tomet/templates/` and `docs/examples/blueprints/`, times three
//! filename spellings -- which is how `docs/docs.settings.tmt` came to
//! point at a file that does not exist and still resolve: the misses
//! were invisible and one hit. Folding that to a single convention made
//! the surface smaller but kept the silence. A declared list moves the
//! error to the declaration: a path that is not there is reported once,
//! where it is written, instead of at every document that wanted it.
//!
//! A name -> path map was rejected as a duplicate: the name would appear
//! in the key, in the path, and again in the file's own `@blueprint(X)`.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use tomet_ast::{Block, Value};
use tomet_compute::EvaluationContext;
use tomet_config::PrinterConfig;
use tomet_semantics::{ElementKind, ValueExt, classify_lenient, normalized_element_args};
use tomet_transform::instantiate_blueprint;

/// Every blueprint `config` declares, as `(name, path)`, sorted by name.
///
/// A declared path that is missing or does not name itself is skipped
/// here and reported by [`declaration_errors`]; listing and creating
/// should not fail wholesale because one entry rotted.
pub fn list_blueprints(root: &Path, config: &PrinterConfig) -> Vec<(String, PathBuf)> {
    let mut found: Vec<(String, PathBuf)> = Vec::new();
    for declared in &config.blueprints {
        let path = root.join(declared);
        let Some(name) = blueprint_name(&path) else {
            continue;
        };
        if !found.iter().any(|(existing, _)| *existing == name) {
            found.push((name, path));
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

/// What is wrong with the declared list: paths that do not exist, do not
/// parse, or do not name themselves, and names claimed twice.
///
/// Reported at the declaration, which is the whole point of declaring.
pub fn declaration_errors(root: &Path, config: &PrinterConfig) -> Vec<String> {
    let mut errors = Vec::new();
    let mut seen: HashMap<String, String> = HashMap::new();

    for declared in &config.blueprints {
        let path = root.join(declared);
        if !path.is_file() {
            errors.push(format!("declared blueprint does not exist: {declared}"));
            continue;
        }
        match blueprint_name(&path) {
            None => errors.push(format!(
                "declared blueprint names no target -- it needs `@blueprint(<name>)`: {declared}"
            )),
            Some(name) => {
                if let Some(first) = seen.get(&name) {
                    errors.push(format!(
                        "two blueprints both call themselves `{name}`: {first} and {declared}"
                    ));
                } else {
                    seen.insert(name, declared.clone());
                }
            }
        }
    }
    errors
}

/// The blueprint named `name`, among the ones `config` declares.
pub fn find_blueprint(root: &Path, name: &str, config: &PrinterConfig) -> Option<PathBuf> {
    list_blueprints(root, config)
        .into_iter()
        .find(|(declared, _)| declared == name)
        .map(|(_, path)| path)
}

/// The target a blueprint file names for itself, from its
/// `@blueprint(X)`.
fn blueprint_name(path: &Path) -> Option<String> {
    let src = fs::read_to_string(path).ok()?;
    let doc = tomet_parser::parse_document(&src).ok()?;
    doc.blocks.iter().find_map(|block| {
        let Block::Element(el) = block else {
            return None;
        };
        if classify_lenient(el) != ElementKind::Blueprint {
            return None;
        }
        let args = normalized_element_args(el)?;
        if let Some(s) = args.as_str() {
            return Some(s.to_string());
        }
        args.get("target")
            .and_then(|v| v.as_str())
            .map(str::to_string)
    })
}

/// Instantiates a blueprint file into formatted Tomet source text for `output_path`.
pub fn instantiate_blueprint_file(
    blueprint_path: &Path,
    output_path: &Path,
    vars: &HashMap<String, Value>,
    config: &PrinterConfig,
) -> Result<String> {
    let src = fs::read_to_string(blueprint_path)
        .with_context(|| format!("failed to read blueprint at {}", blueprint_path.display()))?;

    let mut doc = tomet_parser::parse_document(&src)
        .map_err(|e| anyhow::anyhow!("failed to parse blueprint: {e}"))?;

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

/// Creates a new document from a blueprint and writes it to disk.
pub fn create_file_from_blueprint(
    root: &Path,
    blueprint_name: &str,
    target_rel_path: &Path,
    vars: &HashMap<String, Value>,
    config: &PrinterConfig,
    overwrite: bool,
) -> Result<PathBuf> {
    let blueprint_file = find_blueprint(root, blueprint_name, config).ok_or_else(|| {
        let known = list_blueprints(root, config)
            .into_iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>();
        if known.is_empty() {
            anyhow::anyhow!(
                "blueprint '{blueprint_name}' not found: this vault declares none. \
                 Add its path to `blueprints` in the config."
            )
        } else {
            anyhow::anyhow!(
                "blueprint '{blueprint_name}' not found. Declared: {}",
                known.join(", ")
            )
        }
    })?;

    let target_abs_path = root.join(target_rel_path);

    if target_abs_path.exists() && !overwrite {
        bail!("target file already exists: {}", target_abs_path.display());
    }

    if let Some(parent) = target_abs_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directories for {}", parent.display()))?;
    }

    let content = instantiate_blueprint_file(&blueprint_file, &target_abs_path, vars, config)?;
    fs::write(&target_abs_path, content)
        .with_context(|| format!("failed to write file {}", target_abs_path.display()))?;

    Ok(target_abs_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tm_blueprint_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(".tomet/blueprints")).unwrap();
        fs::write(
            dir.join(".tomet/blueprints/daily-note.blueprint.tmt"),
            "@kind(blueprint)\n@blueprint(daily-note)\n@meta{\n  id: ${uuid(\"nil\")}\n  title: ${title}\n}\n\n#[ Tasks for ${title} ] {id: tasks}\n",
        )
        .unwrap();
        dir
    }

    fn declaring(paths: &[&str]) -> PrinterConfig {
        PrinterConfig {
            blueprints: paths.iter().map(|p| p.to_string()).collect(),
            ..PrinterConfig::default()
        }
    }

    #[test]
    fn a_declared_blueprint_is_found_by_the_name_it_gives_itself() {
        let root = vault("found");
        let config = declaring(&[".tomet/blueprints/daily-note.blueprint.tmt"]);

        assert_eq!(
            list_blueprints(&root, &config)
                .into_iter()
                .map(|(n, _)| n)
                .collect::<Vec<_>>(),
            ["daily-note"]
        );
        assert!(find_blueprint(&root, "daily-note", &config).is_some());
        assert!(declaration_errors(&root, &config).is_empty());

        let _ = fs::remove_dir_all(&root);
    }

    /// The file is on disk and spelled the way the convention used to
    /// expect. Nothing declares it, so nothing finds it -- which is the
    /// point: what exists is what was declared, not what was dropped in a
    /// directory.
    #[test]
    fn an_undeclared_file_is_not_a_blueprint() {
        let root = vault("undeclared");
        let config = PrinterConfig::default();

        assert!(list_blueprints(&root, &config).is_empty());
        assert!(find_blueprint(&root, "daily-note", &config).is_none());

        let _ = fs::remove_dir_all(&root);
    }

    /// A typo in a declared path is reported where it is written, once --
    /// not at every document that wanted that blueprint.
    #[test]
    fn a_declared_path_that_is_not_there_is_reported_at_the_declaration() {
        let root = vault("typo");
        let config = declaring(&[".tomet/blueprints/daily-note.blueprnit.tmt"]);

        let errors = declaration_errors(&root, &config);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("does not exist"), "{errors:?}");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn instantiating_a_declared_blueprint_writes_the_document() {
        let root = vault("create");
        let config = declaring(&[".tomet/blueprints/daily-note.blueprint.tmt"]);

        let mut vars = HashMap::new();
        vars.insert(
            "title".to_string(),
            Value::String("2026-09-01 Daily".into()),
        );

        let created = create_file_from_blueprint(
            &root,
            "daily-note",
            Path::new("notes/2026-09-01.tmt"),
            &vars,
            &config,
            false,
        )
        .unwrap();

        let content = fs::read_to_string(&created).unwrap();
        assert!(content.contains("@kind(daily-note)"));
        assert!(content.contains("Tasks for 2026-09-01 Daily"));
        assert!(!content.contains("@blueprint"));

        let _ = fs::remove_dir_all(&root);
    }
}
