//! Link rewriting for exported documents.
//!
//! When a document is exported in a given format, links and embeds pointing to
//! a document via `tmt:` (including the document itself) are rewritten to point at
//! the target's output file, relative to the directory of the output file being exported.
//!
//! When a target document is not exported in the format being rendered, but the document
//! being exported has a destination file that lies within the vault root, the link points
//! at the target's source file, as a path relative to the directory of the document's output file.
//! In every other case (output goes to stdout, destination lies outside the vault root,
//! or the target does not exist as a file), the target remains as written.

use std::borrow::Cow;

use tomet_ast::File;
use tomet_tree::for_each_element_mut;

use crate::elements::target::link_target_of;
use crate::rebase::{directory, relative_to, set_target};

/// The export destination of the document whose links are being rewritten.
///
/// The variant names describe where the destination lies (inside or outside the vault root,
/// or stdout). The paths of the destination, of target documents and of target source files
/// must be in the same form (both relative to the same base, or all absolute); the function
/// does not check this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportDestination<'a> {
    /// The document is exported to a destination file inside the vault root.
    InsideVault(Cow<'a, str>),
    /// The document is exported to a destination file outside the vault root
    /// (e.g. `--out` to an external directory where vault layout cannot be assumed).
    OutsideVault(Cow<'a, str>),
    /// The document has no destination file (output goes to stdout).
    Stdout,
}

/// What a target document resolves to for export link rewriting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportTarget<S> {
    /// The target document is exported in this format to the given destination path.
    Exported(S),
    /// The target document exists as a source file at the given path,
    /// but is not exported in this format in this run.
    Source(S),
}

