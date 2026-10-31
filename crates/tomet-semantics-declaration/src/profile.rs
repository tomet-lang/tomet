//! The profiles a document can promise to stay within.

/// A subset of tomet that a document promises to stay inside: `::profile(oneline, standalone)`.
///
/// A profile never changes what a document means or how it is read: a document that keeps its
/// promise reads the same with or without the declaration. What the declaration buys is a guarantee
/// a reader can rely on. The two profiles are independent of each other; a document may promise
/// either, both, or neither.
///
/// Closed on purpose, like [`DeclKind`](crate::DeclKind): a name that is not here is reported, not
/// ignored, because a promise nobody can check is worse than none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Profile {
    /// Every line is told what it is from its own bytes: nothing spans lines, so a document can be
    /// cut at any newline and read in parallel.
    Oneline,
    /// What the document means is a function of its own bytes: nothing is supplied from outside it
    /// (the vault's config, the folder it sits in, another file), so it can be taken out of its vault.
    Standalone,
}

impl Profile {
    /// Every profile, in the order they are listed to people.
    pub const ALL: [Profile; 2] = [Profile::Oneline, Profile::Standalone];

    /// The name written in `::profile(...)`.
    pub fn name(self) -> &'static str {
        match self {
            Profile::Oneline => "oneline",
            Profile::Standalone => "standalone",
        }
    }

    /// The profile a name stands for.
    pub fn from_name(name: &str) -> Option<Profile> {
        Profile::ALL
            .into_iter()
            .find(|profile| profile.name() == name)
    }
}

impl std::fmt::Display for Profile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_profile_is_found_by_its_name_and_nothing_else_is() {
        for profile in Profile::ALL {
            assert_eq!(Profile::from_name(profile.name()), Some(profile));
        }
        assert_eq!(Profile::from_name("pure"), None);
        assert_eq!(Profile::from_name("Oneline"), None);
        assert_eq!(Profile::from_name(""), None);
    }
}
