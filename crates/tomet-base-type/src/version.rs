//! A version: dotted non-negative integers (`1`, `1.0`, `1.10.0`), with optional
//! pre-release (`-beta`, `-rc.1`) and build metadata (`+5`, `+build.123`).
//!
//! Compared the way versions are compared:
//! - Core components are compared as numbers, so `1.10` is after `1.9`.
//!   Trailing zeros are omitted in ordering and equality: `1.0` and `1.0.0`
//!   are the same version in the order, though not the same text.
//! - A version without a pre-release takes precedence over one with a pre-release
//!   (`1.0.0 > 1.0.0-beta`).
//! - Pre-release identifiers are compared left to right: numeric identifiers are
//!   compared as numbers (`alpha.10 > alpha.9`, with leading zeros tolerated so
//!   `01 == 1`), non-numeric identifiers are compared in ASCII sort order, and
//!   numeric identifiers have lower precedence than non-numeric ones (`1.0.0-1 < 1.0.0-alpha`).
//! - Build metadata (`+...`) is preserved for display and raw text, but completely
//!   ignored in equality, ordering, and hashing (`1.0.0+1 == 1.0.0+2`).

use std::cmp::Ordering;

/// An identifier within a pre-release version string.
///
/// Pre-release identifiers are dot-separated and can be either numeric
/// (interpreted and compared as non-negative integers) or alphanumeric
/// (compared in ASCII lexicographical order).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Identifier {
    /// A numeric identifier, parsed and compared as a 64-bit unsigned integer.
    Numeric(u64),
    /// An alphanumeric identifier, compared in ASCII lexicographical order.
    Alpha(String),
}

impl Identifier {
    /// Whether this identifier is numeric.
    pub fn is_numeric(&self) -> bool {
        matches!(self, Self::Numeric(_))
    }

    /// The numeric value, if this identifier is numeric.
    pub fn as_numeric(&self) -> Option<u64> {
        match self {
            Self::Numeric(n) => Some(*n),
            Self::Alpha(_) => None,
        }
    }

    /// The string slice if alphanumeric, or `None` if numeric.
    pub fn as_alpha(&self) -> Option<&str> {
        match self {
            Self::Numeric(_) => None,
            Self::Alpha(s) => Some(s),
        }
    }
}

impl std::fmt::Display for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Numeric(n) => write!(f, "{n}"),
            Self::Alpha(s) => f.write_str(s),
        }
    }
}

/// A version, read from text.
#[derive(Debug, Clone)]
pub struct Version {
    parts: Vec<u64>,
    prerelease: Option<Vec<Identifier>>,
    build: Option<Vec<String>>,
    raw: String,
}

impl Version {
    /// Reads `text` as a version: `<core>[-<pre-release>][+<build>]`.
    ///
    /// - The numeric core consists of one or more components of ASCII digits joined
    ///   by single `.`, each fitting a `u64`. Leading zeros are permitted (`2026.01`).
    /// - Pre-release identifiers are preceded by `-` and joined by single `.`.
    ///   They consist of ASCII alphanumeric characters and hyphens `[0-9A-Za-z-]`.
    ///   Numeric identifiers allow leading zeros and compare numerically (`01 == 1`).
    /// - Build metadata identifiers are preceded by `+` and joined by single `.`.
    ///   They consist of ASCII alphanumeric characters and hyphens `[0-9A-Za-z-]`.
    ///
    /// Surrounding whitespace is not permitted. Underscores `_` and empty identifiers
    /// are forbidden.
    pub fn parse(text: &str) -> Option<Version> {
        if text.is_empty() {
            return None;
        }

        // Build metadata starts at the first '+'.
        let (core_and_pre, build) = if let Some(plus_idx) = text.find('+') {
            let build_str = &text[plus_idx + 1..];
            if build_str.is_empty() {
                return None;
            }
            let mut build_ids = Vec::new();
            for id in build_str.split('.') {
                if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
                    return None;
                }
                build_ids.push(id.to_string());
            }
            (&text[..plus_idx], Some(build_ids))
        } else {
            (text, None)
        };

