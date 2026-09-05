use tomet_ast::{Element, Sigil, Value};

/// One table, keyed on the bare name alone.
///
/// This used to be two parallel tables, one per sigil, and they had
/// silently drifted: `codeblock`, `embed` and `callout` had entries only
/// under `<T>`, so `@embed(x)` and `<embed>(x)` disagreed about whether
/// `x` meant `target`. Since the sigil now encodes shape rather than
/// origin, keying on shape here would have preserved that bug as a
/// feature. `callout` is kept even though it is not in `BUILTIN_KINDS`;
/// it is a widely used custom element in the docs and dropping its
/// positional key would silently change how `(warning)` reads.
pub fn builtin_positional_arg_keys(sigil: &Sigil) -> &'static [&'static str] {
    let Some(name) = sigil.name() else {
        return &[];
    };
    if !name.is_bare() {
        return &[];
    }
    match name.name.as_str() {
        "kind" => &["kind"],
        "version" => &["version"],
        "codeblock" => &["lang"],
        "embed" | "link" => &["target"],
        // The namespace to bring into scope. The vault says where it
        // lives, so this names it, not its file.
        "use" => &["target"],
        // The kind the blueprint produces. `target` and not `kind`:
        // `@kind(blueprint)` sits directly above it, and `kind` there would
        // mean the output's kind while `@kind` means this file's.
        "blueprint" => &["target"],
        "callout" => &["variant"],
        "meta" | "config" => &["format"],
        "heading" => &["level"],
        _ => &[],
    }
}

/// The builtin positional key for a list item's `(...)` marker (a list
/// item is `Element{ sigil: Sigil::Bare, args: <marker>, .. }`, see
/// `tomet_ast::Element::list_item`). `Sigil::Bare` has no name to
/// look a per-kind key up by (unlike `builtin_positional_arg_key`), so
/// this is a single fixed key rather than a lookup table.
pub const LIST_MARKER_POSITIONAL_KEY: &str = "marker";

/// Normalizes a list item's `(...)` marker `Value` the same way
/// [`normalized_element_args`] normalizes an element's `args`: a `Map`
/// (explicit `key:value`, e.g. `(color: red, priority: high)`) passes
/// through unchanged; any other `Value` (a bare scalar like `("12:01")`,
/// or a `Seq`) is wrapped under [`LIST_MARKER_POSITIONAL_KEY`].
pub fn normalized_list_marker(marker: &Value) -> Value {
    if let Value::Map(_) = marker {
        return marker.clone();
    }
    Value::Map(vec![(
        LIST_MARKER_POSITIONAL_KEY.to_string(),
        marker.clone(),
    )])
}

/// The old per-scheme argument keys (`url`/`file`/`tm`/`id`/`ref`), from
/// before `@link`/`<embed>` were unified onto one `target` key -- a bare
/// `tm:foo/bar` (no `target:` wrapper) still parses as a map entry
/// `("tm", "foo/bar")`, since the parser's `identifier:` rule has no
/// notion of "target" being the only real key these two elements have (it
/// only sees a value position, not an element name). This list exists
/// solely to recover that split back into one `target` string, in
/// [`recover_target_scheme_entry`] -- reusing `target_scheme`'s own
/// recognized explicit prefixes (see `crate::target`) rather than a
/// separately invented vocabulary.
const RECOVERABLE_TARGET_SCHEMES: [&str; 5] = ["url", "file", "tm", "id", "ref"];

/// Sentinel key `tomet_parser` tags a bare, unkeyed positional entry
/// with (`(a, b)`'s two entries, or `@link(tm:foo, depends_on)`'s second
/// one) -- must match `tomet_syntax_parser::value::POSITIONAL_ENTRY_KEY`
/// exactly. Duplicated here rather than imported since `tomet-semantics`
/// deliberately doesn't depend on `tomet-parser` (see this crate's own
/// module doc) --
/// `eat_ident` never produces an empty *real* key, so `""` can't collide.
const POSITIONAL_ENTRY_KEY: &str = "";

