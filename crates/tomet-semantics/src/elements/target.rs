//! Extracts the raw target string of a link-shaped element (`Link`/`Embed`,
//! per [`crate::classify_std`]) under their one canonical `target` key, and
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
/// should be `classify_std_lenient(el)`'s result -- only `Link`/`Embed` ever return
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
    target_arg(el)
}

/// The path a `@file`/`@dir` names, or `None` for anything else.
///
/// No scheme prefix to strip: the element name already said which of the
/// two this is, so a `file:`/`dir:` inside the value would be saying it
/// twice. That is the difference from [`link_target`], where one element
/// serves every scheme and the string has to carry which.
pub fn path_target(el: &Element, kind: &ElementKind) -> Option<String> {
    if !matches!(kind, ElementKind::File | ElementKind::Dir) {
        return None;
    }
    target_arg(el)
}

fn target_arg(el: &Element) -> Option<String> {
    let args = normalized_element_args(el)?;
    if let Some(target) = args.get("target").and_then(|v| v.as_str()) {
        return Some(target.to_string());
    }
    // `@link(https://x)`/`@embed(a.png)`: both `link` and `embed` are
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
/// that don't already have `classify_std_lenient(el)`'s result on hand.
pub fn link_target_of(el: &Element) -> Option<(ElementKind, String)> {
    let kind = crate::classify_std_lenient(el);
    link_target(el, &kind).map(|target| (kind, target))
}

/// [`path_target`] with the kind it classified to, for a caller sweeping
/// a document without one in hand.
pub fn path_target_of(el: &Element) -> Option<(ElementKind, String)> {
    let kind = crate::classify_std_lenient(el);
    path_target(el, &kind).map(|target| (kind, target))
}

pub use tomet_address::{
    Address, AddressKind, AddressKind as TargetScheme, EXPLICIT_SCHEMES, is_explicit_scheme,
};

/// Classifies a `target` string's scheme and strips a recognized explicit prefix.
pub fn target_scheme(target: &str) -> (AddressKind, &str) {
    Address::split_scheme(target)
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
        assert_eq!(crate::classify_std_lenient(&el), ElementKind::Link);
        assert_eq!(
            link_target_of(&el),
            Some((ElementKind::Link, "x.md".to_string()))
        );
    }

    #[test]
    fn named_at_link_element() {
        let el = map_el(Sigil::named("link"), vec![("target", s("x.md"))]);
        assert_eq!(crate::classify_std_lenient(&el), ElementKind::Link);
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
        assert_eq!(crate::classify_std_lenient(&el), ElementKind::Link);
        assert_eq!(
            link_target_of(&el),
            Some((ElementKind::Link, "x.md".to_string()))
        );
    }

    #[test]
    fn embed_target_named_key() {
        let el = map_el(Sigil::named("embed"), vec![("target", s("a.png"))]);
        assert_eq!(crate::classify_std_lenient(&el), ElementKind::Embed);
        assert_eq!(
            link_target_of(&el),
            Some((ElementKind::Embed, "a.png".to_string()))
        );
    }

    #[test]
    fn embed_bare_positional_maps_to_target_via_normalized_args() {
        // `@embed(a.png)`: "embed" IS in builtin_positional_arg_key
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
        assert_eq!(
            target_scheme("tmt:foo/bar"),
            (AddressKind::Document, "foo/bar")
        );
        assert_eq!(
            target_scheme("tmt:foo/bar#some-id"),
            (AddressKind::Document, "foo/bar#some-id")
        );
        assert_eq!(
            target_scheme("id:greeting"),
            (AddressKind::Internal, "greeting")
        );
        assert_eq!(
            target_scheme("#greeting"),
            (AddressKind::Internal, "greeting")
        );
        assert_eq!(
            target_scheme("ref:Some Page"),
            (AddressKind::Ref, "Some Page")
        );
        assert_eq!(
            target_scheme("unresolved:Future Note"),
            (AddressKind::Unresolved, "Future Note")
        );
        assert_eq!(target_scheme("file:x.md"), (AddressKind::File, "x.md"));
        assert_eq!(target_scheme("dir:spec"), (AddressKind::Dir, "spec"));
    }

    #[test]
    fn every_explicit_scheme_round_trips_through_its_own_name() {
        for name in EXPLICIT_SCHEMES {
            let input = format!("{name}:x");
            let (scheme, rest) = target_scheme(&input);
            assert_eq!(scheme.as_str(), name, "{name}: prefix and as_str disagree");
            assert_eq!(rest, "x");
        }
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
        let el = map_el(Sigil::document("meta"), vec![("key", s("val"))]);
        assert_eq!(crate::classify_std_lenient(&el), ElementKind::Meta);
        assert_eq!(link_target_of(&el), None);
    }
}

#[cfg(test)]
mod path_tests {
    use super::*;
    use tomet_ast::{Sigil, Value};
    use tomet_tree::element_new;

    fn path_el(name: &str, value: &str) -> Element {
        let mut el = element_new(Sigil::named(name));
        el.args = Some(Value::String(value.to_string()));
        el
    }

    #[test]
    fn file_and_dir_carry_a_bare_path() {
        // No scheme prefix to strip: the element name is the scheme.
        // Writing `@file(file:x)` would say it twice, and this is what
        // makes that visible -- the value comes back whole.
        let el = path_el("file", "docs/README.tmt");
        assert_eq!(
            path_target(&el, &ElementKind::File),
            Some("docs/README.tmt".to_string())
        );
        let el = path_el("dir", "docs/spec");
        assert_eq!(
            path_target(&el, &ElementKind::Dir),
            Some("docs/spec".to_string())
        );
    }

    #[test]
    fn path_target_and_link_target_do_not_answer_for_each_other() {
        // Two accessors because they are two questions. A caller sweeping
        // for links must not pick up a mention, and a caller sweeping for
        // mentions must not pick up a link -- they render differently and
        // only one produces an `<a>`.
        let file = path_el("file", "x.rs");
        assert_eq!(link_target(&file, &ElementKind::File), None);
        let link = path_el("link", "x.rs");
        assert_eq!(path_target(&link, &ElementKind::Link), None);
    }
}
