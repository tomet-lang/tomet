//! Blueprint discovery and file instantiation across a workspace.
//!
//! A vault declares its blueprints in its config, as a list of paths, and
//! each blueprint names itself with `::blueprint(X)`. Nothing is derived
//! from a filename and nothing is searched for by convention.
//!
//! The reason is the silence a search has. A lookup that tries
//! `templates/`, `.tomet/templates/` and `docs/examples/blueprints/`,
//! times three filename spellings, lets a settings file point at a file
//! that does not exist and still resolve: the misses are invisible and
//! one hits. Folding that to a single convention would make the surface
//! smaller but keep the silence. A declared list moves the
//! error to the declaration: a path that is not there is reported once,
//! where it is written, instead of at every document that wanted it.
//!
//! A name -> path map was rejected as a duplicate: the name would appear
//! in the key, in the path, and again in the file's own `::blueprint(X)`.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use tomet_ast::Value;
use tomet_config::PrinterConfig;
use tomet_expr::{DocRef, EvalContext, MemoryResolver};
use tomet_position::{LineCol, LineIndex};
use tomet_transform::{SeatProblem, Source, SystemSource, instantiate_blueprint};

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
                "declared blueprint names no target -- it needs `::blueprint(<name>)`: {declared}"
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
/// `::blueprint(X)`.
fn blueprint_name(path: &Path) -> Option<String> {
    let src = fs::read_to_string(path).ok()?;
    let doc = tomet_ast_parser::parse_file(&src).ok()?;
    tomet_declaration::document_blueprint(&doc)
}

/// An expression in a blueprint that had no value, and so stands in the new
/// document as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unfilled {
    /// The expression as written, without the surrounding `{$ ` and ` }`.
    pub source: String,
    /// The 1-based line it is written on in the blueprint file.
    pub line: usize,
    /// Why it has no value.
    pub reason: String,
    /// One line to show: what `tomet check` shows for the same thing.
    pub message: String,
}

/// A new document made from a blueprint, not yet written anywhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instantiation {
    /// The document, as Tomet source text.
    pub content: String,
    /// What it was left to fill in, in the order the blueprint has them.
    pub unfilled: Vec<Unfilled>,
}

/// A new document written to disk by [`create_file_from_blueprint`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Created {
    /// Where the new document is.
    pub path: PathBuf,
    /// The blueprint file it was made from, which [`Unfilled::line`] counts in.
    pub blueprint: PathBuf,
    /// What the new document was left to fill in.
    pub unfilled: Vec<Unfilled>,
}

/// A blueprint that cannot be made into a document, because some of its type seats cannot be
/// filled. Nothing is made from it and nothing is written.
///
/// This is the error [`instantiate_blueprint_file`] and [`create_file_from_blueprint`] fail with
/// for that cause, found with `downcast_ref` on the [`anyhow::Error`]; its `Display` is every
/// problem on a line of its own, in the form `tomet check` reports them, and a last line that
/// counts them.
#[derive(Debug, Clone, PartialEq)]
pub struct SeatsNotFilled {
    /// The blueprint file the seats are written in.
    pub blueprint: PathBuf,
    /// Every seat that cannot be filled, in the order the blueprint has them.
    pub problems: Vec<LocatedSeatProblem>,
}

/// A seat that cannot be filled, and where it is written in the blueprint file.
#[derive(Debug, Clone, PartialEq)]
pub struct LocatedSeatProblem {
    /// Where the seat begins: a 1-based line and column.
    pub at: LineCol,
    pub problem: SeatProblem,
}

impl SeatsNotFilled {
    fn new(blueprint: &Path, src: &str, problems: Vec<SeatProblem>) -> Self {
        let index = LineIndex::new(src);
        SeatsNotFilled {
            blueprint: blueprint.to_path_buf(),
            problems: problems
                .into_iter()
                .map(|problem| LocatedSeatProblem {
                    at: index.line_col(problem.span.start),
                    problem,
                })
                .collect(),
        }
    }
}