/// Rewrites `tmt:` links in `doc` for an export to `current_dest`.
///
/// Every link or embed target of the form `tmt:<vault-path>[#anchor]` pointing to
/// a document (including a link naming the document itself) is resolved using `lookup`.
///
/// The variant names of `current_dest` describe where the destination lies. The paths
/// of the destination, of target documents and of target source files must be in the same
/// form (both relative to the same base, or all absolute); this function does not check this.
///
/// If `lookup(vault_path)` returns `Some(target_info)`:
/// - If `target_info` is [`ExportTarget::Exported`]`(target_dest)`:
///   The target points at that output file, as a path relative to the directory of
///   `current_dest`, with `/` separators. In particular, a link pointing to the document's
///   own vault path is rewritten to its own file name relative to its output directory.
/// - If `target_info` is [`ExportTarget::Source`]`(source_path)`:
///   When the document being exported has a destination file that lies inside the vault root
///   ([`ExportDestination::InsideVault`]), the link points at the target's source file,
///   as a path relative to the directory of the document's output file.
///   When the document goes to stdout ([`ExportDestination::Stdout`]) or its destination
///   lies outside the vault root ([`ExportDestination::OutsideVault`]), the target stays
///   as written.
/// - Any `#anchor` fragment is kept verbatim as written (heading-slug rules are not applied).
///
/// If `lookup(vault_path)` returns `None` (for instance, when the target file does not
/// exist on disk in the vault):
/// - The target stays as it was (the vault-relative source path).
///
/// Targets pointing inside the document via the empty-path form `tmt:#id`, `id:` and `ref:`
/// targets, `file:`, `https:` and plain relative paths (`./`, `../`) are not touched.
pub fn rewrite_export_links<F, S>(doc: &mut File, current_dest: ExportDestination<'_>, lookup: F)
where
    F: Fn(&str) -> Option<ExportTarget<S>>,
    S: AsRef<str>,
{
    let (dest_path, is_inside_vault) = match &current_dest {
        ExportDestination::InsideVault(p) => (Some(p.as_ref()), true),
        ExportDestination::OutsideVault(p) => (Some(p.as_ref()), false),
        ExportDestination::Stdout => (None, false),
    };

    let norm_dest = dest_path.map(|p| p.replace('\\', "/"));
    let from_dir = norm_dest.as_deref().map(directory);

    for_each_element_mut(doc, |element| {
        let Some((_, target)) = link_target_of(element.as_view()) else {
            return;
        };
        // Only `tmt:` document targets are rewritten; other schemes and paths stay as written.
        let Some(rest) = target.strip_prefix("tmt:") else {
            return;
        };
        let (path, anchor) = match rest.find('#') {
            Some(at) => rest.split_at(at),
            None => (rest, ""),
        };
        let norm_path = path.replace('\\', "/");
        let bare_path = norm_path.strip_prefix("./").unwrap_or(&norm_path);

        // An empty path `tmt:#id` is a self-link within the same document and stays untouched.
        if bare_path.is_empty() {
            return;
        }

        // Look up the target document; if it does not exist as a file, leave as written.
        let Some(target_info) = lookup(bare_path) else {
            return;
        };

        let rebase = |from: &[&str], to_str: &str| {
            let norm_to = to_str.replace('\\', "/");
            let to_parts: Vec<&str> = norm_to
                .split('/')
                .filter(|p| !p.is_empty() && *p != ".")
                .collect();
            let rel = relative_to(from, &to_parts);
            format!("{rel}{anchor}")
        };

        match target_info {
            ExportTarget::Exported(target_dest) => {
                if let Some(from) = &from_dir {
                    set_target(element, rebase(from, target_dest.as_ref()));
                }
            }
            ExportTarget::Source(source_path) => {
                if is_inside_vault && let Some(from) = &from_dir {
                    set_target(element, rebase(from, source_path.as_ref()));
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract_targets(file: &File) -> Vec<String> {
        let mut targets = Vec::new();
        let mut copy = file.clone();
        for_each_element_mut(&mut copy, |el| {
            if let Some((_, target)) = link_target_of(el.as_view()) {
                targets.push(target);
            }
        });
        targets
    }

    #[test]
    fn tmt_path_with_and_without_anchor() {
        let src = "@link(tmt:locales/ja/readme.tmt)\n\n@link(tmt:locales/ja/readme.tmt#intro)\n";
        let mut doc = tomet_ast_parser::parse_file(src).unwrap();
        rewrite_export_links(
            &mut doc,
            ExportDestination::InsideVault("out/README.md".into()),
            |path| match path {
                "locales/ja/readme.tmt" => Some(ExportTarget::Exported("out/README.ja.md")),
                _ => None,
            },
        );
        let targets = extract_targets(&doc);
        assert_eq!(targets, vec!["README.ja.md", "README.ja.md#intro"]);
    }

    #[test]
    fn destinations_same_directory_subdirectory_and_parent_directory() {
        // 1. Same directory
        let src_same = "@link(tmt:locales/ja/readme.tmt)\n";
        let mut doc_same = tomet_ast_parser::parse_file(src_same).unwrap();
        rewrite_export_links(
            &mut doc_same,
            ExportDestination::InsideVault("out/README.md".into()),
            |_| Some(ExportTarget::Exported("out/README.ja.md")),
        );
        assert_eq!(extract_targets(&doc_same), vec!["README.ja.md"]);

        // 2. Subdirectory
        let src_sub = "@link(tmt:docs/sub/guide.tmt)\n";
        let mut doc_sub = tomet_ast_parser::parse_file(src_sub).unwrap();
        rewrite_export_links(
            &mut doc_sub,
            ExportDestination::InsideVault("out/README.md".into()),
            |_| Some(ExportTarget::Exported("out/sub/guide.md")),
        );
        assert_eq!(extract_targets(&doc_sub), vec!["sub/guide.md"]);

        // 3. Parent directory
        let src_parent = "@link(tmt:readme.tmt)\n";
        let mut doc_parent = tomet_ast_parser::parse_file(src_parent).unwrap();
        rewrite_export_links(
            &mut doc_parent,
            ExportDestination::InsideVault("out/docs/note.md".into()),
            |_| Some(ExportTarget::Exported("out/README.md")),
        );
        assert_eq!(extract_targets(&doc_parent), vec!["../README.md"]);
    }

    #[test]
    fn self_link_with_document_vault_path_is_rewritten_to_filename() {
        // In README.ja.md, a link to the document's own vault path (e.g. from a shared language bar)
        // points to its own output destination relative to its directory: just the file name.
        let src = "@link(tmt:locales/ja/readme.tmt)\n\n@link(tmt:locales/ja/readme.tmt#section)\n";
        let mut doc = tomet_ast_parser::parse_file(src).unwrap();
        rewrite_export_links(
            &mut doc,
            ExportDestination::InsideVault("out/README.ja.md".into()),
            |path| match path {
                "locales/ja/readme.tmt" => Some(ExportTarget::Exported("out/README.ja.md")),
                _ => None,
            },
        );
        assert_eq!(
            extract_targets(&doc),
            vec!["README.ja.md", "README.ja.md#section"]
        );
    }

    #[test]
    fn self_link_with_empty_path_anchor_stays_untouched() {
        // The empty-path form `tmt:#id` is a local anchor inside the document and is left untouched.
        let src = "@link(tmt:#local-anchor)\n";
        let mut doc = tomet_ast_parser::parse_file(src).unwrap();
        rewrite_export_links(
            &mut doc,
            ExportDestination::InsideVault("out/README.md".into()),
            |_| Some(ExportTarget::Exported("out/other.md")),
        );
        assert_eq!(extract_targets(&doc), vec!["tmt:#local-anchor"]);
    }

    #[test]
    fn self_link_unexported_stays_untouched() {
        // If the document is not exported in this format or goes to stdout, the link remains as-is.
        let src = "@link(tmt:locales/ja/readme.tmt)\n";
        let mut doc = tomet_ast_parser::parse_file(src).unwrap();
        rewrite_export_links(
            &mut doc,
            ExportDestination::InsideVault("out/README.ja.md".into()),
            |_| None::<ExportTarget<&str>>,
        );
        assert_eq!(extract_targets(&doc), vec!["tmt:locales/ja/readme.tmt"]);
    }

    #[test]
    fn lookup_returns_nothing_leaves_source_path_untouched() {
        let src = "@link(tmt:missing/doc.tmt)\n";
        let mut doc = tomet_ast_parser::parse_file(src).unwrap();
        rewrite_export_links(
            &mut doc,
            ExportDestination::InsideVault("out/README.md".into()),
            |_| None::<ExportTarget<&str>>,
        );
        assert_eq!(extract_targets(&doc), vec!["tmt:missing/doc.tmt"]);
    }

    #[test]
    fn untouched_kinds_of_target() {
        let src = "\
@link(tmt:#local-id)
@link(id:anchor)
@link(ref:Note)
@link(file:docs/img.png)
@link(https://example.com)
@link(./relative.md)
@link(../parent.md)
";
        let mut doc = tomet_ast_parser::parse_file(src).unwrap();
        rewrite_export_links(
            &mut doc,
            ExportDestination::InsideVault("out/README.md".into()),
            |_| Some(ExportTarget::Exported("out/other.md")),
        );
        let targets = extract_targets(&doc);
        assert_eq!(
            targets,
            vec![
                "tmt:#local-id",
                "id:anchor",
                "ref:Note",
                "file:docs/img.png",
                "https://example.com",
                "./relative.md",
                "../parent.md",
            ]
        );
    }

    #[test]
    fn link_inside_table() {
        let src = "\
| Header |
| --- |
| @link(tmt:locales/ja/readme.tmt) |
";
        let mut doc = tomet_ast_parser::parse_file(src).unwrap();
        rewrite_export_links(
            &mut doc,
            ExportDestination::InsideVault("out/README.md".into()),
            |_| Some(ExportTarget::Exported("out/README.ja.md")),
        );
        assert_eq!(extract_targets(&doc), vec!["README.ja.md"]);
    }

    #[test]
    fn link_inside_list_item() {
        let src = "- @link(tmt:locales/ja/readme.tmt)\n";
        let mut doc = tomet_ast_parser::parse_file(src).unwrap();
        rewrite_export_links(
            &mut doc,
            ExportDestination::InsideVault("out/README.md".into()),
            |_| Some(ExportTarget::Exported("out/README.ja.md")),
        );
        assert_eq!(extract_targets(&doc), vec!["README.ja.md"]);
    }

    #[test]
    fn link_inside_heading() {
        let src = "#[ Title with @link(tmt:locales/ja/readme.tmt) ]\n";
        let mut doc = tomet_ast_parser::parse_file(src).unwrap();
        rewrite_export_links(
            &mut doc,
            ExportDestination::InsideVault("out/README.md".into()),
            |_| Some(ExportTarget::Exported("out/README.ja.md")),
        );
        assert_eq!(extract_targets(&doc), vec!["README.ja.md"]);
    }

    #[test]
    fn embed_target_is_rewritten() {
        let src = "@embed(tmt:diagrams/arch.tmt)\n";
        let mut doc = tomet_ast_parser::parse_file(src).unwrap();
        rewrite_export_links(
            &mut doc,
            ExportDestination::InsideVault("out/README.md".into()),
            |_| Some(ExportTarget::Exported("out/diagrams/arch.svg")),
        );
        assert_eq!(extract_targets(&doc), vec!["diagrams/arch.svg"]);
    }

    #[test]
    fn unexported_target_relative_from_docs_directory() {
        let src = "@link(tmt:tmtroot/docs/why/root.tmt)\n";
        let mut doc = tomet_ast_parser::parse_file(src).unwrap();
        rewrite_export_links(
            &mut doc,
            ExportDestination::InsideVault("docs/quick_start.md".into()),
            |path| match path {
                "tmtroot/docs/why/root.tmt" => {
                    Some(ExportTarget::Source("tmtroot/docs/why/root.tmt"))
                }
                _ => None,
            },
        );
        assert_eq!(extract_targets(&doc), vec!["../tmtroot/docs/why/root.tmt"]);
    }

    #[test]
    fn unexported_target_relative_from_root() {
        let src = "@link(tmt:tmtroot/docs/why/root.tmt)\n";
        let mut doc = tomet_ast_parser::parse_file(src).unwrap();
        rewrite_export_links(
            &mut doc,
            ExportDestination::InsideVault("README.md".into()),
            |path| match path {
                "tmtroot/docs/why/root.tmt" => {
                    Some(ExportTarget::Source("tmtroot/docs/why/root.tmt"))
                }
                _ => None,
            },
        );
        assert_eq!(extract_targets(&doc), vec!["tmtroot/docs/why/root.tmt"]);
    }

    #[test]
    fn unexported_target_relative_from_deeper_directory() {
        let src = "@link(tmt:tmtroot/docs/why/root.tmt)\n";
        let mut doc = tomet_ast_parser::parse_file(src).unwrap();
        rewrite_export_links(
            &mut doc,
            ExportDestination::InsideVault("docs/sub/deep/page.md".into()),
            |path| match path {
                "tmtroot/docs/why/root.tmt" => {
                    Some(ExportTarget::Source("tmtroot/docs/why/root.tmt"))
                }
                _ => None,
            },
        );
        assert_eq!(
            extract_targets(&doc),
            vec!["../../../tmtroot/docs/why/root.tmt"]
        );
    }

    #[test]
    fn unexported_target_anchor_kept() {
        let src = "@link(tmt:tmtroot/spec/tome.tmt#syntax)\n";
        let mut doc = tomet_ast_parser::parse_file(src).unwrap();
        rewrite_export_links(
            &mut doc,
            ExportDestination::InsideVault("docs/quick_start.md".into()),
            |path| match path {
                "tmtroot/spec/tome.tmt" => Some(ExportTarget::Source("tmtroot/spec/tome.tmt")),
                _ => None,
            },
        );
        assert_eq!(
            extract_targets(&doc),
            vec!["../tmtroot/spec/tome.tmt#syntax"]
        );
    }

    #[test]
    fn unexported_target_with_no_destination_stdout_stays_untouched() {
        let src = "@link(tmt:tmtroot/docs/why/root.tmt)\n";
        let mut doc = tomet_ast_parser::parse_file(src).unwrap();
        rewrite_export_links(&mut doc, ExportDestination::Stdout, |path| match path {
            "tmtroot/docs/why/root.tmt" => Some(ExportTarget::Source("tmtroot/docs/why/root.tmt")),
            _ => None,
        });
        assert_eq!(extract_targets(&doc), vec!["tmt:tmtroot/docs/why/root.tmt"]);
    }

    #[test]
    fn unexported_target_with_destination_outside_vault_root_stays_untouched() {
        let src = "@link(tmt:tmtroot/docs/why/root.tmt)\n";
        let mut doc = tomet_ast_parser::parse_file(src).unwrap();
        rewrite_export_links(
            &mut doc,
            ExportDestination::OutsideVault("/tmp/dist/quick_start.md".into()),
            |path| match path {
                "tmtroot/docs/why/root.tmt" => {
                    Some(ExportTarget::Source("tmtroot/docs/why/root.tmt"))
                }
                _ => None,
            },
        );
        assert_eq!(extract_targets(&doc), vec!["tmt:tmtroot/docs/why/root.tmt"]);
    }
}
