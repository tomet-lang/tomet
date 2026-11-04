//! UTF-8 byte trie supporting greedy longest-prefix matching.

use crate::error::CollationError;

/// An action associated with a terminal node in the collation trie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    /// Skip the matched token completely without emitting any weight.
    Ignore,
    /// Emit the given primary and secondary collation weights.
    Weight { primary: u64, secondary: u32 },
}

/// A node in the arena-allocated byte trie.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TrieNode {
    pub(crate) terminal: Option<Action>,
    /// Transitions sorted by byte (`u8`) to enable binary search.
    pub(crate) transitions: Vec<(u8, u32)>,
}

/// Internal arena-allocated UTF-8 byte trie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Trie {
    nodes: Vec<TrieNode>,
}

impl Trie {
    /// Creates a new trie containing only an empty root node.
    pub(crate) fn new() -> Self {
        Self {
            nodes: vec![TrieNode::default()],
        }
    }

    /// Returns `true` if the trie contains only the root node with no transitions.
    pub(crate) fn is_empty(&self) -> bool {
        self.nodes.len() <= 1 && self.nodes[0].transitions.is_empty()
    }

    /// Inserts a token with the associated action.
    ///
    /// Tokens must be non-empty valid UTF-8 strings.
    pub(crate) fn insert(&mut self, token: &str, action: Action) -> Result<(), CollationError> {
        let mut node_idx = 0;
        for &b in token.as_bytes() {
            let child_idx = match self.nodes[node_idx]
                .transitions
                .binary_search_by_key(&b, |&(byte, _)| byte)
            {
                Ok(pos) => self.nodes[node_idx].transitions[pos].1 as usize,
                Err(pos) => {
                    let new_idx = self.nodes.len();
                    let new_idx_u32 =
                        u32::try_from(new_idx).map_err(|_| CollationError::TooManyEntries)?;
                    self.nodes.push(TrieNode::default());
                    self.nodes[node_idx]
                        .transitions
                        .insert(pos, (b, new_idx_u32));
                    new_idx
                }
            };
            node_idx = child_idx;
        }
        self.nodes[node_idx].terminal = Some(action);
        Ok(())
    }

    /// Performs greedy longest-prefix match on `bytes[pos..]`.
    ///
    /// Returns `Some((match_len, action))` if a terminal node was traversed,
    /// where `match_len` is the number of bytes consumed.
    /// Returns `None` if no terminal was reached.
    #[inline]
    pub(crate) fn longest_match(&self, bytes: &[u8], pos: usize) -> Option<(usize, Action)> {
        let mut node_idx = 0;
        let mut last_match = None;
        let slice = &bytes[pos..];

        for (i, &b) in slice.iter().enumerate() {
            let node = &self.nodes[node_idx];
            match node.transitions.binary_search_by_key(&b, |&(byte, _)| byte) {
                Ok(match_idx) => {
                    let next_node_idx = node.transitions[match_idx].1 as usize;
                    let next_node = &self.nodes[next_node_idx];
                    if let Some(action) = next_node.terminal {
                        last_match = Some((i + 1, action));
                    }
                    node_idx = next_node_idx;
                }
                Err(_) => break,
            }
        }

        last_match
    }
}
