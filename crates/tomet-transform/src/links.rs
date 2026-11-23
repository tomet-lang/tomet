//! The links of a document that is published as a page of a site.
//!
//! A `.tmt` document names what it links to the way its vault does: a note by name (`ref:Linux`), a
//! file by its path from the vault's root (`file:tmtroot/spec/x.tmt`), a document with its anchor
//! (`tmt:spec/x#id`). A page on a site has none of that: it has URLs. [`resolve_site_links`] is the
//! one pass that turns the first into the second, and says so, with the reason, for every link it
//! cannot turn -- so a page never carries a link that goes nowhere without something having
//! reported it.
//!
//! =[ Where a link goes ]
//!
//! - A document the site publishes is its page: `<url_prefix>/<path without extension>`, with the
//!   `#anchor` of the link.
//! - A file that is not a document, named by `ref:` or embedded, is an asset: `<asset_prefix>/<path>`.
//!   Serving it is the site's job.
//! - Any other file or directory of the vault, and any path outside the tree the site loads, is
//!   where the source of the project keeps it: the [`SourceUrl`] template of the site, with the
//!   path from the vault's root. There is no raw, vault-relative `href`.
//!
//! =[ What is an error ]
//!
//! An [`UnresolvedLink`], and the link is rewritten to the `unresolved:` scheme so the renderer
//! draws it as a placeholder: a `ref:` or an embed that names no file; a `ref:` to a document the
//! site does not publish (there is no fallback: a note name says "that note", and the site has no
//! page for it); a path inside the loaded tree that is not there; a path that needs the source URL
//! when the site has none.
//!
//! What is outside the loaded tree cannot be looked up, so it is not checked: that is
//! `tomet check-links`' job in the vault that owns those files.

use std::path::Path;

use tomet_ast::{Block, Element, File, Inline, Paragraph, Text, Tuple, Value};
use tomet_scalar::Scalar;
use tomet_semantics::{
    ElementKind, TargetScheme, classify_std_lenient, link_target, target_scheme,
};
use tomet_span::Span;
use tomet_tree::for_each_element_mut;

/// What a site knows about the files of its vault. Paths are `/`-separated and relative to the tree
/// the site loads (the vault's own root, unless [`LinkSite::base_path`] says the tree is a part of it).
pub trait LinkCatalog {
    /// The file a note name stands for (`ref:Linux`, `ref:+hash.png`), seen from the document
    /// `from`. `None` if no file of the vault is meant.
    fn resolve_ref(&self, name: &str, from: Option<&str>) -> Option<String>;
    /// Whether the vault has a file or a directory at `path`.
    fn exists(&self, path: &str) -> bool;
    /// Whether the site has a page for the document at `path`.
    fn is_published(&self, path: &str) -> bool;
}

/// Where the source of the project is on the web, as two URL templates. `{path}` stands for the
/// path from the vault's root, with each segment percent-encoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceUrl {
    /// The URL of a file, such as `https://github.com/org/repo/blob/<revision>/{path}`.
    pub file: String,
    /// The URL of a directory, such as `https://github.com/org/repo/tree/<revision>/{path}`.
    pub dir: String,
}

/// What the links of a document are turned into.
pub struct LinkSite<'a> {
    /// Where the pages are: `/docs` gives `/docs/spec/x`.
    pub url_prefix: &'a str,
    /// Where the assets are: `/vault` gives `/vault/img/x.png`.
    pub asset_prefix: &'a str,
    /// Where the loaded tree sits in the vault, from the vault's root (`tmtroot`), or `""` when the
    /// tree is the vault. Paths in a document are written from the vault's root.
    pub base_path: &'a str,
    /// Where the source of the project is, for what the site has no page for.
    pub source_url: Option<&'a SourceUrl>,
    /// The files.
    pub catalog: &'a dyn LinkCatalog,
}

/// A link that could not be turned into a URL, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedLink {
    /// What it was written as: `ref`, `file`, `dir`, `tmt`, or `embed`.
    pub scheme: &'static str,
    /// The target as written, without its scheme.
    pub target: String,
    /// Why, in words.
    pub reason: String,
    /// The link in the document.
    pub span: Span,
}

