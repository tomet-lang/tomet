//! Guard: the parser must build the tree without consulting any element
//! vocabulary.
//!
//! External files may add constraints or change rendering; they must never
//! change the tree. Concretely, renaming an element -- or renaming a key
//! inside its `(args)` -- must not change the *shape* of what the parser
//! produces, only the name recorded in it.
//!
//! Each case below is a pair of sources differing in exactly one identifier
//! the parser is not allowed to recognize. Both are parsed, the identifier is
//! normalized to a common placeholder in each tree, and the two trees are
//! compared. `Span`'s `PartialEq` is unconditionally `true`
//! (`tomet-syntax-ast/src/lib.rs`), so `assert_eq!` on a `Document` compares
//! shape rather than source locations -- which is exactly what this guard
//! wants.
//!
//! These are deliberately hand-written pairs rather than a sweep over
//! `parseable_corpus()`. A corpus sweep would have to rewrite element names
//! inside arbitrary Japanese prose, where the same token also occurs as plain
//! text; the rewrite would change the text content and the trees would differ
//! for reasons that have nothing to do with vocabulary. Targeted pairs say the
//! same thing without that noise, and each one names the branch it pins
//! down -- a branch the parser used to have, and would fail again the
//! moment one was reintroduced.

use tomet_ast::{Document, Element, Name, Sigil, Value};
use tomet_parser::parse_document;
use tomet_tree::for_each_element_mut;

/// Which kind of identifier a case renames.
#[derive(Clone, Copy)]
enum Ident {
    /// An element name: `codeblock` in `#codeblock`, `meta` in `#meta`.
    ElementName,
    /// A key inside `(args)`: `content:`, `format:`, ...
    ArgKey,
    /// The namespace half of an element name: `deck` in `#deck.card`.
    Namespace,
}

/// The placeholder both spellings are normalized to before comparing.
const PLACEHOLDER: &str = "__normalized__";

struct Case {
    /// What branch of the parser this pins down.
    label: &'static str,
    /// Source using the identifier the parser currently recognizes.
    known: &'static str,
    /// The same source with that identifier replaced by an unknown one.
    unknown: &'static str,
    /// The recognized spelling, as it appears in `known`.
    known_ident: &'static str,
    /// The unknown spelling, as it appears in `unknown`.
    unknown_ident: &'static str,
    kind: Ident,
}

/// Every place the parser once consulted an element vocabulary.
///
/// `*a*` is the probe in the content cases: parsed as markup it becomes an
/// `em` element, parsed raw it stays one `Text`. That difference is what
/// made a vocabulary lookup visible in the tree.
const CASES: &[Case] = &[
    Case {
        label: "`#raw`'s content is not parsed differently from any \
                other element's (was codeblock.rs `is_codeblock`)",
        known: "@raw[ *a* ]",
        unknown: "@zzz[ *a* ]",
        known_ident: "raw",
        unknown_ident: "zzz",
        kind: Ident::ElementName,
    },
    Case {
        label: "`(content:raw)` is an ordinary argument and does not switch \
                `[...]` to verbatim (was codeblock.rs `is_verbatim_content`)",
        known: "@memo(content:raw)[ *a* ]",
        unknown: "@memo(zcontent:raw)[ *a* ]",
        known_ident: "content",
        unknown_ident: "zcontent",
        kind: Ident::ArgKey,
    },
    Case {
        label: "`(format:json)` does not change how a `+++` fence is lexed \
                (was element.rs `local_format_key`)",
        known: "@x(format:json)+++\n{\"a\": 1}\n+++\n",
        unknown: "@x(zformat:json)+++\n{\"a\": 1}\n+++\n",
        known_ident: "format",
        unknown_ident: "zformat",
        kind: Ident::ArgKey,
    },
    Case {
        label: "`#meta` gets no bare-string format shorthand of its own \
                (was element.rs `is_format_target_element`)",
        // `~` is the probe: read as YAML it is null, read by Tomet's own
        // value grammar it is the string "~". `a: 1` would have parsed
        // identically either way and quietly passed.
        known: "@meta(yaml){ a: ~ }",
        unknown: "@zzz(yaml){ a: ~ }",
        known_ident: "meta",
        unknown_ident: "zzz",
        kind: Ident::ElementName,
    },
    Case {
        label: "`#config` sets no document-wide lexing mode \
                (was element.rs `is_config`, threaded as `running_format`)",
        known: "@config(format:json)+++\n{\"a\": 1}\n+++\n\n@x{ b: 2 }\n",
        unknown: "@zzz(format:json)+++\n{\"a\": 1}\n+++\n\n@x{ b: 2 }\n",
        known_ident: "config",
        unknown_ident: "zzz",
        kind: Ident::ElementName,
    },
    Case {
        label: "a namespaced name is lexed exactly like a bare one",
        known: "@deck.card(a:1)[ x ]",
        unknown: "@zzz.card(a:1)[ x ]",
        known_ident: "deck",
        unknown_ident: "zzz",
        kind: Ident::Namespace,
    },
];

