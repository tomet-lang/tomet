//! Decodes a `:rule(allow:list(...), direct:@bool)` connect element's own
//! `args` into a usable [`RuleArgs`]. The check itself (walking an
//! element's descendants against it) lives in `lib.rs`, alongside the
//! other `check_*` passes -- this module is just the args reader.

use tomet_ast::{Element, Name, Value};

/// The decoded arguments of one `:rule(...)` connect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleArgs {
    /// The names `allow:list(...)` named, in source order. Never checked
    /// against declared vocabulary here (or anywhere, in this MVP) --
    /// only compared against what a descendant actually classifies as.
    pub allow: Vec<Name>,
    /// `direct:@bool` -- restrict the check to immediate children only.
    /// Defaults to `false` (every descendant, any depth).
    pub direct: bool,
}

/// Reads `connect_el.args` for a `:rule(...)` connect, or `None` if the
/// shape isn't recognized.
///
/// Permissive by design (an explicit MVP scope decision, not an
/// oversight): a `:rule(...)` with no `allow:`, or one whose `allow:`
/// isn't a `Value::Call` of strings, applies no rule at all -- no
/// diagnostic. The call's own callee (`list`/`enum`/a typo) is never
/// checked; only its arguments matter. Deciding whether malformed
/// `:rule` args deserve their own diagnostic is future work, once real
/// usage shows whether silent no-op is actually confusing in practice.
pub fn decode_rule_args(connect_el: &Element) -> Option<RuleArgs> {
    let Some(Value::Map(entries)) = &connect_el.args else {
        return None;
    };

    let allow_value = entries.iter().find(|(k, _)| k == "allow").map(|(_, v)| v)?;
    let Value::Call(_callee, items) = allow_value else {
        return None;
    };
    let mut allow = Vec::with_capacity(items.len());
    for item in items {
        let Value::String(s) = item else {
            return None;
        };
        allow.push(name_from_dotted(s));
    }

    let direct = match entries.iter().find(|(k, _)| k == "direct").map(|(_, v)| v) {
        Some(Value::Bool(b)) => *b,
        Some(_) => return None,
        None => false,
    };

    Some(RuleArgs { allow, direct })
}

/// `"ns.mycard"` -> a namespaced [`Name`]; `"card"` -> a bare one. Split
/// on the first `.`, matching how `tomet-syntax-ast::Name` itself joins a
/// namespace and a name back together for `Display`.
fn name_from_dotted(s: &str) -> Name {
    match s.split_once('.') {
        Some((namespace, name)) => Name::namespaced(namespace, name),
        None => Name::bare(s),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_ast::Sigil;
    use tomet_tree::element_new;

    fn parse_connect(src: &str) -> Element {
        let doc = tomet_parser::parse_document(src).expect("valid Tomet source");
        let el = match &doc.blocks[0] {
            tomet_ast::Block::Element(el) => el,
            other => panic!("expected an element, got {other:?}"),
        };
        el.connects
            .first()
            .cloned()
            .expect("expected exactly one connect")
    }

    #[test]
    fn decodes_allow_and_defaults_direct_to_false() {
        let connect = parse_connect("@x[a]:rule(allow: list(card))\n");
        assert_eq!(
            decode_rule_args(&connect),
            Some(RuleArgs {
                allow: vec![Name::bare("card")],
                direct: false,
            })
        );
    }

    #[test]
    fn decodes_multiple_allow_entries_including_namespaced_ones() {
        let connect = parse_connect("@x[a]:rule(allow: list(card, ns.mycard), direct: true)\n");
        assert_eq!(
            decode_rule_args(&connect),
            Some(RuleArgs {
                allow: vec![Name::bare("card"), Name::namespaced("ns", "mycard")],
                direct: true,
            })
        );
    }

    #[test]
    fn missing_allow_is_none() {
        let connect = parse_connect("@x[a]:rule(direct: true)\n");
        assert_eq!(decode_rule_args(&connect), None);
    }

    #[test]
    fn no_args_at_all_is_none() {
        let mut connect = element_new(Sigil::named("rule"));
        connect.args = None;
        assert_eq!(decode_rule_args(&connect), None);
    }

    #[test]
    fn allow_not_a_call_is_none() {
        let connect = parse_connect("@x[a]:rule(allow: card)\n");
        assert_eq!(decode_rule_args(&connect), None);
    }
}
