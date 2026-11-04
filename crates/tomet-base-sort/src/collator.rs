//! Compiled collation engine and zero-allocation comparison iterators.

use std::cmp::Ordering;
use std::collections::HashSet;

use crate::direction::Direction;
use crate::error::CollationError;
use crate::key::{SortKey, pack_primary};
use crate::spec::{CollationSpec, Step};
use crate::trie::{Action, Trie};

/// A compiled collator for comparing and sorting strings according to custom collation rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Collator {
    pub(crate) trie: Trie,
    pub(crate) rest_slot: u32,
}

impl Default for Collator {
    fn default() -> Self {
        Self::codepoint()
    }
}

impl Collator {
    /// Creates a collator that orders strings strictly by Unicode scalar values.
    pub fn codepoint() -> Self {
        Self {
            trie: Trie::new(),
            rest_slot: 0,
        }
    }

    /// Returns `true` if this collator performs pure Unicode scalar value ordering.
    #[inline]
    pub fn is_codepoint(&self) -> bool {
        self.trie.is_empty()
    }

    /// Compiles a collation specification into a [`Collator`].
    pub fn new(spec: &CollationSpec) -> Result<Self, CollationError> {
        let mut seen_tokens = HashSet::new();
        let mut rest_slot: Option<u32> = None;

        for (step_idx, step) in spec.steps.iter().enumerate() {
            match step {
                Step::Rest => {
                    if rest_slot.is_some() {
                        return Err(CollationError::DuplicateRest);
                    }
                    let slot_u32 =
                        u32::try_from(step_idx).map_err(|_| CollationError::TooManyEntries)?;
                    rest_slot = Some(slot_u32);
                }
                Step::Group(tokens) => {
                    if tokens.is_empty() {
                        return Err(CollationError::EmptyGroup { step: step_idx });
                    }
                    u32::try_from(tokens.len()).map_err(|_| CollationError::TooManyEntries)?;
                    for token in tokens {
                        if token.is_empty() {
                            return Err(CollationError::EmptyToken);
                        }
                        if !seen_tokens.insert(token.as_str()) {
                            return Err(CollationError::DuplicateToken(token.clone()));
                        }
                    }
                }
            }
        }

        for token in &spec.ignore {
            if token.is_empty() {
                return Err(CollationError::EmptyToken);
            }
            if !seen_tokens.insert(token.as_str()) {
                return Err(CollationError::DuplicateToken(token.clone()));
            }
        }

        let default_rest_slot =
            u32::try_from(spec.steps.len()).map_err(|_| CollationError::TooManyEntries)?;
        let rest_slot = rest_slot.unwrap_or(default_rest_slot);
        let mut trie = Trie::new();

        for (step_idx, step) in spec.steps.iter().enumerate() {
            if let Step::Group(tokens) = step {
                let slot = u32::try_from(step_idx).map_err(|_| CollationError::TooManyEntries)?;
                let primary = pack_primary(slot, 0);
                for (sec_idx, token) in tokens.iter().enumerate() {
                    let sec = u32::try_from(sec_idx).map_err(|_| CollationError::TooManyEntries)?;
                    trie.insert(
                        token,
                        Action::Weight {
                            primary,
                            secondary: sec,
                        },
                    )?;
                }
            }
        }

        for token in &spec.ignore {
            trie.insert(token, Action::Ignore)?;
        }

        Ok(Self { trie, rest_slot })
    }

    /// Compares two strings using this collation specification.
    ///
    /// This method is zero-allocation: it streams primary weights first,
    /// enforces prefix dominance, and breaks ties by secondary weights if and
    /// only if both strings produce identical primary weight sequences.
    pub fn cmp(&self, a: &str, b: &str) -> Ordering {
        if self.is_codepoint() {
            return a.cmp(b);
        }

        let mut iter_a = TokenIterator::new(self, a);
        let mut iter_b = TokenIterator::new(self, b);
        let mut first_secondary_diff = None;

        loop {
            match (iter_a.next(), iter_b.next()) {
                (Some((pa, sa)), Some((pb, sb))) => {
                    if pa != pb {
                        return pa.cmp(&pb);
                    }
                    if first_secondary_diff.is_none() && sa != sb {
                        first_secondary_diff = Some(sa.cmp(&sb));
                    }
                }
                (None, Some(_)) => return Ordering::Less,
                (Some(_), None) => return Ordering::Greater,
                (None, None) => return first_secondary_diff.unwrap_or(Ordering::Equal),
            }
        }
    }

