use tomet_ast::Name;

/// The closed vocabulary of `:name(...)` connect identifiers
/// (`@x(...):as(y):rule(...)`), parallel to but distinct from
/// [`crate::kind::ElementKind`]/[`crate::kind::BUILTIN_KINDS`] -- a
/// connect name lives in its own namespace, not the element-name one, so
/// `:rule` and a hypothetical `@rule` element do not collide.
///
/// Not to be confused with two other, unrelated things this codebase
/// already calls "connect":
/// - [`crate::connect::merge_connected_values`] -- the bare
///   `:(...)`/`:{...}` *value-merge* mechanism (`@memo(a:1):(b:2)`),
///   which has nothing to do with names at all.
/// - `tomet-semantics-resolver`'s `RemoteConnection` -- the `<id:...>`
///   remote-reference mechanism.
///
/// This module is the third, separate thing: which bare words are legal
/// right after a `:`. The parser reads `:name(...)` uniformly, without
/// judging the name (mirroring how it never judges an `@name`); this is
/// where the judging happens, the same split `normalized_element_args`
/// already draws between "where" (parser) and "what it means"
/// (semantics).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectMember {
    /// `:rule(allow:list(...), direct:@bool)` -- restricts what element
    /// kinds may appear among an element's descendants (or, with
    /// `direct:true`, its immediate children only).
    Rule,
}

impl ConnectMember {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConnectMember::Rule => "rule",
        }
    }
}

/// The one registration point for connect names -- add a member here and
/// to the enum above, nowhere else, the same discipline `BUILTIN_KINDS`
/// keeps for element names.
pub const CONNECT_MEMBERS: [(&str, ConnectMember); 1] = [("rule", ConnectMember::Rule)];

/// A `:name(...)` whose name is not in [`CONNECT_MEMBERS`].
///
/// Simpler than [`crate::kind::UnknownName`]: a connect name is never
/// namespaced (see [`classify_connect_member`]'s first check), so there
/// is no "missing `@use`" reading to distinguish -- an unrecognized
/// connect name is unconditionally a typo or an unimplemented member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownConnectMember {
    pub name: String,
}

impl std::fmt::Display for UnknownConnectMember {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let known: Vec<&str> = CONNECT_MEMBERS.iter().map(|(name, _)| *name).collect();
        write!(
            f,
            "unknown connect `:{}`; only a closed set may follow `:` (currently: {})",
            self.name,
            known.join(", ")
        )
    }
}

impl std::error::Error for UnknownConnectMember {}

/// Classifies a connect's name (`:name(...)`'s `name`) against the closed
/// [`CONNECT_MEMBERS`] table.
///
/// `name` is never namespaced -- a connect name is Tomet's own closed
/// syntax, not something a document's vocabulary can extend, so
/// `:ns.rule(...)` is unknown unconditionally rather than "namespace not
/// in scope".
pub fn classify_connect_member(name: &Name) -> Result<ConnectMember, UnknownConnectMember> {
    if !name.is_bare() {
        return Err(UnknownConnectMember {
            name: name.to_string(),
        });
    }
    CONNECT_MEMBERS
        .iter()
        .find(|(candidate, _)| *candidate == name.name)
        .map(|(_, member)| *member)
        .ok_or_else(|| UnknownConnectMember {
            name: name.name.clone(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_classifies() {
        assert_eq!(
            classify_connect_member(&Name::bare("rule")),
            Ok(ConnectMember::Rule)
        );
    }

    #[test]
    fn a_typo_is_unknown() {
        let err = classify_connect_member(&Name::bare("rul")).unwrap_err();
        assert_eq!(err.name, "rul");
    }

    #[test]
    fn a_namespaced_connect_name_is_unknown() {
        let err = classify_connect_member(&Name::namespaced("ns", "rule")).unwrap_err();
        assert_eq!(err.name, "ns.rule");
    }
}