/// Turns every `ref:`, `file:`, `dir:` and `tmt:` link, and every embed, of `doc` into the URL it
/// has on the site, and returns the ones that have none.
///
/// `from` is the path of `doc` in the tree, which is what a `./` or `../` target is written from.
/// Links that are already URLs, anchors of the same document (`#x`, `id:x`) and absolute web paths
/// (`/docs/x`) are left as they are.
pub fn resolve_site_links(
    doc: &mut File,
    from: Option<&str>,
    site: &LinkSite<'_>,
) -> Vec<UnresolvedLink> {
    let mut unresolved = Vec::new();
    for_each_element_mut(doc, |el| {
        let view = el.as_view();
        let kind = classify_std_lenient(view);
        if kind != ElementKind::Link && kind != ElementKind::Embed {
            return;
        }
        let Some(raw_target) = link_target(view, &kind) else {
            return;
        };
        let (scheme, rest) = target_scheme(&raw_target);
        if matches!(
            scheme,
            TargetScheme::Url | TargetScheme::Internal | TargetScheme::Unresolved
        ) || rest.starts_with('/')
        {
            return;
        }

        let is_embed = kind == ElementKind::Embed;
        let (path, fragment) = match rest.split_once('#') {
            Some((path, frag)) => (path, Some(frag).filter(|f| !f.is_empty())),
            None => (rest, None),
        };
        let resolved = match (is_embed, scheme) {
            (true, TargetScheme::Ref | TargetScheme::File) => site.note_or_asset(path, from, true),
            (false, TargetScheme::Ref) => site.note_or_asset(path, from, false),
            (false, TargetScheme::File | TargetScheme::Dir | TargetScheme::Document) => {
                site.path_link(scheme, path, from)
            }
            _ => return,
        };

        let (new_target, label) = match resolved {
            Ok(found) => (
                found.url_with(fragment),
                with_fragment(&found.label, fragment),
            ),
            Err(reason) => {
                unresolved.push(UnresolvedLink {
                    scheme: if is_embed { "embed" } else { scheme.as_str() },
                    target: rest.to_string(),
                    reason,
                    span: el.span,
                });
                (
                    format!("unresolved:{rest}"),
                    with_fragment(stem_of(path), fragment),
                )
            }
        };

        update_element_target(el, new_target);
        if !is_embed && !has_label(el) {
            el.content = Some(vec![Block::Paragraph(Paragraph::new(
                vec![Inline::Text(Text::from(label))],
                Span::dummy(),
            ))]);
        }
    });
    unresolved
}

/// Where a link goes on the site, and what to call it when it has no text of its own.
struct Found {
    /// The URL, without the anchor of the link.
    url: String,
    /// Whether the `#anchor` of the link is a place in `url`: true for a page.
    anchored: bool,
    label: String,
}

impl Found {
    fn url_with(&self, fragment: Option<&str>) -> String {
        match fragment {
            Some(fragment) if self.anchored => format!("{}#{fragment}", self.url),
            _ => self.url.clone(),
        }
    }
}

fn with_fragment(label: &str, fragment: Option<&str>) -> String {
    match fragment {
        Some(fragment) => format!("{label}#{fragment}"),
        None => label.to_string(),
    }
}

fn last_segment(path: &str) -> &str {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path)
}

fn is_document(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| matches!(ext.to_ascii_lowercase().as_str(), "tmt" | "tm" | "md"))
}

fn stem_of(path: &str) -> &str {
    let name = last_segment(path);
    name.rsplit_once('.').map_or(name, |(stem, _)| stem)
}

