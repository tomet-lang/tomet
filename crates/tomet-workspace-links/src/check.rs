//! Resolves a vault's `File`/`Embed`/`Tm`/`Ref` links against what
//! actually exists on disk, and reports the result.
//!
//! Resolution rule for `File`/`Embed` targets (decided by this project's
//! author): a target starting with `./` or `../` is relative to the
//! *referencing file's own directory*; a target starting with `/` is an
//! OS-absolute path; anything else (a bare relative-looking string, e.g.
//! `docs/asdf`) is relative to the *project root*. One combination is
//! explicitly undecided: a `file:` value starting with `/` -- this code
//! applies the general `/`-means-OS-absolute rule to it for now, but
//! that's a placeholder, not a confirmed-correct behavior for that
//! specific case, pending a decision.
//!
//! `Tm` targets are project-root-relative like `File`, so they reuse the
//! same resolution rule -- with any `#fragment` stripped first (the
//! fragment names an id *inside* the target document; validating that
//! the id actually exists there is out of scope for this pass, which
//! only asks whether the file is there).
//!
//! `Ref` targets are not paths at all: they're resolved by searching
//! the vault for any existing file whose name or stem matches the target
//! string (the common wikilink convention -- readers don't type
//! extensions).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use tomet_ast::Span;
use tomet_config::PrinterConfig;
use tomet_indexer::{normalize_join, resolve_document_relative};

use crate::cache::LinkCache;
use crate::collect::LinkKind;

#[derive(Debug, Clone)]
pub struct BrokenLink {
    pub source: PathBuf,
    pub target: String,
    pub span: Span,
}

#[derive(Debug, Clone, Default)]
pub struct CheckReport {
    pub broken: Vec<BrokenLink>,
    pub files_scanned: usize,
    pub links_checked: usize,
    /// Files that couldn't be read/parsed, with their error message.
    /// Doesn't abort the whole check -- one unreadable/unparseable file
    /// (e.g. content the parser doesn't yet support) shouldn't prevent
    /// checking the rest of the vault, matching `apps/cli`'s
    /// `export_directory`'s "report and keep going" convention.
    pub errors: Vec<(PathBuf, String)>,
}

fn is_external(target: &str) -> bool {
    target.contains("://")
}

/// The rule itself lives in `tomet-indexer` because it is not a link
/// rule: it is what a relative path written inside a document means,
/// and `@config(export: path)` needs the same answer.
///
/// (For `file:` specifically, a leading `/` is a placeholder pending a
/// decision -- see this module's doc.)
fn resolve_file_target(source: &Path, target: &str, project_root: &Path) -> PathBuf {
    resolve_document_relative(source, target, project_root)
}

/// Puts a resolved target into the same form as the walked file set.
///
/// `existing` is built by walking `project_root`, so its entries carry
/// whatever form that path has. A target resolved through the referencing
/// file's own directory carries the form of *that* path instead, and the
/// two stop matching the moment they differ -- which is why
/// `tomet check-links docs/README.tmt` used to call `../tests/SYNTAX.md`
/// broken while the same link resolved when the whole repository was
/// scanned. `Path::join` leaves an already-absolute path alone, so this is
/// a no-op when both sides are absolute.
fn rebase_on_project_root(resolved: PathBuf, project_root: &Path) -> PathBuf {
    if resolved.is_absolute() {
        resolved
    } else {
        normalize_join(project_root, &resolved)
    }
}

/// Whether `target` resolves to a path in the vault of the wanted kind.
///
/// Two questions, and both have to hold. `existing` says the path is in
/// the vault at all -- it honours `.gitignore` and `workspace.ignore`, so
/// a path on disk that the vault excludes is not a target. `wanted` says
/// it is the kind the scheme claimed.
///
/// The second half is what `file:` was missing. The set carries
/// directories as well as files, so `file:./spec/` resolved and the
/// checker reported a file where there was a directory. `dir:` exists so
/// the writer says which, and this is where the saying is held to.
fn exists_as(
    target: &str,
    from: &Path,
    project_root: &Path,
    existing: &HashSet<PathBuf>,
    wanted: fn(&Path) -> bool,
) -> bool {
    let path = rebase_on_project_root(resolve_file_target(from, target, project_root), project_root);
    existing.contains(&path) && wanted(&path)
}

/// Whether `target` names any existing file by full filename or stem
/// (extension-agnostic, the usual wikilink convention).
fn resolve_ref_target(target: &str, existing: &HashSet<PathBuf>) -> bool {
    existing.iter().any(|p| {
        p.file_name().and_then(|n| n.to_str()) == Some(target)
            || p.file_stem().and_then(|n| n.to_str()) == Some(target)
    })
}

