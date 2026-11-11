//! =[ tomet-id ]
//!
//! The shape of a document's id, whether a text has it, and drawing a new one.
//!
//! =[ Architecture & Responsibilities ]
//!
//! - (One policy)
//!   | A vault says what its ids look like -- a [`Type`] that is a `uuid` or a `nanoid`, with
//!   | the `prefix` and `length` the type carries. Whoever makes an id (`tomet convert`, a
//!   | blueprint) and whoever holds one to the vault's policy (the validator) ask [`IdSpec`],
//!   | so they cannot disagree about it.
//! - (No settings here)
//!   | [`IdSpec`] is built from a [`Type`]. Reading one from a vault's settings is the
//!   | caller's, which keeps this crate below the crates that know the settings.
//! - (Drawing is random)
//!   | [`IdSpec::generate`] is the one place that draws. A caller that needs a fixed result
//!   | does not call it.

use tomet_type::{Kind, Type, is_uuid};

/// What a vault's ids look like: the type's prefix, then a body of the form the type gives.
///
/// - `uuid`: a UUID, in the 8-4-4-4-12 form ([`is_uuid`]); a `uuid` has no length.
/// - `nanoid`: a nanoid of the type's length ([`DEFAULT_LENGTH`](IdSpec::DEFAULT_LENGTH) when it
///   gives none), from the letters, digits, `_` and `-`.
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
    /// The length of a nanoid when the type gives none.
    pub const DEFAULT_LENGTH: usize = 8;

    /// The spec for ids of type `ty`: its [`prefix`](Type::prefix) before the body and, for a
    /// nanoid, a body of its [`length`](Type::length).
    ///
    /// `None` when `ty` is not a form an id can have: only `uuid` and `nanoid` are. Whether the
    /// type's `default` is `auto` is not asked here: it says when an id is made, not what one is.
    pub fn new(ty: &Type) -> Option<IdSpec> {
        let body = match ty.kind() {
            Kind::Uuid => Body::Uuid,
            Kind::Nanoid => Body::Nanoid {
                length: ty.length().unwrap_or(Self::DEFAULT_LENGTH),
            },
            _ => return None,
        };
        Some(IdSpec {
            prefix: ty.prefix().unwrap_or_default().to_string(),
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
    use tomet_type::{Arg, Datum};

    /// The type `name` with the arguments given: what a vault's `id` settings come to.
    fn ty(name: &str, prefix: Option<&str>, length: Option<i64>) -> Type {
        let mut named = Vec::new();
        if let Some(length) = length {
            named.push(("length", Arg::Datum(Datum::Int(length))));
        }
        if let Some(prefix) = prefix {
            named.push(("prefix", Arg::Datum(Datum::Text(prefix.to_string()))));
        }
        Type::build(name, &[], &named).expect("a type")
    }

    fn spec(name: &str, prefix: Option<&str>, length: Option<i64>) -> IdSpec {
        IdSpec::new(&ty(name, prefix, length)).expect("an id form")
    }

    #[test]
    fn a_nanoid_has_the_length_and_the_prefix_asked_for() {
        let spec = spec("nanoid", Some("doc-"), Some(8));
        let id = spec.generate();
        assert!(id.starts_with("doc-"));
        assert_eq!(id.len(), 4 + 8);
    }

    #[test]
    fn a_nanoid_is_eight_long_when_no_length_is_given() {
        let id = spec("nanoid", None, None).generate();
        assert_eq!(id.len(), IdSpec::DEFAULT_LENGTH);
    }

    #[test]
    fn the_spec_is_read_from_the_type_as_written() {
        let spec = IdSpec::new(&"nanoid(length: 5, prefix: \"n-\")".parse().unwrap()).unwrap();
        let id = spec.generate();
        assert!(id.starts_with("n-"));
        assert_eq!(id.len(), 2 + 5);
        // `default: auto` says when an id is made; the shape is the same.
        let auto = IdSpec::new(&"uuid(default: auto)".parse().unwrap()).unwrap();
        assert_eq!(auto, IdSpec::new(&"uuid".parse().unwrap()).unwrap());
    }

    #[test]
    fn a_uuid_is_as_long_as_a_uuid_is() {
        let id = spec("uuid", None, None).generate();
        assert_eq!(id.len(), 36);
        assert_eq!(id.matches('-').count(), 4);
    }

    #[test]
    fn a_type_that_is_neither_is_no_form_of_id() {
        for kind in Kind::ALL {
            if matches!(kind, Kind::Uuid | Kind::Nanoid) {
                continue;
            }
            assert_eq!(IdSpec::new(&Type::bare(kind)), None, "{kind}");
        }
    }

    #[test]
    fn what_a_spec_draws_it_accepts() {
        for spec in [
            spec("nanoid", Some("doc-"), Some(12)),
            spec("nanoid", None, None),
            spec("uuid", None, None),
            spec("uuid", Some("u:"), None),
        ] {
            for _ in 0..50 {
                let id = spec.generate();
                assert!(spec.accepts(&id), "{spec:?} drew {id:?}");
            }
        }
    }

    #[test]
    fn a_nanoid_is_held_to_prefix_length_and_alphabet() {
        let spec = spec("nanoid", Some("doc-"), Some(8));
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
        let spec = spec("uuid", None, None);
        assert!(spec.accepts("123e4567-e89b-12d3-a456-426614174000"));
        assert!(spec.accepts("123E4567-E89B-12D3-A456-426614174000"));
        assert!(!spec.accepts("12345678"));
        assert!(!spec.accepts("123e4567-e89b-12d3-a456-42661417400"));
    }

    #[test]
    fn a_uuid_with_a_prefix_needs_the_prefix() {
        let spec = spec("uuid", Some("u:"), None);
        assert!(spec.accepts("u:123e4567-e89b-12d3-a456-426614174000"));
        assert!(!spec.accepts("123e4567-e89b-12d3-a456-426614174000"));
    }
}
