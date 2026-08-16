use typedmark_ast::{Element, Sigil, Value};

use crate::infer_at_kind;

/// What a parsed `Element` officially means, replacing the ad-hoc
/// stringly-typed `kind: String` that `typedmark-html` and
/// `typedmark-markdown/export.rs` each used to compute independently.
///
/// This only expresses *recognition* ("this element is `@meta`"), not
/// *action* ("`@meta` produces no output") -- consumers still decide for
/// themselves what to do with a given kind, since the same kind can mean
/// different things for different output formats (e.g. `Meta` renders as
/// nothing in both HTML and CommonMark today, but for unrelated reasons
/// specific to each format).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElementKind {
    Meta,
    Config,
    Links,
    Url,
    File,
    Ref,
    Embed,
    Hr,
    Em,
    Strong,
    Mark,
    Codeblock,
    Blockquote,
    /// A `<T>`/`@name` (or an unnamed `@` with no inferred key) that
    /// doesn't match any of the built-in kinds above -- consumers fall
    /// back to their own generic rendering, keyed on the name.
    Custom(String),
    /// `Sigil::Bare` -- the untyped `(key)[content]` entries inside a
    /// container like `@links{}`. Its string form ("bare") can collide
    /// with a user writing a literal `<bare>`/`@bare` element (which
    /// would classify as `Custom("bare")` instead), but that's the same
    /// ambiguity the old stringly-typed `element_kind()` already had --
    /// not a regression.
    Bare,
}

impl ElementKind {
    /// The kind's name as `typedmark-html`/`typedmark-markdown` (and
    /// the TUI's structural search) use it: as a `data-*`/CSS-class
    /// fragment, or to match against `match kind.as_str() { "hr" => ..., ...
    /// }`. `Custom(name)` returns `name` itself.
    pub fn as_str(&self) -> &str {
        match self {
            ElementKind::Meta => "meta",
            ElementKind::Config => "config",
            ElementKind::Links => "links",
            ElementKind::Url => "url",
            ElementKind::File => "file",
            ElementKind::Ref => "ref",
            ElementKind::Embed => "embed",
            ElementKind::Hr => "hr",
            ElementKind::Em => "em",
            ElementKind::Strong => "strong",
            ElementKind::Mark => "mark",
            ElementKind::Codeblock => "codeblock",
            ElementKind::Blockquote => "blockquote",
            ElementKind::Custom(name) => name,
            ElementKind::Bare => "bare",
        }
    }
}

/// Recognized `Sigil::Type`/`Sigil::At(Some(_))` names with fixed meaning
/// -- kept as one list so `classify`'s name -> variant match and
/// `ElementKind::as_str`'s variant -> name match can't silently drift
/// apart (see `builtin_kind_round_trips_through_as_str` below, which
/// checks every entry here).
const BUILTIN_KINDS: [(&str, ElementKind); 13] = [
    ("meta", ElementKind::Meta),
    ("config", ElementKind::Config),
    ("links", ElementKind::Links),
    ("url", ElementKind::Url),
    ("file", ElementKind::File),
    ("ref", ElementKind::Ref),
    ("embed", ElementKind::Embed),
    ("hr", ElementKind::Hr),
    ("em", ElementKind::Em),
    ("strong", ElementKind::Strong),
    ("mark", ElementKind::Mark),
    ("codeblock", ElementKind::Codeblock),
    ("blockquote", ElementKind::Blockquote),
];

fn builtin_kind(name: &str) -> Option<ElementKind> {
    BUILTIN_KINDS
        .iter()
        .find(|(builtin, _)| *builtin == name)
        .map(|(_, kind)| kind.clone())
}

fn classify_name(name: &str) -> ElementKind {
    builtin_kind(name).unwrap_or_else(|| ElementKind::Custom(name.to_string()))
}

/// A named `@name(...)` element: `name` wins outright if it's one of the
/// built-in kinds (`@embed(...)`, `@url(...)`, ...) -- an explicit name for
/// a *different* real kind is never silently reinterpreted. Otherwise this
/// falls back to the same `url`/`file`/`ref` inference a bare `@(...)`
/// gets from [`infer_at_kind`], so giving an inference-eligible element an
/// organizational name (`@link(url:...)`) doesn't quietly opt it out of
/// being a real link -- previously that rendered as a content-less,
/// non-clickable generic element (see
/// `.agents/tasks/ssg-readiness.md` step 4). `<T>` typed elements
/// (`Sigil::Type`) don't get this fallback: inference is specifically an
/// `@`-shorthand feature (see [`INFERRED_AT_KEYS`]'s doc comment), not a
/// general "authors may name anything" convenience.
fn classify_named_at(name: &str, args: Option<&Value>) -> ElementKind {
    if let Some(kind) = builtin_kind(name) {
        return kind;
    }
    match infer_at_kind(args) {
        Some(key) => classify_name(key),
        None => ElementKind::Custom(name.to_string()),
    }
}

