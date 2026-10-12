//! Address syntax representation and pure parser.
//!
//! Represents addresses used in link-shaped elements and interpolations,
//! such as `tmt:path/to/doc.tmt#intro`, `#intro`, `ref:Note#anchor`, etc.

use serde::{Deserialize, Serialize};

/// The kind/category of an address, without its payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddressKind {
    /// Same-document anchor (`#anchor` or `id:anchor`).
    Internal,
    /// Document link (`tmt:path#anchor`).
    Document,
    /// Wikilink note reference (`ref:Note#anchor`).
    Ref,
    /// Unresolved reference (`unresolved:Note`).
    Unresolved,
    /// Project-root-relative or source-relative file path (`file:path` or `path`).
    File,
    /// Directory reference (`dir:path`).
    Dir,
    /// External URL (`https:...` or `url:...`).
    Url,
}

impl AddressKind {
    /// All address kinds in fixed order.
    pub const ALL: [AddressKind; 7] = [
        AddressKind::Internal,
        AddressKind::Document,
        AddressKind::Ref,
        AddressKind::Unresolved,
        AddressKind::File,
        AddressKind::Dir,
        AddressKind::Url,
    ];

    /// Canonical string identifier for this address kind.
    pub fn as_str(&self) -> &'static str {
        match self {
            AddressKind::Internal => "id",
            AddressKind::Document => "tmt",
            AddressKind::Ref => "ref",
            AddressKind::Unresolved => "unresolved",
            AddressKind::File => "file",
            AddressKind::Dir => "dir",
            AddressKind::Url => "url",
        }
    }
}

impl std::str::FromStr for AddressKind {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "id" | "internal" => Ok(AddressKind::Internal),
            "tmt" | "doc" | "document" => Ok(AddressKind::Document),
            "ref" => Ok(AddressKind::Ref),
            "unresolved" => Ok(AddressKind::Unresolved),
            "file" => Ok(AddressKind::File),
            "dir" => Ok(AddressKind::Dir),
            "url" => Ok(AddressKind::Url),
            _ => Err(()),
        }
    }
}

/// The kind of document link tracked for workspace link checking and statistics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkKind {
    File,
    Dir,
    Embed,
    Tmt,
    Ref,
}

impl LinkKind {
    /// All recognized link kinds in fixed order.
    pub const ALL: [LinkKind; 5] = [
        LinkKind::File,
        LinkKind::Dir,
        LinkKind::Embed,
        LinkKind::Tmt,
        LinkKind::Ref,
    ];

    /// Canonical string identifier for this link kind.
    pub fn as_str(&self) -> &'static str {
        match self {
            LinkKind::File => "file",
            LinkKind::Dir => "dir",
            LinkKind::Embed => "embed",
            LinkKind::Tmt => "tmt",
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
            "tmt" => Ok(LinkKind::Tmt),
            "ref" => Ok(LinkKind::Ref),
            _ => Err(()),
        }
    }
}

/// Structured representation of a Tomet address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Address {
    /// Same-document anchor: `#intro` or `id:intro`.
    Internal { anchor: String },

    /// Document reference with an optional anchor: `tmt:path/to/doc.tmt#intro`.
    Document {
        path: String,
        anchor: Option<String>,
    },

    /// Note/wikilink reference by title or stem: `ref:Note Name#intro`.
    Ref {
        target: String,
        anchor: Option<String>,
    },

    /// Unresolved reference: `unresolved:Note Name`.
    Unresolved { target: String },

    /// Project-root-relative or relative file path: `file:path/to/file` or `readme.md`.
    File(String),

    /// Directory reference: `dir:path/to/dir`.
    Dir(String),

    /// External URL: `https://example.com` or explicit `url:...`.
    Url(String),
}

/// Recognized explicit scheme prefix names without their `:`.
pub const EXPLICIT_SCHEMES: [&str; 7] = ["tmt", "id", "ref", "unresolved", "file", "dir", "url"];

/// Checks whether `name` is one of the recognized explicit scheme prefixes.
pub fn is_explicit_scheme(name: &str) -> bool {
    EXPLICIT_SCHEMES.contains(&name)
}

impl Address {
    /// Returns the kind of this address.
    pub fn kind(&self) -> AddressKind {
        match self {
            Address::Internal { .. } => AddressKind::Internal,
            Address::Document { .. } => AddressKind::Document,
            Address::Ref { .. } => AddressKind::Ref,
            Address::Unresolved { .. } => AddressKind::Unresolved,
            Address::File(_) => AddressKind::File,
            Address::Dir(_) => AddressKind::Dir,
            Address::Url(_) => AddressKind::Url,
        }
    }

    /// Returns the raw path or target string without its scheme prefix.
    pub fn target_str(&self) -> &str {
        match self {
            Address::Internal { anchor } => anchor,
            Address::Document { path, .. } => path,
            Address::Ref { target, .. } => target,
            Address::Unresolved { target } => target,
            Address::File(path) => path,
            Address::Dir(path) => path,
            Address::Url(url) => url,
        }
    }

    /// Returns the anchor fragment, if any.
    pub fn anchor(&self) -> Option<&str> {
        match self {
            Address::Internal { anchor } => Some(anchor),
            Address::Document { anchor, .. } => anchor.as_deref(),
            Address::Ref { anchor, .. } => anchor.as_deref(),
            Address::Unresolved { .. } | Address::File(_) | Address::Dir(_) | Address::Url(_) => {
                None
            }
        }
    }

