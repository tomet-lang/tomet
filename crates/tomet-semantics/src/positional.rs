use tomet_ast::{Data, Declaration, ElementView, Name, Tuple, Value};
use tomet_scalar::Scalar;
use tomet_semantics_core::{Slots, TupleView};
use tomet_tree::ValueExt;

use crate::vocabulary::{AttributeDecl, Bindings, ConstructorDecl, ElementDecl};

/// The positional slots of a `std` body element (`@name`), keyed on the bare name alone.
///
/// Shape comes from position, so keying on anything but the name would let two spellings of one
/// element disagree about whether `x` means `target`. A standard declaration's slots are the
/// declaration's own ([`tomet_declaration::DeclKind::positional_keys`]), the same ones for every
/// host that reads it, and are not here: `::kind` and `@kind` are looked up in different tables.
pub fn builtin_positional_arg_keys(name: &Name) -> &'static [&'static str] {
    if !name.is_bare() {
        return &[];
    }
    match name.name.as_str() {
        "ftn" => &["id"],
        "raw" => &["lang"],
        "embed" | "link" => &["target"],
        // A path mention names one thing, so its single positional slot
        // is the path. The element name says file or directory; unlike
        // `@link`, there is no scheme prefix to carry it.
        "file" | "dir" => &["target"],
        // A mention names one term: `tomet#id`, `tomet#name("...")` or the term itself. In a
        // `::dictionary` the same element, as a block of the body, defines the term it names.
        "term" => &["target"],
        "callout" => &["variant"],
        "heading" => &["level"],
        "task" => &["status"],
        // `@conflict(x, y)` assigns by written position -- `a` first,
        // `b` second. Deliberately just an order, not a "mine/theirs"
        // signal: see `ElementKind::Conflict`'s own doc comment for why
        // that distinction matters here specifically.
        "conflict" => &["a", "b"],
        _ => &[],
    }
}

/// The builtin positional key for a list item's `(...)` marker (a list
/// item is a [`tomet_ast::ListItem`] whose `data.args` is the marker). A
/// plain item has no name to look a per-kind key up by (unlike
/// [`builtin_positional_arg_keys`]), so this is a single fixed key rather
/// than a lookup table.
pub const LIST_MARKER_POSITIONAL_KEY: &str = "marker";

/// Views a list item's `(...)` marker [`Tuple`] through [`LIST_MARKER_POSITIONAL_KEY`].
pub fn list_marker_view(marker: &Tuple) -> TupleView<'_, 'static> {
    TupleView::new(marker, &[LIST_MARKER_POSITIONAL_KEY])
}

/// The per-scheme argument keys (`url`/`file`/`tm`/`id`/`ref`), which
/// `@link`/`@embed` do not have (they have the one `target` key) -- a bare
/// `tm:foo/bar` (no `target:` wrapper) still parses as a named entry
/// `("tm", "foo/bar")`, since the parser's `identifier:` rule has no
/// notion of "target" being the only real key these two elements have (it
/// only sees a value position, not an element name). This list exists
/// solely to recover that split back into one `target` string, in
/// [`recover_target_scheme_entry`] -- reusing `target_scheme`'s own
/// recognized explicit prefixes (see `crate::elements::target`) rather than a
/// separately invented vocabulary.
fn is_recoverable_target_scheme(key: &str) -> bool {
    tomet_address::is_explicit_scheme(key)
}

/// Scoped to elements whose *first* positional slot is `"target"`
/// (`@link`/`@embed`'s builtin default; a custom element is not given
/// that slot through `::settings`, which is a `::vocabulary` question and
/// unread either way) -- when that's the only legitimate
/// key defined, any *other* single key present is unambiguously an
/// artifact of the parser's `identifier:` rule splitting a bare
/// `scheme:value` positional argument, not a real intentional field.
/// Recovers exactly one such entry (skips the whole tuple untouched if
/// there's already a `target` key, or if more than one candidate entry
/// matches and it'd be ambiguous which one was meant) back into
/// `target: "scheme:value"`, leaving every other entry -- genuine extra
/// data alongside the target, e.g. `@link(tm:foo, predicate: depends_on)`
/// -- untouched.
fn recover_target_scheme_entry(first_positional_key: Option<&str>, tuple: &Tuple) -> Option<Tuple> {
    if first_positional_key != Some("target") || tuple.contains_key("target") {
        return None;
    }
    let candidates: Vec<&str> = tuple
        .named()
        .filter_map(|(name, _)| is_recoverable_target_scheme(name).then_some(name))
        .collect();
    let [scheme] = candidates[..] else {
        return None;
    };
    let scheme = scheme.to_string();
    let mut tuple = tuple.clone();
    if let Some(value) = tuple.get_mut(&scheme) {
        *value = Value::Scalar(Scalar::Text(format!("{scheme}:{}", value.plain_text())));
    }
    let _ = tuple.rename(&scheme, "target");
    Some(tuple)
}

