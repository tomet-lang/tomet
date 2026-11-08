//! Where a bracket-less body's trailing groups can be: the lexical half of finding them.
//!
//! `- text {value}`, `= text #(id)`, `- text ::connect(...)`: the groups at the end of a body's
//! line are the owner's, not the text's, but only if they are groups -- `- note: see (the
//! manual)` ends in something that looks like one and is prose. Telling the two apart needs the
//! real grammar (the groups are lowered and have to be valid), and the real grammar is not cheap:
//! trying it at every `:` or `{` of a long line read the rest of the line once for each, which made
//! a line of `a: a: a: ...` take a time proportional to its length squared.
//!
//! So the question is put in two steps, and only the second reads the line with the grammar. This
//! module answers the first from the characters alone, in time proportional to the line: where a
//! chain of groups that could be the tail begins and ends. It overestimates -- anything it
//! rejects is not a tail, a chain it accepts may still be prose -- and the parser tries the
//! grammar once on what it accepts.

use std::collections::HashMap;

/// How many lines the groups a bracket-less body ends in may span.
pub(crate) const MAX_TRAILING_GROUP_LINES: usize = 64;

/// The brackets of one body's line (and the lines its groups run over), paired off once.
pub(crate) struct TailScan<'a> {
    src: &'a str,
    /// For each opening bracket, its closing one.
    closer: HashMap<usize, usize>,
    /// Candidate byte offsets on the first line where a body's trailing groups may begin.
    candidates: Vec<usize>,
    /// For each place a group, or the `:` of a connect, can start: whether the chain of groups from
    /// there reaches the end of a line, and if so whether it holds a group and where it ends.
    chains: HashMap<usize, Option<(bool, usize)>>,
}

impl<'a> TailScan<'a> {
    /// The line of `src` that `from` is on. A group may run over later lines, as long as its
    /// brackets stay open; a blank line ends the search, and so do
    /// [`MAX_TRAILING_GROUP_LINES`] lines: an unclosed bracket in a line of prose must not make
    /// every such line read to the end of the file.
    pub(crate) fn new(src: &'a str, from: usize) -> Self {
        let bytes = src.as_bytes();
        let mut closer = HashMap::new();
        let mut candidates = Vec::new();
        let mut open: Vec<(usize, u8)> = Vec::new();
        let (mut after_colon, mut after_name_char) = (false, false);
        let (mut lines, mut line_start, mut i) = (0, from, from);
        while i < bytes.len() {
            if let Some(len) = crate::inline::escaped_run_len(bytes, i) {
                after_colon = false;
                after_name_char = false;
                i += len;
                continue;
            }
            if lines == 0 && !matches!(bytes[i], b'\n' | b'\r') {
                let hash_paren = bytes[i] == b'#' && bytes.get(i + 1) == Some(&b'(');
                if bytes[i] == b':'
                    || ((hash_paren || bytes[i] == b'{') && !after_colon && !after_name_char)
                {
                    candidates.push(i);
                }
                if matches!(bytes[i], b' ' | b'\t') {
                    after_name_char = false;
                } else {
                    after_colon = bytes[i] == b':';
                    after_name_char = matches!(
                        bytes[i],
                        b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-'
                    );
                }
            }
            match bytes[i] {
                b'(' | b'[' | b'{' => open.push((i, bytes[i])),
                b')' | b']' | b'}' => {
                    if let Some(&(opener, kind)) = open.last() {
                        if matches!((kind, bytes[i]), (b'(', b')') | (b'[', b']') | (b'{', b'}')) {
                            closer.insert(opener, i);
                            open.pop();
                        }
                    }
                }
                b'\n' => {
                    lines += 1;
                    let blank = line_start > from && src[line_start..i].trim().is_empty();
                    if open.is_empty() || blank || lines >= MAX_TRAILING_GROUP_LINES {
                        break;
                    }
                    line_start = i + 1;
                }
                _ => {}
            }
            i += 1;
        }
        Self {
            src,
            closer,
            candidates,
            chains: HashMap::new(),
        }
    }

    /// The first place at or after `from`, on the same line, where a body's trailing groups may
    /// begin: every `:`, and every `{` and `#(` that is not part of a connect (`:{...}`,
    /// `::name(...)`) that starts earlier. A `(` alone is not one -- a parenthetical remark is
    /// prose -- and only reaches here through a `:`.
    pub(crate) fn next_candidate(&self, from: usize) -> Option<usize> {
        self.candidates
            .get(self.candidates.partition_point(|&c| c < from))
            .copied()
    }

    /// Where the groups that begin at `at` end the line, if they do and hold a group: the chain of
    /// `{...}`, `(...)`, `#(...)`, `::name` and a closing `|` run from `at` on, with nothing else between
    /// them, up to a newline. The offset of that newline (or of the end of the text).
    pub(crate) fn chain(&mut self, at: usize) -> Option<usize> {
        let bytes = self.src.as_bytes();
        // The elements walked over, so that each is walked once for the whole line however many
        // candidates reach it.
        let mut path: Vec<(usize, bool)> = Vec::new();
        let mut q = self.skip_blanks(at);
        let mut outcome = loop {
            if let Some(&known) = self.chains.get(&q) {
                break known;
            }
            let (group, next) = match bytes.get(q) {
                None | Some(b'\n' | b'\r') => break Some((false, q)),
                Some(b'{' | b'(') => (true, self.closer.get(&q).map(|&c| c + 1)),
                Some(b'#') if bytes.get(q + 1) == Some(&b'(') => {
                    (true, self.closer.get(&(q + 1)).map(|&c| c + 1))
                }
                Some(b':') => (false, Some(self.skip_name(q + 1))),
                // A `|` run is `[content]` spelled without brackets: it holds the rest of the line.
                Some(b'|') => (true, Some(self.line_end(q))),
                _ => break None,
            };
            let Some(next) = next else { break None };
            path.push((q, group));
            q = self.skip_blanks(next);
        };
        for &(at, group) in path.iter().rev() {
            outcome = outcome.map(|(holds, end)| (holds || group, end));
            self.chains.insert(at, outcome);
        }
        outcome.filter(|&(holds, _)| holds).map(|(_, end)| end)
    }

    /// The end of the line `i` is on: its newline, or the end of the text.
    fn line_end(&self, mut i: usize) -> usize {
        let bytes = self.src.as_bytes();
        while i < bytes.len() && !matches!(bytes[i], b'\n' | b'\r') {
            i += 1;
        }
        i
    }

    fn skip_blanks(&self, mut i: usize) -> usize {
        let bytes = self.src.as_bytes();
        while matches!(bytes.get(i), Some(b' ' | b'\t')) {
            i += 1;
        }
        i
    }

    /// Past a connect's name, if there is one after the `:`. Any run of characters that cannot
    /// start or end a group counts: the grammar decides what a name is.
    fn skip_name(&self, i: usize) -> usize {
        let bytes = self.src.as_bytes();
        let mut i = self.skip_blanks(i);
        while i < bytes.len()
            && !matches!(
                bytes[i],
                b' ' | b'\t'
                    | b'\r'
                    | b'\n'
                    | b':'
                    | b'{'
                    | b'}'
                    | b'('
                    | b')'
                    | b'['
                    | b']'
                    | b'#'
                    | b'|'
                    | b'\\'
            )
        {
            i += 1;
        }
        i
    }
}