/// Scoped to elements whose *first* positional slot is `"target"`
/// (`@link`/`<embed>`'s builtin default, or any custom element a project
/// gives that name via `@settings`) -- when that's the only legitimate
/// key defined, any *other* single key present is unambiguously an
/// artifact of the parser's `identifier:` rule splitting a bare
/// `scheme:value` positional argument, not a real intentional field.
/// Recovers exactly one such entry (skips the whole map untouched if
/// there's already a `target` key, or if more than one candidate entry
/// matches and it'd be ambiguous which one was meant) back into
/// `target: "scheme:value"`, leaving every other entry -- genuine extra
/// data alongside the target, e.g. `@link(tm:foo, predicate: depends_on)`
/// -- untouched.
fn recover_target_scheme_entry(
    first_positional_key: Option<&str>,
    mut entries: Vec<(String, Value)>,
) -> Value {
    if first_positional_key != Some("target") || entries.iter().any(|(k, _)| k == "target") {
        return Value::Map(entries);
    }
    let candidates: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(_, (k, _))| RECOVERABLE_TARGET_SCHEMES.contains(&k.as_str()))
        .map(|(idx, _)| idx)
        .collect();
    let [idx] = candidates[..] else {
        return Value::Map(entries);
    };
    let (key, value) = &entries[idx];
    entries[idx] = (
        "target".to_string(),
        Value::String(format!("{key}:{}", scalar_to_plain(value))),
    );
    Value::Map(entries)
}

/// Assigns each remaining bare positional entry (tagged with
/// [`POSITIONAL_ENTRY_KEY`] by the parser, or recovered as `target` by
/// [`recover_target_scheme_entry`] just before this runs) to the next slot
/// in `positional_keys` that isn't already present as a real key, in
/// left-to-right order on both sides. Slots with no positional entry left
/// to fill them, and positional entries beyond the number of slots, are
/// both left as-is -- nothing is silently dropped either way; an
/// unassigned positional entry just keeps its `""` key.
fn fill_positional_slots(positional_keys: &[String], mut entries: Vec<(String, Value)>) -> Value {
    for key in positional_keys {
        if entries.iter().any(|(k, _)| k == key) {
            continue;
        }
        if let Some(pos) = entries.iter().position(|(k, _)| k == POSITIONAL_ENTRY_KEY) {
            entries[pos].0 = key.clone();
        }
    }
    Value::Map(entries)
}

/// The ordered positional-key list for `sigil`, owned.
///
/// There used to be a second source here: an `@settings` `elements:` map
/// could give a custom element its own `positional: [ ... ]` list, and it
/// won over the builtin table. That surface is gone -- which names exist
/// and what their parameters are is a `@vocabulary` question now, and the
/// list form was the duplicate `explicit-form-first` names outright (an
/// order the parameter declarations already carry). Until `@param` is
/// read, `std`'s table is the whole of it.
fn positional_keys(sigil: &Sigil) -> Vec<String> {
    builtin_positional_arg_keys(sigil)
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// Stringifies a scalar `Value` back to the plain text it would have come
/// from as a bare (unquoted) scalar -- used to glue a recovered scheme
/// prefix back onto its value (`tm` + `Int(5)` -> `"tm:5"`). `Seq`/`Map`
/// can't legitimately appear here (the parser only ever produces those
/// for a bracketed/braced value, never for a bare `word:value` entry's
/// value), so they fall back to empty rather than fabricating something
/// misleading.
fn scalar_to_plain(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::String(s) => s.clone(),
        Value::Seq(_) | Value::Map(_) => String::new(),
    }
}