/// Classifies `el` by its `Sigil`: a named `Sigil::Type` against the
/// built-in vocabulary (falling back to `Custom` for anything else, e.g. a
/// hand-authored `<caution>`), a named `Sigil::At(Some)` the same way but
/// with an inference fallback (see [`classify_named_at`]), an unnamed
/// `Sigil::At(None)` via `infer_at_kind` (falling back to `Custom("at")` if
/// no key is recognized), and `Sigil::Bare` always as `ElementKind::Bare`.
pub fn classify(el: &Element) -> ElementKind {
    match &el.sigil {
        Sigil::Type(name) => classify_name(name),
        Sigil::At(Some(name)) => classify_named_at(name, el.args.as_ref()),
        Sigil::At(None) => match infer_at_kind(el.args.as_ref()) {
            Some(key) => classify_name(key),
            None => ElementKind::Custom("at".to_string()),
        },
        Sigil::Bare => ElementKind::Bare,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use typedmark_ast::Value;

    #[test]
    fn builtin_kind_round_trips_through_as_str() {
        for (name, kind) in &BUILTIN_KINDS {
            assert_eq!(kind.as_str(), *name, "as_str() mismatch for {name:?}");
            assert_eq!(
                classify_name(name),
                *kind,
                "classify_name() mismatch for {name:?}"
            );
        }
    }

    #[test]
    fn type_sigil_with_unrecognized_name_is_custom() {
        let el = Element::new(Sigil::Type("caution".to_string()));
        assert_eq!(classify(&el), ElementKind::Custom("caution".to_string()));
    }

    #[test]
    fn type_sigil_with_builtin_name_is_recognized() {
        let el = Element::new(Sigil::Type("codeblock".to_string()));
        assert_eq!(classify(&el), ElementKind::Codeblock);
    }

    #[test]
    fn named_at_sigil_is_recognized() {
        let el = Element::new(Sigil::At(Some("meta".to_string())));
        assert_eq!(classify(&el), ElementKind::Meta);
    }

    #[test]
    fn named_at_sigil_with_inferable_args_still_infers() {
        // `@link(url:...)` -- "link" isn't a built-in kind, so an explicit
        // name doesn't opt this out of the same `url`/`file`/`ref`
        // inference a bare `@(url:...)` gets. See
        // `.agents/tasks/ssg-readiness.md` step 4.
        let mut el = Element::new(Sigil::At(Some("link".to_string())));
        el.args = Some(Value::Map(vec![(
            "url".to_string(),
            Value::String("https://example.com".to_string()),
        )]));
        assert_eq!(classify(&el), ElementKind::Url);
    }

    #[test]
    fn named_at_sigil_with_a_real_builtin_name_is_never_reinterpreted() {
        // `@embed(url:...)` -- "embed" IS a built-in kind, so the explicit
        // name always wins outright, even though its args would also
        // satisfy `url`/`file`/`ref` inference.
        let mut el = Element::new(Sigil::At(Some("embed".to_string())));
        el.args = Some(Value::Map(vec![(
            "url".to_string(),
            Value::String("https://example.com".to_string()),
        )]));
        assert_eq!(classify(&el), ElementKind::Embed);
    }

    #[test]
    fn named_at_sigil_with_no_inferable_args_stays_custom() {
        let el = Element::new(Sigil::At(Some("caution".to_string())));
        assert_eq!(classify(&el), ElementKind::Custom("caution".to_string()));
    }

    #[test]
    fn unnamed_at_sigil_infers_from_args() {
        let mut el = Element::new(Sigil::At(None));
        el.args = Some(Value::Map(vec![(
            "url".to_string(),
            Value::String("https://example.com".to_string()),
        )]));
        assert_eq!(classify(&el), ElementKind::Url);
    }

    #[test]
    fn unnamed_at_sigil_with_no_recognized_key_is_custom_at() {
        let el = Element::new(Sigil::At(None));
        assert_eq!(classify(&el), ElementKind::Custom("at".to_string()));
    }

    #[test]
    fn bare_sigil_is_always_bare() {
        let el = Element::new(Sigil::Bare);
        assert_eq!(classify(&el), ElementKind::Bare);
    }
}
