use tomet_ast::{Element, Name, Sigil};

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
    /// `#settings{...}` -- the schema/config block. Not previously in
    /// `BUILTIN_KINDS` despite three places matching it by raw string; a
    /// bare name has to be built-in now, so it is listed properly.
    Settings,
    /// `#import(file:..., as:ns)` -- binds a namespace. The binding is
    /// resolved by `tomet-resolver`; this only recognizes the element.
    Import,
    /// `#references[...]` -- the container for remote connections.
    References,
    /// `#id(taskA):{...}` -- attaches attributes to a remote element by
    /// id. Previously spelled `<id:taskA>`, with the target smuggled
    /// through `<T>`'s permissive name charset.
    Id,
    Blueprint,
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
            ElementKind::Settings => "settings",
            ElementKind::Import => "import",
            ElementKind::References => "references",
            ElementKind::Id => "id",
            ElementKind::Blueprint => "blueprint",
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

/// Recognized **bare** names with fixed meaning -- kept as one list so
/// `classify`'s name -> variant match and `ElementKind::as_str`'s variant
/// -> name match can't silently drift apart (see
/// `builtin_kind_round_trips_through_as_str` below, which checks every
/// entry here).
///
/// Bare names are reserved for exactly this list. A user-defined element
/// must be namespaced (`deck.bookmark`), which is why an unrecognized bare
/// name is an error rather than a `Custom` kind.
pub const BUILTIN_KINDS: [(&str, ElementKind); 23] = [
    ("kind", ElementKind::Kind),
    ("version", ElementKind::Version),
    ("meta", ElementKind::Meta),
    ("config", ElementKind::Config),
    ("settings", ElementKind::Settings),
    ("import", ElementKind::Import),
    ("references", ElementKind::References),
    ("id", ElementKind::Id),
    ("blueprint", ElementKind::Blueprint),
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

/// Classifies a [`Name`].
///
/// A namespaced name is always `Custom` -- namespaces are exactly how a
/// user-defined element declares it is not part of the built-in
/// vocabulary. A bare name must be in [`BUILTIN_KINDS`]; anything else is
/// [`UnknownName`], not a silent `Custom`.
///
/// That silent fallback -- `builtin_kind(name).unwrap_or_else(|| Custom(name))`
/// -- is where the old sigil ambiguity actually lived. `<T>` and `@name`
/// were meant to separate official from user-defined elements, but both
/// classified through this one arm, so the distinction never reached the
/// tree. Namespaces carry it now, and this returns an error instead of
/// inventing a kind.
pub fn classify_name(name: &Name) -> Result<ElementKind, UnknownName> {
    if !name.is_bare() {
        return Ok(ElementKind::Custom(name.to_string()));
    }
    builtin_kind(&name.name).ok_or_else(|| UnknownName {
        name: name.name.clone(),
    })
}

/// An unrecognized bare element name.
///
/// Bare names are reserved for Tomet's own vocabulary, so this is what a
/// `#tag` alone on a line reports -- deliberately, since hashtags are not
/// a feature and a bare unknown name is far more likely to be a typo or a
/// missing namespace binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownName {
    pub name: String,
}

impl std::fmt::Display for UnknownName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "unknown element `{}`: bare names are reserved for built-in elements; \
             namespace it (`ns.{}`) or bind a namespace with `#import(file:..., as:ns)`",
            self.name, self.name
        )
    }
}

impl std::error::Error for UnknownName {}

/// Classifies `el` by its `Sigil`.
///
/// `Sigil::Bare` is `ElementKind::Bare`, `Sigil::Dollar` is
/// `ElementKind::Interp`, and a nameless inline `@` is `Custom("at")`.
/// Everything else goes through [`classify_name`].
///
/// The sigil itself no longer contributes to the *kind* -- it encodes
/// shape (`@` inline, `#` block), and a shape that disagrees with the
/// element's definition is reported separately by [`shape_mismatch`]
/// rather than producing a different kind.
///
/// No inference from `args` happens here -- `@(url:...)`-style key-based
/// guessing was retired; the only way to get `ElementKind::Link` is to
/// write `@link` explicitly.
pub fn classify(el: &Element) -> Result<ElementKind, UnknownName> {
    match &el.sigil {
        Sigil::Block(name) => classify_name(name),
        Sigil::Inline(Some(name)) => classify_name(name),
        Sigil::Inline(None) => Ok(ElementKind::Custom("at".to_string())),
        Sigil::Bare => Ok(ElementKind::Bare),
        Sigil::Dollar => Ok(ElementKind::Interp),
    }
}

/// Classifies `el`, falling back to `Custom` for an unknown bare name.
///
/// For consumers that render whatever they are given and have no way to
/// report a diagnostic -- an HTML writer mid-document, say. Validation
/// belongs to [`classify`]; this is the lenient read of the same thing.
pub fn classify_lenient(el: &Element) -> ElementKind {
    classify(el).unwrap_or_else(|e| ElementKind::Custom(e.name))
}

/// The shape a built-in element must be written with.
///
/// `None` means either shape is legal.
fn required_shape(kind: &ElementKind) -> Option<Shape> {
    use ElementKind::*;
    Some(match kind {
        Meta | Config | Settings | Import | References | Id | Blueprint | Links | Hr
        | Codeblock | Blockquote | Table | Heading | OrderedList | UnorderedList | Kind
        | Version => Shape::Block,
        Em | Strong | Mark | Link | Embed | Icon => Shape::Inline,
        Custom(_) | Bare | Interp => return None,
    })
}

