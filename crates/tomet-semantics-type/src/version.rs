//! A version: dotted non-negative integers, `1`, `1.0`, `1.10.0`.
//!
//! Compared the way versions are compared, component by component as numbers,
//! so `1.10` is after `1.9` -- which as `f64`s it is not. A missing component is
//! `0`: `1.0` and `1.0.0` are the same version in the order, though not the same
//! text.
//!
//! Only the numeric core is read. A pre-release (`1.0.0-beta`) or build
//! (`+5`) part is not yet: text with one is not a version here, and it is added
//! when something needs it.

use std::cmp::Ordering;

/// A version, read from text.
#[derive(Debug, Clone)]
pub struct Version {
    parts: Vec<u64>,
}

impl Version {
    /// Reads `text` as a version: one or more components of ASCII digits joined
    /// by single `.`, each fitting a `u64`. Surrounding whitespace is not part
    /// of it. A component may have a leading zero (`2026.01`): the calendar
    /// versions that are written so are versions.
    pub fn parse(text: &str) -> Option<Version> {
        let parts = text
            .split('.')
            .map(|component| {
                (!component.is_empty() && component.bytes().all(|b| b.is_ascii_digit()))
                    .then(|| component.parse::<u64>().ok())
                    .flatten()
            })
            .collect::<Option<Vec<u64>>>()?;
        Some(Version { parts })
    }

    /// The components, in the order written.
    pub fn components(&self) -> &[u64] {
        &self.parts
    }

    /// The first component: the major version.
    pub fn major(&self) -> u64 {
        self.parts[0]
    }

    /// The components with trailing zeros dropped, which is what the order and
    /// equality look at: `1.0` and `1.0.0` both come to `1`.
    fn significant(&self) -> &[u64] {
        let end = self
            .parts
            .iter()
            .rposition(|&n| n != 0)
            .map_or(0, |i| i + 1);
        &self.parts[..end]
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.significant() == other.significant()
    }
}

impl Eq for Version {}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        // A shorter list is the same list with zeros after it, so comparing
        // component by component with `0` for what is missing is comparing the
        // two lists as they are, once the trailing zeros are gone.
        self.significant().cmp(other.significant())
    }
}

impl std::hash::Hash for Version {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.significant().hash(state);
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text: Vec<String> = self.parts.iter().map(u64::to_string).collect();
        f.write_str(&text.join("."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap_or_else(|| panic!("{text} should be a version"))
    }

    #[test]
    fn dotted_non_negative_integers_are_versions() {
        for text in [
            "1",
            "0",
            "1.0",
            "1.10",
            "1.10.0",
            "0.5.0",
            "2026.01.5",
            "10.0.0.1",
        ] {
            assert!(Version::parse(text).is_some(), "{text}");
        }
        assert_eq!(v("1.10.0").components(), [1, 10, 0]);
        assert_eq!(v("1.10.0").major(), 1);
    }

    #[test]
    fn anything_else_is_not() {
        for text in [
            "",
            ".",
            "1.",
            ".1",
            "1..0",
            "1.0.",
            "-1",
            "+1",
            "1.0-beta",
            "1.0+5",
            "v1.0",
            "1.x",
            "1,0",
            " 1.0",
            "1.0 ",
            "1_0",
            "0x10",
            "1e3",
            "99999999999999999999",
        ] {
            assert!(Version::parse(text).is_none(), "{text:?}");
        }
    }

    #[test]
    fn they_are_ordered_as_versions_not_as_numbers_or_text() {
        assert!(v("1.10") > v("1.9"));
        assert!("1.10" < "1.9", "as text the order is the other way");
        assert!(1.10_f64 < 1.9_f64, "and as numbers too");
        assert!(v("1.10.0") > v("1.9.9"));
        assert!(v("2") > v("1.99.99"));
        assert!(v("0.5.0") < v("0.10"));
        assert!(v("1.0.1") > v("1.0"));
    }

    #[test]
    fn a_missing_component_is_zero() {
        assert_eq!(v("1.0"), v("1.0.0"));
        assert_eq!(v("1"), v("1.0.0.0"));
        assert_ne!(v("1.0"), v("1.0.1"));
        assert_eq!(v("1.0").cmp(&v("1.0.0")), Ordering::Equal);
        // equal in the order, different as written
        assert_eq!(v("1.0").to_string(), "1.0");
        assert_eq!(v("1.0.0").to_string(), "1.0.0");
    }

    #[test]
    fn equal_versions_hash_alike() {
        use std::collections::HashSet;
        let set: HashSet<Version> = [v("1.0"), v("1.0.0"), v("1.0.1")].into_iter().collect();
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn a_leading_zero_in_a_component_is_read_as_the_number() {
        assert_eq!(v("2026.01"), v("2026.1"));
        assert!(v("2026.10") > v("2026.09"));
    }
}