/// Enumerates `.tmt`/`.tmt` files under `root`, pulls each one's links
/// through `cache` (re-parsing only what changed since the last run),
/// and resolves every `File`/`Embed`/`Tm`/`Ref` target against every
/// non-ignored path that currently exists under `root`. `project_root`
/// is the base for bare (no `./`/`../`/leading-`/`) `File`/`Embed`/`Tm`
/// targets -- typically the same `config_root` `PrinterConfig` was
/// loaded from.
pub fn check_vault(
    root: &Path,
    config: &PrinterConfig,
    config_root: &Path,
    project_root: &Path,
    cache: &mut LinkCache,
) -> CheckReport {
    let files = tomet_indexer::collect_tm_files_with_config(root, config, config_root);
    // Built from `project_root`, not `root`: what exists on disk does not
    // depend on what was asked to be checked. Building it from `root` made
    // single-file mode report every link as broken, since the set then held
    // exactly the one file being checked.
    let existing = tomet_indexer::collect_all_paths_with_config(project_root, config, config_root);

    let mut report = CheckReport {
        files_scanned: files.len(),
        ..Default::default()
    };

    for file in &files {
        let outcome = match cache.links_for(file) {
            Ok(outcome) => outcome,
            Err(e) => {
                report.errors.push((file.clone(), e.to_string()));
                continue;
            }
        };
        for link in outcome.links() {
            if is_external(&link.target) {
                continue;
            }
            report.links_checked += 1;

            let resolved = match link.kind {
                LinkKind::Ref => resolve_ref_target(&link.target, &existing),
                LinkKind::File | LinkKind::Embed => {
                    exists_as(&link.target, file, project_root, &existing, Path::is_file)
                }
                LinkKind::Dir => {
                    exists_as(&link.target, file, project_root, &existing, Path::is_dir)
                }
                LinkKind::Tm => {
                    let path_part = link.target.split('#').next().unwrap_or(&link.target);
                    exists_as(path_part, file, project_root, &existing, Path::is_file)
                }
            };
            if !resolved {
                report.broken.push(BrokenLink {
                    source: file.clone(),
                    target: link.target.clone(),
                    span: link.span,
                });
            }
        }
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dot_slash_prefix_resolves_relative_to_source_dir() {
        let source = Path::new("/vault/notes/nested.tmt");
        let root = Path::new("/vault");
        assert_eq!(
            resolve_file_target(source, "./sibling.txt", root),
            PathBuf::from("/vault/notes/sibling.txt")
        );
        assert_eq!(
            resolve_file_target(source, "../up.txt", root),
            PathBuf::from("/vault/up.txt")
        );
    }

    #[test]
    fn leading_slash_is_treated_as_os_absolute() {
        let source = Path::new("/vault/notes/nested.tmt");
        let root = Path::new("/vault");
        assert_eq!(
            resolve_file_target(source, "/etc/hosts", root),
            PathBuf::from("/etc/hosts")
        );
    }

    #[test]
    fn bare_relative_path_resolves_against_project_root() {
        let source = Path::new("/vault/notes/nested.tmt");
        let root = Path::new("/vault");
        assert_eq!(
            resolve_file_target(source, "docs/asdf.txt", root),
            PathBuf::from("/vault/docs/asdf.txt")
        );
    }

    #[test]
    fn ref_target_matches_by_stem_or_full_filename() {
        let mut existing = HashSet::new();
        existing.insert(PathBuf::from("/vault/notes/real.tmt"));

        assert!(resolve_ref_target("real", &existing));
        assert!(resolve_ref_target("real.tmt", &existing));
        assert!(!resolve_ref_target("missing", &existing));
    }

    #[test]
    fn tm_target_strips_fragment_before_resolving() {
        let source = Path::new("/vault/notes/nested.tmt");
        let root = Path::new("/vault");
        let path_part = "docs/asdf.tmt#some-id".split('#').next().unwrap();
        assert_eq!(
            resolve_file_target(source, path_part, root),
            PathBuf::from("/vault/docs/asdf.tmt")
        );
    }

    #[test]
    fn external_urls_are_never_treated_as_local_paths() {
        assert!(is_external("https://example.com"));
        assert!(is_external("http://example.com/x.png"));
        assert!(!is_external("docs/asdf.txt"));
        assert!(!is_external("./sibling.txt"));
    }
}

#[cfg(test)]
mod kind_tests {
    use super::*;
    use std::collections::HashSet;

    /// A vault with one file and one directory, both in `existing`.
    fn vault() -> (PathBuf, HashSet<PathBuf>) {
        let root = std::env::temp_dir().join(format!("tm_dir_scheme_{}", uuid::Uuid::new_v4()));
        let dir = root.join("spec");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("syntax.tmt");
        std::fs::write(&file, "@kind(config)\n").unwrap();
        // As `collect_all_paths_with_config` builds it: files and
        // directories alike.
        (root.clone(), HashSet::from([dir, file]))
    }

    #[test]
    fn file_does_not_accept_a_directory() {
        // The whole reason `dir:` exists. This used to resolve, and the
        // checker then reported a file where there was a directory --
        // `.exists()` proves neither.
        let (root, existing) = vault();
        let source = root.join("README.tmt");
        assert!(!exists_as("./spec/", &source, &root, &existing, Path::is_file));
        assert!(exists_as("./spec/", &source, &root, &existing, Path::is_dir));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn dir_does_not_accept_a_file() {
        // The other direction, which was never possible to say at all.
        let (root, existing) = vault();
        let source = root.join("README.tmt");
        let target = "./spec/syntax.tmt";
        assert!(!exists_as(target, &source, &root, &existing, Path::is_dir));
        assert!(exists_as(target, &source, &root, &existing, Path::is_file));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_trailing_slash_changes_nothing_for_dir() {
        let (root, existing) = vault();
        let source = root.join("README.tmt");
        assert!(exists_as("./spec/", &source, &root, &existing, Path::is_dir));
        assert!(exists_as("./spec", &source, &root, &existing, Path::is_dir));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_path_outside_the_vault_is_not_a_target() {
        // `existing` honours `.gitignore` and `workspace.ignore`, so being
        // on disk is not enough. Both halves have to hold.
        let (root, existing) = vault();
        let source = root.join("README.tmt");
        std::fs::create_dir_all(root.join("ignored")).unwrap();
        assert!(!exists_as("./ignored/", &source, &root, &existing, Path::is_dir));
        std::fs::remove_dir_all(&root).ok();
    }
}