/// The ordered positional-key slots of the element `name`: a `std` body element's, or what the
/// `::element` entry in scope declares.
fn element_slots(name: &Name, bindings: &Bindings) -> Slots<'static> {
    let builtin = builtin_positional_arg_keys(name);
    if !builtin.is_empty() {
        return Slots::Borrowed(builtin);
    }
    let keys = bindings
        .declaration(name)
        .map(ElementDecl::positional_keys)
        .unwrap_or_default();
    Slots::Owned(keys)
}

/// The ordered positional-key slots of the attribute `name`: a standard declaration's, or what
/// the `::attribute` entry in scope declares.
fn attribute_slots(name: &Name, bindings: &Bindings) -> Slots<'static> {
    if crate::kind::is_written_in_std(name)
        && let Some(declaration) = tomet_declaration::DeclKind::from_name(&name.name)
    {
        return Slots::Borrowed(declaration.positional_keys());
    }
    let keys = bindings
        .attribute(name)
        .map(AttributeDecl::positional_keys)
        .unwrap_or_default();
    Slots::Owned(keys)
}

/// Returns the arguments view for an [`Element`]. Knows only `std`.
pub fn element_args_view(el: ElementView<'_>) -> Option<TupleView<'_, 'static>> {
    element_args_view_in(el, &Bindings::default())
}

/// [`element_args_view`], plus the positional slots the `::element` entry in scope declares
/// (`args: ( name: (positional: true) )`).
pub fn element_args_view_in<'a>(
    el: ElementView<'a>,
    bindings: &Bindings,
) -> Option<TupleView<'a, 'static>> {
    args_view_in(element_slots(el.name, bindings), el.data)
}

/// Arguments view for a declaration: `::kind(config, 2)` views `kind` and `version`.
pub fn declaration_args_view(declaration: &Declaration) -> Option<TupleView<'_, 'static>> {
    declaration_args_view_in(declaration, &Bindings::default())
}

/// [`declaration_args_view`], plus the positional slots the `::attribute` entry in scope
/// declares.
pub fn declaration_args_view_in<'a>(
    declaration: &'a Declaration,
    bindings: &Bindings,
) -> Option<TupleView<'a, 'static>> {
    args_view_in(
        attribute_slots(&declaration.name, bindings),
        &declaration.data,
    )
}

/// Views a constructor call's `args` through the positional slots `constructor` declares.
pub fn constructor_args_view<'a>(
    args: &'a Tuple,
    constructor: &ConstructorDecl,
) -> TupleView<'a, 'static> {
    TupleView::with_owned_slots(args, constructor.positional_keys())
}

