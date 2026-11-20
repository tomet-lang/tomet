//! How commas are written on the entries of a multi-line `(...)` group.

/// How optional line-ending and trailing commas are written on the entries of a `(...)` group
/// laid out over several lines. All three spellings read back as the same document.
///
/// ```text
/// Never        Between       Always
/// (            (             (
///   a: 1         a: 1,         a: 1,
///   b: 2         b: 2          b: 2,
/// )            )             )
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MultilineComma {
    /// Omit optional line-ending and trailing commas; newlines separate entries.
    Never,
    /// Write a comma after every entry except the last (no trailing comma).
    Between,
    /// Write a comma after every entry, including the last (trailing comma).
    Always,
}

impl MultilineComma {
    /// Every alternative in the order presented to people.
    pub const ALL: [MultilineComma; 3] = [
        MultilineComma::Never,
        MultilineComma::Between,
        MultilineComma::Always,
    ];

    /// The name a configuration setting writes it by.
    pub fn name(self) -> &'static str {
        match self {
            MultilineComma::Never => "never",
            MultilineComma::Between => "between",
            MultilineComma::Always => "always",
        }
    }

    /// The variant a setting value stands for.
    pub fn from_name(name: &str) -> Option<MultilineComma> {
        Self::ALL.into_iter().find(|c| c.name() == name)
    }

    /// Whether the entry at `index` out of `total` entries should end with a comma on a line break.
    pub fn trailing_for(self, index: usize, total: usize) -> bool {
        match self {
            MultilineComma::Never => false,
            MultilineComma::Between => index + 1 < total,
            MultilineComma::Always => index < total,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_names() {
        for variant in MultilineComma::ALL {
            assert_eq!(MultilineComma::from_name(variant.name()), Some(variant));
        }
        assert_eq!(MultilineComma::from_name("unknown"), None);
    }

    #[test]
    fn trailing_for_matches_each_variant() {
        assert_eq!(
            (0..3)
                .map(|i| MultilineComma::Never.trailing_for(i, 3))
                .collect::<Vec<_>>(),
            [false, false, false]
        );
        assert_eq!(
            (0..3)
                .map(|i| MultilineComma::Between.trailing_for(i, 3))
                .collect::<Vec<_>>(),
            [true, true, false]
        );
        assert_eq!(
            (0..3)
                .map(|i| MultilineComma::Always.trailing_for(i, 3))
                .collect::<Vec<_>>(),
            [true, true, true]
        );
        assert!(!MultilineComma::Between.trailing_for(0, 1));
        assert!(MultilineComma::Always.trailing_for(0, 1));
    }
}