impl std::fmt::Display for SeatsNotFilled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let blueprint = self.blueprint.display();
        for found in &self.problems {
            writeln!(
                f,
                "{blueprint}:{}:{}: error: {}",
                found.at.line, found.at.column, found.problem
            )?;
        }
        write!(
            f,
            "{} of {blueprint} cannot be filled; no document was made",
            match self.problems.len() {
                1 => "1 type seat".to_string(),
                n => format!("{n} type seats"),
            }
        )
    }
}

impl std::error::Error for SeatsNotFilled {}

/// The clock and the random source a blueprint's `default: auto` is made from: the real ones, with
/// the moment written in the offset the settings give `created` (or `modified`), UTC if neither
/// has one.
pub fn source_for(config: &PrinterConfig) -> SystemSource {
    ["created", "modified"]
        .iter()
        .find_map(|field| {
            let offset = config.meta.fields.get(*field)?.offset.as_deref()?;
            SystemSource::in_offset(offset)
        })
        .unwrap_or_else(SystemSource::utc)
}

/// Instantiates a blueprint file into formatted Tomet source text for the new
/// document `target`, a path relative to the vault, with the real clock and random source
/// ([`source_for`]).
///
/// See [`instantiate_blueprint_file_with`], which is this with the source given.
pub fn instantiate_blueprint_file(
    blueprint_path: &Path,
    target: &Path,
    vars: &HashMap<String, Value>,
    config: &PrinterConfig,
) -> Result<Instantiation> {
    instantiate_blueprint_file_with(blueprint_path, target, vars, config, &source_for(config))
}

/// Instantiates a blueprint file into formatted Tomet source text for the new
/// document `target`, a path relative to the vault.
///
/// `vars` are the values passed to `tomet new --var key=value`, read in the
/// blueprint as `$(var.key)`. A name nothing declares is refused, and so is a
/// variable that has neither a value nor a `default`: the blueprint's `::var`
/// (over the vault's) says which there are. The document being created is
/// `self`, so `$(self::filename)` is the new file's name.
///
/// Every type seat (`<$uuid(default: auto)>`) is filled from its own source, and `source` is
/// where the `default: auto` ones come from. A seat that cannot be filled fails the making with
/// [`SeatsNotFilled`], which says every one of them.
///
/// An expression that has no value stays in the new document as written, so
/// the person who made it can see what was left to fill in; each one is also
/// returned in [`Instantiation::unfilled`], so the caller can say so.
pub fn instantiate_blueprint_file_with(
    blueprint_path: &Path,
    target: &Path,
    vars: &HashMap<String, Value>,
    config: &PrinterConfig,
    source: &dyn Source,
) -> Result<Instantiation> {
    let src = fs::read_to_string(blueprint_path)
        .with_context(|| format!("failed to read blueprint at {}", blueprint_path.display()))?;

    let mut doc = tomet_ast_parser::parse_file(&src)
        .map_err(|e| anyhow::anyhow!("failed to parse blueprint: {e}"))?;

    let self_doc = DocRef(target.to_string_lossy().replace('\\', "/"));

    // The blueprint's own declarations win over the vault's.
    let blueprint_config = tomet_semantics::document_config(&doc);
    let mut declared = blueprint_config.vars.clone();
    for (name, decl) in &config.vars {
        if !declared.iter().any(|(held, _)| held == name) {
            declared.push((name.clone(), decl.clone()));
        }
    }
    let mut passed: Vec<(String, Value)> = vars
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    passed.sort_by(|a, b| a.0.cmp(&b.0));
    let scope = tomet_expr::var_scope(&declared, &self_doc, &passed)?;
    if !scope.unset.is_empty() {
        let names = scope.unset.join(", ");
        bail!(
            "no value for {names}: pass `--var {}=...`, or give it a `default` in the blueprint's `::var`",
            scope.unset[0]
        );
    }

    // The new document stands in for itself: a reference to one of its own
    // ids is answered from the blueprint it is being made from.
    let resolver = MemoryResolver::new().with_document(&self_doc.0, Arc::new(doc.clone()));

    let mut macros = blueprint_config.macros;
    for (name, template) in &config.macros {
        macros
            .entry(name.clone())
            .or_insert_with(|| template.clone());
    }

    let cx = EvalContext {
        resolver: &resolver,
        self_doc,
        macros: &macros,
        var: &scope.values,
        var_unset: &scope.unset,
        includes: &[],
        heading_level: 0,
        notes: Default::default(),
    };
    let instantiated = instantiate_blueprint(&mut doc, &cx, source)
        .map_err(|problems| SeatsNotFilled::new(blueprint_path, &src, problems))?;

    // The spans belong to the blueprint as it was read, not to the document
    // about to be printed, so the line is a line of the blueprint.
    let unfilled = instantiated
        .unresolved
        .into_iter()
        .map(|u| Unfilled {
            line: 1 + src[..u.span.start.min(src.len())].matches('\n').count(),
            message: u.message(),
            source: u.source,
            reason: u.reason,
        })
        .collect();

    Ok(Instantiation {
        content: tomet_printer::document_to_tm_with_config(&doc, config),
        unfilled,
    })
}