    /// Classifies `raw` and returns `(kind, rest)`, stripping any recognized scheme prefix.
    pub fn split_scheme(raw: &str) -> (AddressKind, &str) {
        let trimmed = raw.trim();

        // 1. Same-document anchor starting with `#` (e.g. `#intro`)
        if let Some(anchor) = trimmed.strip_prefix('#') {
            return (AddressKind::Internal, anchor);
        }

        // 2. Explicit scheme prefix (`scheme:...`)
        if let Some((scheme, rest)) = trimmed.split_once(':') {
            match scheme {
                "tmt" => return (AddressKind::Document, rest),
                "id" => return (AddressKind::Internal, rest),
                "ref" => return (AddressKind::Ref, rest),
                "unresolved" => return (AddressKind::Unresolved, rest),
                "file" => return (AddressKind::File, rest),
                "dir" => return (AddressKind::Dir, rest),
                "url" => return (AddressKind::Url, rest),
                _ => {
                    if is_scheme_uri(trimmed) {
                        return (AddressKind::Url, trimmed);
                    }
                }
            }
        }

        // 3. Generic URI (e.g. `https://...`) without leading `url:` prefix
        if is_scheme_uri(trimmed) {
            return (AddressKind::Url, trimmed);
        }

        // 4. Default fallback: treat as a file path
        (AddressKind::File, trimmed)
    }

    /// Parses a raw target string into an [`Address`].
    pub fn parse(raw: &str) -> Self {
        let (kind, rest) = Self::split_scheme(raw);
        match kind {
            AddressKind::Internal => Address::Internal {
                anchor: rest.to_string(),
            },
            AddressKind::Document => {
                let (path, anchor) = split_anchor(rest);
                Address::Document { path, anchor }
            }
            AddressKind::Ref => {
                let (target, anchor) = split_anchor(rest);
                Address::Ref { target, anchor }
            }
            AddressKind::Unresolved => Address::Unresolved {
                target: rest.to_string(),
            },
            AddressKind::File => Address::File(rest.to_string()),
            AddressKind::Dir => Address::Dir(rest.to_string()),
            AddressKind::Url => Address::Url(rest.to_string()),
        }
    }
}

/// Splits `s` into `(base, Some(anchor))` if `#` is present, or `(s, None)` if not.
/// If `#` is present but the anchor part is empty, anchor is `None`.
fn split_anchor(s: &str) -> (String, Option<String>) {
    if let Some((base, anchor)) = s.split_once('#') {
        let trimmed_anchor = anchor.trim();
        let anchor_opt = if trimmed_anchor.is_empty() {
            None
        } else {
            Some(trimmed_anchor.to_string())
        };
        (base.trim().to_string(), anchor_opt)
    } else {
        (s.trim().to_string(), None)
    }
}

/// Checks if `s` looks like `scheme://...` per RFC 3986.
fn is_scheme_uri(s: &str) -> bool {
    let Some(colon) = s.find(':') else {
        return false;
    };
    let (scheme, rest) = s.split_at(colon);
    !scheme.is_empty()
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        && rest.starts_with("://")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_internal_anchors() {
        assert_eq!(
            Address::parse("#intro"),
            Address::Internal {
                anchor: "intro".to_string()
            }
        );
        assert_eq!(
            Address::parse("id:intro"),
            Address::Internal {
                anchor: "intro".to_string()
            }
        );
    }

    #[test]
    fn parse_document_addresses() {
        // Canonical tmt:
        assert_eq!(
            Address::parse("tmt:docs/spec.tmt#intro"),
            Address::Document {
                path: "docs/spec.tmt".to_string(),
                anchor: Some("intro".to_string()),
            }
        );
        assert_eq!(
            Address::parse("tmt:docs/spec.tmt"),
            Address::Document {
                path: "docs/spec.tmt".to_string(),
                anchor: None,
            }
        );
        // Empty anchor after #
        assert_eq!(
            Address::parse("tmt:docs/spec.tmt#"),
            Address::Document {
                path: "docs/spec.tmt".to_string(),
                anchor: None,
            }
        );
        // tm: is no longer a document scheme (falls back to File)
        assert_eq!(
            Address::parse("tm:docs/spec.tmt"),
            Address::File("tm:docs/spec.tmt".to_string())
        );
    }

    #[test]
    fn parse_ref_addresses() {
        assert_eq!(
            Address::parse("ref:My Note#section"),
            Address::Ref {
                target: "My Note".to_string(),
                anchor: Some("section".to_string()),
            }
        );
        assert_eq!(
            Address::parse("ref:My Note"),
            Address::Ref {
                target: "My Note".to_string(),
                anchor: None,
            }
        );
    }

    #[test]
    fn parse_unresolved_addresses() {
        assert_eq!(
            Address::parse("unresolved:Future Note"),
            Address::Unresolved {
                target: "Future Note".to_string(),
            }
        );
    }

    #[test]
    fn parse_files_and_dirs() {
        assert_eq!(
            Address::parse("file:readme.md"),
            Address::File("readme.md".to_string())
        );
        assert_eq!(
            Address::parse("readme.md"),
            Address::File("readme.md".to_string())
        );
        assert_eq!(
            Address::parse("./sub/doc.txt"),
            Address::File("./sub/doc.txt".to_string())
        );
        assert_eq!(
            Address::parse("dir:docs/spec"),
            Address::Dir("docs/spec".to_string())
        );
    }

    #[test]
    fn parse_urls() {
        assert_eq!(
            Address::parse("https://example.com/foo#bar"),
            Address::Url("https://example.com/foo#bar".to_string())
        );
        assert_eq!(
            Address::parse("url:ftp://example.com"),
            Address::Url("ftp://example.com".to_string())
        );
    }
}
