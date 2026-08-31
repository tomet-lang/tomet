use std::collections::HashMap;
use tomet_ast::{Element, ElementValue, Sigil, Value};
use tomet_tree::ValueExt;

/// Schema definition extracted from `@settings` block for custom elements.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ElementSchema {
    /// Ordered list of keys allowed for positional inference.
    pub positional: Vec<String>,
}

/// Project/Document level settings schema parsed from `@settings`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SettingsSchema {
    pub elements: HashMap<String, ElementSchema>,
}

impl SettingsSchema {
    /// Parses a [`SettingsSchema`] from an `@settings` element.
    pub fn from_element(el: &Element) -> Self {
        let is_settings =
            matches!(&el.sigil, Sigil::At(Some(name)) | Sigil::Type(name) if name == "settings");
        if !is_settings {
            return Self::default();
        }

        let val = match &el.value {
            Some(ElementValue::Data(v)) => v,
            _ => return Self::default(),
        };

        Self::from_value(val)
    }

    /// Parses a [`SettingsSchema`] from a `Value::Map` representing `@settings` data.
    pub fn from_value(val: &Value) -> Self {
        let mut schema = Self::default();
        let Value::Map(root_entries) = val else {
            return schema;
        };

        for (root_k, root_v) in root_entries {
            if root_k == "elements" {
                if let Value::Map(elem_entries) = root_v {
                    for (elem_name, elem_v) in elem_entries {
                        let mut elem_schema = ElementSchema::default();
                        if let Value::Map(props) = elem_v {
                            for (prop_k, prop_v) in props {
                                if prop_k == "positional" {
                                    elem_schema.positional = parse_string_list(prop_v);
                                }
                            }
                        }
                        schema.elements.insert(elem_name.clone(), elem_schema);
                    }
                }
            }
        }
        schema
    }

    /// Returns the positional arg key list for an element name, if defined in schema.
    pub fn positional_keys(&self, elem_name: &str) -> Option<&[String]> {
        self.elements
            .get(elem_name)
            .map(|s| s.positional.as_slice())
    }
}

fn parse_string_list(val: &Value) -> Vec<String> {
    match val {
        Value::String(s) => vec![s.clone()],
        Value::Seq(seq) => seq
            .iter()
            .filter_map(|item| item.as_str().map(|s| s.to_string()))
            .collect(),
        _ => Vec::new(),
    }
}

