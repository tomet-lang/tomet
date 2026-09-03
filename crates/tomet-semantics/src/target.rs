//! Extracts the raw target string of a link-shaped element (`Link`/`Embed`,
//! per [`crate::classify`]) under their one canonical `target` key, and
//! classifies *what kind* of target that string is (`TargetScheme`) by its
//! own scheme prefix -- `url`/`file`/`tm`/`id`/`ref` are no longer carried
//! by which argument key was used (there's only ever `target:` now); they
//! live entirely in the target string itself. This is the canonical
//! extraction, adopted by `tomet-html`/`tomet-markdown`/`tomet-links`
//! instead of each reading `el.args` raw.

use tomet_ast::Element;
use tomet_tree::ValueExt;

use crate::kind::ElementKind;
use crate::positional::normalized_element_args;

/// The raw target string of a link-shaped element, if it has one. `kind`
/// should be `classify_lenient(el)`'s result -- only `Link`/`Embed` ever return
/// `Some`, everything else returns `None`.
///
/// `tm:path/to/doc#some-id`'s `#fragment` is returned verbatim, still
/// attached -- splitting it off is a caller concern, not done here.
///
/// Deliberately stricter than the existing renderers: a non-string value
/// under `target` (e.g. `target: 42`) returns `None` rather than a
/// stringified `"42"`, which would be a wrong filesystem-lookup target
/// even though it's harmless for display purposes.
pub fn link_target(el: &Element, kind: &ElementKind) -> Option<String> {
    if !matches!(kind, ElementKind::Link | ElementKind::Embed) {
        return None;
    }
    let args = normalized_element_args(el)?;
    if let Some(target) = args.get("target").and_then(|v| v.as_str()) {
        return Some(target.to_string());
    }
    // `@link(https://x)`/`<embed>(a.png)`: both `link` and `embed` are
    // in `builtin_positional_arg_key` (`-> "target"`), so a bare
    // positional scalar should already have arrived here as a map via
    // `normalized_element_args` -- this fallback only matters for a
    // `Sigil` this function is never called with a `Link`/`Embed`
    // `kind` for (guarded above), so it's effectively unreachable in
    // practice but kept for symmetry/safety rather than relying on
    // that invariant silently.
    if let Some(s) = args.as_str() {
        return Some(s.to_string());
    }
    None
}

/// Classifies `el` and extracts its link target in one step, for callers
/// that don't already have `classify_lenient(el)`'s result on hand.
pub fn link_target_of(el: &Element) -> Option<(ElementKind, String)> {
    let kind = crate::classify_lenient(el);
    link_target(el, &kind).map(|target| (kind, target))
}

/// What kind of thing a `target` string points at, derived purely from the
/// string's own scheme prefix (never from which key/element name carried
/// it -- there's only ever one key, `target`, for both `@link` and
/// `<embed>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetScheme {
    /// `scheme://...` (`https://...`, ...), or an explicit `url:...`
    /// prefix -- an external URL, opaque to this codebase.
    Url,
    /// An OS-absolute path (`/...`), a doc-relative path (`./...`/`../...`),
    /// an explicit `file:...` (project-root-relative) prefix, or -- the
    /// fallback for anything with no recognized prefix at all (a bare
    /// relative word like `readme.md`) -- treated as a project/doc-relative
    /// file path by default, since `target` no longer has a separate
    /// explicit key to opt into that meaning.
    File,
    /// `tm:path/to/doc` -- another Tomet document, project-root-relative
    /// like `File` but semantically a document, not an arbitrary
    /// attachment. Composable with `#fragment` to target a specific id
    /// inside that document.
    Tm,
    /// `id:...` -- id-based reference (same-document-only for now).
    Id,
    /// `ref:...` -- resolved by searching the project for a file whose
    /// name/title matches the given string (the old "wikilink" resolution).
    Ref,
}

impl TargetScheme {
    /// Mirrors the retired per-kind names (`"url"/"file"/"tm"/"id"/"ref"`)
    /// so callers building CSS classes / `data-*` attributes keep the same
    /// vocabulary as before this scheme moved from key to value.
    pub fn as_str(&self) -> &'static str {
        match self {
            TargetScheme::Url => "url",
            TargetScheme::File => "file",
            TargetScheme::Tm => "tm",
            TargetScheme::Id => "id",
            TargetScheme::Ref => "ref",
        }
    }
}

/// Classifies a `target` string's scheme and strips a recognized explicit
/// prefix (`tm:`/`id:`/`ref:`/`file:`) off of it, if any -- an implicit
/// shape (`scheme://...`, a leading `/`, `./`, `../`, or no recognizable
/// prefix at all) is returned unchanged since there's nothing to strip.
pub fn target_scheme(target: &str) -> (TargetScheme, &str) {
    for (prefix, scheme) in [
        ("tm:", TargetScheme::Tm),
        ("id:", TargetScheme::Id),
        ("ref:", TargetScheme::Ref),
        ("file:", TargetScheme::File),
        ("url:", TargetScheme::Url),
    ] {
        if let Some(rest) = target.strip_prefix(prefix) {
            return (scheme, rest);
        }
    }
    if is_scheme_uri(target) {
        return (TargetScheme::Url, target);
    }
    (TargetScheme::File, target)
}