        // Pre-release starts at the first '-' in core_and_pre.
        let (core_str, prerelease) = if let Some(hyphen_idx) = core_and_pre.find('-') {
            let pre_str = &core_and_pre[hyphen_idx + 1..];
            if pre_str.is_empty() {
                return None;
            }
            let mut pre_ids = Vec::new();
            for id in pre_str.split('.') {
                if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
                    return None;
                }
                if id.bytes().all(|b| b.is_ascii_digit()) {
                    let num = id.parse::<u64>().ok()?;
                    pre_ids.push(Identifier::Numeric(num));
                } else {
                    pre_ids.push(Identifier::Alpha(id.to_string()));
                }
            }
            (&core_and_pre[..hyphen_idx], Some(pre_ids))
        } else {
            (core_and_pre, None)
        };

        if core_str.is_empty() {
            return None;
        }

        let parts = core_str
            .split('.')
            .map(|component| {
                (!component.is_empty() && component.bytes().all(|b| b.is_ascii_digit()))
                    .then(|| component.parse::<u64>().ok())
                    .flatten()
            })
            .collect::<Option<Vec<u64>>>()?;

        Some(Version {
            parts,
            prerelease,
            build,
            raw: text.to_string(),
        })
    }

    /// The components of the numeric core, in the order written.
    pub fn components(&self) -> &[u64] {
        &self.parts
    }

    /// The first component: the major version.
    pub fn major(&self) -> u64 {
        self.parts[0]
    }

    /// The second component: the minor version, or 0 if omitted.
    pub fn minor(&self) -> u64 {
        self.parts.get(1).copied().unwrap_or(0)
    }

    /// The third component: the patch version, or 0 if omitted.
    pub fn patch(&self) -> u64 {
        self.parts.get(2).copied().unwrap_or(0)
    }

    /// The exact text as originally parsed.
    pub fn raw(&self) -> &str {
        &self.raw
    }

    /// Whether this version has pre-release identifiers.
    pub fn is_prerelease(&self) -> bool {
        self.prerelease.is_some()
    }

    /// The pre-release identifiers, if present.
    pub fn prerelease(&self) -> Option<&[Identifier]> {
        self.prerelease.as_deref()
    }

    /// The build metadata identifiers, if present.
    pub fn build(&self) -> Option<&[String]> {
        self.build.as_deref()
    }

    /// The core components with trailing zeros dropped, which is what the order and
    /// equality look at: `1.0` and `1.0.0` both come to `[1]`.
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
        self.significant() == other.significant() && self.prerelease == other.prerelease
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
        let core_cmp = self.significant().cmp(other.significant());
        if core_cmp != Ordering::Equal {
            return core_cmp;
        }
        match (&self.prerelease, &other.prerelease) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(a), Some(b)) => a.cmp(b),
        }
    }
}