/// Maps official built-in element names to their ordered positional
/// argument key(s) -- an empty slice means no positional support at all.
///
/// Official built-in elements (all still single-slot; nothing here has
/// been given a second builtin slot):
/// - `<codeblock>` -> `["lang"]`
/// - `<embed>` -> `["target"]`
/// - `@link` / `<link>` -> `["target"]`
/// - `@meta` / `<meta>` -> `["format"]`
/// - `@config` / `<config>` -> `["format"]`
/// - `@heading` / `<heading>` -> `["level"]`
///
/// A project wanting more slots on a *custom* element (or on `link`
/// itself) defines them via `@settings`'s `positional:[...]` instead --
/// see [`SettingsSchema`]/[`normalized_element_args_with_schema`], which
/// checks that first and only falls back to this table when no schema
/// entry is defined. Note: unnamed `@` elements (e.g. `@(...)`) are
/// deliberately excluded here -- `@()`/`@[]()` inference was retired, so a
/// bare `@(...)` is always `ElementKind::Custom("at")`, never
/// `link`/`embed`/anything else.
pub fn builtin_positional_arg_keys(sigil: &Sigil) -> &'static [&'static str] {
    match sigil {
        Sigil::Type(name) => match name.as_str() {
            "kind" => &["kind"],
            "version" => &["version"],
            "codeblock" => &["lang"],
            "embed" | "link" => &["target"],
            "callout" => &["variant"],
            "meta" | "config" => &["format"],
            "heading" => &["level"],
            _ => &[],
        },
        Sigil::At(Some(name)) => match name.as_str() {
            "kind" => &["kind"],
            "version" => &["version"],
            "meta" | "config" => &["format"],
            "link" => &["target"],
            "heading" => &["level"],
            _ => &[],
        },
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
/// module doc / `docs/develop/architecture.md`'s pipeline diagram) --
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

/// The effective ordered positional-key list for `el`: a custom
/// `@settings`-defined schema entry if one exists and is non-empty for
/// this element name, else the builtin table (see
/// [`builtin_positional_arg_keys`]) -- same priority the two mechanisms
/// already had before being unified into one lookup.
fn effective_positional_keys(
    elem_name: Option<&str>,
    sigil: &Sigil,
    schema: &SettingsSchema,
) -> Vec<String> {
    if let Some(name) = elem_name {
        if let Some(pos_keys) = schema.positional_keys(name) {
            if !pos_keys.is_empty() {
                return pos_keys.to_vec();
            }
        }
    }
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

/// Returns the normalized `args` map for an [`Element`], using default built-in rules.
pub fn normalized_element_args(el: &Element) -> Option<Value> {
    normalized_element_args_with_schema(el, &SettingsSchema::default())
}

/// Returns the normalized `args` map for an [`Element`], taking custom
/// `@settings` into account. One unified pipeline for every shape `args`
/// can take:
/// - `Value::Map`: recover a scheme-collision `target` entry if this
///   element's first slot is `"target"` (see
///   [`recover_target_scheme_entry`]), then fill any still-unfilled slots
///   from bare positional entries the parser tagged with
///   [`POSITIONAL_ENTRY_KEY`] (`@link(tm:foo, depends_on)`'s `depends_on`,
///   or a custom element's `(a, b)`).
/// - `Value::Seq`: an explicit `[...]`-bracketed positional list
///   (`<task>([a, b])`) -- independent of the parser's bare-comma-list
///   sentinel mechanism, zipped against the slot list positionally.
/// - anything else (a bare scalar): wrapped under the first slot, same as
///   before this was generalized.
///
/// An element with no positional slots defined at all (empty list) is
/// returned untouched in every case.
pub fn normalized_element_args_with_schema(el: &Element, schema: &SettingsSchema) -> Option<Value> {
    let args = el.args.as_ref()?;
    let elem_name = match &el.sigil {
        Sigil::Type(name) => Some(name.as_str()),
        Sigil::At(Some(name)) => Some(name.as_str()),
        _ => None,
    };
    let positional_keys = effective_positional_keys(elem_name, &el.sigil, schema);

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
    use tomet_tree::element_new;

    #[test]
    fn normalizes_codeblock_positional_arg() {
        let mut el = element_new(Sigil::Type("codeblock".to_string()));
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
        let mut el = element_new(Sigil::Type("embed".to_string()));
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
        let mut el = element_new(Sigil::Type("link".to_string()));
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
        let mut el = element_new(Sigil::At(Some("link".to_string())));
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
        let mut el = element_new(Sigil::At(Some("link".to_string())));
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
        let mut el = element_new(Sigil::At(Some("link".to_string())));
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
        let mut el = element_new(Sigil::At(Some("link".to_string())));
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
        let mut el = element_new(Sigil::Type("caution".to_string()));
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

    #[test]
    fn fills_a_settings_defined_second_slot_from_a_bare_second_positional_value() {
        // `@link(tm:foo, depends_on)` -- no `predicate` written anywhere
        // in code, purely a project-level `@settings` schema choice. The
        // `tm` entry recovers into `target` first, then the bare
        // (sentinel-keyed) `depends_on` entry fills the still-unfilled
        // `predicate` slot.
        let settings_val = Value::Map(vec![(
            "elements".to_string(),
            Value::Map(vec![(
                "link".to_string(),
                Value::Map(vec![(
                    "positional".to_string(),
                    Value::Seq(vec![
                        Value::String("target".to_string()),
                        Value::String("predicate".to_string()),
                    ]),
                )]),
            )]),
        )]);
        let schema = SettingsSchema::from_value(&settings_val);

        // Args shaped the way the parser actually produces them for
        // `@link(tm:foo, depends_on)`: first entry keyed "tm" (Group A),
        // second bare/sentinel-keyed.
        let mut el = element_new(Sigil::At(Some("link".to_string())));
        el.args = Some(Value::Map(vec![
            ("tm".to_string(), Value::String("foo".to_string())),
            (
                POSITIONAL_ENTRY_KEY.to_string(),
                Value::String("depends_on".to_string()),
            ),
        ]));

        assert_eq!(
            normalized_element_args_with_schema(&el, &schema),
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
    fn settings_positional_schema_now_reachable_from_a_bare_comma_list() {
        // Companion to `normalizes_custom_element_multiple_positional_args_seq_via_settings`
        // (which constructs a `Value::Seq` directly, bypassing the parser)
        // -- this exercises the *other* shape the parser now actually
        // produces for a bare, unbracketed `(a, b)`: a `Value::Map` with
        // two sentinel-keyed entries, not a `Seq`.
        let settings_val = Value::Map(vec![(
            "elements".to_string(),
            Value::Map(vec![(
                "task".to_string(),
                Value::Map(vec![(
                    "positional".to_string(),
                    Value::Seq(vec![
                        Value::String("title".to_string()),
                        Value::String("priority".to_string()),
                    ]),
                )]),
            )]),
        )]);
        let schema = SettingsSchema::from_value(&settings_val);

        let mut el = element_new(Sigil::Type("task".to_string()));
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
            normalized_element_args_with_schema(&el, &schema),
            Some(Value::Map(vec![
                ("title".to_string(), Value::String("Clean room".to_string())),
                ("priority".to_string(), Value::String("high".to_string())),
            ]))
        );
    }

    #[test]
    fn normalizes_meta_positional_arg() {
        let mut el = element_new(Sigil::At(Some("meta".to_string())));
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
        let mut el = element_new(Sigil::Type("codeblock".to_string()));
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
    fn unnamed_at_is_never_normalized() {
        let mut el = element_new(Sigil::At(None));
        el.args = Some(Value::String("https://example.com".to_string()));
        assert_eq!(
            normalized_element_args(&el),
            Some(Value::String("https://example.com".to_string()))
        );
    }

    #[test]
    fn normalizes_custom_element_single_positional_arg_via_settings() {
        let settings_val = Value::Map(vec![(
            "elements".to_string(),
            Value::Map(vec![(
                "task".to_string(),
                Value::Map(vec![(
                    "positional".to_string(),
                    Value::Seq(vec![Value::String("title".to_string())]),
                )]),
            )]),
        )]);
        let schema = SettingsSchema::from_value(&settings_val);

        let mut el = element_new(Sigil::Type("task".to_string()));
        el.args = Some(Value::String("Clean room".to_string()));

        assert_eq!(
            normalized_element_args_with_schema(&el, &schema),
            Some(Value::Map(vec![(
                "title".to_string(),
                Value::String("Clean room".to_string())
            )]))
        );
    }

    #[test]
    fn normalizes_custom_element_multiple_positional_args_seq_via_settings() {
        let settings_val = Value::Map(vec![(
            "elements".to_string(),
            Value::Map(vec![(
                "task".to_string(),
                Value::Map(vec![(
                    "positional".to_string(),
                    Value::Seq(vec![
                        Value::String("title".to_string()),
                        Value::String("priority".to_string()),
                    ]),
                )]),
            )]),
        )]);
        let schema = SettingsSchema::from_value(&settings_val);

        let mut el = element_new(Sigil::Type("task".to_string()));
        el.args = Some(Value::Seq(vec![
            Value::String("Clean room".to_string()),
            Value::String("high".to_string()),
        ]));

        assert_eq!(
            normalized_element_args_with_schema(&el, &schema),
            Some(Value::Map(vec![
                ("title".to_string(), Value::String("Clean room".to_string())),
                ("priority".to_string(), Value::String("high".to_string())),
            ]))
        );
    }
}