/// Runs one case the way the guard does: parse both sources, normalize the
/// one identifier the parser is not allowed to recognize, compare the trees.
/// `Some(report)` means they differed -- a vocabulary lookup got through.
///
/// A separate function rather than a loop body so that
/// [`the_guard_reports_a_difference_it_is_given`] can drive the same code.
/// A guard that cannot be shown to fail is not known to work, and this one
/// is the only thing standing behind `no-vocabulary`.
fn compare_case(case: &Case) -> Option<String> {
    let mut known = match parse_document(case.known) {
        Ok(doc) => doc,
        Err(e) => {
            return Some(format!(
                "{}\n    known source failed to parse: {e}",
                case.label
            ));
        }
    };
    let mut unknown = match parse_document(case.unknown) {
        Ok(doc) => doc,
        Err(e) => {
            return Some(format!(
                "{}\n    unknown source failed to parse: {e}",
                case.label
            ));
        }
    };

    normalize(&mut known, case.known_ident, case.kind);
    normalize(&mut unknown, case.unknown_ident, case.kind);

    if known != unknown {
        return Some(format!(
            "{}\n      with `{}`: {known:?}\n      with `{}`: {unknown:?}",
            case.label, case.known_ident, case.unknown_ident,
        ));
    }
    None
}

#[test]
fn element_names_and_arg_keys_do_not_change_the_tree() {
    let failures: Vec<String> = CASES.iter().filter_map(compare_case).collect();

    assert!(
        failures.is_empty(),
        "the parser consulted an element vocabulary in {} case(s):\n\n{}",
        failures.len(),
        failures.join("\n\n"),
    );
}

/// Proves [`compare_case`] can still say "different".
///
/// Every part of the comparison can rot into a no-op -- drop the `!=`,
/// normalize too much, compare something always equal -- and the result
/// looks exactly like a guard that passes. This feeds it a pair that
/// differs in *shape*, which renaming an identifier cannot hide: `*a*` is
/// an `em` element, `a` is one `Text`. If this stops failing, the guard
/// above has stopped checking anything.
///
/// Not hypothetical. The tree-sitter drift check in this same package
/// carried an empty marker for as long as it existed, and
/// `text.contains("")` is always true.
#[test]
fn the_guard_reports_a_difference_it_is_given() {
    let planted = Case {
        label: "planted difference: the two sources are not the same tree",
        known: "@memo[ *a* ]",
        unknown: "@zzz[ a ]",
        known_ident: "memo",
        unknown_ident: "zzz",
        kind: Ident::ElementName,
    };

    assert!(
        compare_case(&planted).is_some(),
        "the comparison no longer distinguishes trees that differ"
    );
}

/// Rewrites every occurrence of `ident` in `doc` to [`PLACEHOLDER`], so that
/// two trees built from differently-named sources can be compared directly.
fn normalize(doc: &mut Document, ident: &str, kind: Ident) {
    for_each_element_mut(doc, |el| match kind {
        Ident::ElementName => rename_element(el, ident),
        Ident::Namespace => rename_namespace(el, ident),
        Ident::ArgKey => rename_arg_key(el, ident),
    });
}

fn rename_element(el: &mut Element, ident: &str) {
    if let Some(name) = sigil_name_mut(&mut el.sigil)
        && name.name == ident
    {
        name.name = PLACEHOLDER.to_string();
    }
}

fn rename_namespace(el: &mut Element, ident: &str) {
    if let Some(name) = sigil_name_mut(&mut el.sigil)
        && name.namespace.as_deref() == Some(ident)
    {
        name.namespace = Some(PLACEHOLDER.to_string());
    }
}

fn sigil_name_mut(sigil: &mut Sigil) -> Option<&mut Name> {
    match sigil {
        Sigil::Named(name) => Some(name),
        Sigil::Bare | Sigil::Dollar | Sigil::Caret(_) => None,
    }
}

fn rename_arg_key(el: &mut Element, ident: &str) {
    if let Some(args) = &mut el.args {
        rename_keys(args, ident);
    }
    if let Some(value) = &mut el.value {
        for (key, val) in value.pairs_mut() {
            if key == ident {
                *key = PLACEHOLDER.to_string();
            }
            rename_keys(val, ident);
        }
    }
}

fn rename_keys(value: &mut Value, ident: &str) {
    match value {
        Value::Map(entries) => {
            for (key, val) in entries {
                if key == ident {
                    *key = PLACEHOLDER.to_string();
                }
                rename_keys(val, ident);
            }
        }
        Value::Seq(items) => {
            for item in items {
                rename_keys(item, ident);
            }
        }
        _ => {}
    }
}
