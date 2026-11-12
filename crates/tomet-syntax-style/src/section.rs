//! How the title of a section heading is written.

/// How the title of a section heading is written: between brackets (`===[ title ]`) or bare (`=== title`).
///
/// Both spellings read back as the same document. The bracketed spelling ([`SectionTitleStyle::Bracketed`])
/// is the canonical form of the language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionTitleStyle {
    /// The title is written between brackets (`===[ title ]` or `=[title]`).
    Bracketed,
    /// The title is written bare after the marker (`=== title`).
    Bare,
}

impl SectionTitleStyle {
    /// Every alternative in the order presented to people.
    pub const ALL: [SectionTitleStyle; 2] = [SectionTitleStyle::Bracketed, SectionTitleStyle::Bare];

    /// The name a configuration setting writes it by.
    pub fn name(self) -> &'static str {
        match self {
            SectionTitleStyle::Bracketed => "bracketed",
            SectionTitleStyle::Bare => "bare",
        }
    }

    /// The variant a setting value stands for.
    pub fn from_name(name: &str) -> Option<SectionTitleStyle> {
        Self::ALL.into_iter().find(|s| s.name() == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_names() {
        for variant in SectionTitleStyle::ALL {
            assert_eq!(SectionTitleStyle::from_name(variant.name()), Some(variant));
        }
    }
}
