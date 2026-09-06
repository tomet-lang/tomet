//! Pure, I/O-free extraction of every `File`/`Embed`/`Tm`/`Ref` link out of
//! a parsed `Document` (`@link(target:...)`'s target scheme, or `@embed`
//! unconditionally). `Url`/`Id` are deliberately excluded -- see this
//! crate's module doc.

use tomet_ast::{Document, Span};
use tomet_semantics::{ElementKind, TargetScheme, link_target_of, target_scheme, path_target_of};
use tomet_tree::for_each_element;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    File,
    Dir,
    Embed,
    Tm,
    Ref,
}

impl LinkKind {
    /// Every kind, so the cache schema can spell its CHECK from this
    /// rather than from a hand-copied list that nothing holds to it.
    pub const ALL: [LinkKind; 5] = [
        LinkKind::File,
        LinkKind::Dir,
        LinkKind::Embed,
        LinkKind::Tm,
        LinkKind::Ref,
    ];

    /// `Embed` (`<embed>`/`@embed`) is always a link regardless of what its
    /// `target` string looks like -- it's "this is embedded media", not a
    /// scheme distinction. `Link` (`@link`/`<link>`) has no fixed kind of
    /// its own anymore; its `target` string's own scheme decides File/Tm/Ref
    /// (or excludes it: `Url` needs network, `Id` is same-document-only --
    /// see this module's doc comment). Returns the scheme-stripped
    /// remainder of `target` alongside the resolved `LinkKind`, since the
    /// scheme prefix (`tm:`/`ref:`/`file:`/`dir:`) is no longer part of the actual
    /// target value once it's been recognized.
    fn from_element_kind_and_target(kind: &ElementKind, target: &str) -> Option<(Self, String)> {
        match kind {
            ElementKind::Embed => Some((LinkKind::Embed, target.to_string())),
            // `@file`/`@dir` carry no scheme prefix: the element name is
            // the scheme. Checked exactly as `file:`/`dir:` are, because
            // the question is the same one -- only the rendering differs.
            ElementKind::File => Some((LinkKind::File, target.to_string())),
            ElementKind::Dir => Some((LinkKind::Dir, target.to_string())),
            ElementKind::Link => {
                let (scheme, rest) = target_scheme(target);
                let link_kind = match scheme {
                    TargetScheme::File => LinkKind::File,
                    TargetScheme::Dir => LinkKind::Dir,
                    TargetScheme::Tm => LinkKind::Tm,
                    TargetScheme::Ref => LinkKind::Ref,
                    TargetScheme::Url | TargetScheme::Id => return None,
                };
                Some((link_kind, rest.to_string()))
            }
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            LinkKind::File => "file",
            LinkKind::Dir => "dir",
            LinkKind::Embed => "embed",
            LinkKind::Tm => "tm",
            LinkKind::Ref => "ref",
        }
    }
}

impl std::str::FromStr for LinkKind {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "file" => Ok(LinkKind::File),
            "dir" => Ok(LinkKind::Dir),
            "embed" => Ok(LinkKind::Embed),
            "tm" => Ok(LinkKind::Tm),
            "ref" => Ok(LinkKind::Ref),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DocumentLink {
    pub kind: LinkKind,
    pub target: String,
    pub span: Span,
}

pub fn collect_links(doc: &Document) -> Vec<DocumentLink> {
    let mut links = Vec::new();
    for_each_element(doc, |el| {
        if let Some((kind, target)) = link_target_of(el).or_else(|| path_target_of(el))
            && let Some((kind, target)) = LinkKind::from_element_kind_and_target(&kind, &target)
        {
            links.push(DocumentLink {
                kind,
                target,
                span: el.span,
            });
        }
    });
    links
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_file_embed_tm_and_ref_links_but_not_url_or_id() {
        // `tm:`/`id:`/`ref:` need the explicit `target:` key here, not the
        // bare positional shorthand: `@link(tm:foo/bar)` parses as
        // `Map([("tm", "foo/bar")])` (the parser's identifier-then-colon
        // rule treats `tm` as a map key regardless of what element it's
        // in), which has no `target` key at all and so isn't recognized as
        // a link -- only `target:`-prefixed or shape-unambiguous bare
        // values (`https://...`, `/abs/path`, a plain relative word) work
        // positionally. See this session's `target_scheme`-consuming
        // callers' notes.
        let src = r#"
@link(readme.md)
[ Readme ]

@link(https://example.com)[External]

@link(target:id:1)

@embed[alt](pic.png)

@link(target:ref:Some Page)

@link(target:tm:foo/bar#some-id)
"#;
        let doc = tomet_parser::parse_document(src).unwrap();
        let links = collect_links(&doc);

        assert_eq!(links.len(), 4, "links: {links:?}");
        assert!(
            links
                .iter()
                .any(|l| l.kind == LinkKind::File && l.target == "readme.md")
        );
        assert!(
            links
                .iter()
                .any(|l| l.kind == LinkKind::Embed && l.target == "pic.png")
        );
        assert!(
            links
                .iter()
                .any(|l| l.kind == LinkKind::Ref && l.target == "Some Page")
        );
        assert!(
            links
                .iter()
                .any(|l| l.kind == LinkKind::Tm && l.target == "foo/bar#some-id")
        );
    }

    #[test]
    fn bare_absolute_path_collects_as_file_bare_scheme_uri_stays_excluded() {
        // `@link(/etc/hosts)` (no explicit scheme prefix) is classified
        // `File` by shape (leading `/`) -- it must show up here same as any
        // other `File` link. `@link(https://example.com)` infers `Url` the
        // same way, but `Url` stays excluded from this crate regardless --
        // see this module's doc comment.
        let src = "@link(/etc/hosts)[Hosts]\n\n@link(https://example.com)[External2]\n";
        let doc = tomet_parser::parse_document(src).unwrap();
        let links = collect_links(&doc);

        assert_eq!(links.len(), 1, "links: {links:?}");
        assert!(
            links
                .iter()
                .any(|l| l.kind == LinkKind::File && l.target == "/etc/hosts")
        );
    }
}