/// Creates a new document from a blueprint and writes it to disk, with the real clock and random
/// source ([`source_for`]).
///
/// See [`create_file_from_blueprint_with`], which is this with the source given.
pub fn create_file_from_blueprint(
    root: &Path,
    blueprint_name: &str,
    target_rel_path: &Path,
    vars: &HashMap<String, Value>,
    config: &PrinterConfig,
    overwrite: bool,
) -> Result<Created> {
    create_file_from_blueprint_with(
        root,
        blueprint_name,
        target_rel_path,
        vars,
        config,
        overwrite,
        &source_for(config),
    )
}

/// Creates a new document from a blueprint and writes it to disk.
///
/// What the document was left to fill in is in [`Created::unfilled`]; it is
/// written anyway, since the expressions are still there to be filled in by
/// hand. A type seat that cannot be filled is not like that: it fails the making with
/// [`SeatsNotFilled`], and then nothing is written -- no file, and no directory made for one.
pub fn create_file_from_blueprint_with(
    root: &Path,
    blueprint_name: &str,
    target_rel_path: &Path,
    vars: &HashMap<String, Value>,
    config: &PrinterConfig,
    overwrite: bool,
    source: &dyn Source,
) -> Result<Created> {
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

    // Made first: a document that cannot be made leaves nothing behind, not even its directory.
    let made =
        instantiate_blueprint_file_with(&blueprint_file, target_rel_path, vars, config, source)?;

    if let Some(parent) = target_abs_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directories for {}", parent.display()))?;
    }
    fs::write(&target_abs_path, made.content)
        .with_context(|| format!("failed to write file {}", target_abs_path.display()))?;

    Ok(Created {
        path: target_abs_path,
        blueprint: blueprint_file,
        unfilled: made.unfilled,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_transform::{FixedSource, SystemSource};

    fn vault(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tm_blueprint_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(".tomet/blueprints")).unwrap();
        fs::write(
            dir.join(".tomet/blueprints/daily-note.blueprint.tmt"),
            "::kind(blueprint)\n::blueprint(daily-note)\n::var(\n  title: { type: string }\n)\n::meta(\n  title: $(var.title)\n)\n\n=[ Tasks for $(var.title) ]#(tasks)\n",
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

    /// The file is on disk and spelled the way a search by convention
    /// would expect. Nothing declares it, so nothing finds it -- which is the
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

        assert!(created.unfilled.is_empty(), "{:?}", created.unfilled);
        let content = fs::read_to_string(&created.path).unwrap();
        assert!(content.contains("::kind(daily-note)"));
        assert!(content.contains("Tasks for 2026-09-01 Daily"));
        assert!(!content.contains("::blueprint"));

        let _ = fs::remove_dir_all(&root);
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
    fn self_is_the_new_document_and_var_is_what_was_passed() {
        let file = blueprint_file(
            "self",
            "::blueprint(note)\n::var(\n  author: { type: string }\n)\n::meta(\n  file: $(self::filename)\n  by: $(var.author)\n)\n",
        );
        let mut vars = HashMap::new();
        vars.insert("author".to_string(), Value::String("Ann".into()));

        let content = instantiate_blueprint_file(
            &file,
            Path::new("notes/idea.tmt"),
            &vars,
            &PrinterConfig::default(),
        )
        .unwrap()
        .content;

        assert!(content.contains("file: idea.tmt"), "{content}");
        assert!(content.contains("by: Ann"), "{content}");
        let _ = fs::remove_dir_all(file.parent().unwrap());
    }

    #[test]
    fn a_variable_takes_its_default_and_a_passed_value_wins() {
        let file = blueprint_file(
            "title",
            "::blueprint(note)\n::var(\n  title: { default: \"Untitled\" }\n)\n\n=[ $(var.title) ]\n",
        );
        let config = PrinterConfig::default();
        let made = |vars: &HashMap<String, Value>| {
            instantiate_blueprint_file(&file, Path::new("a/my-note.tmt"), vars, &config)
                .unwrap()
                .content
        };
        assert!(made(&HashMap::new()).contains("Untitled"));
        let mut vars = HashMap::new();
        vars.insert("title".to_string(), Value::String("Given".into()));
        assert!(made(&vars).contains("Given"));
        let _ = fs::remove_dir_all(file.parent().unwrap());
    }

    #[test]
    fn an_expression_with_no_value_is_left_in_the_new_document() {
        let file = blueprint_file("left", "::blueprint(note)\n::meta(by: $(var.author))\n");
        let made = instantiate_blueprint_file(
            &file,
            Path::new("n.tmt"),
            &HashMap::new(),
            &PrinterConfig::default(),
        )
        .unwrap();
        assert!(made.content.contains("$(var.author)"), "{}", made.content);
        let _ = fs::remove_dir_all(file.parent().unwrap());
    }

    /// What is left is also said, with the line of the blueprint it is
    /// written on -- the new document has no line to point at that the person
    /// did not already have to search for.
    #[test]
    fn an_expression_with_no_value_is_reported_with_its_line_in_the_blueprint() {
        let file = blueprint_file(
            "reported",
            "::blueprint(note)\n\n::meta(\n  by: $(var.author)\n)\n",
        );
        let made = instantiate_blueprint_file(
            &file,
            Path::new("n.tmt"),
            &HashMap::new(),
            &PrinterConfig::default(),
        )
        .unwrap();
        assert_eq!(made.unfilled.len(), 1, "{:?}", made.unfilled);
        assert_eq!(made.unfilled[0].source, "var.author");
        assert_eq!(made.unfilled[0].line, 4);
        assert!(
            made.unfilled[0].reason.contains("author"),
            "{}",
            made.unfilled[0].reason
        );
        let _ = fs::remove_dir_all(file.parent().unwrap());
    }

    fn made_with(name: &str, src: &str, vars: &[(&str, &str)]) -> anyhow::Result<String> {
        let file = blueprint_file(name, src);
        let vars: HashMap<String, Value> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), Value::String(v.to_string())))
            .collect();
        let made =
            instantiate_blueprint_file(&file, Path::new("n.tmt"), &vars, &PrinterConfig::default());
        let _ = fs::remove_dir_all(file.parent().unwrap());
        made.map(|m| m.content)
    }

    #[test]
    fn a_required_variable_that_is_not_passed_stops_the_creation() {
        let src = "::blueprint(note)\n::var(\n  title: { type: string }\n)\n\n=[ $(var.title) ]\n";
        let err = made_with("required", src, &[]).unwrap_err().to_string();
        assert!(
            err.contains("title") && err.contains("--var title"),
            "{err}"
        );
        assert!(
            made_with("required-given", src, &[("title", "Hi")])
                .unwrap()
                .contains("Hi")
        );
    }

    #[test]
    fn a_name_nothing_declares_is_refused() {
        let src = "::blueprint(note)\n::var(\n  title: { default: \"x\" }\n)\n";
        let err = made_with("undeclared", src, &[("titel", "Hi")])
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("titel") && err.contains("declared: title"),
            "{err}"
        );
    }

    #[test]
    fn a_passed_value_is_converted_to_the_declared_type() {
        let src =
            "::blueprint(note)\n::var(\n  n: { type: int }\n)\n::meta(count: $add(var.n, 1))\n";
        assert!(
            made_with("typed", src, &[("n", "41")])
                .unwrap()
                .contains("count: 42")
        );
        let err = made_with("typed-bad", src, &[("n", "many")])
            .unwrap_err()
            .to_string();
        assert!(err.contains("not an integer"), "{err}");
    }

    #[test]
    fn the_blueprints_vars_do_not_go_into_the_new_document() {
        let src =
            "::blueprint(note)\n::var(\n  title: { default: \"x\" }\n)\n\n=[ $(var.title) ]\n";
        let content = made_with("dropped", src, &[]).unwrap();
        assert!(!content.contains("::var"), "{content}");
        assert!(content.contains("::kind(note)"), "{content}");
    }

    #[test]
    fn creating_a_file_reports_what_it_was_left_to_fill_in() {
        let root = vault("unfilled");
        fs::write(
            root.join(".tomet/blueprints/daily-note.blueprint.tmt"),
            "::blueprint(daily-note)\n::meta(by: $(var.author))\n",
        )
        .unwrap();
        let config = declaring(&[".tomet/blueprints/daily-note.blueprint.tmt"]);

        let created = create_file_from_blueprint(
            &root,
            "daily-note",
            Path::new("n.tmt"),
            &HashMap::new(),
            &config,
            false,
        )
        .unwrap();

        assert!(created.path.is_file());
        assert!(created.blueprint.ends_with("daily-note.blueprint.tmt"));
        assert_eq!(created.unfilled.len(), 1, "{:?}", created.unfilled);
        assert_eq!(created.unfilled[0].line, 2);
        let _ = fs::remove_dir_all(&root);
    }

    // ---- type seats ----------------------------------------------------------------------

    const NOW: &str = "2026-11-11T09:30:00+09:00";
    const UUID: &str = "00000000-0000-0000-0000-000000000000";

    /// A clock stopped at [`NOW`] and the ids to draw, in order.
    fn fixed(ids: &[&str]) -> FixedSource {
        FixedSource::at(NOW)
            .expect("a datetime")
            .with_ids(ids.iter().copied())
    }

    fn instantiated(name: &str, src: &str, source: &dyn Source) -> Result<String> {
        let file = blueprint_file(name, src);
        let made = instantiate_blueprint_file_with(
            &file,
            Path::new("notes/idea.tmt"),
            &HashMap::new(),
            &PrinterConfig::default(),
            source,
        );
        let _ = fs::remove_dir_all(file.parent().unwrap());
        made.map(|m| m.content)
    }

    #[test]
    fn seats_are_filled_from_the_injected_clock_and_random_source() {
        let content = instantiated(
            "seats-fixed",
            "::blueprint(note)\n::id(<$uuid(default: auto)>)\n::meta(\n  created: <$datetime(default: auto)>\n  day: <$date(default: auto)>\n  file: $(self::filename)\n)\n",
            &fixed(&[UUID]),
        )
        .unwrap();

        assert!(content.contains(&format!("::id({UUID})")), "{content}");
        assert!(
            content.contains("created: 2026-11-11T09:30:00+09:00"),
            "{content}"
        );
        assert!(content.contains("day: 2026-11-11"), "{content}");
        assert!(content.contains("file: idea.tmt"), "{content}");
        assert!(!content.contains("<$"), "{content}");
    }

    #[test]
    fn a_nanoid_seat_is_drawn_with_the_length_and_the_prefix_it_is_written_with() {
        let content = instantiated(
            "seats-nanoid",
            "::blueprint(note)\n::meta(\n  short: <$nanoid(length: 8, prefix: \"note-\", default: auto)>\n)\n",
            &fixed(&["note-ab12cd34"]),
        )
        .unwrap();
        assert!(content.contains("short: note-ab12cd34"), "{content}");

        // Drawn for real, it has the shape the type gives.
        let content = instantiated(
            "seats-nanoid-real",
            "::blueprint(note)\n::meta(\n  short: <$nanoid(length: 8, prefix: \"note-\", default: auto)>\n)\n",
            &SystemSource::utc(),
        )
        .unwrap();
        let id = content
            .split("short: ")
            .nth(1)
            .and_then(|after| after.split([')', ',', '\n']).next())
            .expect("a short id");
        assert!(
            id.starts_with("note-") && id.len() == "note-".len() + 8,
            "{id}"
        );
    }

    #[test]
    fn a_default_written_out_is_the_value_of_its_seat() {
        let content = instantiated(
            "seats-given",
            "::blueprint(note)\n::var(\n  tags: { type: list(string), default: (\"a\", \"b\") }\n)\n::meta(\n  count: <$int(default: 3)>\n  title: <$string(default: \"Untitled\")>\n  tags: <$list(string, length: 2, default: var.tags)>\n)\n",
            &fixed(&[]),
        )
        .unwrap();
        assert!(content.contains("count: 3"), "{content}");
        assert!(content.contains("title: Untitled"), "{content}");
        assert!(content.contains("tags: (a, b)"), "{content}");
    }

    #[test]
    fn a_seat_in_a_heading_and_in_prose_is_its_text() {
        let content = instantiated(
            "seats-text",
            "::blueprint(note)\n\n=[ Notes of <$date(default: auto)> ]#(notes)\n\nWritten on <$date(default: auto)> at <$datetime(default: auto)>.\n",
            &fixed(&[]),
        )
        .unwrap();
        assert!(
            content.contains("=[Notes of 2026-11-11] #(notes)"),
            "{content}"
        );
        assert!(
            content.contains("Written on 2026-11-11 at 2026-11-11T09:30:00+09:00."),
            "{content}"
        );
    }

    /// A seat with no value source stops the making, and the blueprint's own line and column
    /// are in what is said, for every one of them.
    #[test]
    fn seats_that_cannot_be_filled_are_all_reported_with_their_place() {
        let src = "::blueprint(note)\n::meta(\n  title: <$string>\n  kind: <$person>\n  id: <$uuid(default: auto)>\n)\n\n=[ <$int(default: \"x\")> ]\n";
        let err = instantiated("seats-stop", src, &fixed(&[UUID])).unwrap_err();
        let stopped = err
            .downcast_ref::<SeatsNotFilled>()
            .unwrap_or_else(|| panic!("a SeatsNotFilled, got {err:?}"));
        let places: Vec<(usize, usize)> = stopped
            .problems
            .iter()
            .map(|found| (found.at.line, found.at.column))
            .collect();
        assert_eq!(places, [(3, 10), (4, 9), (8, 4)]);

        let shown = err.to_string();
        let lines: Vec<&str> = shown.lines().collect();
        assert_eq!(lines.len(), 4, "{shown}");
        assert!(lines[0].ends_with(":3:10: error: type seat `<$string>`: this seat has no value source: it has no `default`, so nothing says what goes in it"), "{shown}");
        assert!(
            lines[1].contains(
                ":4:9: error: type seat `<$person>`: `person` is not a type; the types are `string`, `int`"
            ),
            "{shown}"
        );
        assert!(
            lines[2].ends_with(
                ":8:4: error: type seat `<$int(default: \"x\")>`: `default`: `x` is not an integer"
            ),
            "{shown}"
        );
        assert!(lines[3].starts_with("3 type seats of "), "{shown}");
        assert!(
            lines[3].ends_with("cannot be filled; no document was made"),
            "{shown}"
        );
    }

    #[test]
    fn the_seats_not_filled_message_names_the_file_and_counts() {
        let problem = |seat: &str, offset: usize, reason: tomet_transform::SeatReason| {
            tomet_transform::SeatProblem {
                seat: seat.to_string(),
                span: tomet_span::Span::new(offset, offset + seat.len()),
                reason,
            }
        };
        let src = "::blueprint(note)\n::meta(a: <$string>)\n";
        let one = SeatsNotFilled::new(
            Path::new("bp/note.blueprint.tmt"),
            src,
            vec![problem(
                "<$string>",
                src.find("<$string>").unwrap(),
                tomet_transform::SeatReason::NoValueSource,
            )],
        );
        assert_eq!(
            one.to_string(),
            "bp/note.blueprint.tmt:2:11: error: type seat `<$string>`: this seat has no value \
             source: it has no `default`, so nothing says what goes in it\n\
             1 type seat of bp/note.blueprint.tmt cannot be filled; no document was made"
        );
    }

    /// The failure is the cause and not a message: `tomet new` can tell it from the rest.
    #[test]
    fn a_stop_for_a_seat_is_told_from_one_for_a_variable() {
        let seat = instantiated(
            "seats-kind",
            "::blueprint(note)\n::meta(a: <$string>)\n",
            &fixed(&[]),
        )
        .unwrap_err();
        assert!(seat.downcast_ref::<SeatsNotFilled>().is_some());
        let var = instantiated(
            "seats-kind-var",
            "::blueprint(note)\n::var(\n  title: { type: string }\n)\n::meta(a: $(var.title))\n",
            &fixed(&[]),
        )
        .unwrap_err();
        assert!(var.downcast_ref::<SeatsNotFilled>().is_none());
    }

    #[test]
    fn creating_from_a_blueprint_with_seats_writes_the_filled_document() {
        let root = vault("seats-create");
        fs::write(
            root.join(".tomet/blueprints/daily-note.blueprint.tmt"),
            "::kind(blueprint)\n::blueprint(daily-note)\n::id(<$uuid(default: auto)>)\n::meta(\n  uuid: <$uuid(default: auto)>\n  created: <$datetime(default: auto)>\n)\n\n=[ Notes of <$date(default: auto)> ]#(notes)\n",
        )
        .unwrap();
        let config = declaring(&[".tomet/blueprints/daily-note.blueprint.tmt"]);

        let created = create_file_from_blueprint_with(
            &root,
            "daily-note",
            Path::new("notes/2026-11-11.tmt"),
            &HashMap::new(),
            &config,
            false,
            &fixed(&[UUID, "11111111-1111-1111-1111-111111111111"]),
        )
        .unwrap();

        let content = fs::read_to_string(&created.path).unwrap();
        assert!(content.contains("::kind(daily-note)"), "{content}");
        assert!(content.contains(&format!("::id({UUID})")), "{content}");
        assert!(
            content.contains("uuid: 11111111-1111-1111-1111-111111111111"),
            "{content}"
        );
        assert!(
            content.contains("created: 2026-11-11T09:30:00+09:00"),
            "{content}"
        );
        assert!(
            content.contains("=[Notes of 2026-11-11] #(notes)"),
            "{content}"
        );
        assert!(!content.contains("<$"), "{content}");
        assert!(created.unfilled.is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    /// A document that cannot be made leaves nothing behind: no file, and no directory made for it.
    #[test]
    fn a_failed_creation_writes_nothing() {
        let root = vault("seats-nothing");
        fs::write(
            root.join(".tomet/blueprints/daily-note.blueprint.tmt"),
            "::blueprint(daily-note)\n::meta(title: <$string>)\n",
        )
        .unwrap();
        let config = declaring(&[".tomet/blueprints/daily-note.blueprint.tmt"]);

        let err = create_file_from_blueprint_with(
            &root,
            "daily-note",
            Path::new("deeper/notes/n.tmt"),
            &HashMap::new(),
            &config,
            false,
            &fixed(&[]),
        )
        .unwrap_err();

        assert!(err.downcast_ref::<SeatsNotFilled>().is_some(), "{err:?}");
        assert!(!root.join("deeper").exists(), "no directory was made");
        assert!(!root.join("deeper/notes/n.tmt").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_real_source_is_written_in_the_offset_the_settings_give_created() {
        let mut config = PrinterConfig::default();
        assert_eq!(source_for(&config).now().offset().whole_hours(), 0);
        config.meta.fields.insert(
            "created".to_string(),
            tomet_config::FieldConfig {
                offset: Some("+09:00".to_string()),
                ..Default::default()
            },
        );
        assert_eq!(source_for(&config).now().offset().whole_hours(), 9);
        // `modified` is the fallback, and what is no offset is passed over.
        let mut config = PrinterConfig::default();
        config.meta.fields.insert(
            "created".to_string(),
            tomet_config::FieldConfig {
                offset: Some("later".to_string()),
                ..Default::default()
            },
        );
        config.meta.fields.insert(
            "modified".to_string(),
            tomet_config::FieldConfig {
                offset: Some("-05:00".to_string()),
                ..Default::default()
            },
        );
        assert_eq!(source_for(&config).now().offset().whole_hours(), -5);
    }
}
