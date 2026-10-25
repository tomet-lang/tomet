//! Relative links, carried from the document they were written in to another.
//!
//! When a document's content is put inside another one -- `include(...)` -- a link written
//! `./img/a.png` still has to reach the same file from where it lands. Which elements are links
//! (`@link`, `@embed`), where their target is kept and what a relative target looks like are the
//! body's knowledge, so the evaluator does not do this itself: it asks the host
//! (`tomet_expr::Resolver::rebase_links`), and a host that knows tomet documents calls this.

use tomet_ast::{Block, Document, Element, Span, Value};
use tomet_tree::for_each_element_mut;

use crate::elements::target::link_target_of;

/// Rewrites the `./` and `../` targets of links and embeds in `blocks`, which were written in the
/// document `from`, so that they reach the same files from the document `to`. Both are vault-relative
/// paths with `/` separators.
///
/// `tmt:`, `ref:`, `https:` and the rest name a file the same way anywhere and are left alone, and so
/// is a `@file` or `@dir`, which names a path rather than linking to it. A target that climbs out of
/// the vault is left as it was. In the same directory there is nothing to rewrite.
pub fn rebase_relative_links(blocks: Vec<Block>, from: &str, to: &str) -> Vec<Block> {
    let (from_dir, to_dir) = (directory(from), directory(to));
    if from_dir == to_dir {
        return blocks;
    }
    let mut holder = Document::new(blocks, Span::default());
    for_each_element_mut(&mut holder, |element| {
        let Some((_, target)) = link_target_of(element) else {
            return;
        };
        if !(target.starts_with("./") || target.starts_with("../")) {
            return;
        }
        let (path, anchor) = match target.find('#') {
            Some(at) => target.split_at(at),
            None => (target.as_str(), ""),
        };
        let Some(resolved) = resolve(&from_dir, path) else {
            return;
        };
        set_target(
            element,
            format!("{}{anchor}", relative_to(&to_dir, &resolved)),
        );
    });
    holder.blocks
}

/// The directory part of a vault-relative path, as components.
fn directory(path: &str) -> Vec<&str> {
    let mut parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    parts.pop();
    parts
}

/// Joins `relative` onto `base` and resolves `.` and `..`. `None` when it climbs out of the vault.
fn resolve(base: &[&str], relative: &str) -> Option<Vec<String>> {
    let mut out: Vec<String> = base.iter().map(|p| p.to_string()).collect();
    for part in relative.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                out.pop()?;
            }
            other => out.push(other.to_string()),
        }
    }
    Some(out)
}

/// `target` spelled relative to `from`, both as components from the vault root.
fn relative_to(from: &[&str], target: &[String]) -> String {
    let common = from
        .iter()
        .zip(target)
        .take_while(|(a, b)| **a == b.as_str())
        .count();
    let ups = from.len() - common;
    let rest = target[common..].join("/");
    if ups == 0 {
        format!("./{rest}")
    } else {
        format!("{}{rest}", "../".repeat(ups))
    }
}

/// Writes a link's target back, whether it was given as the one argument or as `target:`.
fn set_target(element: &mut Element, target: String) {
    match &mut element.args {
        Some(Value::String(existing)) => *existing = target,
        Some(Value::Map(entries)) => {
            if let Some((_, value)) = entries.iter_mut().find(|(key, _)| key == "target") {
                *value = Value::String(target);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `docs/guide.tmt`, in part.
    const GUIDE: &str = "See @link(./img/a.png) and @link(../shared/s.tmt#top) and @link(tmt:abs.tmt).\n\n\
         A file @file(./src/lib.rs) and @embed(./img/b.png).\n";

    fn targets(blocks: &[Block]) -> Vec<String> {
        let mut doc = Document::new(blocks.to_vec(), Span::default());
        let mut out = Vec::new();
        for_each_element_mut(&mut doc, |el| {
            if let Some((_, target)) = link_target_of(el) {
                out.push(target);
            }
        });
        out
    }

    fn rebased(to: &str) -> Vec<String> {
        let blocks = tomet_parser::parse_document(GUIDE).unwrap().blocks;
        targets(&rebase_relative_links(blocks, "docs/guide.tmt", to))
    }

    #[test]
    fn a_relative_link_still_reaches_the_same_file() {
        // from the vault root: `docs/guide.tmt`'s `./img/a.png` is `docs/img/a.png`
        assert_eq!(
            rebased("here.tmt"),
            [
                "./docs/img/a.png",
                "./shared/s.tmt#top",
                "tmt:abs.tmt",
                "./docs/img/b.png"
            ]
        );
        // one level down from the guide
        assert_eq!(
            rebased("docs/sub/host.tmt"),
            [
                "../img/a.png",
                "../../shared/s.tmt#top",
                "tmt:abs.tmt",
                "../img/b.png"
            ]
        );
    }

    #[test]
    fn in_the_same_directory_nothing_is_rewritten() {
        assert_eq!(
            rebased("docs/other.tmt"),
            [
                "./img/a.png",
                "../shared/s.tmt#top",
                "tmt:abs.tmt",
                "./img/b.png"
            ]
        );
    }

    #[test]
    fn a_file_mention_names_a_path_and_is_left_alone() {
        let blocks = tomet_parser::parse_document("@file(./src/lib.rs)\n")
            .unwrap()
            .blocks;
        let moved = rebase_relative_links(blocks.clone(), "docs/guide.tmt", "here.tmt");
        assert_eq!(moved, blocks);
    }

    #[test]
    fn a_target_that_climbs_out_of_the_vault_is_left_as_it_was() {
        let blocks = tomet_parser::parse_document("@link(../../x.md)\n")
            .unwrap()
            .blocks;
        let moved = rebase_relative_links(blocks.clone(), "docs/guide.tmt", "here.tmt");
        assert_eq!(moved, blocks);
    }
}
