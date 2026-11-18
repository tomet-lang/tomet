//! Relative links, carried from the document they were written in to another.
//!
//! When a document's content is put inside another one -- `include(...)` -- a link written
//! `./img/a.png` still has to reach the same file from where it lands. Which elements are links
//! (`@link`, `@embed`), where their target is kept and what a relative target looks like are the
//! body's knowledge, so the evaluator does not do this itself: it asks the host
//! (`tomet_expr::Resolver::rebase_links`), and a host that knows tomet documents calls this.

use tomet_ast::{Block, Element, File, Value};
use tomet_scalar::Scalar;
use tomet_span::Span;
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
    let mut holder = File::new(Vec::new(), blocks, Span::default());
    for_each_element_mut(&mut holder, |element| {
        let Some((_, target)) = link_target_of(element.as_view()) else {
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
        let to_parts: Vec<&str> = resolved.iter().map(String::as_str).collect();
        let rel = relative_to(&to_dir, &to_parts);
        let rel_target = if rel.starts_with("../") {
            rel
        } else {
            format!("./{rel}")
        };
        set_target(element, format!("{}{anchor}", rel_target));
    });
    holder.body
}

/// The directory part of a vault-relative path, as components.
pub(crate) fn directory(path: &str) -> Vec<&str> {
    let mut parts: Vec<&str> = path
        .split('/')
        .filter(|p| !p.is_empty() && *p != ".")
        .collect();
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

/// `to` spelled relative to `from`, both as path components.
///
/// Returns the common relative path: `../` for each level remaining in `from`,
/// followed by the remaining components of `to`.
pub(crate) fn relative_to(from: &[&str], to: &[&str]) -> String {
    let common = from.iter().zip(to).take_while(|(a, b)| *a == *b).count();
    let ups = from.len() - common;
    let rest = to[common..].join("/");
    if ups == 0 {
        rest
    } else {
        format!("{}{rest}", "../".repeat(ups))
    }
}

/// Writes a link's target back, whether it was given as the one argument or as `target:`.
pub(crate) fn set_target(element: &mut Element, target: String) {
    let Some(args) = &mut element.data.args else {
        return;
    };
    if let Some(value) = args.get_mut("target") {
        *value = Value::Scalar(Scalar::Text(target));
    } else if let Some(value) = args.nth_unnamed_mut(0) {
        *value = Value::Scalar(Scalar::Text(target));
    } else {
        let scheme_key = args.named().find_map(|(k, _)| {
            crate::elements::target::is_explicit_scheme(k).then_some(k.to_string())
        });
        if let Some(scheme) = scheme_key {
            match args.rename(&scheme, "target") {
                Ok(true) => {
                    if let Some(value) = args.get_mut("target") {
                        *value = Value::Scalar(Scalar::Text(target));
                    }
                }
                Ok(false) => {}
                Err(err) => unreachable!(
                    "no `target` key was present, so renaming `{scheme}` to `target` cannot fail with duplicate key: {err:?}"
                ),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `docs/guide.tmt`, in part.
    const GUIDE: &str = "See @link(./img/a.png) and @link(../shared/s.tmt#top) and @link(tmt:abs.tmt).\n\n\
         A file @file(./src/lib.rs) and @embed(./img/b.png).\n";

    fn targets(blocks: &[Block]) -> Vec<String> {
        let mut doc = File::new(Vec::new(), blocks.to_vec(), Span::default());
        let mut out = Vec::new();
        for_each_element_mut(&mut doc, |el| {
            if let Some((_, target)) = link_target_of(el.as_view()) {
                out.push(target);
            }
        });
        out
    }

    fn rebased(to: &str) -> Vec<String> {
        let blocks = tomet_ast_parser::parse_file(GUIDE).unwrap().body;
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
        let blocks = tomet_ast_parser::parse_file("@file(./src/lib.rs)\n")
            .unwrap()
            .body;
        let moved = rebase_relative_links(blocks.clone(), "docs/guide.tmt", "here.tmt");
        assert_eq!(moved, blocks);
    }

    #[test]
    fn a_target_that_climbs_out_of_the_vault_is_left_as_it_was() {
        let blocks = tomet_ast_parser::parse_file("@link(../../x.md)\n")
            .unwrap()
            .body;
        let moved = rebase_relative_links(blocks.clone(), "docs/guide.tmt", "here.tmt");
        assert_eq!(moved, blocks);
    }

    #[test]
    fn set_target_handles_all_link_forms() {
        use tomet_ast::{Name, Tuple};
        use tomet_tree::element_new;

        // Form 1: Hand-written bare scheme prefix without space: @link(tmt:x.tmt)
        // Parses as named entry [("tmt", "x.tmt")]; reaches scheme_key rename branch.
        let mut f1 = tomet_ast_parser::parse_file("@link(tmt:x.tmt)\n").unwrap();
        let Block::Element(el1) = &mut f1.body[0] else {
            panic!()
        };
        set_target(el1, "out/x.md".to_string());
        assert_eq!(link_target_of(el1.as_view()).unwrap().1, "out/x.md");

        // Form 2: Hand-written bare scheme prefix with space: @link(tmt: x.tmt)
        // Parses as named entry [("tmt", "x.tmt")]; reaches scheme_key rename branch.
        let mut f2 = tomet_ast_parser::parse_file("@link(tmt: x.tmt)\n").unwrap();
        let Block::Element(el2) = &mut f2.body[0] else {
            panic!()
        };
        set_target(el2, "out/x.md".to_string());
        assert_eq!(link_target_of(el2.as_view()).unwrap().1, "out/x.md");

        // Form 3: Hand-written explicit target key: @link(target: tmt:x.tmt)
        // Parses as named entry [("target", "tmt:x.tmt")]; reaches args.get_mut("target") branch.
        let mut f3 = tomet_ast_parser::parse_file("@link(target: tmt:x.tmt)\n").unwrap();
        let Block::Element(el3) = &mut f3.body[0] else {
            panic!()
        };
        set_target(el3, "out/x.md".to_string());
        assert_eq!(link_target_of(el3.as_view()).unwrap().1, "out/x.md");

        // Form 4: Expression-generated link from d.link()
        // Produces unnamed argument Tuple::from_unnamed([Value::Scalar(Scalar::Text("tmt:..."))]);
        // reaches args.nth_unnamed_mut(0) branch.
        let mut el4 = element_new(Name::bare("link"));
        el4.data.args = Some(Tuple::from_unnamed([Value::Scalar(Scalar::Text(
            "tmt:x.tmt".to_string(),
        ))]));
        set_target(&mut el4, "out/x.md".to_string());
        assert_eq!(link_target_of(el4.as_view()).unwrap().1, "out/x.md");
    }
}