/// Whether an element is written inline (`@`) or as a block (`#`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Block,
    Inline,
}

/// Reports an element written with the wrong sigil for its kind -- a
/// block-only element spelled `@meta`, or an inline-only one spelled
/// `#em`.
///
/// Returns `Some((found, expected))` when they disagree.
pub fn shape_mismatch(el: &Element) -> Option<(Shape, Shape)> {
    let found = match &el.sigil {
        Sigil::Block(_) => Shape::Block,
        Sigil::Inline(Some(_)) => Shape::Inline,
        _ => return None,
    };
    let kind = classify(el).ok()?;
    let expected = required_shape(&kind)?;
    (found != expected).then_some((found, expected))
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
                classify_name(&Name::bare(*name)),
                Ok(kind.clone()),
                "classify_name() mismatch for {name:?}"
            );
        }
    }

    #[test]
    fn unknown_bare_name_is_an_error() {
        // Bare names are reserved for the built-in vocabulary, so this is
        // the diagnostic that replaces the old silent `Custom` fallback.
        let el = element_new(Sigil::block("caution"));
        assert_eq!(
            classify(&el),
            Err(UnknownName {
                name: "caution".to_string()
            })
        );
    }

    #[test]
    fn namespaced_name_is_custom() {
        let el = element_new(Sigil::Block(Name::namespaced("deck", "caution")));
        assert_eq!(
            classify(&el),
            Ok(ElementKind::Custom("deck.caution".to_string()))
        );
    }

    #[test]
    fn namespacing_never_shadows_a_builtin() {
        // `deck.meta` is the user's element, not Tomet's `#meta`.
        let el = element_new(Sigil::Block(Name::namespaced("deck", "meta")));
        assert_eq!(
            classify(&el),
            Ok(ElementKind::Custom("deck.meta".to_string()))
        );
    }

    #[test]
    fn classify_lenient_falls_back_for_an_unknown_bare_name() {
        let el = element_new(Sigil::block("caution"));
        assert_eq!(
            classify_lenient(&el),
            ElementKind::Custom("caution".to_string())
        );
    }

    #[test]
    fn type_sigil_with_builtin_name_is_recognized() {
        let el = element_new(Sigil::block("codeblock"));
        assert_eq!(classify_lenient(&el), ElementKind::Codeblock);
    }

    #[test]
    fn named_at_sigil_is_recognized() {
        let el = element_new(Sigil::block("meta"));
        assert_eq!(classify_lenient(&el), ElementKind::Meta);
    }

    #[test]
    fn named_at_sigil_with_link_name_is_recognized() {
        let mut el = element_new(Sigil::inline("link"));
        el.args = Some(Value::Map(vec![(
            "target".to_string(),
            Value::String("https://example.com".to_string()),
        )]));
        assert_eq!(classify_lenient(&el), ElementKind::Link);
    }

    #[test]
    fn shape_mismatch_reports_a_block_element_written_inline() {
        let el = element_new(Sigil::inline("meta"));
        assert_eq!(shape_mismatch(&el), Some((Shape::Inline, Shape::Block)));
    }

    #[test]
    fn shape_mismatch_reports_an_inline_element_written_as_a_block() {
        let el = element_new(Sigil::block("em"));
        assert_eq!(shape_mismatch(&el), Some((Shape::Block, Shape::Inline)));
    }

    #[test]
    fn a_correctly_shaped_element_has_no_mismatch() {
        assert_eq!(shape_mismatch(&element_new(Sigil::block("meta"))), None);
        assert_eq!(shape_mismatch(&element_new(Sigil::inline("em"))), None);
    }

    #[test]
    fn a_custom_element_may_take_either_shape() {
        let block = element_new(Sigil::Block(Name::namespaced("deck", "card")));
        let inline = element_new(Sigil::Inline(Some(Name::namespaced("deck", "card"))));
        assert_eq!(shape_mismatch(&block), None);
        assert_eq!(shape_mismatch(&inline), None);
    }

    #[test]
    fn unnamed_at_sigil_is_always_custom_at() {
        // No inference happens for a bare `@(...)` anymore -- kind is
        // decided purely by the element's name (`@link`, `<link>`, ...),
        // never guessed from `args`.
        let mut el = element_new(Sigil::Inline(None));
        el.args = Some(Value::Map(vec![(
            "target".to_string(),
            Value::String("https://example.com".to_string()),
        )]));
        assert_eq!(classify_lenient(&el), ElementKind::Custom("at".to_string()));
    }

    #[test]
    fn unnamed_at_sigil_with_no_recognized_key_is_custom_at() {
        let el = element_new(Sigil::Inline(None));
        assert_eq!(classify_lenient(&el), ElementKind::Custom("at".to_string()));
    }

    #[test]
    fn bare_sigil_is_always_bare() {
        let el = element_new(Sigil::Bare);
        assert_eq!(classify_lenient(&el), ElementKind::Bare);
    }

    #[test]
    fn dollar_sigil_is_always_interp() {
        let el = element_new(Sigil::Dollar);
        assert_eq!(classify_lenient(&el), ElementKind::Interp);
    }
}