impl LinkSite<'_> {
    fn page_url(&self, path: &str) -> String {
        let slug = path
            .strip_suffix(".tmt")
            .or_else(|| path.strip_suffix(".tm"))
            .or_else(|| path.strip_suffix(".md"))
            .unwrap_or(path);
        let prefix = self.url_prefix.trim_end_matches('/');
        format!("{prefix}/{}", slug.trim_start_matches('/'))
    }

    fn asset_url(&self, path: &str) -> String {
        let prefix = self.asset_prefix.trim_end_matches('/');
        format!("{prefix}/{}", path.trim_start_matches('/'))
    }

    /// A note name or an embedded file: the document's page, or the asset.
    fn note_or_asset(&self, name: &str, from: Option<&str>, embed: bool) -> Result<Found, String> {
        let Some(path) = self.catalog.resolve_ref(name, from) else {
            return Err(format!("no file of the vault is named `{name}`"));
        };
        if is_document(&path) {
            if embed {
                return Err(format!("`{path}` is a document, not something to embed"));
            }
            if !self.catalog.is_published(&path) {
                return Err(format!(
                    "`{path}` is not published: the site has no page for it"
                ));
            }
            return Ok(Found {
                url: self.page_url(&path),
                anchored: true,
                label: stem_of(&path).to_string(),
            });
        }
        Ok(Found {
            url: self.asset_url(&path),
            anchored: false,
            label: stem_of(&path).to_string(),
        })
    }

    /// `file:`, `dir:` and `tmt:`: a path from the vault's root, or from the document with `./`.
    fn path_link(
        &self,
        scheme: TargetScheme,
        path: &str,
        from: Option<&str>,
    ) -> Result<Found, String> {
        if path.is_empty() {
            return Err("the link names no path".to_string());
        }
        let base = self.base_path.trim_matches('/');
        let joined = if path.starts_with("./") || path.starts_with("../") {
            let dir = from
                .and_then(|f| f.rsplit_once('/'))
                .map_or("", |(dir, _)| dir);
            [base, dir, path]
                .iter()
                .filter(|part| !part.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join("/")
        } else {
            path.to_string()
        };
        let Some(from_root) = normalize(&joined) else {
            return Err(format!("`{path}` leaves the vault"));
        };

        let inside = if base.is_empty() {
            Some(from_root.as_str())
        } else {
            from_root
                .strip_prefix(base)
                .and_then(|rest| rest.strip_prefix('/'))
        };
        let wants_dir = scheme == TargetScheme::Dir;
        if let Some(inside) = inside {
            if !self.catalog.exists(inside) {
                return Err(format!("`{from_root}` is not in the vault"));
            }
            if !wants_dir && is_document(inside) && self.catalog.is_published(inside) {
                return Ok(Found {
                    url: self.page_url(inside),
                    anchored: true,
                    label: stem_of(inside).to_string(),
                });
            }
        }
        let Some(source) = self.source_url else {
            return Err(format!(
                "`{from_root}` has no page on the site, and the site has no source URL to send it to"
            ));
        };
        let template = if wants_dir { &source.dir } else { &source.file };
        Ok(Found {
            url: template.replace("{path}", &percent_encode_path(&from_root)),
            anchored: false,
            label: last_segment(&from_root).to_string(),
        })
    }
}

/// `a/./b/../c` as `a/c`; `None` if it climbs out of the root.
fn normalize(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}

/// Each segment of `path` percent-encoded, the `/` between them kept.
fn percent_encode_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                out.push(byte as char);
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// Whether a link element has text of its own to show.
fn has_label(el: &Element) -> bool {
    el.content.as_ref().is_some_and(|c| {
        !c.is_empty()
            && c.iter().any(|block| match block {
                Block::Paragraph(p) => p.content.iter().any(|inline| match inline {
                    Inline::Text(t) => !t.value.trim().is_empty(),
                    Inline::Raw(r) => !r.value.trim().is_empty(),
                    Inline::Element(_)
                    | Inline::Interp(_)
                    | Inline::Hole(_)
                    | Inline::TypeSeat(_) => true,
                    Inline::SoftBreak(_) | Inline::LineBreak(_) => false,
                }),
                Block::Element(_)
                | Block::Section(_)
                | Block::List(_)
                | Block::Interp(_)
                | Block::Hole(_)
                | Block::TypeSeat(_) => true,
            })
    })
}

