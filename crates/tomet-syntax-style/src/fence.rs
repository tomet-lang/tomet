//! How the body lines of a fenced code block are written.

/// How the body lines of a fenced code block are written. Both spellings read back as
/// the same document: every line marked with ` `` `, or lines written bare.
///
/// The marked spelling ([`FenceLines::Marked`]) is the canonical form of the language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FenceLines {
    /// Every body line begins with ` `` ` (the canonical spelling).
    Marked,
    /// Lines are written bare where possible.
    Bare,
}

impl FenceLines {
    /// Every alternative in the order presented to people.
    pub const ALL: [FenceLines; 2] = [FenceLines::Marked, FenceLines::Bare];

    /// The name a configuration setting writes it by.
    pub fn name(self) -> &'static str {
        match self {
            FenceLines::Marked => "marked",
            FenceLines::Bare => "bare",
        }
    }

    /// The variant a setting value stands for.
    pub fn from_name(name: &str) -> Option<FenceLines> {
        Self::ALL.into_iter().find(|s| s.name() == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_names() {
        for variant in FenceLines::ALL {
            assert_eq!(FenceLines::from_name(variant.name()), Some(variant));
        }
    }
}
