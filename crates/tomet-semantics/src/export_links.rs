//! Link rewriting for exported documents.
//!
//! When a document is exported in a given format, links and embeds pointing to
//! a document via `tmt:` (including the document itself) are rewritten to point at
//! the target's output file, relative to the directory of the output file being exported.

use tomet_ast::File;
use tomet_tree::for_each_element_mut;

use crate::elements::target::link_target_of;
use crate::rebase::{directory, relative_to, set_target};

/// Rewrites `tmt:` links in `doc` for an export to `current_dest`.
///
/// Every link or embed target of the form `tmt:<vault-path>[#anchor]` pointing to
/// a document (including a link naming the document itself) is rewritten using `lookup`.
///
/// If `lookup(vault_path)` returns `Some(target_dest)`:
/// - The target points at that output file, as a path relative to the directory of
///   `current_dest`, with `/` separators. In particular, a link pointing to the document's
///   own vault path is rewritten to its own file name relative to its output directory.
/// - Any `#anchor` fragment is kept verbatim as written (heading-slug rules are not applied).
///
/// If `lookup(vault_path)` returns `None` (for instance, when the target is not exported in
/// this format or goes to stdout):
/// - The target stays as it was (the vault-relative source path).
///
/// Targets pointing inside the document via the empty-path form `tmt:#id`, `id:` and `ref:`
/// targets, `file:`, `https:` and plain relative paths (`./`, `../`) are not touched.
pub fn rewrite_export_links<F, S>(
    doc: &mut File,
    current_dest: &str,
    lookup: F,
) where
    F: Fn(&str) -> Option<S>,
    S: AsRef<str>,
{
    let norm_dest = current_dest.replace('\\', "/");
    let from_dir = directory(&norm_dest);

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

        // Look up the target document's exported destination; if unexported, leave the link as written.
        let Some(target_dest) = lookup(bare_path) else {
            return;
        };

        let norm_target = target_dest.as_ref().replace('\\', "/");
        let to_parts: Vec<&str> = norm_target
            .split('/')
            .filter(|p| !p.is_empty() && *p != ".")
            .collect();
        let rel = relative_to(&from_dir, &to_parts);
        set_target(element, format!("{rel}{anchor}"));
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
            "out/README.md",
            |path| match path {
                "locales/ja/readme.tmt" => Some("out/README.ja.md"),
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
            "out/README.md",
            |_| Some("out/README.ja.md"),
        );
        assert_eq!(extract_targets(&doc_same), vec!["README.ja.md"]);

        // 2. Subdirectory
        let src_sub = "@link(tmt:docs/sub/guide.tmt)\n";
        let mut doc_sub = tomet_ast_parser::parse_file(src_sub).unwrap();
        rewrite_export_links(
            &mut doc_sub,
            "out/README.md",
            |_| Some("out/sub/guide.md"),
        );
        assert_eq!(extract_targets(&doc_sub), vec!["sub/guide.md"]);

        // 3. Parent directory
        let src_parent = "@link(tmt:readme.tmt)\n";
        let mut doc_parent = tomet_ast_parser::parse_file(src_parent).unwrap();
        rewrite_export_links(
            &mut doc_parent,
            "out/docs/note.md",
            |_| Some("out/README.md"),
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
            "out/README.ja.md",
            |path| match path {
                "locales/ja/readme.tmt" => Some("out/README.ja.md"),
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
            "out/README.md",
            |_| Some("out/other.md"),
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
            "out/README.ja.md",
            |_| None::<&str>,
        );
        assert_eq!(extract_targets(&doc), vec!["tmt:locales/ja/readme.tmt"]);
    }

    #[test]
    fn lookup_returns_nothing_leaves_source_path_untouched() {
        let src = "@link(tmt:missing/doc.tmt)\n";
        let mut doc = tomet_ast_parser::parse_file(src).unwrap();
        rewrite_export_links(
            &mut doc,
            "out/README.md",
            |_| None::<&str>,
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
            "out/README.md",
            |_| Some("out/other.md"),
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
            "out/README.md",
            |_| Some("out/README.ja.md"),
        );
        assert_eq!(extract_targets(&doc), vec!["README.ja.md"]);
    }

    #[test]
    fn link_inside_list_item() {
        let src = "- @link(tmt:locales/ja/readme.tmt)\n";
        let mut doc = tomet_ast_parser::parse_file(src).unwrap();
        rewrite_export_links(
            &mut doc,
            "out/README.md",
            |_| Some("out/README.ja.md"),
        );
        assert_eq!(extract_targets(&doc), vec!["README.ja.md"]);
    }

    #[test]
    fn link_inside_heading() {
        let src = "#[ Title with @link(tmt:locales/ja/readme.tmt) ]\n";
        let mut doc = tomet_ast_parser::parse_file(src).unwrap();
        rewrite_export_links(
            &mut doc,
            "out/README.md",
            |_| Some("out/README.ja.md"),
        );
        assert_eq!(extract_targets(&doc), vec!["README.ja.md"]);
    }

    #[test]
    fn embed_target_is_rewritten() {
        let src = "@embed(tmt:diagrams/arch.tmt)\n";
        let mut doc = tomet_ast_parser::parse_file(src).unwrap();
        rewrite_export_links(
            &mut doc,
            "out/README.md",
            |_| Some("out/diagrams/arch.svg"),
        );
        assert_eq!(extract_targets(&doc), vec!["diagrams/arch.svg"]);
    }
}
