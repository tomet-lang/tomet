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
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use tomet_ast::Value;
use tomet_compute::eval::{
    DocRef, EvalContext, EvalValue, Generate, MemoryResolver, OffsetGenerate, Phase,
};
use tomet_config::PrinterConfig;
use tomet_semantics::{ElementKind, ValueExt, classify_std_lenient, normalized_element_args};
use tomet_transform::instantiate_blueprint;

/// Every blueprint `config` declares, as `(name, path)`, sorted by name.
///
/// A declared path that is missing or does not name itself is skipped
/// here and reported by [`declaration_errors`]; listing and creating
/// should not fail wholesale because one entry rotted.
pub fn list_blueprints(root: &Path, config: &PrinterConfig) -> Vec<(String, PathBuf)> {
    let mut found: Vec<(String, PathBuf)> = Vec::new();
    for declared in &config.registry.blueprints {
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

    for declared in &config.registry.blueprints {
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
    let mut found = None;
    tomet_tree::for_each_top_level_element(&doc, |el| {
        if found.is_some() || classify_std_lenient(el) != ElementKind::Blueprint {
            return;
        }
        let Some(args) = normalized_element_args(el) else {
            return;
        };
        found = if let Some(s) = args.as_str() {
            Some(s.to_string())
        } else {
            args.get("target")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        };
    });
    found
}

/// Instantiates a blueprint file into formatted Tomet source text for the new
/// document `target`, a path relative to the vault.
///
/// `vars` are the values passed to `tomet new --var key=value`, read in the
/// blueprint as `${vars.key}`; `title` defaults to the file's name. The
/// document being created is `self`, so `${self.filename}` is the new file's
/// name. `${generate.now()}` is written in the offset the settings give
/// `created` (or `modified`), UTC if neither has one.
///
/// An expression that has no value stays in the new document as written, so
/// the person who made it can see what was left to fill in.
pub fn instantiate_blueprint_file(
    blueprint_path: &Path,
    target: &Path,
    vars: &HashMap<String, Value>,
    config: &PrinterConfig,
) -> Result<String> {
    instantiate_blueprint_file_with(blueprint_path, target, vars, config, &generator_for(config))
}

fn instantiate_blueprint_file_with(
    blueprint_path: &Path,
    target: &Path,
    vars: &HashMap<String, Value>,
    config: &PrinterConfig,
    generate: &dyn Generate,
) -> Result<String> {
    let src = fs::read_to_string(blueprint_path)
        .with_context(|| format!("failed to read blueprint at {}", blueprint_path.display()))?;

    let mut doc = tomet_parser::parse_document(&src)
        .map_err(|e| anyhow::anyhow!("failed to parse blueprint: {e}"))?;

    let self_doc = DocRef(target.to_string_lossy().replace('\\', "/"));

    let mut passed: Vec<(String, EvalValue)> = vars
        .iter()
        .map(|(key, value)| (key.clone(), EvalValue::from_ast(value, &self_doc)))
        .collect();
    passed.sort_by(|a, b| a.0.cmp(&b.0));
    if !passed.iter().any(|(key, _)| key == "title") {
        let default_title = target
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Document")
            .to_string();
        passed.push(("title".to_string(), EvalValue::String(default_title)));
    }
    let vars = EvalValue::Map(passed);

    // The new document stands in for itself: a reference to one of its own
    // ids is answered from the blueprint it is being made from.
    let resolver = MemoryResolver::new().with_document(&self_doc.0, Arc::new(doc.clone()));

    let mut macros = tomet_semantics::document_config(&doc).macros;
    for (name, template) in &config.macros {
        macros
            .entry(name.clone())
            .or_insert_with(|| template.clone());
    }

    let cx = EvalContext {
        phase: Phase::Create,
        resolver: &resolver,
        generate,
        self_doc,
        macros: &macros,
        vars: &vars,
    };
    instantiate_blueprint(&mut doc, &cx);

    Ok(tomet_printer::document_to_tm_with_config(&doc, config))
}

/// The clock `generate.now()` reads: the real one, in the offset the settings
/// give a creation date.
fn generator_for(config: &PrinterConfig) -> OffsetGenerate {
    ["created", "modified"]
        .iter()
        .find_map(|field| {
            let offset = config.meta.fields.get(*field)?.offset.as_deref()?;
            OffsetGenerate::from_offset_text(offset)
        })
        .unwrap_or_else(OffsetGenerate::utc)
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

    let content = instantiate_blueprint_file(&blueprint_file, target_rel_path, vars, config)?;
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
            "@kind(blueprint)\n@blueprint(daily-note)\n@meta{\n  id: ${generate.uuid()}\n  title: ${vars.title}\n}\n\n=[ Tasks for ${vars.title} ]#(tasks)\n",
        )
        .unwrap();
        dir
    }

    fn declaring(paths: &[&str]) -> PrinterConfig {
        PrinterConfig {
            registry: tomet_config::RegistryConfig {
                blueprints: paths.iter().map(|p| p.to_string()).collect(),
                ..Default::default()
            },
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

    /// A clock stopped at the epoch and a UUID that is always the same.
    struct Fixed;

    impl Generate for Fixed {
        fn now(&self) -> time::OffsetDateTime {
            time::OffsetDateTime::UNIX_EPOCH
        }
        fn uuid(&self) -> String {
            "00000000-0000-0000-0000-000000000000".to_string()
        }
    }

    fn blueprint_file(name: &str, src: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("tm_blueprint_src_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("x.blueprint.tmt");
        fs::write(&path, src).unwrap();
        path
    }

    #[test]
    fn generated_values_come_from_the_injected_clock_and_random_source() {
        let file = blueprint_file(
            "fixed",
            "@blueprint(note)\n@meta{\n  id: ${generate.uuid()}\n  day: ${generate.now().format(\"YYYY-MM-DD\")}\n  file: ${self.filename}\n  by: ${vars.author}\n}\n",
        );
        let mut vars = HashMap::new();
        vars.insert("author".to_string(), Value::String("Ann".into()));

        let content = instantiate_blueprint_file_with(
            &file,
            Path::new("notes/idea.tmt"),
            &vars,
            &PrinterConfig::default(),
            &Fixed,
        )
        .unwrap();

        assert!(
            content.contains("id: 00000000-0000-0000-0000-000000000000"),
            "{content}"
        );
        assert!(content.contains("day: 1970-01-01"), "{content}");
        assert!(content.contains("file: idea.tmt"), "{content}");
        assert!(content.contains("by: Ann"), "{content}");
        let _ = fs::remove_dir_all(file.parent().unwrap());
    }

    #[test]
    fn the_title_defaults_to_the_files_name_and_can_be_passed() {
        let file = blueprint_file("title", "@blueprint(note)\n\n=[ ${vars.title} ]\n");
        let config = PrinterConfig::default();
        let made = |vars: &HashMap<String, Value>| {
            instantiate_blueprint_file_with(
                &file,
                Path::new("a/my-note.tmt"),
                vars,
                &config,
                &Fixed,
            )
            .unwrap()
        };
        assert!(made(&HashMap::new()).contains("my-note"));
        let mut vars = HashMap::new();
        vars.insert("title".to_string(), Value::String("Given".into()));
        assert!(made(&vars).contains("Given"));
        let _ = fs::remove_dir_all(file.parent().unwrap());
    }

    #[test]
    fn an_expression_with_no_value_is_left_in_the_new_document() {
        let file = blueprint_file("left", "@blueprint(note)\n@meta{ by: ${vars.author} }\n");
        let content = instantiate_blueprint_file_with(
            &file,
            Path::new("n.tmt"),
            &HashMap::new(),
            &PrinterConfig::default(),
            &Fixed,
        )
        .unwrap();
        assert!(content.contains("${vars.author}"), "{content}");
        let _ = fs::remove_dir_all(file.parent().unwrap());
    }

    #[test]
    fn now_is_written_in_the_offset_the_settings_give_created() {
        let mut config = PrinterConfig::default();
        assert_eq!(generator_for(&config).now().offset().whole_hours(), 0);
        config.meta.fields.insert(
            "created".to_string(),
            tomet_config::FieldConfig {
                offset: Some("+09:00".to_string()),
                ..Default::default()
            },
        );
        assert_eq!(generator_for(&config).now().offset().whole_hours(), 9);
    }
}