/// Returns the normalized `args` map for an [`Element`]. One unified
/// pipeline for every shape `args` can take:
/// - `Value::Map`: recover a scheme-collision `target` entry if this
///   element's first slot is `"target"` (see
///   [`recover_target_scheme_entry`]), then fill any still-unfilled slots
///   from bare positional entries the parser tagged with
///   [`POSITIONAL_ENTRY_KEY`] (`@link(tm:foo, depends_on)`'s `depends_on`,
///   or a custom element's `(a, b)`).
/// - `Value::Seq`: an explicit `[...]`-bracketed positional list
///   (`@task([a, b])`) -- independent of the parser's bare-comma-list
///   sentinel mechanism, zipped against the slot list positionally.
/// - anything else (a bare scalar): wrapped under the first slot, same as
///   before this was generalized.
///
/// An element with no positional slots defined at all (empty list) is
/// returned untouched in every case.
///
/// There was a `_with_schema` twin taking a `SettingsSchema`, and every
/// caller passed the default. It has been folded back in here along with
/// the `@settings` surface it read (see [`positional_keys`]).
pub fn normalized_element_args(el: &Element) -> Option<Value> {
    let args = el.args.as_ref()?;
    let positional_keys = positional_keys(&el.sigil);

    match args {
        Value::Map(entries) => {
            if positional_keys.is_empty() {
                return Some(Value::Map(entries.clone()));
            }
            let recovered = recover_target_scheme_entry(
                positional_keys.first().map(String::as_str),
                entries.clone(),
            );
            let Value::Map(recovered_entries) = recovered else {
                unreachable!("recover_target_scheme_entry always returns Value::Map")
            };
            Some(fill_positional_slots(&positional_keys, recovered_entries))
        }
        Value::Seq(seq) => {
            if positional_keys.is_empty() {
                return Some(args.clone());
            }
            let mut entries = Vec::new();
            for (idx, val) in seq.iter().enumerate() {
                if idx < positional_keys.len() {
                    entries.push((positional_keys[idx].clone(), val.clone()));
                }
            }
            Some(Value::Map(entries))
        }
        scalar => match positional_keys.first() {
            Some(first) => Some(Value::Map(vec![(first.clone(), scalar.clone())])),
            None => Some(scalar.clone()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_ast::Name;
    use tomet_tree::element_new;

    #[test]
    fn normalizes_codeblock_positional_arg() {
        let mut el = element_new(Sigil::named("codeblock"));
        el.args = Some(Value::String("rust".to_string()));
        assert_eq!(
            normalized_element_args(&el),
            Some(Value::Map(vec![(
                "lang".to_string(),
                Value::String("rust".to_string())
            )]))
        );
    }

    #[test]
    fn normalizes_embed_positional_arg() {
        let mut el = element_new(Sigil::named("embed"));
        el.args = Some(Value::String("foo.png".to_string()));
        assert_eq!(
            normalized_element_args(&el),
            Some(Value::Map(vec![(
                "target".to_string(),
                Value::String("foo.png".to_string())
            )]))
        );
    }

    #[test]
    fn normalizes_link_positional_arg() {
        let mut el = element_new(Sigil::named("link"));
        el.args = Some(Value::String("https://example.com".to_string()));
        assert_eq!(
            normalized_element_args(&el),
            Some(Value::Map(vec![(
                "target".to_string(),
                Value::String("https://example.com".to_string())
            )]))
        );
    }

    #[test]
    fn recovers_bare_scheme_prefixed_link_target_from_parser_key_split() {
        // `@link(tm:foo/bar)` parses as `Map([("tm", "foo/bar")])` (the
        // parser's `identifier:` rule has no notion of `target` being the
        // only real key `@link` has) -- this recovers it back into one
        // `target` string.
        let mut el = element_new(Sigil::named("link"));
        el.args = Some(Value::Map(vec![(
            "tm".to_string(),
            Value::String("foo/bar".to_string()),
        )]));
        assert_eq!(
            normalized_element_args(&el),
            Some(Value::Map(vec![(
                "target".to_string(),
                Value::String("tm:foo/bar".to_string())
            )]))
        );
    }

    #[test]
    fn recovers_scheme_prefixed_target_alongside_genuine_extra_keys() {
        // `@link(tm:foo, predicate: depends_on)` -- only the `tm` entry is
        // a parser-split scheme prefix; `predicate` is real extra data and
        // stays untouched.
        let mut el = element_new(Sigil::named("link"));
        el.args = Some(Value::Map(vec![
            ("tm".to_string(), Value::String("foo".to_string())),
            (
                "predicate".to_string(),
                Value::String("depends_on".to_string()),
            ),
        ]));
        assert_eq!(
            normalized_element_args(&el),
            Some(Value::Map(vec![
                ("target".to_string(), Value::String("tm:foo".to_string())),
                (
                    "predicate".to_string(),
                    Value::String("depends_on".to_string())
                ),
            ]))
        );
    }

    #[test]
    fn does_not_recover_when_target_key_already_present() {
        let mut el = element_new(Sigil::named("link"));
        el.args = Some(Value::Map(vec![
            ("target".to_string(), Value::String("tm:foo".to_string())),
            ("tm".to_string(), Value::String("bar".to_string())),
        ]));
        assert_eq!(
            normalized_element_args(&el),
            Some(Value::Map(vec![
                ("target".to_string(), Value::String("tm:foo".to_string())),
                ("tm".to_string(), Value::String("bar".to_string())),
            ]))
        );
    }

    #[test]
    fn does_not_recover_when_multiple_scheme_candidates_are_ambiguous() {
        let mut el = element_new(Sigil::named("link"));
        el.args = Some(Value::Map(vec![
            ("tm".to_string(), Value::String("foo".to_string())),
            ("id".to_string(), Value::String("bar".to_string())),
        ]));
        assert_eq!(
            normalized_element_args(&el),
            Some(Value::Map(vec![
                ("tm".to_string(), Value::String("foo".to_string())),
                ("id".to_string(), Value::String("bar".to_string())),
            ]))
        );
    }

    #[test]
    fn does_not_recover_for_non_link_embed_elements() {
        // An arbitrary custom element's own `tm`-named field is left
        // alone -- recovery is scoped to elements whose positional key
        // is `target` (`@link`/`<embed>`), not a general mechanism.
        let mut el = element_new(Sigil::named("caution"));
        el.args = Some(Value::Map(vec![(
            "tm".to_string(),
            Value::String("foo".to_string()),
        )]));
        assert_eq!(
            normalized_element_args(&el),
            Some(Value::Map(vec![(
                "tm".to_string(),
                Value::String("foo".to_string())
            )]))
        );
    }

    /// A custom element's positional args are left as the parser produced
    /// them, sentinel keys and all.
    ///
    /// Two tests used to sit here giving `link` a second `predicate` slot
    /// and `task` a `title`/`priority` pair through an `@settings`
    /// `elements: { <name>: { positional: [...] } }` map. That surface is
    /// gone -- it was a second source for a fact the parameter
    /// declarations already order, and nothing outside those tests ever
    /// built a `SettingsSchema`. Until `@param` is read, `std`'s table is
    /// the only source, and it has no entry for a custom name.
    ///
    /// A consequence worth stating: every builtin has at most one
    /// positional slot, so `fill_positional_slots`' multi-slot loop is
    /// written for `@param` and exercised with one slot today.
    #[test]
    fn a_custom_element_gets_no_positional_inference() {
        let mut el = element_new(Sigil::named("task"));
        el.args = Some(Value::Map(vec![
            (
                POSITIONAL_ENTRY_KEY.to_string(),
                Value::String("Clean room".to_string()),
            ),
            (
                POSITIONAL_ENTRY_KEY.to_string(),
                Value::String("high".to_string()),
            ),
        ]));

        assert_eq!(
            normalized_element_args(&el),
            Some(Value::Map(vec![
                (
                    POSITIONAL_ENTRY_KEY.to_string(),
                    Value::String("Clean room".to_string()),
                ),
                (
                    POSITIONAL_ENTRY_KEY.to_string(),
                    Value::String("high".to_string()),
                ),
            ]))
        );
    }

    #[test]
    fn normalizes_meta_positional_arg() {
        let mut el = element_new(Sigil::named("meta"));
        el.args = Some(Value::String("json".to_string()));
        assert_eq!(
            normalized_element_args(&el),
            Some(Value::Map(vec![(
                "format".to_string(),
                Value::String("json".to_string())
            )]))
        );
    }

    #[test]
    fn leaves_map_args_unchanged() {
        let mut el = element_new(Sigil::named("codeblock"));
        let map_val = Value::Map(vec![(
            "lang".to_string(),
            Value::String("rust".to_string()),
        )]);
        el.args = Some(map_val.clone());
        assert_eq!(normalized_element_args(&el), Some(map_val));
    }

    #[test]
    fn normalizes_bare_scalar_list_marker_under_marker_key() {
        assert_eq!(
            normalized_list_marker(&Value::String("12:01".to_string())),
            Value::Map(vec![(
                "marker".to_string(),
                Value::String("12:01".to_string())
            )])
        );
    }

    #[test]
    fn leaves_explicit_key_value_list_marker_unchanged() {
        let map_val = Value::Map(vec![
            ("color".to_string(), Value::String("red".to_string())),
            ("priority".to_string(), Value::String("high".to_string())),
        ]);
        assert_eq!(normalized_list_marker(&map_val), map_val);
    }

    #[test]
    fn an_element_with_no_positional_key_leaves_a_bare_scalar_alone() {
        // `deck.card` is namespaced, so it has no builtin positional key
        // and nothing to normalize the scalar into.
        let mut el = element_new(Sigil::Named(Name::namespaced("deck", "card")));
        el.args = Some(Value::String("https://example.com".to_string()));
        assert_eq!(
            normalized_element_args(&el),
            Some(Value::String("https://example.com".to_string()))
        );
    }

    /// The `Seq` arm is the explicit `@name([a, b])` form, and it too is
    /// left alone for a custom name -- same reason as
    /// [`a_custom_element_gets_no_positional_inference`].
    #[test]
    fn a_custom_elements_bracketed_positional_list_is_left_alone() {
        let mut el = element_new(Sigil::named("task"));
        el.args = Some(Value::Seq(vec![
            Value::String("Clean room".to_string()),
            Value::String("high".to_string()),
        ]));

        assert_eq!(
            normalized_element_args(&el),
            Some(Value::Seq(vec![
                Value::String("Clean room".to_string()),
                Value::String("high".to_string()),
            ]))
        );
    }
}
