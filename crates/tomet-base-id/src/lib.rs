//! =[ tomet-id ]
//!
//! The shape of a document's id, whether a text has it, and drawing a new one.
//!
//! =[ Architecture & Responsibilities ]
//!
//! - (One policy)
//!   | A vault says what its ids look like -- a [`Type`], a prefix, a length. Whoever makes an
//!   | id (`tomet convert`, a blueprint) and whoever holds one to the vault's policy (the
//!   | validator) ask [`IdSpec`], so they cannot disagree about it.
//! - (No settings here)
//!   | [`IdSpec`] is built from plain values. Reading them from a vault's settings is the
//!   | caller's, which keeps this crate below the crates that know the settings.
//! - (Drawing is random)
//!   | [`IdSpec::generate`] is the one place that draws. A caller that needs a fixed result
//!   | does not call it.

use tomet_type::{Type, is_uuid};

/// What a vault's ids look like: a prefix, then a body of the form the type gives.
///
/// - `uuid`: a UUID, in the 8-4-4-4-12 form ([`is_uuid`]). The length is not asked for.
/// - `nanoid`: a nanoid of the given length, from the letters, digits, `_` and `-`.
///
/// No other type is a form an id can have.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdSpec {
    prefix: String,
    body: Body,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Body {
    Uuid,
    Nanoid { length: usize },
}

impl IdSpec {
    /// The length of a nanoid when the vault gives none.
    pub const DEFAULT_LENGTH: usize = 8;

    /// The spec for ids of type `ty`, with `prefix` before the body and, for a nanoid, a body of
    /// `length` ([`DEFAULT_LENGTH`](Self::DEFAULT_LENGTH) when `None`).
    ///
    /// `None` when `ty` is not a form an id can have: only `uuid` and `nanoid` are.
    pub fn new(ty: &Type, prefix: Option<&str>, length: Option<usize>) -> Option<IdSpec> {
        let body = match ty {
            Type::Uuid => Body::Uuid,
            Type::Nanoid => Body::Nanoid {
                length: length.unwrap_or(Self::DEFAULT_LENGTH),
            },
            _ => return None,
        };
        Some(IdSpec {
            prefix: prefix.unwrap_or_default().to_string(),
            body,
        })
    }

    /// A new id of this shape. Every call draws a new one.
    pub fn generate(&self) -> String {
        let body = match self.body {
            Body::Uuid => uuid::Uuid::new_v4().to_string(),
            Body::Nanoid { length } => nanoid::nanoid!(length),
        };
        format!("{}{body}", self.prefix)
    }

    /// Whether `text` has this shape: the prefix, then a body of the form.
    pub fn accepts(&self, text: &str) -> bool {
        let Some(body) = text.strip_prefix(self.prefix.as_str()) else {
            return false;
        };
        match self.body {
            Body::Uuid => is_uuid(body),
            Body::Nanoid { length } => {
                body.len() == length
                    && body
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(ty: Type, prefix: Option<&str>, length: Option<usize>) -> IdSpec {
        IdSpec::new(&ty, prefix, length).expect("an id form")
    }

    #[test]
    fn a_nanoid_has_the_length_and_the_prefix_asked_for() {
        let spec = spec(Type::Nanoid, Some("doc-"), Some(8));
        let id = spec.generate();
        assert!(id.starts_with("doc-"));
        assert_eq!(id.len(), 4 + 8);
    }

    #[test]
    fn a_nanoid_is_eight_long_when_no_length_is_given() {
        let id = spec(Type::Nanoid, None, None).generate();
        assert_eq!(id.len(), IdSpec::DEFAULT_LENGTH);
    }

    #[test]
    fn a_uuid_ignores_the_length() {
        let id = spec(Type::Uuid, None, Some(3)).generate();
        assert_eq!(id.len(), 36);
        assert_eq!(id.matches('-').count(), 4);
    }

    #[test]
    fn a_type_that_is_neither_is_no_form_of_id() {
        for ty in [
            Type::String,
            Type::Int,
            Type::Datetime,
            Type::Version,
            Type::Bool,
        ] {
            assert_eq!(IdSpec::new(&ty, None, None), None, "{ty}");
        }
    }

    #[test]
    fn what_a_spec_draws_it_accepts() {
        for spec in [
            spec(Type::Nanoid, Some("doc-"), Some(12)),
            spec(Type::Nanoid, None, None),
            spec(Type::Uuid, None, None),
            spec(Type::Uuid, Some("u:"), None),
        ] {
            for _ in 0..50 {
                let id = spec.generate();
                assert!(spec.accepts(&id), "{spec:?} drew {id:?}");
            }
        }
    }

    #[test]
    fn a_nanoid_is_held_to_prefix_length_and_alphabet() {
        let spec = spec(Type::Nanoid, Some("doc-"), Some(8));
        assert!(spec.accepts("doc-12345678"));
        assert!(spec.accepts("doc-a_b-C9d0"));
        assert!(!spec.accepts("invalid-slug"));
        assert!(!spec.accepts("doc-123"));
        assert!(!spec.accepts("doc-123456789"));
        assert!(!spec.accepts("12345678"));
        assert!(!spec.accepts("doc-1234 678"));
    }

    /// The length is the nanoid's; a uuid is as long as a uuid is.
    #[test]
    fn a_uuid_is_held_to_the_uuid_form_not_to_a_length() {
        let spec = spec(Type::Uuid, None, None);
        assert!(spec.accepts("123e4567-e89b-12d3-a456-426614174000"));
        assert!(spec.accepts("123E4567-E89B-12D3-A456-426614174000"));
        assert!(!spec.accepts("12345678"));
        assert!(!spec.accepts("123e4567-e89b-12d3-a456-42661417400"));
    }

    #[test]
    fn a_uuid_with_a_prefix_needs_the_prefix() {
        let spec = spec(Type::Uuid, Some("u:"), None);
        assert!(spec.accepts("u:123e4567-e89b-12d3-a456-426614174000"));
        assert!(!spec.accepts("123e4567-e89b-12d3-a456-426614174000"));
    }
}