    /// Compares two strings with an explicit [`Direction`].
    #[inline]
    pub fn cmp_with_direction(&self, a: &str, b: &str, direction: Direction) -> Ordering {
        direction.apply(self.cmp(a, b))
    }

    /// Computes the 2-level [`SortKey`] for a given string.
    pub fn sort_key(&self, text: &str) -> SortKey {
        let mut iter = TokenIterator::new(self, text);
        let mut primary = Vec::with_capacity(text.len());
        let mut secondary = Vec::with_capacity(text.len());
        for (p, s) in &mut iter {
            primary.push(p);
            secondary.push(s);
        }
        SortKey { primary, secondary }
    }

    /// Stably sorts a slice of string-like values in ascending order.
    #[inline]
    pub fn sort<T: AsRef<str>>(&self, slice: &mut [T]) {
        self.sort_with_direction(slice, Direction::Asc);
    }

    /// Stably sorts a slice of string-like values with an explicit [`Direction`].
    pub fn sort_with_direction<T: AsRef<str>>(&self, slice: &mut [T], direction: Direction) {
        slice.sort_by(|a, b| self.cmp_with_direction(a.as_ref(), b.as_ref(), direction));
    }

    /// Stably sorts a slice of values by extracting a borrowed `&str` key in ascending order.
    #[inline]
    pub fn sort_by_key<T>(&self, slice: &mut [T], key: impl FnMut(&T) -> &str) {
        self.sort_by_key_with_direction(slice, Direction::Asc, key);
    }

    /// Stably sorts a slice of values by extracting a borrowed `&str` key with an explicit [`Direction`].
    pub fn sort_by_key_with_direction<T>(
        &self,
        slice: &mut [T],
        direction: Direction,
        mut key: impl FnMut(&T) -> &str,
    ) {
        slice.sort_by(|a, b| self.cmp_with_direction(key(a), key(b), direction));
    }
}

/// Zero-allocation streaming iterator yielding `(primary, secondary)` collation weights.
struct TokenIterator<'a> {
    collator: &'a Collator,
    text: &'a str,
    pos: usize,
}

impl<'a> TokenIterator<'a> {
    fn new(collator: &'a Collator, text: &'a str) -> Self {
        Self {
            collator,
            text,
            pos: 0,
        }
    }
}

impl<'a> Iterator for TokenIterator<'a> {
    type Item = (u64, u32);

    // UTF-8 boundary invariant:
    // Every terminal in `Trie` corresponds to a complete, valid UTF-8 `&str`.
    // Because UTF-8 is a self-synchronizing, prefix-free encoding, matching an
    // exact byte prefix that forms a valid UTF-8 string starting from a valid
    // character boundary is guaranteed to end on a valid character boundary.
    // Fallback decoding of `char` also advances strictly by `ch.len_utf8()`.
    // Therefore, `self.pos` always remains on a valid UTF-8 character boundary.
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let bytes = self.text.as_bytes();
        while self.pos < bytes.len() {
            if let Some((len, action)) = self.collator.trie.longest_match(bytes, self.pos) {
                self.pos += len;
                match action {
                    Action::Ignore => continue,
                    Action::Weight { primary, secondary } => {
                        return Some((primary, secondary));
                    }
                }
            } else {
                let ch = self.text[self.pos..]
                    .chars()
                    .next()
                    .expect("valid utf-8 character boundary");
                self.pos += ch.len_utf8();
                let primary = pack_primary(self.collator.rest_slot, ch as u32);
                return Some((primary, 0));
            }
        }
        None
    }
}