fn update_element_target(el: &mut Element, new_target: String) {
    if let Some(tuple) = &mut el.data.args {
        if let Some(v) = tuple.get_mut("target") {
            *v = Value::Scalar(Scalar::Text(new_target));
        } else if tuple.contains_key("ref") {
            tuple.remove("ref");
            tuple.set("target", Value::Scalar(Scalar::Text(new_target)));
        } else if let Some(v) = tuple.nth_unnamed_mut(0) {
            *v = Value::Scalar(Scalar::Text(new_target));
        } else {
            tuple.set("target", Value::Scalar(Scalar::Text(new_target)));
        }
    } else {
        el.data.args = Some(Tuple::from_unnamed([Value::Scalar(Scalar::Text(
            new_target,
        ))]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_doc(src: &str) -> File {
        tomet_ast_parser::parse_file(src).expect("valid source")
    }

    /// A vault of the files it lists, with the documents it publishes; a note name is a file name
    /// or the file name without its extension.
    struct Files {
        files: Vec<&'static str>,
        published: Vec<&'static str>,
    }

    impl LinkCatalog for Files {
        fn resolve_ref(&self, name: &str, _from: Option<&str>) -> Option<String> {
            self.files
                .iter()
                .find(|f| last_segment(f) == name || stem_of(f) == name)
                .map(|f| f.to_string())
        }
        fn exists(&self, path: &str) -> bool {
            self.files
                .iter()
                .any(|f| *f == path || f.starts_with(&format!("{path}/")))
        }
        fn is_published(&self, path: &str) -> bool {
            self.published.contains(&path)
        }
    }

    fn files() -> Files {
        Files {
            files: vec![
                "30-39 Knowledge/Linux.tmt",
                "30-39 Knowledge/Draft.tmt",
                "30-39 Knowledge/ミーム/-/+8c30.png",
                "40-49 Master/manual.pdf",
                "spec/a.tmt",
                "spec/b.tmt",
                "spec/data/x.toml",
            ],
            published: vec!["30-39 Knowledge/Linux.tmt", "spec/a.tmt", "spec/b.tmt"],
        }
    }

    fn site<'a>(
        catalog: &'a Files,
        base_path: &'a str,
        source_url: Option<&'a SourceUrl>,
    ) -> LinkSite<'a> {
        LinkSite {
            url_prefix: "/docs",
            asset_prefix: "/vault",
            base_path,
            source_url,
            catalog,
        }
    }

    fn targets(doc: &mut File) -> Vec<String> {
        let mut targets = Vec::new();
        for_each_element_mut(doc, |el| {
            if let Some((_, t)) = tomet_semantics::link_target_of(el.as_view()) {
                targets.push(t);
            }
        });
        targets
    }

    fn git() -> SourceUrl {
        SourceUrl {
            file: "https://git.example/blob/abc/{path}".into(),
            dir: "https://git.example/tree/abc/{path}".into(),
        }
    }

    #[test]
    fn a_note_name_is_the_page_of_a_published_document() {
        let mut doc = parse_doc(
            r#"
- @link("ref:Linux")[Go to Linux]
- @link("ref:Linux#kernel")[Go to Kernel]
- @link("ref:MissingNote")[Go to Missing]
- @link("https://example.com")[External]
"#,
        );
        let catalog = files();
        let unresolved = resolve_site_links(&mut doc, None, &site(&catalog, "", None));

        assert_eq!(
            targets(&mut doc),
            vec![
                "/docs/30-39 Knowledge/Linux".to_string(),
                "/docs/30-39 Knowledge/Linux#kernel".to_string(),
                "unresolved:MissingNote".to_string(),
                "https://example.com".to_string(),
            ]
        );
        assert_eq!(unresolved.len(), 1);
        assert_eq!(unresolved[0].scheme, "ref");
        assert_eq!(unresolved[0].target, "MissingNote");
        assert!(unresolved[0].reason.contains("MissingNote"));
    }

    #[test]
    fn a_note_name_of_an_unpublished_document_is_an_error_not_a_fallback() {
        let mut doc = parse_doc("- @link(\"ref:Draft\")[Draft]\n");
        let catalog = files();
        let source = git();
        let unresolved = resolve_site_links(&mut doc, None, &site(&catalog, "", Some(&source)));

        assert_eq!(targets(&mut doc), vec!["unresolved:Draft".to_string()]);
        assert_eq!(unresolved.len(), 1);
        assert!(unresolved[0].reason.contains("not published"));
    }

    #[test]
    fn embeds_and_asset_notes_go_to_the_asset_prefix() {
        let mut doc = parse_doc(
            r#"
- @embed(+8c30.png)
- @embed("https://example.com/cat.png")
- @embed(+missing.png)
- @link("ref:manual.pdf")[Download PDF]
"#,
        );
        let catalog = files();
        let unresolved = resolve_site_links(&mut doc, None, &site(&catalog, "", None));

        assert_eq!(
            targets(&mut doc),
            vec![
                "/vault/30-39 Knowledge/ミーム/-/+8c30.png".to_string(),
                "https://example.com/cat.png".to_string(),
                "unresolved:+missing.png".to_string(),
                "/vault/40-49 Master/manual.pdf".to_string(),
            ]
        );
        assert_eq!(unresolved.len(), 1);
        assert_eq!(unresolved[0].scheme, "embed");
    }

    #[test]
    fn a_link_without_text_is_named_after_its_file() {
        let mut doc = parse_doc(
            r#"
- @link("ref:Linux")
- @link("ref:Linux#kernel")
- @link("ref:MissingNote")
- @link("ref:Linux")[Custom Label]
"#,
        );
        let catalog = files();
        resolve_site_links(&mut doc, None, &site(&catalog, "", None));

        let mut labels = Vec::new();
        for_each_element_mut(&mut doc, |el| {
            if el.name.is_bare_named("link") {
                let text = el.content.as_ref().map(|c| {
                    c.iter()
                        .filter_map(|block| match block {
                            Block::Paragraph(p) => {
                                Some(p.content.iter().map(|inline| match inline {
                                    Inline::Text(t) => t.value.as_str(),
                                    Inline::Raw(r) => r.value.as_str(),
                                    _ => "",
                                }))
                            }
                            _ => None,
                        })
                        .flatten()
                        .collect::<String>()
                });
                labels.push(text);
            }
        });
        assert_eq!(
            labels,
            vec![
                Some("Linux".to_string()),
                Some("Linux#kernel".to_string()),
                Some("MissingNote".to_string()),
                Some("Custom Label".to_string()),
            ]
        );
    }

    #[test]
    fn a_path_to_a_published_document_is_its_page() {
        let mut doc = parse_doc(
            r#"
- @link("file:tmtroot/spec/a.tmt")[a]
- @link("tmt:tmtroot/spec/b.tmt#sec")[b]
- @link("file:./b.tmt")[sibling]
"#,
        );
        let catalog = files();
        let unresolved = resolve_site_links(
            &mut doc,
            Some("spec/a.tmt"),
            &site(&catalog, "tmtroot", None),
        );

        assert_eq!(
            targets(&mut doc),
            vec![
                "/docs/spec/a".to_string(),
                "/docs/spec/b#sec".to_string(),
                "/docs/spec/b".to_string(),
            ]
        );
        assert!(unresolved.is_empty(), "{unresolved:?}");
    }

    #[test]
    fn what_has_no_page_goes_to_the_source_url() {
        let mut doc = parse_doc(
            r#"
- @link("file:tmtroot/spec/data/x.toml")[data]
- @link("dir:tmtroot/spec/data")[dir]
- @link("file:tests/fixtures/ほげ.tmt")[outside]
- @link("file:tmtroot/30-39 Knowledge/Draft.tmt")[unpublished]
"#,
        );
        let catalog = files();
        let source = git();
        let unresolved = resolve_site_links(
            &mut doc,
            Some("spec/a.tmt"),
            &site(&catalog, "tmtroot", Some(&source)),
        );

        assert_eq!(
            targets(&mut doc),
            vec![
                "https://git.example/blob/abc/tmtroot/spec/data/x.toml".to_string(),
                "https://git.example/tree/abc/tmtroot/spec/data".to_string(),
                "https://git.example/blob/abc/tests/fixtures/%E3%81%BB%E3%81%92.tmt".to_string(),
                "https://git.example/blob/abc/tmtroot/30-39%20Knowledge/Draft.tmt".to_string(),
            ]
        );
        assert!(unresolved.is_empty(), "{unresolved:?}");
    }

    #[test]
    fn a_path_in_the_loaded_tree_that_is_not_there_is_an_error() {
        let mut doc = parse_doc("- @link(\"file:tmtroot/spec/nope.tmt\")[gone]\n");
        let catalog = files();
        let source = git();
        let unresolved =
            resolve_site_links(&mut doc, None, &site(&catalog, "tmtroot", Some(&source)));

        assert_eq!(targets(&mut doc), vec!["unresolved:tmtroot/spec/nope.tmt"]);
        assert_eq!(unresolved.len(), 1);
        assert!(unresolved[0].reason.contains("not in the vault"));
    }

    #[test]
    fn without_a_source_url_a_path_with_no_page_is_an_error() {
        let mut doc = parse_doc("- @link(\"file:crates/x.rs\")[x]\n");
        let catalog = files();
        let unresolved = resolve_site_links(&mut doc, None, &site(&catalog, "tmtroot", None));

        assert_eq!(unresolved.len(), 1);
        assert!(unresolved[0].reason.contains("source URL"));
    }

    #[test]
    fn urls_anchors_and_web_paths_are_left_alone() {
        let mut doc = parse_doc(
            "- @link(\"https://example.com\")[a]\n- @link(\"id:intro\")[b]\n- @link(\"/docs/x\")[c]\n",
        );
        let catalog = files();
        let unresolved = resolve_site_links(&mut doc, None, &site(&catalog, "", None));
        assert_eq!(
            targets(&mut doc),
            vec!["https://example.com", "id:intro", "/docs/x"]
        );
        assert!(unresolved.is_empty());
    }

    #[test]
    fn a_path_that_climbs_out_of_the_vault_is_an_error() {
        let mut doc = parse_doc("- @link(\"file:../../x\")[x]\n");
        let catalog = files();
        let unresolved = resolve_site_links(&mut doc, Some("a.tmt"), &site(&catalog, "", None));
        assert_eq!(unresolved.len(), 1);
        assert!(unresolved[0].reason.contains("leaves the vault"));
    }
}