impl std::hash::Hash for Version {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.significant().hash(state);
        self.prerelease.hash(state);
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text: Vec<String> = self.parts.iter().map(u64::to_string).collect();
        f.write_str(&text.join("."))?;
        if let Some(ref pre) = self.prerelease {
            f.write_str("-")?;
            for (i, id) in pre.iter().enumerate() {
                if i > 0 {
                    f.write_str(".")?;
                }
                write!(f, "{id}")?;
            }
        }
        if let Some(ref build) = self.build {
            f.write_str("+")?;
            f.write_str(&build.join("."))?;
        }
        Ok(())
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
        assert_eq!(v("1.10.0").minor(), 10);
        assert_eq!(v("1.10.0").patch(), 0);
        assert_eq!(v("1").minor(), 0);
        assert_eq!(v("1").patch(), 0);
    }

    #[test]
    fn prerelease_and_build_versions_are_parsed() {
        for text in [
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-0.3.7",
            "1.0.0-x.7.z.92",
            "1.0.0-x-7.x.92",
            "1.0.0-alpha+001",
            "1.0.0+20130313144700",
            "1.0.0-beta+exp.sha.5114f85",
            "1.0.0+21AF26D3----117B344092BD",
            "1.0-beta",
            "1-beta",
            "0-alpha.1",
            "1.0.0-01",
            "1.0.0--alpha",
            "1.0.0--",
        ] {
            assert!(Version::parse(text).is_some(), "{text}");
        }

        let ver = v("1.2.3-alpha.1+build.42");
        assert_eq!(ver.components(), [1, 2, 3]);
        assert_eq!(ver.major(), 1);
        assert_eq!(ver.minor(), 2);
        assert_eq!(ver.patch(), 3);
        assert!(ver.is_prerelease());
        assert_eq!(
            ver.prerelease(),
            Some(
                [
                    Identifier::Alpha("alpha".to_string()),
                    Identifier::Numeric(1),
                ]
                .as_slice()
            )
        );
        assert_eq!(
            ver.build(),
            Some(["build".to_string(), "42".to_string()].as_slice())
        );
        assert_eq!(ver.raw(), "1.2.3-alpha.1+build.42");
    }

    #[test]
    fn invalid_versions_are_rejected() {
        for text in [
            "",
            ".",
            "1.",
            ".1",
            "1..0",
            "1.0.",
            "-1",
            "+1",
            "v1.0",
            "v1.0.0-beta",
            "1.x",
            "1,0",
            " 1.0",
            "1.0 ",
            "1_0",
            "0x10",
            "1e3",
            "99999999999999999999",
            "1.0.0-",
            "1.0.0+",
            "1.0.0-+",
            "1.0.0-alpha.",
            "1.0.0-.alpha",
            "1.0.0-alpha..beta",
            "1.0.0+build.",
            "1.0.0+.build",
            "1.0.0+build..1",
            "1.0.0-alpha_beta",
            "1.0.0+build_id",
            "1.0.0+build+extra",
            "1.0.0-alpha+beta+gamma",
            "1.0.0-99999999999999999999",
            "1.0.0-alpha@",
            "1.0.0+build$",
        ] {
            assert!(Version::parse(text).is_none(), "{text:?}");
        }
    }

    #[test]
    fn they_are_ordered_as_versions_not_as_numbers_or_text() {
        assert!(v("1.10") > v("1.9"));
        assert!("1.10" < "1.9", "as text the order is the other way");
        let number = |text: &str| text.parse::<f64>().unwrap();
        assert!(number("1.10") < number("1.9"), "and as numbers too");
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
    fn short_cores_with_prerelease() {
        assert_eq!(v("1-beta"), v("1.0-beta"));
        assert_eq!(v("1.0-beta"), v("1.0.0-beta"));
        assert_eq!(v("1-beta").cmp(&v("1.0.0-beta")), Ordering::Equal);
        assert!(v("1") > v("1-beta"));
        assert!(v("1.0") > v("1-beta"));
        assert!(v("1.0.0") > v("1-beta"));
    }

    #[test]
    fn prerelease_precedence_rules() {
        // Normal version takes precedence over pre-release
        assert!(v("1.0.0") > v("1.0.0-alpha"));
        assert!(v("1.0.0") > v("1.0.0-rc.1"));

        // Standard SemVer 2.0.0 sequence:
        // 1.0.0-alpha < 1.0.0-alpha.1 < 1.0.0-alpha.beta < 1.0.0-beta < 1.0.0-beta.2 < 1.0.0-beta.11 < 1.0.0-rc.1 < 1.0.0
        let chain = [
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.beta",
            "1.0.0-beta",
            "1.0.0-beta.2",
            "1.0.0-beta.11",
            "1.0.0-rc.1",
            "1.0.0",
        ];
        for window in chain.windows(2) {
            assert!(
                v(window[0]) < v(window[1]),
                "expected {} < {}",
                window[0],
                window[1]
            );
        }

        // Numeric identifier has lower precedence than non-numeric
        assert!(v("1.0.0-1") < v("1.0.0-alpha"));
        assert!(v("1.0.0-999") < v("1.0.0-a"));

        // Larger set of identifiers has higher precedence
        assert!(v("1.0.0-alpha.1") > v("1.0.0-alpha"));
        assert!(v("1.0.0-alpha.1.1") > v("1.0.0-alpha.1"));

        // Core difference dominates pre-release
        assert!(v("2.0.0-alpha") > v("1.0.0"));
        assert!(v("1.0.1-alpha") > v("1.0.0"));
        assert!(v("1.1.0-alpha") > v("1.0.9"));
    }

    #[test]
    fn leading_zeros_in_prerelease_identifiers() {
        // Leading zeros permitted and interpreted numerically
        assert_eq!(v("1.0.0-01"), v("1.0.0-1"));
        assert_eq!(v("1.0.0-alpha.01"), v("1.0.0-alpha.1"));
        assert!(v("1.0.0-alpha.10") > v("1.0.0-alpha.09"));
        assert_eq!(v("1.0.0-00"), v("1.0.0-0"));
        assert!(v("1.0.0-01") > v("1.0.0-00"));
    }

    #[test]
    fn build_metadata_ignored_in_equality_and_order() {
        assert_eq!(v("1.0.0+1"), v("1.0.0+2"));
        assert_eq!(v("1.0.0+exp.sha.5114f85"), v("1.0.0+exp.sha.1234567"));
        assert_eq!(v("1.0.0-alpha+1"), v("1.0.0-alpha+2"));
        assert_eq!(v("1.0.0+1").cmp(&v("1.0.0+2")), Ordering::Equal);
        assert_eq!(v("1.0.0-alpha+1").cmp(&v("1.0.0-alpha+2")), Ordering::Equal);
    }

    #[test]
    fn equal_versions_hash_alike() {
        use std::collections::HashSet;
        let set: HashSet<Version> = [
            v("1.0"),
            v("1.0.0"),
            v("1.0.0+build1"),
            v("1.0.0+build2"),
            v("1.0.0-beta"),
            v("1.0-beta"),
            v("1.0.0-01"),
            v("1.0.0-1"),
            v("1.0.1"),
        ]
        .into_iter()
        .collect();

        // 1.0 == 1.0.0 == 1.0.0+build1 == 1.0.0+build2 (group 1)
        // 1.0.0-beta == 1.0-beta (group 2)
        // 1.0.0-01 == 1.0.0-1 (group 3)
        // 1.0.1 (group 4)
        assert_eq!(set.len(), 4);
    }

    #[test]
    fn raw_and_display() {
        let v1 = v("2026.01.5-alpha.01+build.001");
        assert_eq!(v1.raw(), "2026.01.5-alpha.01+build.001");
        assert_eq!(v1.to_string(), "2026.1.5-alpha.1+build.001");

        let v2 = v("1.0.0-beta");
        assert_eq!(v2.raw(), "1.0.0-beta");
        assert_eq!(v2.to_string(), "1.0.0-beta");

        let v3 = v("1.0.0+5");
        assert_eq!(v3.raw(), "1.0.0+5");
        assert_eq!(v3.to_string(), "1.0.0+5");

        let v4 = v("1.0-beta");
        assert_eq!(v4.raw(), "1.0-beta");
        assert_eq!(v4.to_string(), "1.0-beta");
    }

    #[test]
    fn a_leading_zero_in_a_component_is_read_as_the_number() {
        assert_eq!(v("2026.01"), v("2026.1"));
        assert!(v("2026.10") > v("2026.09"));
    }
}