/// Views `args` through `slots`.
fn args_view_in<'a>(slots: Slots<'static>, data: &'a Data) -> Option<TupleView<'a, 'static>> {
    let args = data.args.as_ref()?;
    let first_key = slots.get(0);
    let recovered = if !slots.is_empty() {
        recover_target_scheme_entry(first_key, args)
    } else {
        None
    };
    Some(match (recovered, slots) {
        (Some(owned), Slots::Borrowed(b)) => TupleView::from_owned(owned, b),
        (Some(owned), Slots::Owned(o)) => TupleView::from_owned_with_owned_slots(owned, o),
        (None, Slots::Borrowed(b)) => TupleView::new(args, b),
        (None, Slots::Owned(o)) => TupleView::with_owned_slots(args, o),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_ast::{Name, Tuple, Value};
    use tomet_scalar::Scalar;
    use tomet_tree::{ValueExt, element_new};

    fn s(text: &str) -> Value {
        Value::Scalar(Scalar::Text(text.to_string()))
    }
    fn i(n: i64) -> Value {
        Value::Scalar(Scalar::Int(n))
    }

    #[test]
    fn normalizes_raw_positional_arg() {
        let mut el = element_new(Name::bare("raw"));
        el.data.args = Some(Tuple::from_unnamed([s("rust")]));
        let view = element_args_view(el.as_view()).unwrap();
        assert_eq!(view.get("lang"), Some(&s("rust")));
        assert_eq!(view.get("0"), Some(&s("rust")));
        assert_eq!(view.entry_alias(0), Some("lang"));
        assert_eq!(view.tuple(), el.data.args.as_ref().unwrap());
    }

    #[test]
    fn normalizes_conflict_positional_args_by_written_order() {
        // `@conflict(1, 2)` -- two bare entries, no keys
        let mut el = element_new(Name::bare("conflict"));
        el.data.args = Some(Tuple::from_unnamed([i(1), i(2)]));
        let view = element_args_view(el.as_view()).unwrap();
        assert_eq!(view.get("a"), Some(&i(1)));
        assert_eq!(view.get("0"), Some(&i(1)));
        assert_eq!(view.get("b"), Some(&i(2)));
        assert_eq!(view.get("1"), Some(&i(2)));
        assert_eq!(view.tuple(), el.data.args.as_ref().unwrap());
    }

    #[test]
    fn kind_and_use_take_an_optional_version_after_the_name() {
        // `::kind(name, 2)` is two bare entries; `::kind(name)` is one scalar.
        for (name, name_key) in [("kind", "kind"), ("use", "target")] {
            let mut decl = tomet_tree::declaration_new(name);
            decl.data.args = Some(Tuple::from_unnamed([s("deck"), i(2)]));
            let view = declaration_args_view(&decl).unwrap();
            assert_eq!(view.get(name_key), Some(&s("deck")), "::{name}(deck, 2)");
            assert_eq!(view.get("version"), Some(&i(2)));
            assert_eq!(view.tuple(), decl.data.args.as_ref().unwrap());

            // the version may be left out
            decl.data.args = Some(Tuple::from_unnamed([s("deck")]));
            let view = declaration_args_view(&decl).unwrap();
            assert_eq!(view.get(name_key), Some(&s("deck")), "::{name}(deck)");
            assert_eq!(view.get("version"), None);
            assert_eq!(view.tuple(), decl.data.args.as_ref().unwrap());

            // `@kind` is no element, and has no slots
            let mut el = element_new(Name::bare(name));
            el.data.args = Some(Tuple::from_unnamed([s("deck")]));
            assert_eq!(element_args_view(el.as_view()).unwrap().get(name_key), None);
        }
    }

    #[test]
    fn normalizes_embed_positional_arg() {
        let mut el = element_new(Name::bare("embed"));
        el.data.args = Some(Tuple::from_unnamed([s("foo.png")]));
        let view = element_args_view(el.as_view()).unwrap();
        assert_eq!(view.get("target"), Some(&s("foo.png")));
        assert_eq!(view.tuple(), el.data.args.as_ref().unwrap());
    }

    #[test]
    fn normalizes_link_positional_arg() {
        let mut el = element_new(Name::bare("link"));
        el.data.args = Some(Tuple::from_unnamed([s("https://example.com")]));
        let view = element_args_view(el.as_view()).unwrap();
        assert_eq!(view.get("target"), Some(&s("https://example.com")));
        assert_eq!(view.get("0"), Some(&s("https://example.com")));
        assert_eq!(view.tuple(), el.data.args.as_ref().unwrap());
    }

    #[test]
    fn a_term_mention_has_one_positional_target_in_each_of_its_three_spellings() {
        // The dictionary is on the left of `#`, as `<document>#<place>`; the term alone has none.
        for written in [
            "tomet#vault-config",
            "tomet#name(\"vault.config.tmt\")",
            "\"vault.config.tmt\"",
        ] {
            let doc = tomet_ast_parser::parse_file(&format!("a @term({written}) b\n"))
                .expect("a mention parses");
            let mut target = None;
            tomet_tree::for_each_element(&doc, |el| {
                if el.name.name == "term" {
                    target = element_args_view(el.as_view())
                        .and_then(|args| args.get("target").cloned())
                        .and_then(|v| v.as_str().map(str::to_string));
                }
            });
            let expected = written.trim_matches('"').to_string();
            assert_eq!(target, Some(expected), "{written}");
        }
    }

    #[test]
    fn recovers_bare_scheme_prefixed_link_target_from_parser_key_split() {
        // `@link(tmt:foo/bar)` parses as named entry `[("tmt", "foo/bar")]` (the
        // parser's `identifier:` rule has no notion of `target` being the
        // only real key `@link` has) -- this recovers it back into one
        // `target` string.
        let mut el = element_new(Name::bare("link"));
        el.data.args = Some(Tuple::from_named([("tmt".to_string(), s("foo/bar"))]));
        let view = element_args_view(el.as_view()).unwrap();
        assert_eq!(view.get("target"), Some(&s("tmt:foo/bar")));
    }

    #[test]
    fn recovers_scheme_prefixed_target_alongside_genuine_extra_keys() {
        // `@link(tmt:foo, predicate: depends_on)` -- only the `tmt` entry is
        // a parser-split scheme prefix; `predicate` is real extra data and
        // stays untouched.
        let mut el = element_new(Name::bare("link"));
        el.data.args = Some(Tuple::from_named([
            ("tmt".to_string(), s("foo")),
            ("predicate".to_string(), s("depends_on")),
        ]));
        let view = element_args_view(el.as_view()).unwrap();
        assert_eq!(view.get("target"), Some(&s("tmt:foo")));
        assert_eq!(view.get("predicate"), Some(&s("depends_on")));
    }

    #[test]
    fn does_not_recover_when_target_key_already_present() {
        let mut el = element_new(Name::bare("link"));
        el.data.args = Some(Tuple::from_named([
            ("target".to_string(), s("tmt:foo")),
            ("tmt".to_string(), s("bar")),
        ]));
        let view = element_args_view(el.as_view()).unwrap();
        assert_eq!(view.get("target"), Some(&s("tmt:foo")));
        assert_eq!(view.get("tmt"), Some(&s("bar")));
    }

    #[test]
    fn does_not_recover_when_multiple_scheme_candidates_are_ambiguous() {
        let mut el = element_new(Name::bare("link"));
        el.data.args = Some(Tuple::from_named([
            ("tmt".to_string(), s("foo")),
            ("id".to_string(), s("bar")),
        ]));
        let view = element_args_view(el.as_view()).unwrap();
        assert_eq!(view.get("target"), None);
        assert_eq!(view.get("tmt"), Some(&s("foo")));
        assert_eq!(view.get("id"), Some(&s("bar")));
    }

    #[test]
    fn does_not_recover_for_non_link_embed_elements() {
        let mut el = element_new(Name::bare("caution"));
        el.data.args = Some(Tuple::from_named([("tmt".to_string(), s("foo"))]));
        let view = element_args_view(el.as_view()).unwrap();
        assert_eq!(view.get("target"), None);
        assert_eq!(view.get("tmt"), Some(&s("foo")));
    }

    #[test]
    fn a_custom_element_gets_no_positional_inference() {
        let mut el = element_new(Name::namespaced("deck", "job"));
        el.data.args = Some(Tuple::from_unnamed([s("Clean room"), s("high")]));
        let view = element_args_view(el.as_view()).unwrap();
        assert_eq!(view.get("0"), Some(&s("Clean room")));
        assert_eq!(view.get("1"), Some(&s("high")));
        assert_eq!(view.entry_alias(0), None);
        assert_eq!(view.entry_alias(1), None);
    }

    #[test]
    fn leaves_map_args_unchanged() {
        let mut el = element_new(Name::bare("raw"));
        let tuple_val = Tuple::from_named([("lang".to_string(), s("rust"))]);
        el.data.args = Some(tuple_val.clone());
        let view = element_args_view(el.as_view()).unwrap();
        assert_eq!(view.get("lang"), Some(&s("rust")));
        assert_eq!(view.tuple(), &tuple_val);
    }

    #[test]
    fn normalizes_bare_scalar_list_marker_under_marker_key() {
        let marker = Tuple::from_unnamed([s("12:01")]);
        let view = list_marker_view(&marker);
        assert_eq!(view.get("marker"), Some(&s("12:01")));
        assert_eq!(view.get("0"), Some(&s("12:01")));
        assert_eq!(view.tuple(), &marker);
    }

    #[test]
    fn leaves_explicit_key_value_list_marker_unchanged() {
        let tuple_val = Tuple::from_named([
            ("color".to_string(), s("red")),
            ("priority".to_string(), s("high")),
        ]);
        let view = list_marker_view(&tuple_val);
        assert_eq!(view.get("color"), Some(&s("red")));
        assert_eq!(view.get("priority"), Some(&s("high")));
        assert_eq!(view.tuple(), &tuple_val);
    }

    #[test]
    fn an_element_with_no_positional_key_leaves_a_bare_scalar_alone() {
        let mut el = element_new(Name::namespaced("deck", "card"));
        el.data.args = Some(Tuple::from_unnamed([s("https://example.com")]));
        let view = element_args_view(el.as_view()).unwrap();
        assert_eq!(view.get("0"), Some(&s("https://example.com")));
        assert_eq!(view.entry_alias(0), None);
        assert_eq!(view.tuple(), el.data.args.as_ref().unwrap());
    }

    #[test]
    fn normalizes_task_status_positional_arg() {
        let mut el = element_new(Name::bare("task"));
        el.data.args = Some(Tuple::from_unnamed([s("x")]));
        let view = element_args_view(el.as_view()).unwrap();
        assert_eq!(view.get("status"), Some(&s("x")));
        assert_eq!(view.get("0"), Some(&s("x")));
        assert_eq!(view.tuple(), el.data.args.as_ref().unwrap());
    }

    #[test]
    fn signature_alias_and_position_with_and_without_vocabulary_and_invariance() {
        use crate::Vocabulary;
        let vocab_src = r#"::vocabulary(deck)
::element(
  card: (
    args: (
      target: (positional: true)
    )
  )
)
"#;
        let vocab_doc = tomet_ast_parser::parse_file(vocab_src).expect("valid vocabulary");
        let vocab = Vocabulary::from_file(&vocab_doc).expect("parsed vocabulary");
        let mut bindings = Bindings::default();
        bindings.used.insert("deck".to_string(), vocab);

        let mut el = element_new(Name::namespaced("deck", "card"));
        el.data.args = Some(Tuple::from_unnamed([s("https://example.com")]));

        // 1. Without vocabulary: position 0 is accessible, alias "target" is None
        let view_without = element_args_view_in(el.as_view(), &Bindings::default()).unwrap();
        assert_eq!(view_without.get("0"), Some(&s("https://example.com")));
        assert_eq!(view_without.get("target"), None);
        assert_eq!(view_without.entry_alias(0), None);

        // 2. With vocabulary: position 0 and alias "target" give the EXACT SAME value
        let view_with = element_args_view_in(el.as_view(), &bindings).unwrap();
        assert_eq!(view_with.get("0"), Some(&s("https://example.com")));
        assert_eq!(view_with.get("target"), Some(&s("https://example.com")));
        assert_eq!(view_with.get("0"), view_with.get("target"));
        assert_eq!(view_with.entry_alias(0), Some("target"));

        // 3. Invariance: an alias NEVER shows up in the tuple's printed text, JSON or equality
        assert_eq!(view_with.tuple(), el.data.args.as_ref().unwrap());
        let json = serde_json::to_string(view_with.tuple()).unwrap();
        assert_eq!(json, r#"["https://example.com"]"#);
        assert!(!json.contains("target"));

        // 4. The slot an entry already has by NAME is skipped
        let mut el_named = element_new(Name::namespaced("deck", "card"));
        el_named.data.args = Some(
            Tuple::try_from_entries([
                tomet_ast::TupleEntry::named("target", s("named_url")),
                tomet_ast::TupleEntry::unnamed(s("positional_url")),
            ])
            .unwrap(),
        );
        let view_skipped = element_args_view_in(el_named.as_view(), &bindings).unwrap();
        assert_eq!(view_skipped.get("target"), Some(&s("named_url")));
        assert_eq!(view_skipped.get("0"), Some(&s("positional_url")));
        assert_eq!(view_skipped.entry_alias(0), None);
    }
}