/// Whether `s` looks like `scheme://...` per RFC 3986's scheme grammar (a
/// leading letter, then letters/digits/`+`/`-`/`.`).
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
    use tomet_ast::{Sigil, Value};
    use tomet_tree::element_new;

    fn map_el(sigil: Sigil, entries: Vec<(&str, Value)>) -> Element {
        let mut el = element_new(sigil);
        el.args = Some(Value::Map(
            entries
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        ));
        el
    }

    fn s(v: &str) -> Value {
        Value::String(v.to_string())
    }

    #[test]
    fn named_target_key_on_typed_link_element() {
        let el = map_el(Sigil::named("link"), vec![("target", s("x.md"))]);
        assert_eq!(crate::classify_lenient(&el), ElementKind::Link);
        assert_eq!(
            link_target_of(&el),
            Some((ElementKind::Link, "x.md".to_string()))
        );
    }

    #[test]
    fn named_at_link_element() {
        let el = map_el(Sigil::named("link"), vec![("target", s("x.md"))]);
        assert_eq!(crate::classify_lenient(&el), ElementKind::Link);
        assert_eq!(
            link_target_of(&el),
            Some((ElementKind::Link, "x.md".to_string()))
        );
    }

    #[test]
    fn bare_positional_link_has_no_named_key() {
        // `@link(x.md)`: args is a bare `Value::String`, not a map, but
        // "link" IS in `builtin_positional_arg_key` (-> "target"), so
        // `normalized_element_args` turns this into a map before
        // `link_target` ever sees it.
        let mut el = element_new(Sigil::named("link"));
        el.args = Some(s("x.md"));
        assert_eq!(crate::classify_lenient(&el), ElementKind::Link);
        assert_eq!(
            link_target_of(&el),
            Some((ElementKind::Link, "x.md".to_string()))
        );
    }

    #[test]
    fn embed_target_named_key() {
        let el = map_el(Sigil::named("embed"), vec![("target", s("a.png"))]);
        assert_eq!(crate::classify_lenient(&el), ElementKind::Embed);
        assert_eq!(
            link_target_of(&el),
            Some((ElementKind::Embed, "a.png".to_string()))
        );
    }

    #[test]
    fn embed_bare_positional_maps_to_target_via_normalized_args() {
        // `<embed>(a.png)`: "embed" IS in builtin_positional_arg_key
        // (-> "target"), so normalized_element_args already turns this
        // into a map before link_target ever sees it.
        let mut el = element_new(Sigil::named("embed"));
        el.args = Some(s("a.png"));
        assert_eq!(
            link_target_of(&el),
            Some((ElementKind::Embed, "a.png".to_string()))
        );
    }

    #[test]
    fn target_scheme_recognizes_explicit_prefixes() {
        assert_eq!(target_scheme("tm:foo/bar"), (TargetScheme::Tm, "foo/bar"));
        assert_eq!(
            target_scheme("tm:foo/bar#some-id"),
            (TargetScheme::Tm, "foo/bar#some-id")
        );
        assert_eq!(target_scheme("id:greeting"), (TargetScheme::Id, "greeting"));
        assert_eq!(
            target_scheme("ref:Some Page"),
            (TargetScheme::Ref, "Some Page")
        );
        assert_eq!(target_scheme("file:x.md"), (TargetScheme::File, "x.md"));
    }

    #[test]
    fn target_scheme_recognizes_implicit_shapes() {
        assert_eq!(
            target_scheme("https://example.com"),
            (TargetScheme::Url, "https://example.com")
        );
        assert_eq!(
            target_scheme("/etc/hosts"),
            (TargetScheme::File, "/etc/hosts")
        );
        assert_eq!(
            target_scheme("./readme.md"),
            (TargetScheme::File, "./readme.md")
        );
    }

    #[test]
    fn target_scheme_defaults_a_bare_word_to_file() {
        // No `/`, no `scheme://`, no recognized explicit prefix -- there's
        // only one key (`target`) left to carry meaning, so this is the
        // fallback rather than an error.
        assert_eq!(
            target_scheme("readme.md"),
            (TargetScheme::File, "readme.md")
        );
    }

    #[test]
    fn non_string_value_under_target_key_is_rejected() {
        let el = map_el(Sigil::named("link"), vec![("target", Value::Int(42))]);
        assert_eq!(link_target_of(&el), None);
    }

    #[test]
    fn non_link_kind_has_no_target() {
        let el = map_el(Sigil::named("meta"), vec![("format", s("json"))]);
        assert_eq!(crate::classify_lenient(&el), ElementKind::Meta);
        assert_eq!(link_target_of(&el), None);
    }
}
