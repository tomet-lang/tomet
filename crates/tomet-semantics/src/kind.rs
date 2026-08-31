use tomet_ast::{Element, Sigil};

/// What a parsed `Element` officially means, replacing the ad-hoc
/// stringly-typed `kind: String` that `tomet-html` and
/// `tomet-markdown/export.rs` each used to compute independently.
///
/// This only expresses *recognition* ("this element is `@meta`"), not
/// *action* ("`@meta` produces no output") -- consumers still decide for
/// themselves what to do with a given kind, since the same kind can mean
/// different things for different output formats (e.g. `Meta` renders as
/// nothing in both HTML and CommonMark today, but for unrelated reasons
/// specific to each format).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElementKind {
    Kind,
    Version,
    Meta,
    Config,
    Links,
    /// The one officially-supported link element, `@link(target:...)`
    /// (or the positional `@link(...)` shorthand -- see
    /// `crate::positional::builtin_positional_arg_key`). What kind of
    /// target it is (url/file/tm/id/ref) is not carried by the element or
    /// its key anymore -- it's derived from the `target` string's own
    /// scheme prefix via `crate::target::target_scheme`, once the target
    /// has been extracted with `crate::target::link_target`.
    Link,
    Embed,
    Hr,
    Em,
    Strong,
    Mark,
    Codeblock,
    Blockquote,
    Table,
    Heading,
    Icon,
    /// `Element { sigil: Type("ol"), .. }` -- a `-.` (auto-numbered) list.
    /// See `crate::list`.
    OrderedList,
    /// `Element { sigil: Type("ul"), .. }` -- a plain `-` list.
    /// See `crate::list`.
    UnorderedList,
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
    /// `Sigil::Dollar` -- `${...}` interpolation. A dedicated kind (not
    /// `Custom`) since, like `Em`/`Hr`/`Ref`, it's built-in recognized
    /// syntax with fixed grammar-level meaning, not an arbitrary
    /// user-chosen element name.
    Interp,
}

impl ElementKind {
    /// The kind's name as `tomet-html`/`tomet-markdown` (and
    /// the TUI's structural search) use it: as a `data-*`/CSS-class
    /// fragment, or to match against `match kind.as_str() { "hr" => ..., ...
    /// }`. `Custom(name)` returns `name` itself.
    pub fn as_str(&self) -> &str {
        match self {
            ElementKind::Kind => "kind",
            ElementKind::Version => "version",
            ElementKind::Meta => "meta",
            ElementKind::Config => "config",
            ElementKind::Links => "links",
            ElementKind::Link => "link",
            ElementKind::Embed => "embed",
            ElementKind::Icon => "icon",
            ElementKind::Hr => "hr",
            ElementKind::Em => "em",
            ElementKind::Strong => "strong",
            ElementKind::Mark => "mark",
            ElementKind::Codeblock => "codeblock",
            ElementKind::Blockquote => "blockquote",
            ElementKind::Table => "table",
            ElementKind::Heading => "heading",
            ElementKind::OrderedList => "ol",
            ElementKind::UnorderedList => "ul",
            ElementKind::Custom(name) => name,
            ElementKind::Bare => "bare",
            ElementKind::Interp => "interp",
        }
    }
}

/// Recognized `Sigil::Type`/`Sigil::At(Some(_))` names with fixed meaning
/// -- kept as one list so `classify`'s name -> variant match and
/// `ElementKind::as_str`'s variant -> name match can't silently drift
/// apart (see `builtin_kind_round_trips_through_as_str` below, which
/// checks every entry here).
const BUILTIN_KINDS: [(&str, ElementKind); 18] = [
    ("kind", ElementKind::Kind),
    ("version", ElementKind::Version),
    ("meta", ElementKind::Meta),
    ("config", ElementKind::Config),
    ("links", ElementKind::Links),
    ("link", ElementKind::Link),
    ("embed", ElementKind::Embed),
    ("icon", ElementKind::Icon),
    ("hr", ElementKind::Hr),
    ("em", ElementKind::Em),
    ("strong", ElementKind::Strong),
    ("mark", ElementKind::Mark),
    ("codeblock", ElementKind::Codeblock),
    ("blockquote", ElementKind::Blockquote),
    ("table", ElementKind::Table),
    ("heading", ElementKind::Heading),
    ("ol", ElementKind::OrderedList),
    ("ul", ElementKind::UnorderedList),
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

/// Classifies `el` by its `Sigil`: a named `Sigil::Type` or `Sigil::At(Some)`
/// against the built-in vocabulary (falling back to `Custom` for anything
/// else, e.g. a hand-authored `<caution>` or `@caution`), an unnamed
/// `Sigil::At(None)` always as `Custom("at")`, and `Sigil::Bare` always as
/// `ElementKind::Bare`. No inference from `args` happens anywhere here
/// anymore -- `@(url:...)`/`@link(url:...)`-style key-based guessing was
/// retired; the only way to get `ElementKind::Link` is to write `@link`/
/// `<link>` explicitly.
pub fn classify(el: &Element) -> ElementKind {
    match &el.sigil {
        Sigil::Type(name) | Sigil::At(Some(name)) => classify_name(name),
        Sigil::At(None) => ElementKind::Custom("at".to_string()),
        Sigil::Bare => ElementKind::Bare,
        Sigil::Dollar => ElementKind::Interp,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_ast::Value;
    use tomet_tree::element_new;

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
        let el = element_new(Sigil::Type("caution".to_string()));
        assert_eq!(classify(&el), ElementKind::Custom("caution".to_string()));
    }

    #[test]
    fn type_sigil_with_builtin_name_is_recognized() {
        let el = element_new(Sigil::Type("codeblock".to_string()));
        assert_eq!(classify(&el), ElementKind::Codeblock);
    }

    #[test]
    fn named_at_sigil_is_recognized() {
        let el = element_new(Sigil::At(Some("meta".to_string())));
        assert_eq!(classify(&el), ElementKind::Meta);
    }

    #[test]
    fn named_at_sigil_with_link_name_is_recognized() {
        let mut el = element_new(Sigil::At(Some("link".to_string())));
        el.args = Some(Value::Map(vec![(
            "target".to_string(),
            Value::String("https://example.com".to_string()),
        )]));
        assert_eq!(classify(&el), ElementKind::Link);
    }

    #[test]
    fn named_at_sigil_with_no_inferable_args_stays_custom() {
        let el = element_new(Sigil::At(Some("caution".to_string())));
        assert_eq!(classify(&el), ElementKind::Custom("caution".to_string()));
    }

    #[test]
    fn unnamed_at_sigil_is_always_custom_at() {
        // No inference happens for a bare `@(...)` anymore -- kind is
        // decided purely by the element's name (`@link`, `<link>`, ...),
        // never guessed from `args`.
        let mut el = element_new(Sigil::At(None));
        el.args = Some(Value::Map(vec![(
            "target".to_string(),
            Value::String("https://example.com".to_string()),
        )]));
        assert_eq!(classify(&el), ElementKind::Custom("at".to_string()));
    }

    #[test]
    fn unnamed_at_sigil_with_no_recognized_key_is_custom_at() {
        let el = element_new(Sigil::At(None));
        assert_eq!(classify(&el), ElementKind::Custom("at".to_string()));
    }

    #[test]
    fn bare_sigil_is_always_bare() {
        let el = element_new(Sigil::Bare);
        assert_eq!(classify(&el), ElementKind::Bare);
    }

    #[test]
    fn dollar_sigil_is_always_interp() {
        let el = element_new(Sigil::Dollar);
        assert_eq!(classify(&el), ElementKind::Interp);
    }
}
